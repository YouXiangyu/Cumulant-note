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
             VALUES ('schema_version', '2', ?1)
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
            PRAGMA user_version = 2;

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
            ",
        )?;
        Ok(())
    }
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
                    'action_candidates',
                    'sticky_notes'
                )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 12);
    }
}
