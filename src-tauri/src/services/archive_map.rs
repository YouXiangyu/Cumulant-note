use crate::services::index::IndexService;
use crate::services::ledger::LedgerService;
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, INBOX_DIR, INTERNAL_DIR,
};
use crate::services::{ServiceError, ServiceResult};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const ARCHIVE_MAP_RELATIVE_PATH: &str = ".thebrain/rules/archive-map.md";
pub const ARCHIVE_MAP_RULES_RELATIVE_PATH: &str = ".thebrain/rules/archive-map-rules.md";
const MAX_SEMANTIC_FILES_PER_DIRECTORY: usize = 5;
const MAX_SEMANTIC_BYTES_PER_FILE: u64 = 8192;
const MAX_HEADING_HINTS_PER_DIRECTORY: usize = 6;
const MAX_CONTENT_HINTS_PER_DIRECTORY: usize = 8;
const MAX_SEMANTIC_SUMMARY_CHARS: usize = 240;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapDirectory {
    pub relative_path: String,
    pub depth: usize,
    pub file_count: usize,
    pub child_count: usize,
    pub sample_files: Vec<String>,
    pub keyword_hints: Vec<String>,
    pub heading_hints: Vec<String>,
    pub content_hints: Vec<String>,
    pub semantic_summary: String,
    pub historical_moves: usize,
    pub rule: Option<ArchiveMapDirectoryRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapRun {
    pub id: i64,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub directory_count: i64,
    pub file_count: i64,
    pub history_count: i64,
    pub markdown_path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapSnapshot {
    pub run: ArchiveMapRun,
    pub generated_at: String,
    pub markdown_path: String,
    pub directory_count: usize,
    pub file_count: usize,
    pub history_count: usize,
    pub directories: Vec<ArchiveMapDirectory>,
    pub health: ArchiveMapHealth,
    pub stale_directories: Vec<ArchiveMapStaleDirectory>,
    pub top_hit_directories: Vec<ArchiveMapDirectoryHitStat>,
    pub rules: Vec<ArchiveMapDirectoryRule>,
    pub rules_markdown_path: String,
    pub locked_directory_count: usize,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapDirectoryRule {
    pub id: i64,
    pub relative_path: String,
    pub user_note: String,
    pub organizing_hint: String,
    pub locked: bool,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub exists_in_current_map: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapDirectoryRuleInput {
    pub relative_path: String,
    pub user_note: Option<String>,
    pub organizing_hint: Option<String>,
    pub locked: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapHealth {
    pub status: String,
    pub is_stale: bool,
    pub stale_reasons: Vec<String>,
    pub generated_at: String,
    pub age_seconds: Option<i64>,
    pub markdown_exists: bool,
    pub latest_run_status: String,
    pub cached_directory_count: usize,
    pub current_directory_count: usize,
    pub stale_directory_count: usize,
    pub added_directories: Vec<String>,
    pub removed_directories: Vec<String>,
    pub changed_directories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapStaleDirectory {
    pub relative_path: String,
    pub reason: String,
    pub cached_file_count: Option<usize>,
    pub current_file_count: Option<usize>,
    pub historical_moves: usize,
    pub last_moved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapDirectoryHitStat {
    pub relative_path: String,
    pub move_count: usize,
    pub last_moved_at: Option<String>,
    pub last_source_relative_path: Option<String>,
    pub last_target_relative_path: Option<String>,
    pub exists_in_current_map: bool,
}

pub struct ArchiveMapService;

impl ArchiveMapService {
    pub fn latest_or_rebuild(vault_path: &str) -> ServiceResult<ArchiveMapSnapshot> {
        match Self::latest(vault_path)? {
            Some(snapshot) => Ok(snapshot),
            None => Self::rebuild(vault_path),
        }
    }

    pub fn latest(vault_path: &str) -> ServiceResult<Option<ArchiveMapSnapshot>> {
        let (root, connection) = open_connection(vault_path)?;
        let run_id = connection
            .query_row(
                "SELECT id FROM archive_map_runs
                 WHERE status = 'ok'
                 ORDER BY id DESC
                 LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        run_id
            .map(|id| snapshot_by_run_id(&root, &connection, id))
            .transpose()
    }

    pub fn rebuild(vault_path: &str) -> ServiceResult<ArchiveMapSnapshot> {
        let (root, mut connection) = open_connection(vault_path)?;
        let started_at = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO archive_map_runs
                (status, started_at, markdown_path)
             VALUES ('running', ?1, ?2)",
            params![started_at, ARCHIVE_MAP_RELATIVE_PATH],
        )?;
        let run_id = connection.last_insert_rowid();

        match rebuild_inner(vault_path, &root, &mut connection, run_id) {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                let finished_at = Utc::now().to_rfc3339();
                let message = error.to_string();
                let _ = connection.execute(
                    "UPDATE archive_map_runs
                     SET status = 'failed', finished_at = ?1, error = ?2
                     WHERE id = ?3",
                    params![finished_at, message, run_id],
                );
                Err(error)
            }
        }
    }

    pub fn markdown_context(vault_path: &str) -> ServiceResult<String> {
        let snapshot = Self::latest_or_rebuild(vault_path)?;
        let rules_markdown = build_rules_markdown(&snapshot.rules);
        if rules_markdown.trim().is_empty() {
            Ok(snapshot.markdown)
        } else {
            Ok(format!("{}\n\n{}", snapshot.markdown, rules_markdown))
        }
    }

    pub fn contains_directory(snapshot: &ArchiveMapSnapshot, relative_dir: &str) -> bool {
        snapshot
            .directories
            .iter()
            .any(|entry| entry.relative_path == relative_dir)
    }

    pub fn list_directory_rules(vault_path: &str) -> ServiceResult<Vec<ArchiveMapDirectoryRule>> {
        let (root, connection) = open_connection(vault_path)?;
        let current_paths = current_directory_paths(&root)?;
        read_directory_rules(&connection, &current_paths, true)
    }

    pub fn upsert_directory_rule(
        vault_path: &str,
        input: ArchiveMapDirectoryRuleInput,
    ) -> ServiceResult<ArchiveMapDirectoryRule> {
        let (root, connection) = open_connection(vault_path)?;
        let current_paths = current_directory_paths(&root)?;
        let relative_path = validate_rule_directory(&current_paths, &input.relative_path)?;
        let existing = directory_rule_by_path(&connection, &relative_path, &current_paths)?;
        let now = Utc::now().to_rfc3339();
        let user_note = input
            .user_note
            .map(|value| clean_rule_text(&value, 1200))
            .or_else(|| existing.as_ref().map(|rule| rule.user_note.clone()))
            .unwrap_or_default();
        let organizing_hint = input
            .organizing_hint
            .map(|value| clean_rule_text(&value, 1200))
            .or_else(|| existing.as_ref().map(|rule| rule.organizing_hint.clone()))
            .unwrap_or_default();
        let locked = input
            .locked
            .or_else(|| existing.as_ref().map(|rule| rule.locked))
            .unwrap_or(false);
        let status = validate_rule_status(
            input
                .status
                .or_else(|| existing.as_ref().map(|rule| rule.status.clone()))
                .unwrap_or_else(|| "active".to_string())
                .as_str(),
        )?;

        match existing {
            Some(rule) => {
                connection.execute(
                    "UPDATE archive_map_directory_rules
                     SET user_note = ?1,
                         organizing_hint = ?2,
                         locked = ?3,
                         status = ?4,
                         updated_at = ?5
                     WHERE id = ?6",
                    params![
                        user_note,
                        organizing_hint,
                        locked as i64,
                        status,
                        now,
                        rule.id
                    ],
                )?;
            }
            None => {
                connection.execute(
                    "INSERT INTO archive_map_directory_rules
                        (relative_path, user_note, organizing_hint, locked, status, created_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        &relative_path,
                        &user_note,
                        &organizing_hint,
                        locked as i64,
                        &status,
                        &now,
                        &now
                    ],
                )?;
            }
        }

        let rules = read_directory_rules(&connection, &current_paths, true)?;
        write_rules_markdown(&root, &rules)?;
        directory_rule_by_path(&connection, &relative_path, &current_paths)?.ok_or_else(|| {
            ServiceError::InvalidState("archive map rule was not persisted".to_string())
        })
    }
}

fn open_connection(vault_path: &str) -> ServiceResult<(PathBuf, Connection)> {
    let root = canonical_vault_root(vault_path)?;
    let opened = IndexService::open_or_create(&root)?;
    Ok((root, Connection::open(opened.path)?))
}

fn rebuild_inner(
    vault_path: &str,
    root: &Path,
    connection: &mut Connection,
    run_id: i64,
) -> ServiceResult<ArchiveMapSnapshot> {
    let mut directories = scan_formal_directories(root)?;
    let history_counts = read_history_counts(vault_path, connection)?;
    for directory in &mut directories {
        directory.historical_moves = history_counts
            .get(&directory.relative_path)
            .copied()
            .unwrap_or_default();
    }
    let file_count = directories
        .iter()
        .map(|directory| directory.file_count)
        .sum::<usize>();
    let history_count = history_counts.values().sum::<usize>();
    let markdown = build_markdown(&directories, &history_counts);
    write_markdown(root, &markdown)?;

    let finished_at = Utc::now().to_rfc3339();
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE archive_map_runs
         SET status = 'ok',
             finished_at = ?1,
             directory_count = ?2,
             file_count = ?3,
             history_count = ?4,
             error = NULL
         WHERE id = ?5",
        params![
            finished_at,
            directories.len() as i64,
            file_count as i64,
            history_count as i64,
            run_id
        ],
    )?;
    for directory in &directories {
        transaction.execute(
            "INSERT INTO archive_map_entries
                (run_id, relative_path, depth, file_count, child_count,
                 sample_files_json, keyword_hints_json, heading_hints_json,
                 content_hints_json, semantic_summary, historical_moves, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'active')",
            params![
                run_id,
                directory.relative_path,
                directory.depth as i64,
                directory.file_count as i64,
                directory.child_count as i64,
                serde_json::to_string(&directory.sample_files)?,
                serde_json::to_string(&directory.keyword_hints)?,
                serde_json::to_string(&directory.heading_hints)?,
                serde_json::to_string(&directory.content_hints)?,
                directory.semantic_summary,
                directory.historical_moves as i64
            ],
        )?;
    }
    transaction.commit()?;
    snapshot_by_run_id(root, connection, run_id)
}

fn snapshot_by_run_id(
    root: &Path,
    connection: &Connection,
    run_id: i64,
) -> ServiceResult<ArchiveMapSnapshot> {
    let run = run_by_id(connection, run_id)?;
    let mut statement = connection.prepare(
        "SELECT relative_path, depth, file_count, child_count, sample_files_json,
                keyword_hints_json, historical_moves, heading_hints_json,
                content_hints_json, semantic_summary
         FROM archive_map_entries
         WHERE run_id = ?1 AND status = 'active'
         ORDER BY relative_path",
    )?;
    let rows = statement.query_map(params![run_id], row_to_directory)?;
    let mut directories = rows.collect::<Result<Vec<_>, _>>()?;
    let current_paths = current_directory_paths(root)?;
    let rules = read_directory_rules(connection, &current_paths, true)?;
    attach_directory_rules(&mut directories, &rules);
    let locked_directory_count = rules
        .iter()
        .filter(|rule| rule.status == "active" && rule.locked && rule.exists_in_current_map)
        .count();
    let markdown = fs::read_to_string(root.join(&run.markdown_path)).unwrap_or_default();
    let generated_at = run
        .finished_at
        .clone()
        .unwrap_or_else(|| run.started_at.clone());
    let (health, stale_directories, top_hit_directories) =
        analyze_snapshot(root, connection, &run, &directories, &generated_at)?;
    Ok(ArchiveMapSnapshot {
        markdown_path: run.markdown_path.clone(),
        directory_count: run.directory_count as usize,
        file_count: run.file_count as usize,
        history_count: run.history_count as usize,
        generated_at,
        directories,
        health,
        stale_directories,
        top_hit_directories,
        rules,
        rules_markdown_path: ARCHIVE_MAP_RULES_RELATIVE_PATH.to_string(),
        locked_directory_count,
        markdown,
        run,
    })
}

fn analyze_snapshot(
    root: &Path,
    connection: &Connection,
    run: &ArchiveMapRun,
    cached_directories: &[ArchiveMapDirectory],
    generated_at: &str,
) -> ServiceResult<(
    ArchiveMapHealth,
    Vec<ArchiveMapStaleDirectory>,
    Vec<ArchiveMapDirectoryHitStat>,
)> {
    let current_directories = scan_formal_directories(root)?;
    let cached_map = cached_directories
        .iter()
        .map(|directory| (directory.relative_path.clone(), directory))
        .collect::<BTreeMap<_, _>>();
    let current_map = current_directories
        .iter()
        .map(|directory| (directory.relative_path.clone(), directory))
        .collect::<BTreeMap<_, _>>();
    let current_paths = current_map.keys().cloned().collect::<BTreeSet<_>>();
    let top_hit_directories = read_movement_hit_stats(connection, &current_paths)?;

    let mut added_directories = Vec::new();
    let mut removed_directories = Vec::new();
    let mut changed_directories = Vec::new();
    let mut stale_directories = Vec::new();
    let mut reported = BTreeSet::new();

    for (relative_path, current) in &current_map {
        if !cached_map.contains_key(relative_path) {
            added_directories.push(relative_path.clone());
            reported.insert(relative_path.clone());
            stale_directories.push(ArchiveMapStaleDirectory {
                relative_path: relative_path.clone(),
                reason: "current formal directory is not cached; rebuild archive map".to_string(),
                cached_file_count: None,
                current_file_count: Some(current.file_count),
                historical_moves: 0,
                last_moved_at: None,
            });
        }
    }

    for (relative_path, cached) in &cached_map {
        match current_map.get(relative_path) {
            Some(current) if directory_changed(cached, current) => {
                changed_directories.push(relative_path.clone());
                reported.insert(relative_path.clone());
                stale_directories.push(ArchiveMapStaleDirectory {
                    relative_path: relative_path.clone(),
                    reason:
                        "directory file, child, or semantic summary details changed since last rebuild"
                            .to_string(),
                    cached_file_count: Some(cached.file_count),
                    current_file_count: Some(current.file_count),
                    historical_moves: cached.historical_moves,
                    last_moved_at: top_hit_directories
                        .iter()
                        .find(|stat| stat.relative_path == *relative_path)
                        .and_then(|stat| stat.last_moved_at.clone()),
                });
            }
            Some(_) => {}
            None => {
                removed_directories.push(relative_path.clone());
                reported.insert(relative_path.clone());
                stale_directories.push(ArchiveMapStaleDirectory {
                    relative_path: relative_path.clone(),
                    reason: "cached directory no longer exists in the formal Vault tree"
                        .to_string(),
                    cached_file_count: Some(cached.file_count),
                    current_file_count: None,
                    historical_moves: cached.historical_moves,
                    last_moved_at: top_hit_directories
                        .iter()
                        .find(|stat| stat.relative_path == *relative_path)
                        .and_then(|stat| stat.last_moved_at.clone()),
                });
            }
        }
    }

    for stat in top_hit_directories
        .iter()
        .filter(|stat| !stat.exists_in_current_map)
    {
        if reported.insert(stat.relative_path.clone()) {
            stale_directories.push(ArchiveMapStaleDirectory {
                relative_path: stat.relative_path.clone(),
                reason: "movement history points to a directory outside the current formal map"
                    .to_string(),
                cached_file_count: cached_map
                    .get(&stat.relative_path)
                    .map(|directory| directory.file_count),
                current_file_count: None,
                historical_moves: stat.move_count,
                last_moved_at: stat.last_moved_at.clone(),
            });
        }
    }

    added_directories.sort();
    removed_directories.sort();
    changed_directories.sort();
    stale_directories.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let markdown_exists = root.join(&run.markdown_path).is_file();
    let mut stale_reasons = Vec::new();
    if !markdown_exists {
        stale_reasons.push("archive map markdown file is missing".to_string());
    }
    if !added_directories.is_empty() {
        stale_reasons.push(format!(
            "{} formal directories were added after the last rebuild",
            added_directories.len()
        ));
    }
    if !removed_directories.is_empty() {
        stale_reasons.push(format!(
            "{} cached directories no longer exist",
            removed_directories.len()
        ));
    }
    if !changed_directories.is_empty() {
        stale_reasons.push(format!(
            "{} cached directories changed file, child, or semantic summary details",
            changed_directories.len()
        ));
    }
    let stale_history_count = top_hit_directories
        .iter()
        .filter(|stat| !stat.exists_in_current_map)
        .count();
    if stale_history_count > 0 {
        stale_reasons.push(format!(
            "{stale_history_count} historical target directories are outside the current map"
        ));
    }

    let is_stale = !stale_reasons.is_empty();
    let status = if run.status != "ok" {
        "failed"
    } else if is_stale {
        "stale"
    } else if current_directories.is_empty() {
        "empty"
    } else {
        "ok"
    }
    .to_string();

    Ok((
        ArchiveMapHealth {
            status,
            is_stale,
            stale_reasons,
            generated_at: generated_at.to_string(),
            age_seconds: age_seconds(generated_at),
            markdown_exists,
            latest_run_status: run.status.clone(),
            cached_directory_count: cached_directories.len(),
            current_directory_count: current_directories.len(),
            stale_directory_count: stale_directories.len(),
            added_directories,
            removed_directories,
            changed_directories,
        },
        stale_directories,
        top_hit_directories,
    ))
}

#[derive(Debug, Default)]
struct HitStatAccumulator {
    move_count: usize,
    last_moved_at: Option<String>,
    last_source_relative_path: Option<String>,
    last_target_relative_path: Option<String>,
}

fn read_movement_hit_stats(
    connection: &Connection,
    current_paths: &BTreeSet<String>,
) -> ServiceResult<Vec<ArchiveMapDirectoryHitStat>> {
    let mut statement = connection.prepare(
        "SELECT source_relative_path, target_relative_path, COALESCE(moved_at, created_at)
         FROM movement_log
         WHERE status = 'moved'
         ORDER BY COALESCE(moved_at, created_at), id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut stats = BTreeMap::<String, HitStatAccumulator>::new();
    for row in rows {
        let (source_relative_path, target_relative_path, moved_at) = row?;
        if let Some(parent) = parent_dir(&target_relative_path) {
            if !is_formal_relative_dir(&parent) {
                continue;
            }
            let stat = stats.entry(parent).or_default();
            stat.move_count += 1;
            let is_newer = stat
                .last_moved_at
                .as_deref()
                .map(|current| moved_at.as_str() >= current)
                .unwrap_or(true);
            if is_newer {
                stat.last_moved_at = Some(moved_at);
                stat.last_source_relative_path = Some(source_relative_path);
                stat.last_target_relative_path = Some(target_relative_path);
            }
        }
    }

    let mut result = stats
        .into_iter()
        .map(|(relative_path, stat)| ArchiveMapDirectoryHitStat {
            exists_in_current_map: current_paths.contains(&relative_path),
            relative_path,
            move_count: stat.move_count,
            last_moved_at: stat.last_moved_at,
            last_source_relative_path: stat.last_source_relative_path,
            last_target_relative_path: stat.last_target_relative_path,
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .move_count
            .cmp(&left.move_count)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(result)
}

fn age_seconds(generated_at: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(generated_at)
        .ok()
        .map(|timestamp| {
            (Utc::now() - timestamp.with_timezone(&Utc))
                .num_seconds()
                .max(0)
        })
}

fn directory_changed(cached: &ArchiveMapDirectory, current: &ArchiveMapDirectory) -> bool {
    cached.file_count != current.file_count
        || cached.child_count != current.child_count
        || cached.keyword_hints != current.keyword_hints
        || cached.heading_hints != current.heading_hints
        || cached.content_hints != current.content_hints
        || cached.semantic_summary != current.semantic_summary
}

fn current_directory_paths(root: &Path) -> ServiceResult<BTreeSet<String>> {
    Ok(scan_formal_directories(root)?
        .into_iter()
        .map(|directory| directory.relative_path)
        .collect())
}

fn read_directory_rules(
    connection: &Connection,
    current_paths: &BTreeSet<String>,
    include_disabled: bool,
) -> ServiceResult<Vec<ArchiveMapDirectoryRule>> {
    let sql = if include_disabled {
        "SELECT id, relative_path, user_note, organizing_hint, locked, status, created_at, updated_at
         FROM archive_map_directory_rules
         ORDER BY relative_path"
    } else {
        "SELECT id, relative_path, user_note, organizing_hint, locked, status, created_at, updated_at
         FROM archive_map_directory_rules
         WHERE status = 'active'
         ORDER BY relative_path"
    };
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        let relative_path: String = row.get(1)?;
        Ok(ArchiveMapDirectoryRule {
            id: row.get(0)?,
            exists_in_current_map: current_paths.contains(&relative_path),
            relative_path,
            user_note: row.get(2)?,
            organizing_hint: row.get(3)?,
            locked: row.get::<_, i64>(4)? != 0,
            status: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn directory_rule_by_path(
    connection: &Connection,
    relative_path: &str,
    current_paths: &BTreeSet<String>,
) -> ServiceResult<Option<ArchiveMapDirectoryRule>> {
    connection
        .query_row(
            "SELECT id, relative_path, user_note, organizing_hint, locked, status, created_at, updated_at
             FROM archive_map_directory_rules
             WHERE relative_path = ?1",
            params![relative_path],
            |row| {
                let stored_path: String = row.get(1)?;
                Ok(ArchiveMapDirectoryRule {
                    id: row.get(0)?,
                    exists_in_current_map: current_paths.contains(&stored_path),
                    relative_path: stored_path,
                    user_note: row.get(2)?,
                    organizing_hint: row.get(3)?,
                    locked: row.get::<_, i64>(4)? != 0,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn attach_directory_rules(
    directories: &mut [ArchiveMapDirectory],
    rules: &[ArchiveMapDirectoryRule],
) {
    let active_rules = rules
        .iter()
        .filter(|rule| rule.status == "active")
        .map(|rule| (rule.relative_path.as_str(), rule.clone()))
        .collect::<BTreeMap<_, _>>();
    for directory in directories {
        directory.rule = active_rules.get(directory.relative_path.as_str()).cloned();
    }
}

fn validate_rule_directory(
    current_paths: &BTreeSet<String>,
    relative_path: &str,
) -> ServiceResult<String> {
    let normalized = normalize_relative_path(relative_path)?;
    if !is_formal_relative_dir(&normalized) {
        return Err(ServiceError::InvalidRelativePath(format!(
            "{normalized} is not a formal archive directory"
        )));
    }
    if !current_paths.contains(&normalized) {
        return Err(ServiceError::InvalidRelativePath(format!(
            "{normalized} is not in the current archive map"
        )));
    }
    Ok(normalized)
}

fn validate_rule_status(status: &str) -> ServiceResult<String> {
    let normalized = status.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "active" | "disabled") {
        Ok(normalized)
    } else {
        Err(ServiceError::InvalidState(format!(
            "unsupported archive map rule status: {status}"
        )))
    }
}

fn clean_rule_text(value: &str, max_chars: usize) -> String {
    let normalized = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string();
    normalized.chars().take(max_chars).collect()
}

fn run_by_id(connection: &Connection, run_id: i64) -> ServiceResult<ArchiveMapRun> {
    connection
        .query_row(
            "SELECT id, status, started_at, finished_at, directory_count, file_count,
                    history_count, markdown_path, error
             FROM archive_map_runs
             WHERE id = ?1",
            params![run_id],
            |row| {
                Ok(ArchiveMapRun {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    started_at: row.get(2)?,
                    finished_at: row.get(3)?,
                    directory_count: row.get(4)?,
                    file_count: row.get(5)?,
                    history_count: row.get(6)?,
                    markdown_path: row.get(7)?,
                    error: row.get(8)?,
                })
            },
        )
        .map_err(Into::into)
}

fn row_to_directory(row: &Row<'_>) -> rusqlite::Result<ArchiveMapDirectory> {
    let sample_files_json: String = row.get(4)?;
    let keyword_hints_json: String = row.get(5)?;
    let heading_hints_json: String = row.get(7)?;
    let content_hints_json: String = row.get(8)?;
    Ok(ArchiveMapDirectory {
        relative_path: row.get(0)?,
        depth: row.get::<_, i64>(1)? as usize,
        file_count: row.get::<_, i64>(2)? as usize,
        child_count: row.get::<_, i64>(3)? as usize,
        sample_files: serde_json::from_str(&sample_files_json).unwrap_or_default(),
        keyword_hints: serde_json::from_str(&keyword_hints_json).unwrap_or_default(),
        historical_moves: row.get::<_, i64>(6)? as usize,
        heading_hints: serde_json::from_str(&heading_hints_json).unwrap_or_default(),
        content_hints: serde_json::from_str(&content_hints_json).unwrap_or_default(),
        semantic_summary: row.get(9)?,
        rule: None,
    })
}

fn scan_formal_directories(root: &Path) -> ServiceResult<Vec<ArchiveMapDirectory>> {
    let mut directories = Vec::new();
    scan_dir(root, root, &mut directories)?;
    directories.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(directories)
}

fn scan_dir(
    root: &Path,
    directory: &Path,
    directories: &mut Vec<ArchiveMapDirectory>,
) -> ServiceResult<()> {
    let mut child_dirs = Vec::new();
    let mut sample_files = Vec::new();
    let mut semantic_file_count = 0usize;
    let mut heading_hints = Vec::new();
    let mut content_hint_counts = BTreeMap::<String, usize>::new();
    let mut file_count = 0usize;

    for entry in sorted_entries(directory)? {
        let metadata = fs::symlink_metadata(&entry)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = entry
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string();
        if metadata.is_dir() {
            if is_blocked_dir_name(&name) || is_temporary_or_hidden_name(&name) {
                continue;
            }
            child_dirs.push(entry);
            continue;
        }
        if metadata.is_file() && !is_temporary_or_hidden_name(&name) {
            file_count += 1;
            if sample_files.len() < 5 {
                sample_files.push(name);
            }
            if semantic_file_count < MAX_SEMANTIC_FILES_PER_DIRECTORY
                && is_semantic_source_file(&entry)
            {
                semantic_file_count += 1;
                collect_file_semantics(&entry, &mut heading_hints, &mut content_hint_counts);
            }
        }
    }

    if directory != root {
        let relative = relative_path(root, directory)?;
        let content_hints = ranked_content_hints(content_hint_counts);
        let semantic_summary = semantic_summary(
            &relative,
            child_dirs.len(),
            &sample_files,
            &heading_hints,
            &content_hints,
        );
        directories.push(ArchiveMapDirectory {
            depth: relative.split('/').filter(|part| !part.is_empty()).count(),
            keyword_hints: keyword_hints(&relative, &sample_files),
            relative_path: relative,
            file_count,
            child_count: child_dirs.len(),
            sample_files,
            heading_hints,
            content_hints,
            semantic_summary,
            historical_moves: 0,
            rule: None,
        });
    }

    for child in child_dirs {
        scan_dir(root, &child, directories)?;
    }
    Ok(())
}

fn read_history_counts(
    vault_path: &str,
    connection: &Connection,
) -> ServiceResult<BTreeMap<String, usize>> {
    let mut counts = BTreeMap::new();

    if let Ok(items) = LedgerService::parse_inbox_ledger(vault_path) {
        for item in items {
            remember_history_target(&mut counts, &item.target_relative_path);
        }
    }

    let mut statement = connection.prepare(
        "SELECT target_relative_path
         FROM movement_log
         WHERE status = 'moved'",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        remember_history_target(&mut counts, &row?);
    }

    Ok(counts)
}

fn remember_history_target(counts: &mut BTreeMap<String, usize>, target_relative_path: &str) {
    if let Some(parent) = parent_dir(target_relative_path) {
        if is_formal_relative_dir(&parent) {
            *counts.entry(parent).or_insert(0) += 1;
        }
    }
}

fn parent_dir(relative_path: &str) -> Option<String> {
    Path::new(relative_path)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty() && path != ".")
        .and_then(|path| normalize_relative_path(&path).ok())
}

fn build_markdown(
    directories: &[ArchiveMapDirectory],
    history_counts: &BTreeMap<String, usize>,
) -> String {
    let generated_at = Utc::now().to_rfc3339();
    let file_count = directories
        .iter()
        .map(|directory| directory.file_count)
        .sum::<usize>();
    let history_count = history_counts.values().sum::<usize>();
    let mut markdown = String::new();
    markdown.push_str("# Archive Map\n\n");
    markdown.push_str("> Generated by TheBrain. This file is an internal, readable map of formal Vault archive targets.\n\n");
    markdown.push_str(&format!("- Generated at: `{generated_at}`\n"));
    markdown.push_str(&format!("- Formal directories: `{}`\n", directories.len()));
    markdown.push_str(&format!("- Direct files sampled: `{file_count}`\n"));
    markdown.push_str(&format!(
        "- Historical archive references: `{history_count}`\n\n"
    ));
    markdown.push_str("## Available Archive Directories\n\n");
    markdown.push_str(
        "| Directory | Files | Children | Summary | Hints | Headings | Examples | History |\n",
    );
    markdown.push_str("| --- | ---: | ---: | --- | --- | --- | --- | ---: |\n");
    for directory in directories {
        markdown.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            escape_cell(&directory.relative_path),
            directory.file_count,
            directory.child_count,
            escape_cell(&directory.semantic_summary),
            list_cell(&directory.keyword_hints),
            list_cell(&directory.heading_hints),
            list_cell(&directory.sample_files),
            directory.historical_moves
        ));
    }
    if directories.is_empty() {
        markdown.push_str("| _No formal archive directories yet_ | 0 | 0 |  |  |  |  | 0 |\n");
    }

    markdown.push_str("\n## Historical Targets\n\n");
    if history_counts.is_empty() {
        markdown.push_str("- No completed archive history yet.\n");
    } else {
        for (target_dir, count) in history_counts {
            markdown.push_str(&format!("- `{}`: {} references\n", target_dir, count));
        }
    }
    markdown
}

fn write_markdown(root: &Path, markdown: &str) -> ServiceResult<()> {
    let path = root.join(ARCHIVE_MAP_RELATIVE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, markdown)?;
    Ok(())
}

fn build_rules_markdown(rules: &[ArchiveMapDirectoryRule]) -> String {
    let active_rules = rules
        .iter()
        .filter(|rule| rule.status == "active")
        .collect::<Vec<_>>();
    if active_rules.is_empty() {
        return String::new();
    }

    let generated_at = Utc::now().to_rfc3339();
    let mut markdown = String::new();
    markdown.push_str("# Archive Map Directory Rules\n\n");
    markdown.push_str("> User-authored directory guidance for TheBrain. These rules are advisory and do not bypass Vault path safety, conflict checks, or overwrite protection.\n\n");
    markdown.push_str(&format!("- Generated at: `{generated_at}`\n"));
    markdown.push_str(&format!("- Active rules: `{}`\n\n", active_rules.len()));
    markdown.push_str(
        "| Directory | Locked | In Current Map | User Note | Organizing Hint | Updated |\n",
    );
    markdown.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for rule in active_rules {
        markdown.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | `{}` |\n",
            escape_cell(&rule.relative_path),
            if rule.locked { "yes" } else { "no" },
            if rule.exists_in_current_map {
                "yes"
            } else {
                "no"
            },
            escape_cell(&single_line_cell(&rule.user_note)),
            escape_cell(&single_line_cell(&rule.organizing_hint)),
            escape_cell(&rule.updated_at)
        ));
    }
    markdown
}

fn write_rules_markdown(root: &Path, rules: &[ArchiveMapDirectoryRule]) -> ServiceResult<()> {
    let path = root.join(ARCHIVE_MAP_RULES_RELATIVE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let markdown = build_rules_markdown(rules);
    if markdown.trim().is_empty() {
        fs::write(
            path,
            "# Archive Map Directory Rules\n\nNo active user directory rules yet.\n",
        )?;
    } else {
        fs::write(path, markdown)?;
    }
    Ok(())
}

fn sorted_entries(path: &Path) -> ServiceResult<Vec<PathBuf>> {
    let mut entries = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    Ok(entries)
}

fn relative_path(root: &Path, path: &Path) -> ServiceResult<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ServiceError::EscapedVault(path.to_string_lossy().to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    normalize_relative_path(&relative)
}

fn is_blocked_dir_name(name: &str) -> bool {
    matches!(
        name,
        INBOX_DIR | INTERNAL_DIR | ".secrets" | ".git" | "node_modules" | "target" | "dist"
    )
}

fn is_formal_relative_dir(relative_path: &str) -> bool {
    !relative_path
        .split('/')
        .any(|segment| is_blocked_dir_name(segment) || segment.starts_with('.'))
}

fn is_temporary_or_hidden_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with('.')
        || name.starts_with("~$")
        || name.ends_with('~')
        || matches!(lower.as_str(), "thumbs.db" | ".ds_store")
        || lower.ends_with(".tmp")
        || lower.ends_with(".temp")
        || lower.ends_with(".part")
        || lower.ends_with(".crdownload")
        || lower.ends_with(".download")
        || lower.ends_with(".swp")
}

fn is_semantic_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "txt"
            )
        })
        .unwrap_or(false)
}

fn collect_file_semantics(
    path: &Path,
    heading_hints: &mut Vec<String>,
    content_hint_counts: &mut BTreeMap<String, usize>,
) {
    let Some(text) = read_text_prefix(path) else {
        return;
    };
    let mut in_frontmatter = false;
    let mut frontmatter_checked = false;
    let mut in_code_fence = false;

    for raw_line in text.lines().take(120) {
        let line = raw_line.trim();
        if !frontmatter_checked {
            frontmatter_checked = true;
            if line == "---" {
                in_frontmatter = true;
                continue;
            }
        } else if in_frontmatter {
            if line == "---" {
                in_frontmatter = false;
            }
            continue;
        }

        if line.starts_with("```") || line.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence || line.is_empty() {
            continue;
        }

        if line.starts_with('#') {
            let heading = clean_heading(line);
            if !heading.is_empty() {
                push_unique_limited(heading_hints, heading, MAX_HEADING_HINTS_PER_DIRECTORY);
            }
            continue;
        }
        collect_weighted_hint_parts(line, content_hint_counts);
    }
}

fn read_text_prefix(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_SEMANTIC_BYTES_PER_FILE)
        .read_to_end(&mut bytes)
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn clean_heading(line: &str) -> String {
    let cleaned = line
        .trim_start_matches('#')
        .trim()
        .trim_matches(|character: char| matches!(character, '#' | '-' | '*' | '`'))
        .trim();
    truncate_chars(cleaned, 80)
}

fn push_unique_limited(values: &mut Vec<String>, value: String, limit: usize) {
    if values.len() >= limit || values.iter().any(|current| current == &value) {
        return;
    }
    values.push(value);
}

fn ranked_content_hints(hint_counts: BTreeMap<String, usize>) -> Vec<String> {
    let mut hints = hint_counts.into_iter().collect::<Vec<_>>();
    hints.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    hints
        .into_iter()
        .map(|(hint, _)| hint)
        .take(MAX_CONTENT_HINTS_PER_DIRECTORY)
        .collect()
}

fn semantic_summary(
    relative_path: &str,
    child_count: usize,
    sample_files: &[String],
    heading_hints: &[String],
    content_hints: &[String],
) -> String {
    let summary = if !heading_hints.is_empty() {
        format!(
            "Headings: {}",
            heading_hints
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else if !content_hints.is_empty() {
        format!(
            "Likely topics: {}",
            content_hints
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else if !sample_files.is_empty() {
        format!(
            "Sample files: {}",
            sample_files
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else if child_count > 0 {
        "Parent archive directory; child folders carry the detailed topics.".to_string()
    } else {
        format!("Archive target for {}.", relative_path)
    };
    truncate_chars(&summary, MAX_SEMANTIC_SUMMARY_CHARS)
}

fn keyword_hints(relative_path: &str, sample_files: &[String]) -> Vec<String> {
    let mut hints = BTreeSet::new();
    for value in relative_path.split('/') {
        collect_hint_parts(value, &mut hints);
    }
    for file in sample_files {
        let stem = Path::new(file)
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or(file);
        collect_hint_parts(stem, &mut hints);
    }
    hints.into_iter().take(8).collect()
}

fn collect_hint_parts(value: &str, hints: &mut BTreeSet<String>) {
    for cleaned in hint_parts(value) {
        hints.insert(cleaned);
    }
}

fn collect_weighted_hint_parts(value: &str, hints: &mut BTreeMap<String, usize>) {
    for cleaned in hint_parts(value) {
        *hints.entry(cleaned).or_insert(0) += 1;
    }
}

fn hint_parts(value: &str) -> Vec<String> {
    value
        .split([
            '-', '_', ' ', '.', '／', '/', '：', ':', '，', ',', '、', ';', '；', '(', ')', '[',
            ']', '（', '）', '《', '》', '"', '\'', '`',
        ])
        .filter_map(|part| {
            let cleaned = part
                .trim_matches(|character: char| {
                    character.is_ascii_digit()
                        || matches!(
                            character,
                            '-' | '_' | ' ' | '.' | '(' | ')' | '[' | ']' | '#' | '*' | '>'
                        )
                })
                .trim();
            let length = cleaned.chars().count();
            if (2..=32).contains(&length) && !is_common_hint_word(cleaned) {
                Some(cleaned.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn is_common_hint_word(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "the"
            | "and"
            | "with"
            | "that"
            | "this"
            | "from"
            | "into"
            | "notes"
            | "note"
            | "todo"
            | "draft"
            | "about"
    )
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let mut truncated = value
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        truncated.push('…');
        truncated
    }
}

fn list_cell(values: &[String]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        values
            .iter()
            .map(|value| escape_cell(value))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn single_line_cell(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::movement::{MoveRequest, MovementService};
    use crate::services::vault::{VaultService, LEDGER_FILE};

    #[test]
    fn archive_map_excludes_inbox_internal_and_tool_directories() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        fs::create_dir_all(temp.path().join("000-收集箱").join("Nested")).unwrap();
        fs::create_dir_all(temp.path().join(".secrets")).unwrap();
        fs::create_dir_all(temp.path().join("dist").join("output")).unwrap();
        fs::write(
            temp.path().join("100-School").join("AI").join("notes.md"),
            "hello",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("000-收集箱")
                .join("Nested")
                .join("draft.md"),
            "draft",
        )
        .unwrap();

        let snapshot = ArchiveMapService::rebuild(temp.path().to_str().unwrap()).unwrap();
        let paths = snapshot
            .directories
            .iter()
            .map(|directory| directory.relative_path.as_str())
            .collect::<Vec<_>>();

        assert!(paths.contains(&"100-School"));
        assert!(paths.contains(&"100-School/AI"));
        assert!(!paths.iter().any(|path| path.starts_with("000-收集箱")));
        assert!(!paths.iter().any(|path| path.starts_with(".secrets")));
        assert!(!paths.iter().any(|path| path.starts_with("dist")));
        assert!(temp.path().join(ARCHIVE_MAP_RELATIVE_PATH).is_file());
        assert!(snapshot.markdown.contains("100-School/AI"));
    }

    #[test]
    fn archive_map_builds_semantic_summary_from_formal_markdown_and_text_only() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        fs::create_dir_all(temp.path().join(INBOX_DIR).join("Nested")).unwrap();
        fs::create_dir_all(temp.path().join(INTERNAL_DIR).join("rules")).unwrap();
        fs::write(
            temp.path().join("100-School").join("AI").join("lecture.md"),
            "---\ntitle: hidden\n---\n# Transformer Attention\n\nGradient descent attention matrix lecture notes.",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("100-School")
                .join("AI")
                .join("summary.txt"),
            "Exam review backpropagation optimizer neural network",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("100-School")
                .join("AI")
                .join("diagram.png"),
            "PNG bytes should not be semantic content",
        )
        .unwrap();
        fs::write(
            temp.path().join(INBOX_DIR).join("Nested").join("bait.md"),
            "# InboxShouldNotAppear\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(INTERNAL_DIR).join("rules").join("bait.md"),
            "# InternalShouldNotAppear\n",
        )
        .unwrap();

        let snapshot = ArchiveMapService::rebuild(vault_path).unwrap();
        let ai_dir = snapshot
            .directories
            .iter()
            .find(|directory| directory.relative_path == "100-School/AI")
            .unwrap();

        assert!(ai_dir
            .heading_hints
            .contains(&"Transformer Attention".to_string()));
        assert!(ai_dir.semantic_summary.contains("Transformer Attention"));
        assert!(ai_dir.content_hints.iter().any(|hint| hint == "Gradient"));
        assert!(!snapshot.markdown.contains("InboxShouldNotAppear"));
        assert!(!snapshot.markdown.contains("InternalShouldNotAppear"));
        assert!(!snapshot
            .markdown
            .contains("PNG bytes should not be semantic content"));
        assert!(snapshot.markdown.contains("Transformer Attention"));

        let context = ArchiveMapService::markdown_context(vault_path).unwrap();
        assert!(context.contains("Transformer Attention"));
    }

    #[test]
    fn archive_map_uses_ledger_history_without_scanning_inbox_as_targets() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        fs::write(
            temp.path().join("100-School").join("AI").join("notes.md"),
            "hello",
        )
        .unwrap();
        fs::write(
            temp.path().join(INBOX_DIR).join(LEDGER_FILE),
            "- 2026-06-05 [[../100-School/AI/notes.md]] - test\n",
        )
        .unwrap();

        let snapshot = ArchiveMapService::rebuild(temp.path().to_str().unwrap()).unwrap();
        let ai_dir = snapshot
            .directories
            .iter()
            .find(|directory| directory.relative_path == "100-School/AI")
            .unwrap();

        assert_eq!(ai_dir.historical_moves, 1);
        assert!(snapshot.markdown.contains("Historical Targets"));
        assert!(snapshot.markdown.contains("100-School/AI"));
    }

    #[test]
    fn archive_map_health_reports_stale_when_formal_directories_change() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();

        let initial = ArchiveMapService::rebuild(temp.path().to_str().unwrap()).unwrap();
        assert_eq!(initial.health.status, "ok");
        assert!(!initial.health.is_stale);

        fs::create_dir_all(temp.path().join("200-Projects")).unwrap();
        fs::remove_dir_all(temp.path().join("100-School").join("AI")).unwrap();

        let latest = ArchiveMapService::latest(temp.path().to_str().unwrap())
            .unwrap()
            .unwrap();

        assert_eq!(latest.health.status, "stale");
        assert!(latest.health.is_stale);
        assert!(latest
            .health
            .added_directories
            .contains(&"200-Projects".to_string()));
        assert!(latest
            .health
            .removed_directories
            .contains(&"100-School/AI".to_string()));
        assert!(latest
            .stale_directories
            .iter()
            .any(|directory| directory.relative_path == "100-School/AI"));
        assert!(latest
            .stale_directories
            .iter()
            .any(|directory| directory.relative_path == "200-Projects"));
    }

    #[test]
    fn archive_map_hit_stats_group_moved_logs_by_target_parent() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        ArchiveMapService::rebuild(vault_path).unwrap();
        fs::write(temp.path().join(INBOX_DIR).join("lecture.md"), "notes").unwrap();

        MovementService::move_from_inbox(
            vault_path,
            MoveRequest {
                source_relative_path: format!("{INBOX_DIR}/lecture.md"),
                target_relative_path: "100-School/AI/lecture.md".to_string(),
                reason: Some("test move".to_string()),
            },
        )
        .unwrap();

        let latest = ArchiveMapService::latest(vault_path).unwrap().unwrap();
        let hit = latest
            .top_hit_directories
            .iter()
            .find(|directory| directory.relative_path == "100-School/AI")
            .unwrap();

        assert_eq!(hit.move_count, 1);
        assert!(hit.exists_in_current_map);
        assert_eq!(
            hit.last_source_relative_path.as_deref(),
            Some("000-收集箱/lecture.md")
        );
        assert_eq!(
            hit.last_target_relative_path.as_deref(),
            Some("100-School/AI/lecture.md")
        );
        assert_eq!(latest.health.status, "stale");
        assert!(latest
            .health
            .changed_directories
            .contains(&"100-School/AI".to_string()));
    }

    #[test]
    fn archive_map_directory_rule_persists_attaches_and_updates_context() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        fs::write(
            temp.path().join("100-School").join("AI").join("notes.md"),
            "hello",
        )
        .unwrap();
        ArchiveMapService::rebuild(vault_path).unwrap();

        let rule = ArchiveMapService::upsert_directory_rule(
            vault_path,
            ArchiveMapDirectoryRuleInput {
                relative_path: "100-School/AI".to_string(),
                user_note: Some("Keep exam notes together".to_string()),
                organizing_hint: Some("Prefer course folders before generic topics".to_string()),
                locked: Some(true),
                status: None,
            },
        )
        .unwrap();

        assert_eq!(rule.relative_path, "100-School/AI");
        assert!(rule.locked);
        assert!(rule.exists_in_current_map);

        let latest = ArchiveMapService::latest(vault_path).unwrap().unwrap();
        let directory = latest
            .directories
            .iter()
            .find(|directory| directory.relative_path == "100-School/AI")
            .unwrap();
        assert_eq!(latest.locked_directory_count, 1);
        assert_eq!(
            directory.rule.as_ref().map(|rule| rule.user_note.as_str()),
            Some("Keep exam notes together")
        );
        assert_eq!(latest.rules_markdown_path, ARCHIVE_MAP_RULES_RELATIVE_PATH);

        let rules_markdown =
            fs::read_to_string(temp.path().join(ARCHIVE_MAP_RULES_RELATIVE_PATH)).unwrap();
        assert!(rules_markdown.contains("Keep exam notes together"));
        assert!(rules_markdown.contains("Prefer course folders before generic topics"));

        let context = ArchiveMapService::markdown_context(vault_path).unwrap();
        assert!(context.contains("Archive Map Directory Rules"));
        assert!(context.contains("Keep exam notes together"));
        assert!(context.contains("Prefer course folders before generic topics"));

        let rebuilt = ArchiveMapService::rebuild(vault_path).unwrap();
        let rebuilt_directory = rebuilt
            .directories
            .iter()
            .find(|directory| directory.relative_path == "100-School/AI")
            .unwrap();
        assert!(rebuilt_directory.rule.as_ref().unwrap().locked);
        assert_eq!(rebuilt.locked_directory_count, 1);
    }

    #[test]
    fn archive_map_directory_rule_rejects_unsafe_or_non_current_paths() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();
        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        fs::create_dir_all(temp.path().join("dist").join("output")).unwrap();
        ArchiveMapService::rebuild(vault_path).unwrap();

        for relative_path in [
            "../escape",
            INBOX_DIR,
            INTERNAL_DIR,
            "dist/output",
            "100-School/Missing",
        ] {
            let result = ArchiveMapService::upsert_directory_rule(
                vault_path,
                ArchiveMapDirectoryRuleInput {
                    relative_path: relative_path.to_string(),
                    user_note: Some("should fail".to_string()),
                    organizing_hint: None,
                    locked: Some(true),
                    status: None,
                },
            );
            assert!(
                result.is_err(),
                "expected archive map rule path to be rejected: {relative_path}"
            );
        }
    }
}
