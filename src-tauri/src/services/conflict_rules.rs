use crate::services::audit::{AuditEvent, AuditService};
use crate::services::index::open_index_for_vault;
use crate::services::movement::{MoveRequest, MovementService};
use crate::services::vault::{canonical_vault_root, normalize_relative_path, INTERNAL_DIR};
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;

pub const RULES_DIR: &str = ".thebrain/rules";
pub const INBOX_RULES_FILE: &str = ".thebrain/rules/inbox-organizing-rules.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRule {
    pub id: i64,
    pub rule_key: String,
    pub source_pattern: String,
    pub target_pattern: String,
    pub answer: String,
    pub action: String,
    pub auto_apply: bool,
    pub match_summary: String,
    pub markdown_path: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub hit_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRuleMatch {
    pub rule: ConflictRule,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFilePreview {
    pub relative_path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPreviewContext {
    pub source_relative_path: Option<String>,
    pub target_relative_path: Option<String>,
    pub source_exists: bool,
    pub target_exists: bool,
    pub source: Option<ConflictFilePreview>,
    pub target: Option<ConflictFilePreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSuggestion {
    pub target_relative_path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictDetail {
    pub id: String,
    pub event_id: i64,
    pub event_type: String,
    pub created_at: String,
    pub status: String,
    pub source_relative_path: Option<String>,
    pub target_relative_path: Option<String>,
    pub message: Option<String>,
    pub payload: Value,
    pub preview: Option<ConflictPreviewContext>,
    pub rename_suggestions: Vec<RenameSuggestion>,
    pub recommendations: Vec<ConflictRuleMatch>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAnswerInput {
    pub conflict_id: String,
    pub answer: String,
    pub action: Option<String>,
    pub target_relative_path: Option<String>,
    pub auto_apply: Option<bool>,
    pub match_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAnswerResult {
    pub conflict_id: String,
    pub rule: ConflictRule,
    pub markdown_path: String,
    pub audit_id: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConflictRuleResult {
    pub conflict_id: String,
    pub rule_id: i64,
    pub status: String,
    pub moved: bool,
    pub movement_id: Option<i64>,
    pub message: String,
    pub audit_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRuleUpdateInput {
    pub rule_id: i64,
    pub status: Option<String>,
    pub answer: Option<String>,
    pub action: Option<String>,
    pub target_pattern: Option<String>,
    pub auto_apply: Option<bool>,
    pub match_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRuleUpdateResult {
    pub rule: ConflictRule,
    pub audit_id: i64,
    pub status: String,
}

pub struct ConflictRuleService;

impl ConflictRuleService {
    pub fn list_open_conflicts(vault_path: &str) -> ServiceResult<Vec<ConflictDetail>> {
        let conflicts = AuditService::list_by_type(vault_path, "conflict")?;
        let resolved = AuditService::list_by_type(vault_path, "conflict_resolved")?;
        let resolved_ids = resolved
            .iter()
            .filter_map(|event| conflict_id_from_payload(&event.payload))
            .collect::<std::collections::HashSet<_>>();

        let mut details = Vec::new();
        for event in conflicts {
            if resolved_ids.contains(&event.id) {
                continue;
            }
            let mut detail = detail_from_event(event);
            Self::attach_preview(vault_path, &mut detail)?;
            detail.recommendations =
                Self::match_for_detail(vault_path, &detail, Some(detail.event_id))?;
            details.push(detail);
        }
        Ok(details)
    }

    pub fn get_conflict(vault_path: &str, conflict_id: &str) -> ServiceResult<ConflictDetail> {
        let event_id = parse_conflict_id(conflict_id)?;
        let mut detail = detail_from_event(AuditService::get(vault_path, event_id)?);
        Self::attach_preview(vault_path, &mut detail)?;
        detail.recommendations = Self::match_for_detail(vault_path, &detail, Some(event_id))?;
        Ok(detail)
    }

    pub fn submit_answer(
        vault_path: &str,
        input: ConflictAnswerInput,
    ) -> ServiceResult<ConflictAnswerResult> {
        let detail = Self::get_conflict(vault_path, &input.conflict_id)?;
        let answer = input.answer.trim();
        if answer.is_empty() {
            return Err(ServiceError::InvalidState(
                "conflict answer cannot be empty".to_string(),
            ));
        }

        let source = detail
            .source_relative_path
            .clone()
            .ok_or_else(|| ServiceError::InvalidState("conflict has no source path".to_string()))?;
        let target = input
            .target_relative_path
            .or_else(|| detail.target_relative_path.clone())
            .ok_or_else(|| ServiceError::InvalidState("conflict has no target path".to_string()))?;
        let source = normalize_relative_path(&source)?;
        let target = normalize_relative_path(&target)?;
        let action = input.action.unwrap_or_else(|| "rename".to_string());
        ensure_rule_action(&action)?;
        let auto_apply = input.auto_apply.unwrap_or(false);
        let match_summary = input
            .match_summary
            .unwrap_or_else(|| summarize_match(&source, &target));
        let now = Utc::now().to_rfc3339();
        let rule_key = format!(
            "conflict-{}-{}",
            detail.event_id,
            now.replace([':', '.', '+'], "-")
        );

        let connection = open_index_for_vault(vault_path)?;
        connection.execute(
            "INSERT INTO conflict_rules
                (rule_key, source_pattern, target_pattern, answer, action, auto_apply,
                 match_summary, markdown_path, status, created_at, updated_at, hit_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9, ?9, 0)",
            params![
                rule_key,
                source,
                target,
                answer,
                action,
                auto_apply as i64,
                match_summary,
                INBOX_RULES_FILE,
                now
            ],
        )?;
        let rule = Self::get_rule_by_id(vault_path, connection.last_insert_rowid())?;
        append_rule_markdown(vault_path, &detail, &rule)?;
        let audit = AuditService::record(
            vault_path,
            "conflict_rule_created",
            json!({
                "conflictId": detail.event_id,
                "ruleId": rule.id,
                "sourceRelativePath": rule.source_pattern,
                "targetRelativePath": rule.target_pattern,
                "answer": rule.answer,
                "action": rule.action,
                "autoApply": rule.auto_apply,
                "matchSummary": rule.match_summary,
                "markdownPath": INBOX_RULES_FILE
            }),
        )?;

        Ok(ConflictAnswerResult {
            conflict_id: detail.id,
            rule,
            markdown_path: INBOX_RULES_FILE.to_string(),
            audit_id: audit.id,
            status: "recorded".to_string(),
        })
    }

    pub fn match_rules(
        vault_path: &str,
        source_relative_path: String,
        target_relative_path: String,
        message: Option<String>,
    ) -> ServiceResult<Vec<ConflictRuleMatch>> {
        let detail = ConflictDetail {
            id: "ad-hoc".to_string(),
            event_id: 0,
            event_type: "conflict".to_string(),
            created_at: Utc::now().to_rfc3339(),
            status: "open".to_string(),
            source_relative_path: Some(normalize_relative_path(&source_relative_path)?),
            target_relative_path: Some(normalize_relative_path(&target_relative_path)?),
            message,
            payload: Value::Null,
            preview: None,
            rename_suggestions: Vec::new(),
            recommendations: Vec::new(),
        };
        Self::match_for_detail(vault_path, &detail, None)
    }

    pub fn suggest_rename_targets(
        vault_path: &str,
        target_relative_path: String,
        limit: Option<usize>,
    ) -> ServiceResult<Vec<RenameSuggestion>> {
        let target = normalize_relative_path(&target_relative_path)?;
        suggest_rename_targets(vault_path, &target, limit.unwrap_or(5).min(10))
    }

    pub fn list_rules(
        vault_path: &str,
        include_disabled: bool,
    ) -> ServiceResult<Vec<ConflictRule>> {
        let connection = open_index_for_vault(vault_path)?;
        let sql = if include_disabled {
            "SELECT id, rule_key, source_pattern, target_pattern, answer, action, auto_apply,
                    match_summary, markdown_path, status, created_at, updated_at, hit_count
             FROM conflict_rules
             ORDER BY status ASC, hit_count DESC, id DESC"
        } else {
            "SELECT id, rule_key, source_pattern, target_pattern, answer, action, auto_apply,
                    match_summary, markdown_path, status, created_at, updated_at, hit_count
             FROM conflict_rules
             WHERE status = 'active'
             ORDER BY hit_count DESC, id DESC"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], row_to_rule)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_rule_status(
        vault_path: &str,
        rule_id: i64,
        status: String,
    ) -> ServiceResult<ConflictRuleUpdateResult> {
        Self::update_rule(
            vault_path,
            ConflictRuleUpdateInput {
                rule_id,
                status: Some(status),
                answer: None,
                action: None,
                target_pattern: None,
                auto_apply: None,
                match_summary: None,
            },
        )
    }

    pub fn update_rule(
        vault_path: &str,
        input: ConflictRuleUpdateInput,
    ) -> ServiceResult<ConflictRuleUpdateResult> {
        let before = Self::get_rule_by_id(vault_path, input.rule_id)?;
        let status = match input.status {
            Some(status) => normalize_rule_status(&status)?,
            None => before.status.clone(),
        };
        let answer = match input.answer {
            Some(answer) => {
                let answer = answer.trim().to_string();
                if answer.is_empty() {
                    return Err(ServiceError::InvalidState(
                        "conflict rule answer cannot be empty".to_string(),
                    ));
                }
                answer
            }
            None => before.answer.clone(),
        };
        let action = match input.action {
            Some(action) => {
                ensure_rule_action(&action)?;
                action
            }
            None => before.action.clone(),
        };
        let target_pattern = match input.target_pattern {
            Some(target) => normalize_relative_path(&target)?,
            None => before.target_pattern.clone(),
        };
        let auto_apply = input.auto_apply.unwrap_or(before.auto_apply);
        let match_summary = match input.match_summary {
            Some(summary) => {
                let summary = summary.trim().to_string();
                if summary.is_empty() {
                    return Err(ServiceError::InvalidState(
                        "conflict rule match summary cannot be empty".to_string(),
                    ));
                }
                summary
            }
            None => before.match_summary.clone(),
        };
        let now = Utc::now().to_rfc3339();
        let connection = open_index_for_vault(vault_path)?;
        connection.execute(
            "UPDATE conflict_rules
             SET status = ?1, answer = ?2, action = ?3, target_pattern = ?4,
                 auto_apply = ?5, match_summary = ?6, updated_at = ?7
             WHERE id = ?8",
            params![
                status,
                answer,
                action,
                target_pattern,
                auto_apply as i64,
                match_summary,
                now,
                before.id
            ],
        )?;
        let rule = Self::get_rule_by_id(vault_path, before.id)?;
        let audit = AuditService::record(
            vault_path,
            "conflict_rule_updated",
            json!({
                "ruleId": rule.id,
                "beforeStatus": before.status,
                "status": rule.status,
                "action": rule.action,
                "targetRelativePath": rule.target_pattern,
                "autoApply": rule.auto_apply,
                "matchSummary": rule.match_summary,
                "markdownPath": rule.markdown_path
            }),
        )?;
        Ok(ConflictRuleUpdateResult {
            rule,
            audit_id: audit.id,
            status: "updated".to_string(),
        })
    }

    pub fn apply_rule(
        vault_path: &str,
        conflict_id: String,
        rule_id: i64,
    ) -> ServiceResult<ApplyConflictRuleResult> {
        let detail = Self::get_conflict(vault_path, &conflict_id)?;
        let rule = Self::get_rule_by_id(vault_path, rule_id)?;
        let source = detail
            .source_relative_path
            .clone()
            .ok_or_else(|| ServiceError::InvalidState("conflict has no source path".to_string()))?;
        let source = normalize_relative_path(&source)?;
        let target = normalize_relative_path(&rule.target_pattern)?;

        if rule.action == "skip" || rule.action == "keep_existing" {
            let resolved = AuditService::record(
                vault_path,
                "conflict_resolved",
                json!({
                    "conflictId": detail.event_id,
                    "action": rule.action,
                    "ruleId": rule.id,
                    "status": "resolved"
                }),
            )?;
            Self::record_hit(
                vault_path,
                &rule,
                &detail,
                1.0,
                "confirmed non-moving rule",
                "applied",
            )?;
            return Ok(ApplyConflictRuleResult {
                conflict_id: detail.id,
                rule_id,
                status: "resolved".to_string(),
                moved: false,
                movement_id: None,
                message: "rule recorded conflict as resolved without moving files".to_string(),
                audit_id: Some(resolved.id),
            });
        }

        match MovementService::move_from_inbox(
            vault_path,
            MoveRequest {
                source_relative_path: source,
                target_relative_path: target,
                reason: Some(format!("conflict rule {}", rule.id)),
            },
        ) {
            Ok(log) => {
                let resolved = AuditService::record(
                    vault_path,
                    "conflict_resolved",
                    json!({
                        "conflictId": detail.event_id,
                        "action": "apply_rule",
                        "ruleId": rule.id,
                        "movementId": log.id,
                        "status": "resolved"
                    }),
                )?;
                Self::record_hit(
                    vault_path,
                    &rule,
                    &detail,
                    1.0,
                    "confirmed rule applied",
                    "applied",
                )?;
                Ok(ApplyConflictRuleResult {
                    conflict_id: detail.id,
                    rule_id,
                    status: "resolved".to_string(),
                    moved: true,
                    movement_id: Some(log.id),
                    message: "rule applied and source file moved".to_string(),
                    audit_id: Some(resolved.id),
                })
            }
            Err(ServiceError::Conflict(message)) => {
                let audit = AuditService::record(
                    vault_path,
                    "conflict_rule_apply_failed",
                    json!({
                        "conflictId": detail.event_id,
                        "ruleId": rule.id,
                        "sourceRelativePath": detail.source_relative_path,
                        "targetRelativePath": rule.target_pattern,
                        "status": "conflict",
                        "message": message
                    }),
                )?;
                Self::record_hit(
                    vault_path,
                    &rule,
                    &detail,
                    0.5,
                    "rule target conflicted",
                    "conflict",
                )?;
                Ok(ApplyConflictRuleResult {
                    conflict_id: detail.id,
                    rule_id,
                    status: "conflict".to_string(),
                    moved: false,
                    movement_id: None,
                    message,
                    audit_id: Some(audit.id),
                })
            }
            Err(error) => Err(error),
        }
    }

    pub fn match_for_detail(
        vault_path: &str,
        detail: &ConflictDetail,
        conflict_id: Option<i64>,
    ) -> ServiceResult<Vec<ConflictRuleMatch>> {
        let source = detail.source_relative_path.as_deref().unwrap_or_default();
        let target = detail.target_relative_path.as_deref().unwrap_or_default();
        let source = normalize_relative_path(source).unwrap_or_default();
        let target = normalize_relative_path(target).unwrap_or_default();
        let rules = Self::list_active_rules(vault_path)?;
        let mut matches = rules
            .into_iter()
            .filter_map(|rule| score_rule(&rule, &source, &target, detail.message.as_deref()))
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(3);
        for item in &matches {
            if conflict_id.is_some() {
                Self::record_hit(
                    vault_path,
                    &item.rule,
                    detail,
                    item.score,
                    &item.reason,
                    "recommended",
                )?;
            }
        }
        Ok(matches)
    }

    fn list_active_rules(vault_path: &str) -> ServiceResult<Vec<ConflictRule>> {
        let connection = open_index_for_vault(vault_path)?;
        let mut statement = connection.prepare(
            "SELECT id, rule_key, source_pattern, target_pattern, answer, action, auto_apply,
                    match_summary, markdown_path, status, created_at, updated_at, hit_count
             FROM conflict_rules
             WHERE status = 'active'
             ORDER BY hit_count DESC, id DESC",
        )?;
        let rows = statement.query_map([], row_to_rule)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn attach_preview(vault_path: &str, detail: &mut ConflictDetail) -> ServiceResult<()> {
        let preview = build_preview_context(
            vault_path,
            detail.source_relative_path.as_deref(),
            detail.target_relative_path.as_deref(),
        )?;
        detail.rename_suggestions = detail
            .target_relative_path
            .as_deref()
            .map(|target| suggest_rename_targets(vault_path, target, 5))
            .transpose()?
            .unwrap_or_default();
        detail.preview = Some(preview);
        Ok(())
    }

    fn get_rule_by_id(vault_path: &str, rule_id: i64) -> ServiceResult<ConflictRule> {
        let connection = open_index_for_vault(vault_path)?;
        connection
            .query_row(
                "SELECT id, rule_key, source_pattern, target_pattern, answer, action, auto_apply,
                        match_summary, markdown_path, status, created_at, updated_at, hit_count
                 FROM conflict_rules WHERE id = ?1",
                params![rule_id],
                row_to_rule,
            )
            .optional()?
            .ok_or_else(|| ServiceError::InvalidState(format!("rule {rule_id} does not exist")))
    }

    fn record_hit(
        vault_path: &str,
        rule: &ConflictRule,
        detail: &ConflictDetail,
        score: f64,
        reason: &str,
        status: &str,
    ) -> ServiceResult<()> {
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO conflict_rule_hits
                (rule_id, conflict_id, source_relative_path, target_relative_path, score,
                 reason, status, created_at, applied_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CASE WHEN ?7 = 'applied' THEN ?8 ELSE NULL END)",
            params![
                rule.id,
                detail.event_id,
                detail.source_relative_path.as_deref(),
                detail.target_relative_path.as_deref(),
                score,
                reason,
                status,
                now
            ],
        )?;
        connection.execute(
            "UPDATE conflict_rules
             SET hit_count = hit_count + 1, updated_at = ?1
             WHERE id = ?2",
            params![now, rule.id],
        )?;
        Ok(())
    }
}

fn append_rule_markdown(
    vault_path: &str,
    detail: &ConflictDetail,
    rule: &ConflictRule,
) -> ServiceResult<()> {
    let root = canonical_vault_root(vault_path)?;
    let rules_dir = root.join(RULES_DIR);
    if !rules_dir.starts_with(root.join(INTERNAL_DIR)) {
        return Err(ServiceError::EscapedVault(RULES_DIR.to_string()));
    }
    fs::create_dir_all(&rules_dir)?;
    let path = root.join(INBOX_RULES_FILE);
    if !path.exists() {
        fs::write(
            &path,
            "# Inbox Organizing Rules\n\nThis file is local-readable rule memory for inbox conflict decisions. It is internal TheBrain state and is not indexed as user content.\n",
        )?;
    }
    let entry = format!(
        "\n## Rule {}\n- Created: {}\n- Conflict id: {}\n- Source: `{}`\n- Target: `{}`\n- Action: `{}`\n- Auto apply: {}\n- Match condition: {}\n- Answer: {}\n- Original message: {}\n",
        rule.id,
        rule.created_at,
        detail.event_id,
        rule.source_pattern,
        rule.target_pattern,
        rule.action,
        rule.auto_apply,
        rule.match_summary,
        rule.answer.replace('\n', " "),
        detail.message.clone().unwrap_or_default().replace('\n', " ")
    );
    fs::OpenOptions::new()
        .append(true)
        .open(path)?
        .write_all(entry.as_bytes())?;
    Ok(())
}

fn detail_from_event(event: AuditEvent) -> ConflictDetail {
    let source = value_string(
        &event.payload,
        &["sourceRelativePath", "relativePath", "sourcePath"],
    );
    let target = value_string(&event.payload, &["targetRelativePath", "targetPath"]);
    let message = value_string(&event.payload, &["message", "error", "reason"]);
    let status = event
        .payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("open")
        .to_string();
    ConflictDetail {
        id: event.id.to_string(),
        event_id: event.id,
        event_type: event.event_type,
        created_at: event.created_at,
        status,
        source_relative_path: source,
        target_relative_path: target,
        message,
        payload: event.payload,
        preview: None,
        rename_suggestions: Vec::new(),
        recommendations: Vec::new(),
    }
}

fn build_preview_context(
    vault_path: &str,
    source: Option<&str>,
    target: Option<&str>,
) -> ServiceResult<ConflictPreviewContext> {
    let source_preview = source
        .map(|path| build_file_preview(vault_path, path))
        .transpose()?;
    let target_preview = target
        .map(|path| build_file_preview(vault_path, path))
        .transpose()?;
    Ok(ConflictPreviewContext {
        source_relative_path: source_preview
            .as_ref()
            .map(|preview| preview.relative_path.clone()),
        target_relative_path: target_preview
            .as_ref()
            .map(|preview| preview.relative_path.clone()),
        source_exists: source_preview
            .as_ref()
            .map(|preview| preview.exists)
            .unwrap_or(false),
        target_exists: target_preview
            .as_ref()
            .map(|preview| preview.exists)
            .unwrap_or(false),
        source: source_preview,
        target: target_preview,
    })
}

fn build_file_preview(vault_path: &str, relative_path: &str) -> ServiceResult<ConflictFilePreview> {
    let root = canonical_vault_root(vault_path)?;
    let normalized = normalize_relative_path(relative_path)?;
    let path = root.join(&normalized);
    if !path.exists() {
        return Ok(ConflictFilePreview {
            relative_path: normalized,
            exists: false,
            size_bytes: None,
            snippet: None,
        });
    }
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(&root) {
        return Err(ServiceError::EscapedVault(
            canonical.to_string_lossy().to_string(),
        ));
    }
    let metadata = fs::metadata(&canonical)?;
    if !metadata.is_file() {
        return Ok(ConflictFilePreview {
            relative_path: normalized,
            exists: true,
            size_bytes: None,
            snippet: None,
        });
    }
    let snippet = if is_preview_text_path(&normalized) && metadata.len() <= 1_000_000 {
        fs::read_to_string(&canonical)
            .ok()
            .map(|body| body.chars().take(1200).collect())
    } else {
        None
    };
    Ok(ConflictFilePreview {
        relative_path: normalized,
        exists: true,
        size_bytes: Some(metadata.len()),
        snippet,
    })
}

fn suggest_rename_targets(
    vault_path: &str,
    target_relative_path: &str,
    limit: usize,
) -> ServiceResult<Vec<RenameSuggestion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let root = canonical_vault_root(vault_path)?;
    let target = normalize_relative_path(target_relative_path)?;
    let target_path = Path::new(&target);
    let parent = target_path.parent().and_then(Path::to_str).unwrap_or("");
    let stem = target_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ServiceError::InvalidRelativePath(target.clone()))?;
    let extension = target_path.extension().and_then(|value| value.to_str());
    let mut suggestions = Vec::new();
    let mut suffix = 1;
    while suggestions.len() < limit && suffix <= 100 {
        let file_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}-{suffix}.{extension}"),
            _ => format!("{stem}-{suffix}"),
        };
        let candidate = if parent.is_empty() {
            file_name
        } else {
            format!("{parent}/{file_name}")
        };
        let normalized = normalize_relative_path(&candidate)?;
        let full_path = root.join(&normalized);
        if let Some(parent) = full_path.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize()?;
                if !canonical_parent.starts_with(&root) {
                    return Err(ServiceError::EscapedVault(
                        canonical_parent.to_string_lossy().to_string(),
                    ));
                }
            }
        }
        if !full_path.exists() {
            suggestions.push(RenameSuggestion {
                target_relative_path: normalized,
                exists: false,
            });
        }
        suffix += 1;
    }
    Ok(suggestions)
}

fn is_preview_text_path(relative_path: &str) -> bool {
    matches!(
        Path::new(relative_path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "txt")
    )
}

fn value_string(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn conflict_id_from_payload(payload: &Value) -> Option<i64> {
    payload
        .get("conflictId")
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
}

fn parse_conflict_id(conflict_id: &str) -> ServiceResult<i64> {
    conflict_id.parse::<i64>().map_err(|_| {
        ServiceError::InvalidState("conflict id must be an audit event id".to_string())
    })
}

fn ensure_rule_action(action: &str) -> ServiceResult<()> {
    match action {
        "rename" | "skip" | "keep_existing" => Ok(()),
        other => Err(ServiceError::InvalidState(format!(
            "unsupported conflict rule action: {other}"
        ))),
    }
}

fn normalize_rule_status(status: &str) -> ServiceResult<String> {
    match status {
        "active" | "disabled" => Ok(status.to_string()),
        other => Err(ServiceError::InvalidState(format!(
            "unsupported conflict rule status: {other}"
        ))),
    }
}

fn summarize_match(source: &str, target: &str) -> String {
    let target_dir = target.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let source_ext = source.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    format!("same target directory `{target_dir}` and source extension `{source_ext}`")
}

fn score_rule(
    rule: &ConflictRule,
    source: &str,
    target: &str,
    message: Option<&str>,
) -> Option<ConflictRuleMatch> {
    let mut score = 0.0;
    let mut reasons = Vec::new();
    if parent_dir(&rule.target_pattern) == parent_dir(target) {
        score += 0.55;
        reasons.push("same target directory");
    }
    if extension(&rule.source_pattern) == extension(source) && !extension(source).is_empty() {
        score += 0.25;
        reasons.push("same source file type");
    }
    if !target.is_empty() && rule.target_pattern == target {
        score += 0.3;
        reasons.push("same target path");
    }
    if let Some(message) = message {
        if !message.is_empty() && rule.match_summary.contains(message) {
            score += 0.1;
            reasons.push("message appears in rule summary");
        }
    }
    if score < 0.25 {
        return None;
    }
    Some(ConflictRuleMatch {
        rule: rule.clone(),
        score,
        reason: reasons.join(", "),
    })
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

fn extension(path: &str) -> &str {
    path.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("")
}

fn row_to_rule(row: &Row<'_>) -> rusqlite::Result<ConflictRule> {
    Ok(ConflictRule {
        id: row.get(0)?,
        rule_key: row.get(1)?,
        source_pattern: row.get(2)?,
        target_pattern: row.get(3)?,
        answer: row.get(4)?,
        action: row.get(5)?,
        auto_apply: row.get::<_, i64>(6)? != 0,
        match_summary: row.get(7)?,
        markdown_path: row.get(8)?,
        status: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        hit_count: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::listener::ListenerService;
    use crate::services::vault::{VaultService, INBOX_DIR};
    use serde_json::json;

    #[test]
    fn records_conflict_answer_as_markdown_and_index_row() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "conflict",
            json!({
                "sourceRelativePath": format!("{INBOX_DIR}/a.md"),
                "targetRelativePath": "100-School/a.md",
                "message": "target already exists",
                "status": "open"
            }),
        )
        .unwrap();

        let result = ConflictRuleService::submit_answer(
            temp.path().to_str().unwrap(),
            ConflictAnswerInput {
                conflict_id: event.id.to_string(),
                answer: "School notes with this target should be renamed by date.".to_string(),
                action: Some("rename".to_string()),
                target_relative_path: None,
                auto_apply: Some(false),
                match_summary: Some("same school target folder".to_string()),
            },
        )
        .unwrap();

        let rule_file = temp.path().join(INBOX_RULES_FILE);
        let body = fs::read_to_string(rule_file).unwrap();
        assert!(body.contains("School notes with this target should be renamed by date."));
        assert!(body.contains(&format!("Conflict id: {}", event.id)));
        assert_eq!(result.rule.source_pattern, format!("{INBOX_DIR}/a.md"));
        assert_eq!(result.rule.target_pattern, "100-School/a.md");
    }

    #[test]
    fn recommends_similar_conflict_rules() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "conflict",
            json!({
                "sourceRelativePath": format!("{INBOX_DIR}/a.md"),
                "targetRelativePath": "100-School/a.md",
                "message": "target already exists"
            }),
        )
        .unwrap();
        ConflictRuleService::submit_answer(
            temp.path().to_str().unwrap(),
            ConflictAnswerInput {
                conflict_id: event.id.to_string(),
                answer: "Use date suffix for school notes.".to_string(),
                action: Some("rename".to_string()),
                target_relative_path: None,
                auto_apply: Some(false),
                match_summary: None,
            },
        )
        .unwrap();

        let matches = ConflictRuleService::match_rules(
            temp.path().to_str().unwrap(),
            format!("{INBOX_DIR}/b.md"),
            "100-School/b.md".to_string(),
            Some("target already exists".to_string()),
        )
        .unwrap();

        assert_eq!(matches.len(), 1);
        assert!(matches[0].score >= 0.25);
    }

    #[test]
    fn rename_suggestions_are_preview_only_and_skip_existing_names() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        fs::write(temp.path().join("100-School").join("a.md"), "existing").unwrap();
        fs::write(
            temp.path().join("100-School").join("a-1.md"),
            "existing suffix",
        )
        .unwrap();
        fs::write(temp.path().join(INBOX_DIR).join("a.md"), "source").unwrap();

        let suggestions = ConflictRuleService::suggest_rename_targets(
            temp.path().to_str().unwrap(),
            "100-School/a.md".to_string(),
            Some(3),
        )
        .unwrap();

        assert_eq!(
            suggestions
                .iter()
                .map(|item| item.target_relative_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "100-School/a-2.md",
                "100-School/a-3.md",
                "100-School/a-4.md"
            ]
        );
        assert_eq!(
            fs::read_to_string(temp.path().join(INBOX_DIR).join("a.md")).unwrap(),
            "source"
        );
        assert!(!temp.path().join("100-School").join("a-2.md").exists());
    }

    #[test]
    fn disabled_rules_are_excluded_from_recommendations() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "conflict",
            json!({
                "sourceRelativePath": format!("{INBOX_DIR}/a.md"),
                "targetRelativePath": "100-School/a.md",
                "message": "target already exists"
            }),
        )
        .unwrap();
        let created = ConflictRuleService::submit_answer(
            temp.path().to_str().unwrap(),
            ConflictAnswerInput {
                conflict_id: event.id.to_string(),
                answer: "Use date suffix for school notes.".to_string(),
                action: Some("rename".to_string()),
                target_relative_path: None,
                auto_apply: Some(false),
                match_summary: None,
            },
        )
        .unwrap();

        let updated = ConflictRuleService::set_rule_status(
            temp.path().to_str().unwrap(),
            created.rule.id,
            "disabled".to_string(),
        )
        .unwrap();
        assert_eq!(updated.rule.status, "disabled");

        let matches = ConflictRuleService::match_rules(
            temp.path().to_str().unwrap(),
            format!("{INBOX_DIR}/b.md"),
            "100-School/b.md".to_string(),
            Some("target already exists".to_string()),
        )
        .unwrap();
        assert!(matches.is_empty());
        assert!(
            ConflictRuleService::list_rules(temp.path().to_str().unwrap(), false)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            ConflictRuleService::list_rules(temp.path().to_str().unwrap(), true)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn conflict_detail_preview_reads_bounded_text_context() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        let source_body = "s".repeat(1500);
        fs::write(temp.path().join(INBOX_DIR).join("a.md"), &source_body).unwrap();
        fs::write(temp.path().join("100-School").join("a.md"), "target text").unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "conflict",
            json!({
                "sourceRelativePath": format!("{INBOX_DIR}/a.md"),
                "targetRelativePath": "100-School/a.md",
                "message": "target already exists"
            }),
        )
        .unwrap();

        let detail =
            ConflictRuleService::get_conflict(temp.path().to_str().unwrap(), &event.id.to_string())
                .unwrap();
        let preview = detail.preview.unwrap();

        assert!(preview.source_exists);
        assert!(preview.target_exists);
        assert_eq!(
            preview.source.unwrap().snippet.unwrap().chars().count(),
            1200
        );
        assert_eq!(
            preview.target.unwrap().snippet.unwrap(),
            "target text".to_string()
        );
        assert_eq!(
            detail.rename_suggestions[0].target_relative_path,
            "100-School/a-1.md"
        );
    }

    #[test]
    fn applying_rule_does_not_overwrite_existing_target() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::write(temp.path().join(INBOX_DIR).join("a.md"), "source").unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        fs::write(temp.path().join("100-School").join("a.md"), "existing").unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "conflict",
            json!({
                "sourceRelativePath": format!("{INBOX_DIR}/a.md"),
                "targetRelativePath": "100-School/a.md",
                "message": "target already exists"
            }),
        )
        .unwrap();
        let result = ConflictRuleService::submit_answer(
            temp.path().to_str().unwrap(),
            ConflictAnswerInput {
                conflict_id: event.id.to_string(),
                answer: "Try the school path only after confirmation.".to_string(),
                action: Some("rename".to_string()),
                target_relative_path: None,
                auto_apply: Some(false),
                match_summary: None,
            },
        )
        .unwrap();

        let applied = ConflictRuleService::apply_rule(
            temp.path().to_str().unwrap(),
            event.id.to_string(),
            result.rule.id,
        )
        .unwrap();

        assert_eq!(applied.status, "conflict");
        assert!(!applied.moved);
        assert_eq!(
            fs::read_to_string(temp.path().join("100-School").join("a.md")).unwrap(),
            "existing"
        );
        assert!(temp.path().join(INBOX_DIR).join("a.md").exists());
    }

    #[test]
    fn rejects_rule_target_escape() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "conflict",
            json!({
                "sourceRelativePath": format!("{INBOX_DIR}/a.md"),
                "targetRelativePath": "100-School/a.md"
            }),
        )
        .unwrap();

        let error = ConflictRuleService::submit_answer(
            temp.path().to_str().unwrap(),
            ConflictAnswerInput {
                conflict_id: event.id.to_string(),
                answer: "bad target".to_string(),
                action: Some("rename".to_string()),
                target_relative_path: Some("../outside.md".to_string()),
                auto_apply: Some(false),
                match_summary: None,
            },
        )
        .unwrap_err();

        assert!(matches!(error, ServiceError::InvalidRelativePath(_)));
    }

    #[test]
    fn listener_skips_rule_memory_file() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let rule_file = root.join(INBOX_RULES_FILE);
        fs::create_dir_all(rule_file.parent().unwrap()).unwrap();
        fs::write(&rule_file, "# rules").unwrap();

        let outcome =
            ListenerService::process_path(temp.path().to_str().unwrap(), &root, rule_file, 0)
                .unwrap();

        assert_eq!(outcome.status, "skipped");
    }
}
