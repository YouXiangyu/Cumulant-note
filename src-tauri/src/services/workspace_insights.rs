use crate::services::index::IndexService;
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, INBOX_DIR, INTERNAL_DIR, LEDGER_FILE,
};
use crate::services::{ServiceError, ServiceResult};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInsights {
    pub vault: VaultInsights,
    pub projects: Vec<ProjectInsights>,
    pub recent_files: Vec<RecentFileSummary>,
    pub rag: RagInsights,
    pub candidates: CandidateInsights,
    pub movement: MovementInsights,
    pub audit: AuditInsights,
    pub sticky: StickyInsights,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInsights {
    pub file_count: usize,
    pub directory_count: usize,
    pub markdown_count: usize,
    pub total_bytes: u64,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInsights {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub file_count: usize,
    pub markdown_count: usize,
    pub total_bytes: u64,
    pub updated_at: Option<String>,
    pub recent_files: Vec<RecentFileSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFileSummary {
    pub name: String,
    pub relative_path: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagInsights {
    pub document_count: i64,
    pub chunk_count: i64,
    pub conversation_count: i64,
    pub message_count: i64,
    pub query_count: i64,
    pub last_run_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateInsights {
    pub pending_todo: i64,
    pub pending_schedule: i64,
    pub confirmed: i64,
    pub rejected: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementInsights {
    pub completed: i64,
    pub rolled_back: i64,
    pub conflict: i64,
    pub failed: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditInsights {
    pub total: i64,
    pub recent_count: i64,
    pub latest_event_type: Option<String>,
    pub latest_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickyInsights {
    pub active: i64,
    pub archived: i64,
    pub pinned: i64,
    pub total: i64,
}

#[derive(Debug, Clone)]
struct FileRecord {
    name: String,
    relative_path: String,
    extension: Option<String>,
    size_bytes: u64,
    modified_seconds: Option<i64>,
    modified_at: Option<String>,
}

#[derive(Debug, Default)]
struct ScanAggregate {
    file_count: usize,
    directory_count: usize,
    markdown_count: usize,
    total_bytes: u64,
    latest_modified_seconds: Option<i64>,
    latest_modified_at: Option<String>,
    files: Vec<FileRecord>,
}

pub struct WorkspaceInsightsService;

impl WorkspaceInsightsService {
    pub fn get(vault_path: &str) -> ServiceResult<WorkspaceInsights> {
        let root = canonical_vault_root(vault_path)?;
        let opened = IndexService::open_or_create(&root)?;
        let connection = Connection::open(opened.path)?;

        let vault_scan = scan_vault(&root)?;
        let projects = scan_projects(&root)?;
        Ok(WorkspaceInsights {
            vault: VaultInsights {
                file_count: vault_scan.file_count,
                directory_count: vault_scan.directory_count,
                markdown_count: vault_scan.markdown_count,
                total_bytes: vault_scan.total_bytes,
                updated_at: vault_scan.latest_modified_at,
            },
            projects,
            recent_files: recent_files(vault_scan.files, 8),
            rag: load_rag_insights(&connection)?,
            candidates: load_candidate_insights(&connection)?,
            movement: load_movement_insights(&connection)?,
            audit: load_audit_insights(&connection)?,
            sticky: load_sticky_insights(&connection)?,
        })
    }
}

fn scan_vault(root: &Path) -> ServiceResult<ScanAggregate> {
    let mut aggregate = ScanAggregate::default();
    scan_directory(root, root, &mut aggregate, true)?;
    Ok(aggregate)
}

fn scan_projects(root: &Path) -> ServiceResult<Vec<ProjectInsights>> {
    let mut projects = Vec::new();
    for entry in sorted_entries(root)? {
        let metadata = fs::symlink_metadata(&entry)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let name = file_name(&entry);
        if !is_formal_project_dir_name(&name) {
            continue;
        }
        let relative_path = relative_path(root, &entry)?;
        let mut aggregate = ScanAggregate::default();
        scan_directory(root, &entry, &mut aggregate, false)?;
        projects.push(ProjectInsights {
            id: relative_path.clone(),
            name,
            relative_path,
            file_count: aggregate.file_count,
            markdown_count: aggregate.markdown_count,
            total_bytes: aggregate.total_bytes,
            updated_at: aggregate.latest_modified_at,
            recent_files: recent_files(aggregate.files, 4),
        });
    }
    projects.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(projects)
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    aggregate: &mut ScanAggregate,
    count_directories: bool,
) -> ServiceResult<()> {
    for entry in sorted_entries(directory)? {
        let metadata = fs::symlink_metadata(&entry)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = file_name(&entry);
        if metadata.is_dir() {
            if should_skip_scan_dir(&name) {
                continue;
            }
            if count_directories {
                aggregate.directory_count += 1;
            }
            scan_directory(root, &entry, aggregate, count_directories)?;
            continue;
        }
        if !metadata.is_file() || is_temporary_or_hidden_name(&name) {
            continue;
        }
        let relative_path = relative_path(root, &entry)?;
        if is_inbox_ledger(&relative_path) {
            continue;
        }
        record_file(aggregate, name, relative_path, &metadata);
    }
    Ok(())
}

fn record_file(
    aggregate: &mut ScanAggregate,
    name: String,
    relative_path: String,
    metadata: &fs::Metadata,
) {
    let extension = file_extension(&name);
    let (modified_seconds, modified_at) = modified_parts(metadata);
    let size_bytes = metadata.len();
    aggregate.file_count += 1;
    aggregate.total_bytes += size_bytes;
    if is_markdown_extension(extension.as_deref()) {
        aggregate.markdown_count += 1;
    }
    if let Some(seconds) = modified_seconds {
        let is_newer = aggregate
            .latest_modified_seconds
            .map(|current| seconds > current)
            .unwrap_or(true);
        if is_newer {
            aggregate.latest_modified_seconds = Some(seconds);
            aggregate.latest_modified_at = modified_at.clone();
        }
    }
    aggregate.files.push(FileRecord {
        name,
        relative_path,
        extension,
        size_bytes,
        modified_seconds,
        modified_at,
    });
}

fn recent_files(mut files: Vec<FileRecord>, limit: usize) -> Vec<RecentFileSummary> {
    files.sort_by(|left, right| {
        right
            .modified_seconds
            .cmp(&left.modified_seconds)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    files
        .into_iter()
        .take(limit)
        .map(|file| RecentFileSummary {
            name: file.name,
            relative_path: file.relative_path,
            extension: file.extension,
            size_bytes: file.size_bytes,
            modified_at: file.modified_at,
        })
        .collect()
}

fn load_rag_insights(connection: &Connection) -> ServiceResult<RagInsights> {
    Ok(RagInsights {
        document_count: count_i64(
            connection,
            "SELECT COUNT(*) FROM rag_documents WHERE status = 'active'",
        )?,
        chunk_count: count_i64(
            connection,
            "SELECT COUNT(*) FROM rag_chunks WHERE status = 'active'",
        )?,
        conversation_count: count_i64(connection, "SELECT COUNT(*) FROM rag_conversations")?,
        message_count: count_i64(connection, "SELECT COUNT(*) FROM rag_messages")?,
        query_count: count_i64(connection, "SELECT COUNT(*) FROM rag_queries")?,
        last_run_status: connection
            .query_row(
                "SELECT status FROM rag_index_runs ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?,
    })
}

fn load_candidate_insights(connection: &Connection) -> ServiceResult<CandidateInsights> {
    Ok(CandidateInsights {
        pending_todo: count_i64(
            connection,
            "SELECT COUNT(*) FROM action_candidates
             WHERE status = 'pending' AND candidate_type = 'todo'",
        )?,
        pending_schedule: count_i64(
            connection,
            "SELECT COUNT(*) FROM action_candidates
             WHERE status = 'pending' AND candidate_type = 'schedule'",
        )?,
        confirmed: count_i64(
            connection,
            "SELECT COUNT(*) FROM action_candidates WHERE status = 'confirmed'",
        )?,
        rejected: count_i64(
            connection,
            "SELECT COUNT(*) FROM action_candidates WHERE status = 'rejected'",
        )?,
        total: count_i64(connection, "SELECT COUNT(*) FROM action_candidates")?,
    })
}

fn load_movement_insights(connection: &Connection) -> ServiceResult<MovementInsights> {
    Ok(MovementInsights {
        completed: count_i64(
            connection,
            "SELECT COUNT(*) FROM movement_log WHERE status IN ('moved', 'completed')",
        )?,
        rolled_back: count_i64(
            connection,
            "SELECT COUNT(*) FROM movement_log WHERE status = 'rolled_back'",
        )?,
        conflict: count_i64(
            connection,
            "SELECT COUNT(*) FROM movement_log WHERE status = 'conflict'",
        )?,
        failed: count_i64(
            connection,
            "SELECT COUNT(*) FROM movement_log WHERE status = 'failed'",
        )?,
        total: count_i64(connection, "SELECT COUNT(*) FROM movement_log")?,
    })
}

fn load_audit_insights(connection: &Connection) -> ServiceResult<AuditInsights> {
    let recent_since = (Utc::now() - Duration::days(7)).to_rfc3339();
    let recent_count = connection.query_row(
        "SELECT COUNT(*) FROM audit_events WHERE created_at >= ?1",
        params![recent_since],
        |row| row.get(0),
    )?;
    let latest = connection
        .query_row(
            "SELECT event_type, created_at FROM audit_events ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(AuditInsights {
        total: count_i64(connection, "SELECT COUNT(*) FROM audit_events")?,
        recent_count,
        latest_event_type: latest.as_ref().map(|(event_type, _)| event_type.clone()),
        latest_at: latest.map(|(_, created_at)| created_at),
    })
}

fn load_sticky_insights(connection: &Connection) -> ServiceResult<StickyInsights> {
    Ok(StickyInsights {
        active: count_i64(
            connection,
            "SELECT COUNT(*) FROM sticky_notes WHERE archived = 0",
        )?,
        archived: count_i64(
            connection,
            "SELECT COUNT(*) FROM sticky_notes WHERE archived != 0",
        )?,
        pinned: count_i64(
            connection,
            "SELECT COUNT(*) FROM sticky_notes WHERE pinned != 0",
        )?,
        total: count_i64(connection, "SELECT COUNT(*) FROM sticky_notes")?,
    })
}

fn count_i64(connection: &Connection, sql: &str) -> ServiceResult<i64> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(Into::into)
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

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_string()
}

fn file_extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
}

fn modified_parts(metadata: &fs::Metadata) -> (Option<i64>, Option<String>) {
    metadata
        .modified()
        .ok()
        .map(|modified| {
            let date_time: DateTime<Utc> = modified.into();
            (Some(date_time.timestamp()), Some(date_time.to_rfc3339()))
        })
        .unwrap_or((None, None))
}

fn should_skip_scan_dir(name: &str) -> bool {
    is_internal_or_tool_dir_name(name) || is_temporary_or_hidden_name(name)
}

fn is_formal_project_dir_name(name: &str) -> bool {
    name != INBOX_DIR && !should_skip_scan_dir(name)
}

fn is_internal_or_tool_dir_name(name: &str) -> bool {
    matches!(
        name,
        INTERNAL_DIR | ".secrets" | ".git" | "node_modules" | "target" | "dist" | "tool" | "tools"
    )
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

fn is_inbox_ledger(relative_path: &str) -> bool {
    relative_path == format!("{INBOX_DIR}/{LEDGER_FILE}")
}

fn is_markdown_extension(extension: Option<&str>) -> bool {
    matches!(extension, Some("md" | "markdown"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;
    use rusqlite::params;

    #[test]
    fn scans_vault_and_excludes_internal_temp_and_non_project_paths() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();

        fs::create_dir_all(temp.path().join("100-School").join("AI")).unwrap();
        fs::create_dir_all(temp.path().join("200-Life")).unwrap();
        fs::create_dir_all(temp.path().join(INBOX_DIR)).unwrap();
        fs::create_dir_all(temp.path().join(".secrets")).unwrap();
        fs::create_dir_all(temp.path().join("node_modules").join("pkg")).unwrap();
        fs::create_dir_all(temp.path().join("target")).unwrap();
        fs::create_dir_all(temp.path().join("dist")).unwrap();
        fs::create_dir_all(temp.path().join("tools")).unwrap();
        fs::create_dir_all(temp.path().join(".hidden")).unwrap();

        write_file(temp.path().join("100-School/AI/notes.md"), "alpha");
        write_file(temp.path().join("100-School/AI/brief.txt"), "brief");
        write_file(temp.path().join("100-School/AI/ref1.txt"), "one");
        write_file(temp.path().join("100-School/AI/ref2.txt"), "two");
        write_file(temp.path().join("100-School/AI/ref3.txt"), "tri");
        write_file(temp.path().join("100-School/AI/ref4.txt"), "quad");
        write_file(temp.path().join("100-School/AI/~$draft.md"), "skip");
        write_file(temp.path().join("200-Life/log.markdown"), "life");
        write_file(temp.path().join(INBOX_DIR).join("draft.md"), "draft");
        write_file(temp.path().join(INBOX_DIR).join(LEDGER_FILE), "ledger");
        write_file(temp.path().join(".secrets/secret.md"), "secret");
        write_file(temp.path().join("node_modules/pkg/package.md"), "package");
        write_file(temp.path().join("target/out.md"), "target");
        write_file(temp.path().join("dist/bundle.md"), "dist");
        write_file(temp.path().join("tools/tool.md"), "tool");
        write_file(temp.path().join(".hidden/hidden.md"), "hidden");

        let insights = WorkspaceInsightsService::get(vault_path).unwrap();
        assert_eq!(insights.vault.file_count, 8);
        assert_eq!(insights.vault.directory_count, 4);
        assert_eq!(insights.vault.markdown_count, 3);
        assert_eq!(insights.vault.total_bytes, 32);
        assert!(insights.vault.updated_at.is_some());
        assert_eq!(insights.recent_files.len(), 8);
        assert!(insights
            .recent_files
            .iter()
            .all(|file| !is_excluded_recent_path(&file.relative_path)));

        let project_paths = insights
            .projects
            .iter()
            .map(|project| project.relative_path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(project_paths, vec!["100-School", "200-Life"]);

        let school = insights
            .projects
            .iter()
            .find(|project| project.relative_path == "100-School")
            .unwrap();
        assert_eq!(school.id, "100-School");
        assert_eq!(school.name, "100-School");
        assert_eq!(school.file_count, 6);
        assert_eq!(school.markdown_count, 1);
        assert_eq!(school.total_bytes, 23);
        assert_eq!(school.recent_files.len(), 4);
        assert!(school
            .recent_files
            .iter()
            .all(|file| file.relative_path.starts_with("100-School/")));
    }

    #[test]
    fn aggregates_sql_insights_without_panicking_on_empty_tables() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        let init = VaultService::init(vault_path).unwrap();
        let empty = WorkspaceInsightsService::get(vault_path).unwrap();
        assert_eq!(empty.rag.document_count, 0);
        assert_eq!(empty.rag.chunk_count, 0);
        assert_eq!(empty.rag.conversation_count, 0);
        assert_eq!(empty.rag.message_count, 0);
        assert_eq!(empty.rag.query_count, 0);
        assert_eq!(empty.rag.last_run_status, None);
        assert_eq!(empty.candidates.total, 0);
        assert_eq!(empty.movement.total, 0);
        assert_eq!(empty.audit.total, 0);
        assert_eq!(empty.audit.recent_count, 0);
        assert_eq!(empty.sticky.total, 0);

        let connection = Connection::open(init.index_path).unwrap();
        let now = Utc::now().to_rfc3339();
        let old = (Utc::now() - Duration::days(30)).to_rfc3339();

        connection
            .execute(
                "INSERT INTO rag_documents
                    (relative_path, title, content_hash, status, indexed_at)
                 VALUES ('100-School/a.md', 'a', 'hash-a', 'active', ?1)",
                params![now],
            )
            .unwrap();
        let active_document_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO rag_documents
                    (relative_path, title, content_hash, status, indexed_at)
                 VALUES ('100-School/deleted.md', 'deleted', 'hash-d', 'deleted', ?1)",
                params![now],
            )
            .unwrap();
        for index in 0..2 {
            connection
                .execute(
                    "INSERT INTO rag_chunks
                        (document_id, relative_path, chunk_index, content, snippet,
                         content_hash, status, created_at, updated_at)
                     VALUES (?1, '100-School/a.md', ?2, 'chunk', 'chunk', ?3, 'active', ?4, ?4)",
                    params![active_document_id, index, format!("chunk-{index}"), now],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO rag_index_runs (status, started_at, finished_at)
                 VALUES ('ok', ?1, ?1)",
                params![now],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO rag_conversations (title, created_at, updated_at)
                 VALUES ('one', ?1, ?1), ('two', ?1, ?1)",
                params![now],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO rag_messages (conversation_id, role, content, created_at)
                 VALUES (1, 'user', 'hello', ?1), (1, 'assistant', 'hi', ?1)",
                params![now],
            )
            .unwrap();
        for index in 0..3 {
            connection
                .execute(
                    "INSERT INTO rag_queries (question, status, created_at)
                     VALUES (?1, 'ok', ?2)",
                    params![format!("q{index}"), now],
                )
                .unwrap();
        }

        seed_candidate(&connection, "todo", "pending", &now);
        seed_candidate(&connection, "schedule", "pending", &now);
        seed_candidate(&connection, "todo", "confirmed", &now);
        seed_candidate(&connection, "schedule", "rejected", &now);
        seed_movement(&connection, "moved", &now);
        seed_movement(&connection, "rolled_back", &now);
        seed_movement(&connection, "conflict", &now);
        seed_movement(&connection, "failed", &now);
        seed_audit(&connection, "old_event", &old);
        seed_audit(&connection, "recent_one", &now);
        seed_audit(&connection, "recent_two", &now);
        seed_sticky(&connection, false, true, &now);
        seed_sticky(&connection, false, false, &now);
        seed_sticky(&connection, true, true, &now);

        let insights = WorkspaceInsightsService::get(vault_path).unwrap();
        assert_eq!(insights.rag.document_count, 1);
        assert_eq!(insights.rag.chunk_count, 2);
        assert_eq!(insights.rag.conversation_count, 2);
        assert_eq!(insights.rag.message_count, 2);
        assert_eq!(insights.rag.query_count, 3);
        assert_eq!(insights.rag.last_run_status.as_deref(), Some("ok"));
        assert_eq!(insights.candidates.pending_todo, 1);
        assert_eq!(insights.candidates.pending_schedule, 1);
        assert_eq!(insights.candidates.confirmed, 1);
        assert_eq!(insights.candidates.rejected, 1);
        assert_eq!(insights.candidates.total, 4);
        assert_eq!(insights.movement.completed, 1);
        assert_eq!(insights.movement.rolled_back, 1);
        assert_eq!(insights.movement.conflict, 1);
        assert_eq!(insights.movement.failed, 1);
        assert_eq!(insights.movement.total, 4);
        assert_eq!(insights.audit.total, 3);
        assert_eq!(insights.audit.recent_count, 2);
        assert_eq!(
            insights.audit.latest_event_type.as_deref(),
            Some("recent_two")
        );
        assert!(insights.audit.latest_at.is_some());
        assert_eq!(insights.sticky.active, 2);
        assert_eq!(insights.sticky.archived, 1);
        assert_eq!(insights.sticky.pinned, 2);
        assert_eq!(insights.sticky.total, 3);
    }

    fn write_file(path: impl AsRef<Path>, content: &str) {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn is_excluded_recent_path(relative_path: &str) -> bool {
        relative_path == format!("{INBOX_DIR}/{LEDGER_FILE}")
            || relative_path.starts_with(".thebrain/")
            || relative_path.starts_with(".secrets/")
            || relative_path.starts_with("node_modules/")
            || relative_path.starts_with("target/")
            || relative_path.starts_with("dist/")
            || relative_path.starts_with("tools/")
            || relative_path.contains("/~$")
    }

    fn seed_candidate(connection: &Connection, candidate_type: &str, status: &str, now: &str) {
        connection
            .execute(
                "INSERT INTO action_candidates
                    (candidate_type, title, payload_json, status, created_at, updated_at)
                 VALUES (?1, ?2, '{}', ?3, ?4, ?4)",
                params![
                    candidate_type,
                    format!("{candidate_type}-{status}"),
                    status,
                    now
                ],
            )
            .unwrap();
    }

    fn seed_movement(connection: &Connection, status: &str, now: &str) {
        connection
            .execute(
                "INSERT INTO movement_log
                    (operation, source_relative_path, target_relative_path, status, created_at)
                 VALUES ('move', '000-收集箱/a.md', '100-School/a.md', ?1, ?2)",
                params![status, now],
            )
            .unwrap();
    }

    fn seed_audit(connection: &Connection, event_type: &str, created_at: &str) {
        connection
            .execute(
                "INSERT INTO audit_events (event_type, payload_json, created_at)
                 VALUES (?1, '{}', ?2)",
                params![event_type, created_at],
            )
            .unwrap();
    }

    fn seed_sticky(connection: &Connection, archived: bool, pinned: bool, now: &str) {
        connection
            .execute(
                "INSERT INTO sticky_notes
                    (title, body, color, pinned, archived, created_at, updated_at)
                 VALUES ('note', 'body', '#fff59d', ?1, ?2, ?3, ?3)",
                params![
                    if pinned { 1 } else { 0 },
                    if archived { 1 } else { 0 },
                    now
                ],
            )
            .unwrap();
    }
}
