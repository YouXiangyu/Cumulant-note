use crate::services::index::open_index_for_vault;
use crate::services::vault::normalize_relative_path;
use crate::services::{ServiceError, ServiceResult};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStateUpdate {
    pub enabled: bool,
    pub status: String,
    pub watch_path: Option<String>,
    pub last_event: Option<String>,
    pub last_enqueued_count: Option<i64>,
    pub last_event_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerState {
    pub enabled: bool,
    pub status: String,
    pub watch_path: Option<String>,
    pub last_event: Option<String>,
    pub last_enqueued_count: i64,
    pub last_event_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItemInput {
    pub kind: String,
    pub relative_path: String,
    pub dedupe_key: Option<String>,
    pub payload: Option<Value>,
    pub max_attempts: Option<i64>,
    pub run_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: i64,
    pub kind: String,
    pub relative_path: String,
    pub status: String,
    pub dedupe_key: Option<String>,
    pub payload: Value,
    pub attempts: i64,
    pub max_attempts: i64,
    pub locked_at: Option<String>,
    pub run_after: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct QueueService;

impl QueueService {
    pub fn get_listener_state(vault_path: &str) -> ServiceResult<ListenerState> {
        let connection = open_index_for_vault(vault_path)?;
        let state = connection
            .query_row(
                "SELECT enabled, status, watch_path, last_event, last_enqueued_count,
                        last_event_at, last_error, updated_at
                 FROM listener_state
                 WHERE id = 1",
                [],
                row_to_listener_state,
            )
            .optional()?;

        Ok(state.unwrap_or_else(|| ListenerState {
            enabled: false,
            status: "idle".to_string(),
            watch_path: None,
            last_event: None,
            last_enqueued_count: 0,
            last_event_at: None,
            last_error: None,
            updated_at: Utc::now().to_rfc3339(),
        }))
    }

    pub fn set_listener_state(
        vault_path: &str,
        update: ListenerStateUpdate,
    ) -> ServiceResult<ListenerState> {
        validate_nonempty("listener status", &update.status)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let last_enqueued_count = update.last_enqueued_count.unwrap_or_else(|| {
            connection
                .query_row(
                    "SELECT last_enqueued_count FROM listener_state WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0)
        });
        connection.execute(
            "INSERT INTO listener_state
                (id, enabled, status, watch_path, last_event, last_enqueued_count,
                 last_event_at, last_error, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                status = excluded.status,
                watch_path = COALESCE(excluded.watch_path, listener_state.watch_path),
                last_event = COALESCE(excluded.last_event, listener_state.last_event),
                last_enqueued_count = COALESCE(excluded.last_enqueued_count, listener_state.last_enqueued_count),
                last_event_at = excluded.last_event_at,
                last_error = excluded.last_error,
                updated_at = excluded.updated_at",
            params![
                bool_to_i64(update.enabled),
                update.status,
                update.watch_path,
                update.last_event,
                last_enqueued_count,
                update.last_event_at,
                update.last_error,
                now
            ],
        )?;
        Self::get_listener_state(vault_path)
    }

    pub fn enqueue(vault_path: &str, input: QueueItemInput) -> ServiceResult<QueueItem> {
        validate_nonempty("queue item kind", &input.kind)?;
        let relative_path = normalize_relative_path(&input.relative_path)?;
        let payload = input.payload.unwrap_or_else(|| Value::Object(Map::new()));
        let payload_json = serde_json::to_string(&payload)?;
        let max_attempts = input.max_attempts.unwrap_or(3);
        if max_attempts < 1 {
            return Err(ServiceError::InvalidState(
                "queue item max_attempts must be at least 1".to_string(),
            ));
        }

        let connection = open_index_for_vault(vault_path)?;
        if let Some(dedupe_key) = input.dedupe_key.as_deref() {
            if let Some(existing) = find_by_dedupe_key(&connection, dedupe_key)? {
                return Ok(existing);
            }
        }

        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO queue_items
                (kind, relative_path, status, dedupe_key, payload_json, max_attempts, run_after, created_at, updated_at)
             VALUES (?1, ?2, 'pending', ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                input.kind,
                relative_path,
                input.dedupe_key,
                payload_json,
                max_attempts,
                input.run_after,
                now
            ],
        )?;
        get_queue_item(&connection, connection.last_insert_rowid())
    }

    pub fn list_by_status(vault_path: &str, status: &str) -> ServiceResult<Vec<QueueItem>> {
        validate_nonempty("queue status", status)?;
        let connection = open_index_for_vault(vault_path)?;
        let mut statement = connection.prepare(
            "SELECT id, kind, relative_path, status, dedupe_key, payload_json, attempts,
                    max_attempts, locked_at, run_after, created_at, updated_at
             FROM queue_items
             WHERE status = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![status], row_to_queue_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn claim_next(vault_path: &str) -> ServiceResult<Option<QueueItem>> {
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let item = connection
            .query_row(
                "SELECT id, kind, relative_path, status, dedupe_key, payload_json, attempts,
                        max_attempts, locked_at, run_after, created_at, updated_at
                 FROM queue_items
                 WHERE status = 'pending'
                   AND (run_after IS NULL OR run_after <= ?1)
                 ORDER BY created_at, id
                 LIMIT 1",
                params![now],
                row_to_queue_item,
            )
            .optional()?;
        let Some(item) = item else {
            return Ok(None);
        };
        connection.execute(
            "UPDATE queue_items
             SET status = 'running',
                 attempts = attempts + 1,
                 locked_at = ?1,
                 updated_at = ?1
             WHERE id = ?2 AND status = 'pending'",
            params![now, item.id],
        )?;
        get_queue_item(&connection, item.id).map(Some)
    }

    pub fn mark_status(vault_path: &str, id: i64, status: &str) -> ServiceResult<QueueItem> {
        validate_nonempty("queue status", status)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let changed = connection.execute(
            "UPDATE queue_items
             SET status = ?1, updated_at = ?2
             WHERE id = ?3",
            params![status, now, id],
        )?;
        if changed == 0 {
            return Err(ServiceError::InvalidState(format!(
                "queue item {id} does not exist"
            )));
        }
        get_queue_item(&connection, id)
    }

    pub fn finish(
        vault_path: &str,
        id: i64,
        status: &str,
        error: Option<&str>,
    ) -> ServiceResult<QueueItem> {
        validate_nonempty("queue status", status)?;
        let connection = open_index_for_vault(vault_path)?;
        let mut item = get_queue_item(&connection, id)?;
        let now = Utc::now().to_rfc3339();
        let mut payload = item.payload;
        if let Some(error) = error {
            if let Some(object) = payload.as_object_mut() {
                object.insert("lastError".to_string(), Value::String(error.to_string()));
            }
        }
        connection.execute(
            "UPDATE queue_items
             SET status = ?1,
                 payload_json = ?2,
                 locked_at = NULL,
                 run_after = NULL,
                 updated_at = ?3
             WHERE id = ?4",
            params![status, serde_json::to_string(&payload)?, now, id],
        )?;
        item = get_queue_item(&connection, id)?;
        Ok(item)
    }

    pub fn retry_later(
        vault_path: &str,
        id: i64,
        delay_seconds: i64,
        error: &str,
    ) -> ServiceResult<QueueItem> {
        let connection = open_index_for_vault(vault_path)?;
        let item = get_queue_item(&connection, id)?;
        let status = if item.attempts >= item.max_attempts {
            "failed"
        } else {
            "pending"
        };
        let now = Utc::now();
        let run_after = (now + Duration::seconds(delay_seconds.max(0))).to_rfc3339();
        let mut payload = item.payload;
        if let Some(object) = payload.as_object_mut() {
            object.insert("lastError".to_string(), Value::String(error.to_string()));
        }
        connection.execute(
            "UPDATE queue_items
             SET status = ?1,
                 payload_json = ?2,
                 locked_at = NULL,
                 run_after = CASE WHEN ?1 = 'pending' THEN ?3 ELSE NULL END,
                 updated_at = ?4
             WHERE id = ?5",
            params![
                status,
                serde_json::to_string(&payload)?,
                run_after,
                now.to_rfc3339(),
                id
            ],
        )?;
        get_queue_item(&connection, id)
    }
}

fn get_queue_item(connection: &Connection, id: i64) -> ServiceResult<QueueItem> {
    connection
        .query_row(
            "SELECT id, kind, relative_path, status, dedupe_key, payload_json, attempts,
                    max_attempts, locked_at, run_after, created_at, updated_at
             FROM queue_items
             WHERE id = ?1",
            params![id],
            row_to_queue_item,
        )
        .map_err(Into::into)
}

fn find_by_dedupe_key(
    connection: &Connection,
    dedupe_key: &str,
) -> ServiceResult<Option<QueueItem>> {
    connection
        .query_row(
            "SELECT id, kind, relative_path, status, dedupe_key, payload_json, attempts,
                    max_attempts, locked_at, run_after, created_at, updated_at
             FROM queue_items
             WHERE dedupe_key = ?1",
            params![dedupe_key],
            row_to_queue_item,
        )
        .optional()
        .map_err(Into::into)
}

fn row_to_listener_state(row: &Row<'_>) -> rusqlite::Result<ListenerState> {
    Ok(ListenerState {
        enabled: row.get::<_, i64>(0)? != 0,
        status: row.get(1)?,
        watch_path: row.get(2)?,
        last_event: row.get(3)?,
        last_enqueued_count: row.get(4)?,
        last_event_at: row.get(5)?,
        last_error: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn row_to_queue_item(row: &Row<'_>) -> rusqlite::Result<QueueItem> {
    let payload_json: String = row.get(5)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Object(Map::new()));
    Ok(QueueItem {
        id: row.get(0)?,
        kind: row.get(1)?,
        relative_path: row.get(2)?,
        status: row.get(3)?,
        dedupe_key: row.get(4)?,
        payload,
        attempts: row.get(6)?,
        max_attempts: row.get(7)?,
        locked_at: row.get(8)?,
        run_after: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn validate_nonempty(label: &str, value: &str) -> ServiceResult<()> {
    if value.trim().is_empty() {
        Err(ServiceError::InvalidState(format!(
            "{label} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;
    use serde_json::json;

    #[test]
    fn listener_state_and_queue_items_are_persisted() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let state = QueueService::set_listener_state(
            temp.path().to_str().unwrap(),
            ListenerStateUpdate {
                enabled: true,
                status: "watching".to_string(),
                watch_path: None,
                last_event: None,
                last_enqueued_count: None,
                last_event_at: Some("2026-05-31T00:00:00Z".to_string()),
                last_error: None,
            },
        )
        .unwrap();
        assert!(state.enabled);
        assert_eq!(state.status, "watching");

        let first = QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "file_changed".to_string(),
                relative_path: "000-收集箱/a.md".to_string(),
                dedupe_key: Some("file:000-收集箱/a.md".to_string()),
                payload: Some(json!({ "source": "listener" })),
                max_attempts: None,
                run_after: None,
            },
        )
        .unwrap();
        let second = QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "file_changed".to_string(),
                relative_path: "000-收集箱/a.md".to_string(),
                dedupe_key: Some("file:000-收集箱/a.md".to_string()),
                payload: Some(json!({ "source": "listener" })),
                max_attempts: None,
                run_after: None,
            },
        )
        .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(
            QueueService::list_by_status(temp.path().to_str().unwrap(), "pending")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn queue_rejects_paths_that_escape_vault() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let err = QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "file_changed".to_string(),
                relative_path: "../outside.md".to_string(),
                dedupe_key: None,
                payload: None,
                max_attempts: None,
                run_after: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ServiceError::InvalidRelativePath(_)));
    }

    #[test]
    fn queue_claims_one_item_and_can_retry_later() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let item = QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "inbox_file_changed".to_string(),
                relative_path: "000-收集箱/a.md".to_string(),
                dedupe_key: None,
                payload: Some(json!({ "source": "test" })),
                max_attempts: Some(2),
                run_after: None,
            },
        )
        .unwrap();

        let claimed = QueueService::claim_next(temp.path().to_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, item.id);
        assert_eq!(claimed.status, "running");
        assert_eq!(claimed.attempts, 1);
        assert_eq!(
            QueueService::list_by_status(temp.path().to_str().unwrap(), "pending")
                .unwrap()
                .len(),
            0
        );

        let retried =
            QueueService::retry_later(temp.path().to_str().unwrap(), item.id, 30, "wait").unwrap();
        assert_eq!(retried.status, "pending");
        assert!(retried.run_after.is_some());
    }
}
