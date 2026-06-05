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
use std::path::{Path, PathBuf};

pub const ARCHIVE_MAP_RELATIVE_PATH: &str = ".thebrain/rules/archive-map.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapDirectory {
    pub relative_path: String,
    pub depth: usize,
    pub file_count: usize,
    pub child_count: usize,
    pub sample_files: Vec<String>,
    pub keyword_hints: Vec<String>,
    pub historical_moves: usize,
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
    pub markdown: String,
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
        Ok(snapshot.markdown)
    }

    pub fn contains_directory(snapshot: &ArchiveMapSnapshot, relative_dir: &str) -> bool {
        snapshot
            .directories
            .iter()
            .any(|entry| entry.relative_path == relative_dir)
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
                 sample_files_json, keyword_hints_json, historical_moves, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active')",
            params![
                run_id,
                directory.relative_path,
                directory.depth as i64,
                directory.file_count as i64,
                directory.child_count as i64,
                serde_json::to_string(&directory.sample_files)?,
                serde_json::to_string(&directory.keyword_hints)?,
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
                keyword_hints_json, historical_moves
         FROM archive_map_entries
         WHERE run_id = ?1 AND status = 'active'
         ORDER BY relative_path",
    )?;
    let rows = statement.query_map(params![run_id], row_to_directory)?;
    let directories = rows.collect::<Result<Vec<_>, _>>()?;
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
            Some(current)
                if cached.file_count != current.file_count
                    || cached.child_count != current.child_count =>
            {
                changed_directories.push(relative_path.clone());
                reported.insert(relative_path.clone());
                stale_directories.push(ArchiveMapStaleDirectory {
                    relative_path: relative_path.clone(),
                    reason: "directory file or child count changed since last rebuild".to_string(),
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
            "{} cached directories changed file or child counts",
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
    Ok(ArchiveMapDirectory {
        relative_path: row.get(0)?,
        depth: row.get::<_, i64>(1)? as usize,
        file_count: row.get::<_, i64>(2)? as usize,
        child_count: row.get::<_, i64>(3)? as usize,
        sample_files: serde_json::from_str(&sample_files_json).unwrap_or_default(),
        keyword_hints: serde_json::from_str(&keyword_hints_json).unwrap_or_default(),
        historical_moves: row.get::<_, i64>(6)? as usize,
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
        }
    }

    if directory != root {
        let relative = relative_path(root, directory)?;
        directories.push(ArchiveMapDirectory {
            depth: relative.split('/').filter(|part| !part.is_empty()).count(),
            keyword_hints: keyword_hints(&relative, &sample_files),
            relative_path: relative,
            file_count,
            child_count: child_dirs.len(),
            sample_files,
            historical_moves: 0,
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
    markdown.push_str("| Directory | Files | Children | Hints | Examples | History |\n");
    markdown.push_str("| --- | ---: | ---: | --- | --- | ---: |\n");
    for directory in directories {
        markdown.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            escape_cell(&directory.relative_path),
            directory.file_count,
            directory.child_count,
            list_cell(&directory.keyword_hints),
            list_cell(&directory.sample_files),
            directory.historical_moves
        ));
    }
    if directories.is_empty() {
        markdown.push_str("| _No formal archive directories yet_ | 0 | 0 |  |  | 0 |\n");
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
    for part in value.split(['-', '_', ' ', '.', '／', '/']) {
        let cleaned = part
            .trim_matches(|character: char| {
                character.is_ascii_digit()
                    || matches!(character, '-' | '_' | ' ' | '.' | '(' | ')' | '[' | ']')
            })
            .trim();
        if cleaned.chars().count() >= 2 {
            hints.insert(cleaned.to_string());
        }
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
}
