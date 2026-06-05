use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::path::Path;

use super::vault::INTERNAL_DIR;

pub const INDEX_DB: &str = "index.sqlite";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexOpenResult {
    pub path: String,
}

pub struct IndexService;

impl IndexService {
    pub fn open_or_create(vault_root: &Path) -> ServiceResult<IndexOpenResult> {
        let internal_dir = vault_root.join(INTERNAL_DIR);
        fs::create_dir_all(&internal_dir)?;
        let db_path = internal_dir.join(INDEX_DB);
        let connection = Connection::open(&db_path)?;
        Self::migrate(&connection)?;
        connection.execute(
            "INSERT INTO vault_meta (key, value, updated_at)
            VALUES ('schema_version', '6', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![Utc::now().to_rfc3339()],
        )?;
        Ok(IndexOpenResult {
            path: db_path.to_string_lossy().to_string(),
        })
    }

    pub fn migrate(connection: &Connection) -> ServiceResult<()> {
        connection.execute_batch(
            "
            PRAGMA user_version = 6;

            CREATE TABLE IF NOT EXISTS vault_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS file_index (
                relative_path TEXT PRIMARY KEY,
                content_hash TEXT,
                modified_at INTEGER,
                indexed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ai_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                cost_cents INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS audit_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS listener_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                watch_path TEXT,
                last_event TEXT,
                last_enqueued_count INTEGER NOT NULL DEFAULT 0,
                last_event_at TEXT,
                last_error TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS queue_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                status TEXT NOT NULL,
                dedupe_key TEXT,
                payload_json TEXT NOT NULL DEFAULT '{}',
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                locked_at TEXT,
                run_after TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_queue_items_dedupe_key
                ON queue_items(dedupe_key)
                WHERE dedupe_key IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_queue_items_status_run_after
                ON queue_items(status, run_after);

            CREATE TABLE IF NOT EXISTS dedupe_records (
                dedupe_key TEXT PRIMARY KEY,
                content_hash TEXT,
                relative_path TEXT,
                first_seen_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS budget_settings (
                scope TEXT PRIMARY KEY,
                monthly_limit_cents INTEGER,
                daily_limit_cents INTEGER,
                paused INTEGER NOT NULL DEFAULT 0,
                retry_limit INTEGER NOT NULL DEFAULT 3,
                cooldown_seconds INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS budget_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scope TEXT NOT NULL DEFAULT 'global',
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                cost_cents INTEGER NOT NULL DEFAULT 0,
                reason TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_budget_ledger_scope_created
                ON budget_ledger(scope, created_at);

            CREATE TABLE IF NOT EXISTS movement_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                operation TEXT NOT NULL,
                source_relative_path TEXT NOT NULL,
                target_relative_path TEXT NOT NULL,
                reason TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                moved_at TEXT,
                rolled_back_at TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_movement_log_status
                ON movement_log(status);

            CREATE TABLE IF NOT EXISTS conflict_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rule_key TEXT NOT NULL UNIQUE,
                source_pattern TEXT NOT NULL,
                target_pattern TEXT NOT NULL,
                answer TEXT NOT NULL,
                action TEXT NOT NULL,
                auto_apply INTEGER NOT NULL DEFAULT 0,
                match_summary TEXT NOT NULL,
                markdown_path TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_conflict_rules_status_hits
                ON conflict_rules(status, hit_count);

            CREATE TABLE IF NOT EXISTS conflict_rule_hits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rule_id INTEGER NOT NULL,
                conflict_id INTEGER,
                source_relative_path TEXT,
                target_relative_path TEXT,
                score REAL NOT NULL DEFAULT 0,
                reason TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                applied_at TEXT,
                FOREIGN KEY(rule_id) REFERENCES conflict_rules(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_conflict_rule_hits_rule_status
                ON conflict_rule_hits(rule_id, status, created_at);

            CREATE TABLE IF NOT EXISTS archive_map_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                directory_count INTEGER NOT NULL DEFAULT 0,
                file_count INTEGER NOT NULL DEFAULT 0,
                history_count INTEGER NOT NULL DEFAULT 0,
                markdown_path TEXT NOT NULL,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_archive_map_runs_status_started
                ON archive_map_runs(status, started_at);

            CREATE TABLE IF NOT EXISTS archive_map_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id INTEGER NOT NULL,
                relative_path TEXT NOT NULL,
                depth INTEGER NOT NULL DEFAULT 0,
                file_count INTEGER NOT NULL DEFAULT 0,
                child_count INTEGER NOT NULL DEFAULT 0,
                sample_files_json TEXT NOT NULL DEFAULT '[]',
                keyword_hints_json TEXT NOT NULL DEFAULT '[]',
                historical_moves INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                FOREIGN KEY(run_id) REFERENCES archive_map_runs(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_archive_map_entries_run_path
                ON archive_map_entries(run_id, relative_path);

            CREATE TABLE IF NOT EXISTS action_candidates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                candidate_type TEXT NOT NULL,
                source_relative_path TEXT,
                title TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                confirmed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_action_candidates_status
                ON action_candidates(status);

            CREATE TABLE IF NOT EXISTS sticky_notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                color TEXT NOT NULL,
                x INTEGER NOT NULL DEFAULT 0,
                y INTEGER NOT NULL DEFAULT 0,
                width INTEGER NOT NULL DEFAULT 280,
                height INTEGER NOT NULL DEFAULT 180,
                pinned INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sticky_notes_archived_updated
                ON sticky_notes(archived, updated_at);

            CREATE TABLE IF NOT EXISTS rag_documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                relative_path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                modified_at INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                indexed_at TEXT NOT NULL,
                deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_rag_documents_status
                ON rag_documents(status);

            CREATE TABLE IF NOT EXISTS rag_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id INTEGER NOT NULL,
                relative_path TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                heading_path TEXT NOT NULL DEFAULT '[]',
                content TEXT NOT NULL,
                snippet TEXT NOT NULL,
                start_line INTEGER NOT NULL DEFAULT 1,
                end_line INTEGER NOT NULL DEFAULT 1,
                char_start INTEGER NOT NULL DEFAULT 0,
                char_end INTEGER NOT NULL DEFAULT 0,
                char_count INTEGER NOT NULL DEFAULT 0,
                token_estimate INTEGER NOT NULL DEFAULT 0,
                content_hash TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(document_id) REFERENCES rag_documents(id) ON DELETE CASCADE
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_rag_chunks_document_index
                ON rag_chunks(document_id, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_rag_chunks_relative_status
                ON rag_chunks(relative_path, status);

            CREATE TABLE IF NOT EXISTS rag_index_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                scanned_count INTEGER NOT NULL DEFAULT 0,
                indexed_count INTEGER NOT NULL DEFAULT 0,
                skipped_count INTEGER NOT NULL DEFAULT 0,
                deleted_count INTEGER NOT NULL DEFAULT 0,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                error TEXT
            );

            CREATE TABLE IF NOT EXISTS rag_queries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                question TEXT NOT NULL,
                rewritten_question TEXT,
                answer TEXT,
                status TEXT NOT NULL,
                fallback_reason TEXT,
                top_k INTEGER NOT NULL DEFAULT 6,
                created_at TEXT NOT NULL,
                trace_run_id INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_rag_queries_created
                ON rag_queries(created_at);

            CREATE TABLE IF NOT EXISTS rag_trace_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query_id INTEGER,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                duration_ms INTEGER,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_rag_trace_runs_started
                ON rag_trace_runs(started_at);

            CREATE TABLE IF NOT EXISTS rag_trace_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id INTEGER NOT NULL,
                parent_id INTEGER,
                node_type TEXT NOT NULL,
                name TEXT NOT NULL,
                input_json TEXT NOT NULL DEFAULT '{}',
                output_json TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                duration_ms INTEGER,
                error TEXT,
                FOREIGN KEY(run_id) REFERENCES rag_trace_runs(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_rag_trace_nodes_run_id
                ON rag_trace_nodes(run_id, id);

            CREATE TABLE IF NOT EXISTS rag_conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS rag_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id INTEGER,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                query_id INTEGER,
                created_at TEXT NOT NULL,
                FOREIGN KEY(conversation_id) REFERENCES rag_conversations(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_rag_messages_conversation
                ON rag_messages(conversation_id, created_at);
            ",
        )?;
        ensure_column(connection, "listener_state", "watch_path", "TEXT")?;
        ensure_column(connection, "listener_state", "last_event", "TEXT")?;
        ensure_column(
            connection,
            "listener_state",
            "last_enqueued_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        Ok(())
    }
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> ServiceResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }
    connection.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

pub fn open_index_for_vault(vault_path: &str) -> ServiceResult<Connection> {
    let root = super::vault::canonical_vault_root(vault_path)?;
    let db_path = root.join(INTERNAL_DIR).join(INDEX_DB);
    if !db_path.exists() {
        return Err(ServiceError::InvalidVault(
            "vault index has not been initialized".to_string(),
        ));
    }
    Ok(Connection::open(db_path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_initial_schema() {
        let temp = tempfile::tempdir().unwrap();
        let result = IndexService::open_or_create(temp.path()).unwrap();
        assert!(Path::new(&result.path).exists());

        let connection = Connection::open(result.path).unwrap();
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                    'vault_meta',
                    'file_index',
                    'ai_usage',
                    'audit_events',
                    'listener_state',
                    'queue_items',
                    'dedupe_records',
                    'budget_settings',
                    'budget_ledger',
                    'movement_log',
                    'conflict_rules',
                    'conflict_rule_hits',
                    'archive_map_runs',
                    'archive_map_entries',
                    'action_candidates',
                    'sticky_notes',
                    'rag_documents',
                    'rag_chunks',
                    'rag_index_runs',
                    'rag_queries',
                    'rag_trace_runs',
                    'rag_trace_nodes',
                    'rag_conversations',
                    'rag_messages'
                )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 24);
    }
}
