use crate::services::ai::{
    MimoExtractInput, MimoExtractResult, MimoProvider, MimoStatus, OrganizeDecision,
};
use crate::services::audit::AuditService;
use crate::services::budget::{BudgetService, BudgetSettingsInput, BudgetStatus};
use crate::services::candidates::{ActionCandidate, CandidateInput, CandidateService};
use crate::services::conflict_rules::{
    ApplyConflictRuleResult, ConflictAnswerInput, ConflictAnswerResult, ConflictDetail,
    ConflictRuleMatch, ConflictRuleService,
};
use crate::services::importer::{ImportService, InboxImportResult};
use crate::services::inbox::{InboxItem, InboxService};
use crate::services::ledger::{LedgerItem, LedgerService};
use crate::services::listener::{ListenerService, DEFAULT_LISTENER_STABLE_WAIT_MS};
use crate::services::markdown::{MarkdownDocument, MarkdownExport, MarkdownService};
use crate::services::movement::{MoveLog, MoveRequest, MovementService};
use crate::services::queue::{ListenerState, QueueItem, QueueService};
use crate::services::rag::{RagAnswer, RagIndexRun, RagIndexStatus, RagService};
use crate::services::rag_trace::RagTraceRun;
use crate::services::settings::{AppSettings, AppSettingsInput, SettingsService};
use crate::services::sticky::{StickyNote, StickyNoteInput, StickyService};
use crate::services::usage::{UsageService, UsageSummary};
use crate::services::vault::{
    canonical_vault_root, VaultInitResult, VaultService, VaultTreeNode, INBOX_DIR,
};
use crate::services::worker::{WorkerRunOptions, WorkerRunResult, WorkerService, WorkerStatus};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Default)]
pub struct WatcherRegistry {
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub listener: ListenerState,
    pub pending: Vec<QueueItem>,
    pub running: Vec<QueueItem>,
    pub failed: Vec<QueueItem>,
    pub conflicts: Vec<QueueItem>,
    pub completed: Vec<QueueItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlanResult {
    pub extraction: MimoExtractResult,
    pub plan: OrganizeDecision,
    pub candidates: Vec<ActionCandidate>,
    pub budget: BudgetStatus,
}

fn into_command_result<T>(result: crate::services::ServiceResult<T>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub fn select_vault(app: AppHandle) -> Result<Option<String>, String> {
    let selected = app.dialog().file().blocking_pick_folder();
    Ok(selected.map(|path| path.to_string()))
}

#[tauri::command]
pub fn init_vault(vault_path: String) -> Result<VaultInitResult, String> {
    into_command_result(VaultService::init(&vault_path))
}

#[tauri::command]
pub fn list_vault_tree(vault_path: String) -> Result<Vec<VaultTreeNode>, String> {
    into_command_result(VaultService::list_tree(&vault_path))
}

#[tauri::command]
pub fn read_markdown(
    vault_path: String,
    relative_path: String,
) -> Result<MarkdownDocument, String> {
    into_command_result(MarkdownService::read(&vault_path, &relative_path))
}

#[tauri::command]
pub fn save_markdown(
    vault_path: String,
    relative_path: String,
    content: String,
    frontmatter: Option<Value>,
) -> Result<MarkdownDocument, String> {
    into_command_result(MarkdownService::save(
        &vault_path,
        &relative_path,
        &content,
        frontmatter,
    ))
}

#[tauri::command]
pub fn export_markdown(
    vault_path: String,
    relative_path: String,
) -> Result<MarkdownExport, String> {
    into_command_result(MarkdownService::export(&vault_path, &relative_path))
}

#[tauri::command]
pub fn list_inbox(vault_path: String) -> Result<Vec<InboxItem>, String> {
    into_command_result(InboxService::list(&vault_path))
}

#[tauri::command]
pub fn parse_inbox_ledger(vault_path: String) -> Result<Vec<LedgerItem>, String> {
    into_command_result(LedgerService::parse_inbox_ledger(&vault_path))
}

#[tauri::command]
pub fn import_to_inbox(
    vault_path: String,
    source_paths: Vec<String>,
    mode: Option<String>,
) -> Result<Vec<InboxImportResult>, String> {
    into_command_result(ImportService::import_to_inbox(
        &vault_path,
        source_paths,
        mode,
    ))
}

#[tauri::command]
pub fn get_ai_usage(vault_path: String) -> Result<UsageSummary, String> {
    into_command_result(UsageService::summary(&vault_path))
}

#[tauri::command]
pub fn rebuild_rag_index(vault_path: String) -> Result<RagIndexRun, String> {
    into_command_result(RagService::rebuild_index(&vault_path))
}

#[tauri::command]
pub fn get_rag_index_status(vault_path: String) -> Result<RagIndexStatus, String> {
    into_command_result(RagService::status(&vault_path))
}

#[tauri::command]
pub fn ask_rag(
    vault_path: String,
    question: String,
    top_k: Option<usize>,
    conversation_id: Option<i64>,
) -> Result<RagAnswer, String> {
    into_command_result(RagService::ask(
        &vault_path,
        &question,
        top_k,
        conversation_id,
    ))
}

#[tauri::command]
pub fn get_latest_rag_trace(vault_path: String) -> Result<Option<RagTraceRun>, String> {
    into_command_result(RagService::latest_trace(&vault_path))
}

#[tauri::command]
pub fn get_app_settings(vault_path: String) -> Result<AppSettings, String> {
    into_command_result(SettingsService::get(&vault_path))
}

#[tauri::command]
pub fn save_app_settings(
    vault_path: String,
    settings: AppSettingsInput,
) -> Result<AppSettings, String> {
    into_command_result(SettingsService::save(&vault_path, settings))
}

#[tauri::command]
pub fn get_queue_status(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(queue_status(&vault_path))
}

#[tauri::command]
pub fn get_inbox_listener_status(vault_path: String) -> Result<ListenerState, String> {
    into_command_result(ListenerService::status(&vault_path))
}

#[tauri::command]
pub fn get_worker_status(vault_path: String) -> Result<WorkerStatus, String> {
    into_command_result(WorkerService::status(&vault_path))
}

#[tauri::command]
pub fn run_inbox_worker(
    vault_path: String,
    options: Option<WorkerRunOptions>,
) -> Result<WorkerRunResult, String> {
    into_command_result(WorkerService::drain(
        &vault_path,
        options.unwrap_or(WorkerRunOptions {
            max_items: None,
            stable_wait_ms: None,
            force_mock: None,
        }),
    ))
}

#[tauri::command]
pub fn pause_inbox_worker(vault_path: String) -> Result<WorkerStatus, String> {
    into_command_result(WorkerService::pause(&vault_path))
}

#[tauri::command]
pub fn resume_inbox_worker(vault_path: String) -> Result<WorkerStatus, String> {
    into_command_result(WorkerService::resume(&vault_path))
}

#[tauri::command]
pub fn scan_inbox_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(
        ListenerService::scan_inbox(&vault_path, DEFAULT_LISTENER_STABLE_WAIT_MS)
            .and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn pause_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(
        ListenerService::mark_stopped(&vault_path).and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn resume_queue(vault_path: String) -> Result<QueueStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let inbox = root.join(INBOX_DIR);
    into_command_result(
        ListenerService::mark_running(&vault_path, inbox.to_string_lossy().to_string())
            .and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn retry_queue_item(vault_path: String, queue_id: i64) -> Result<QueueItem, String> {
    into_command_result(QueueService::retry_item(&vault_path, queue_id))
}

#[tauri::command]
pub fn skip_queue_item(
    vault_path: String,
    queue_id: i64,
    reason: Option<String>,
) -> Result<QueueItem, String> {
    into_command_result(QueueService::skip_item(
        &vault_path,
        queue_id,
        reason.as_deref(),
    ))
}

#[tauri::command]
pub fn start_inbox_watcher(
    vault_path: String,
    registry: tauri::State<'_, WatcherRegistry>,
) -> Result<QueueStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let inbox = root.join(INBOX_DIR);
    let watch_key = root.to_string_lossy().to_string();
    let vault_for_callback = vault_path.clone();
    let root_for_callback = root.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            for path in event.paths {
                let _ = ListenerService::process_path_after_wait(
                    &vault_for_callback,
                    &root_for_callback,
                    path,
                    DEFAULT_LISTENER_STABLE_WAIT_MS,
                );
            }
        }
    })
    .map_err(|error| error.to_string())?;
    watcher
        .watch(&inbox, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    registry
        .watchers
        .lock()
        .map_err(|_| "watcher registry poisoned".to_string())?
        .insert(watch_key, watcher);
    into_command_result(
        ListenerService::mark_running(&vault_path, inbox.to_string_lossy().to_string())
            .and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn stop_inbox_watcher(
    vault_path: String,
    registry: tauri::State<'_, WatcherRegistry>,
) -> Result<QueueStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    registry
        .watchers
        .lock()
        .map_err(|_| "watcher registry poisoned".to_string())?
        .remove(&root.to_string_lossy().to_string());
    into_command_result(
        ListenerService::mark_stopped(&vault_path).and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn get_budget_status(vault_path: String) -> Result<BudgetStatus, String> {
    into_command_result(BudgetService::status(&vault_path))
}

#[tauri::command]
pub fn save_budget_settings(vault_path: String, settings: Value) -> Result<BudgetStatus, String> {
    let input = BudgetSettingsInput {
        scope: None,
        monthly_limit_cents: settings
            .get("monthlyLimitCents")
            .or_else(|| settings.get("monthly_limit_cents"))
            .and_then(Value::as_i64),
        daily_limit_cents: settings
            .get("hardStopCents")
            .or_else(|| settings.get("dailyLimitCents"))
            .and_then(Value::as_i64),
        paused: settings
            .get("paused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        retry_limit: settings
            .get("retryLimit")
            .and_then(Value::as_i64)
            .unwrap_or(3),
        cooldown_seconds: settings
            .get("cooldownSeconds")
            .or_else(|| settings.get("cooldown_seconds"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    };
    into_command_result(
        BudgetService::save_settings(&vault_path, input)
            .and_then(|_| BudgetService::status(&vault_path)),
    )
}

#[tauri::command]
pub fn get_mimo_status(vault_path: String) -> Result<MimoStatus, String> {
    into_command_result(MimoProvider::status(&vault_path))
}

#[tauri::command]
pub fn extract_with_mimo(
    vault_path: String,
    input: MimoExtractInput,
) -> Result<MimoExtractResult, String> {
    into_command_result(MimoProvider::extract_file(&vault_path, input))
}

#[tauri::command]
pub fn plan_inbox_item(
    vault_path: String,
    source_relative_path: String,
    force_mock: Option<bool>,
) -> Result<InboxPlanResult, String> {
    let extraction = MimoProvider::extract_file(
        &vault_path,
        MimoExtractInput {
            relative_path: source_relative_path.clone(),
            force_mock: force_mock.unwrap_or(false),
        },
    )
    .map_err(|error| error.to_string())?;
    let plan = MimoProvider::organize_decision(
        &vault_path,
        &source_relative_path,
        Some(&extraction.text),
        force_mock.unwrap_or(false),
    )
    .map_err(|error| error.to_string())?;
    let candidates =
        create_candidates_from_decision(&vault_path, &plan).map_err(|error| error.to_string())?;
    let budget = BudgetService::status(&vault_path).map_err(|error| error.to_string())?;
    Ok(InboxPlanResult {
        extraction,
        plan,
        candidates,
        budget,
    })
}

#[tauri::command]
pub fn plan_ai_organize(
    vault_path: String,
    source_relative_path: Option<String>,
    extracted_text: Option<String>,
    force_mock: Option<bool>,
) -> Result<OrganizeDecision, String> {
    into_command_result(MimoProvider::organize_decision(
        &vault_path,
        source_relative_path
            .as_deref()
            .unwrap_or("000-收集箱/示例笔记.md"),
        extracted_text.as_deref(),
        force_mock.unwrap_or(false),
    ))
}

#[tauri::command]
pub fn run_ai_organize(
    vault_path: String,
    plan_id: Option<String>,
    source_relative_path: Option<String>,
    target_relative_path: Option<String>,
    reason: Option<String>,
) -> Result<Value, String> {
    if let (Some(source), Some(target)) = (source_relative_path, target_relative_path) {
        let log = MovementService::move_from_inbox(
            &vault_path,
            MoveRequest {
                source_relative_path: source,
                target_relative_path: target,
                reason,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(json!({
            "moved": 1,
            "skipped": 0,
            "conflicts": 0,
            "auditId": log.id.to_string(),
            "movement": &log,
            "message": "AI 整理移动已执行"
        }))
    } else {
        Ok(json!({
            "moved": 0,
            "skipped": 0,
            "conflicts": 0,
            "auditId": plan_id.unwrap_or_else(|| "plan-only".to_string()),
            "message": "Plan recorded; select a concrete target before moving"
        }))
    }
}

#[tauri::command]
pub fn move_inbox_item(
    vault_path: String,
    source_relative_path: String,
    target_relative_path: String,
    reason: Option<String>,
) -> Result<MoveLog, String> {
    into_command_result(MovementService::move_from_inbox(
        &vault_path,
        MoveRequest {
            source_relative_path,
            target_relative_path,
            reason,
        },
    ))
}

#[tauri::command]
pub fn rollback_move(
    vault_path: String,
    movement_id: Option<i64>,
    audit_id: Option<String>,
) -> Result<MoveLog, String> {
    let movement_id = movement_id
        .or_else(|| audit_id.and_then(|id| id.parse::<i64>().ok()))
        .ok_or_else(|| "movement_id or audit_id is required".to_string())?;
    into_command_result(MovementService::rollback(&vault_path, movement_id))
}

#[tauri::command]
pub fn list_move_logs(vault_path: String) -> Result<Vec<MoveLog>, String> {
    into_command_result(MovementService::list(&vault_path))
}

#[tauri::command]
pub fn list_todo_schedule_candidates(vault_path: String) -> Result<Vec<ActionCandidate>, String> {
    into_command_result(CandidateService::list_pending(&vault_path))
}

#[tauri::command]
pub fn create_todo_schedule_candidate(
    vault_path: String,
    candidate: CandidateInput,
) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::create(&vault_path, candidate))
}

#[tauri::command]
pub fn confirm_todo_schedule_candidate(
    vault_path: String,
    candidate_id: i64,
) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::confirm(&vault_path, candidate_id))
}

#[tauri::command]
pub fn dismiss_todo_schedule_candidate(
    vault_path: String,
    candidate_id: i64,
) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::reject(&vault_path, candidate_id))
}

#[tauri::command]
pub fn list_sticky_notes(
    vault_path: String,
    include_archived: Option<bool>,
) -> Result<Vec<StickyNote>, String> {
    into_command_result(StickyService::list(
        &vault_path,
        include_archived.unwrap_or(false),
    ))
}

#[tauri::command]
pub fn save_sticky_note(vault_path: String, note: StickyNoteInput) -> Result<StickyNote, String> {
    into_command_result(StickyService::save(&vault_path, note))
}

#[tauri::command]
pub fn delete_sticky_note(vault_path: String, note_id: i64) -> Result<(), String> {
    into_command_result(StickyService::delete(&vault_path, note_id))
}

#[tauri::command]
pub fn autosave_sticky_note(vault_path: String, note_id: i64) -> Result<String, String> {
    into_command_result(StickyService::autosave_to_inbox(&vault_path, note_id))
}

#[tauri::command]
pub async fn prewarm_sticky_windows(app: AppHandle, count: i64) -> Result<Vec<String>, String> {
    let mut labels = Vec::new();
    for index in 0..count.clamp(0, 4) {
        let label = format!("sticky-prewarm-{index}");
        if app.get_webview_window(&label).is_none() {
            WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
                .title("TheBrain Sticky")
                .inner_size(320.0, 240.0)
                .visible(false)
                .build()
                .map_err(|error| error.to_string())?;
        }
        labels.push(label);
    }
    Ok(labels)
}

#[tauri::command]
pub fn register_global_shortcut(app: AppHandle, shortcut: String) -> Result<String, String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| error.to_string())?;
    let registered = shortcut.clone();
    let parsed: Shortcut = shortcut
        .parse()
        .map_err(|error| format!("invalid shortcut: {error}"))?;
    app.global_shortcut()
        .on_shortcut(parsed, move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ =
                        window.emit("thebrain-sticky-shortcut", json!({"shortcut": registered}));
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(shortcut)
}

#[tauri::command]
pub fn list_conflicts(vault_path: String) -> Result<Vec<ConflictDetail>, String> {
    into_command_result(ConflictRuleService::list_open_conflicts(&vault_path))
}

#[tauri::command]
pub fn get_conflict_detail(
    vault_path: String,
    conflict_id: String,
) -> Result<ConflictDetail, String> {
    into_command_result(ConflictRuleService::get_conflict(&vault_path, &conflict_id))
}

#[tauri::command]
pub fn submit_conflict_answer(
    vault_path: String,
    input: ConflictAnswerInput,
) -> Result<ConflictAnswerResult, String> {
    into_command_result(ConflictRuleService::submit_answer(&vault_path, input))
}

#[tauri::command]
pub fn match_conflict_rules(
    vault_path: String,
    source_relative_path: String,
    target_relative_path: String,
    message: Option<String>,
) -> Result<Vec<ConflictRuleMatch>, String> {
    into_command_result(ConflictRuleService::match_rules(
        &vault_path,
        source_relative_path,
        target_relative_path,
        message,
    ))
}

#[tauri::command]
pub fn apply_conflict_rule(
    vault_path: String,
    conflict_id: String,
    rule_id: i64,
) -> Result<ApplyConflictRuleResult, String> {
    into_command_result(ConflictRuleService::apply_rule(
        &vault_path,
        conflict_id,
        rule_id,
    ))
}

#[tauri::command]
pub fn resolve_conflict(
    vault_path: String,
    conflict_id: String,
    action: String,
) -> Result<Value, String> {
    let parsed_id = conflict_id
        .parse::<i64>()
        .map_err(|_| "conflict_id must be an audit event id".to_string())?;
    let event = AuditService::record(
        &vault_path,
        "conflict_resolved",
        json!({
            "conflictId": parsed_id,
            "action": action,
            "status": "resolved"
        }),
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "resolved",
        "conflictId": parsed_id,
        "auditId": event.id
    }))
}

fn create_candidates_from_decision(
    vault_path: &str,
    decision: &OrganizeDecision,
) -> crate::services::ServiceResult<Vec<ActionCandidate>> {
    let mut created = Vec::new();
    for item in &decision.todo_candidates {
        created.push(CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: Some(decision.source_relative_path.clone()),
                title: candidate_title(item, "Untitled todo"),
                payload: Some(candidate_payload(item, decision)),
            },
        )?);
    }
    for item in &decision.schedule_candidates {
        created.push(CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "schedule".to_string(),
                source_relative_path: Some(decision.source_relative_path.clone()),
                title: candidate_title(item, "Untitled schedule item"),
                payload: Some(candidate_payload(item, decision)),
            },
        )?);
    }
    Ok(created)
}

fn candidate_title(item: &Value, fallback: &str) -> String {
    item.get("title")
        .or_else(|| item.get("text"))
        .or_else(|| item.get("summary"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn candidate_payload(item: &Value, decision: &OrganizeDecision) -> Value {
    let mut payload = match item {
        Value::Object(object) => object.clone(),
        other => {
            let mut object = Map::new();
            object.insert("value".to_string(), other.clone());
            object
        }
    };
    payload.insert(
        "sourceRelativePath".to_string(),
        Value::String(decision.source_relative_path.clone()),
    );
    payload.insert(
        "targetRelativePath".to_string(),
        Value::String(decision.target_relative_path.clone()),
    );
    payload.insert(
        "planStatus".to_string(),
        Value::String(decision.status.clone()),
    );
    payload.insert(
        "planProvider".to_string(),
        Value::String(decision.provider.clone()),
    );
    Value::Object(payload)
}

fn queue_status(vault_path: &str) -> crate::services::ServiceResult<QueueStatus> {
    Ok(QueueStatus {
        listener: QueueService::get_listener_state(vault_path)?,
        pending: QueueService::list_by_status(vault_path, "pending")?,
        running: QueueService::list_by_status(vault_path, "running")?,
        failed: QueueService::list_by_status(vault_path, "failed")?,
        conflicts: QueueService::list_by_status(vault_path, "conflict")?,
        completed: QueueService::list_by_status(vault_path, "completed")?,
    })
}
