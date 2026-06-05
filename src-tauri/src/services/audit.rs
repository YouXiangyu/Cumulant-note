use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::services::index::open_index_for_vault;
use crate::services::ServiceResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: i64,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuditSearchQuery {
    pub event_types: Option<Vec<String>>,
    pub text: Option<String>,
    pub source_relative_path: Option<String>,
    pub target_relative_path: Option<String>,
    pub queue_id: Option<i64>,
    pub movement_id: Option<i64>,
    pub status: Option<String>,
    pub created_after: Option<String>,
    pub created_before: Option<String>,
    pub limit: Option<usize>,
    pub before_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventSearchResult {
    pub events: Vec<AuditEvent>,
    pub next_before_id: Option<i64>,
    pub applied_limit: usize,
}

pub struct AuditService;

impl AuditService {
    pub fn mock_event(event_type: &str, payload: Value) -> AuditEvent {
        AuditEvent {
            id: 0,
            event_type: event_type.to_string(),
            payload,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn record(vault_path: &str, event_type: &str, payload: Value) -> ServiceResult<AuditEvent> {
        let connection = open_index_for_vault(vault_path)?;
        let created_at = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO audit_events (event_type, payload_json, created_at)
             VALUES (?1, ?2, ?3)",
            params![event_type, serde_json::to_string(&payload)?, created_at],
        )?;
        Self::get(vault_path, connection.last_insert_rowid())
    }

    pub fn get(vault_path: &str, id: i64) -> ServiceResult<AuditEvent> {
        let connection = open_index_for_vault(vault_path)?;
        connection
            .query_row(
                "SELECT id, event_type, payload_json, created_at
                 FROM audit_events WHERE id = ?1",
                params![id],
                row_to_audit_event,
            )
            .map_err(Into::into)
    }

    pub fn list_by_type(vault_path: &str, event_type: &str) -> ServiceResult<Vec<AuditEvent>> {
        let connection = open_index_for_vault(vault_path)?;
        let mut statement = connection.prepare(
            "SELECT id, event_type, payload_json, created_at
             FROM audit_events
             WHERE event_type = ?1
             ORDER BY id DESC",
        )?;
        let rows = statement.query_map(params![event_type], row_to_audit_event)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list(
        vault_path: &str,
        event_type: Option<&str>,
        limit: Option<usize>,
    ) -> ServiceResult<Vec<AuditEvent>> {
        let connection = open_index_for_vault(vault_path)?;
        let limit = limit.unwrap_or(50).clamp(1, 200) as i64;
        match event_type.map(str::trim).filter(|value| !value.is_empty()) {
            Some(event_type) => {
                let mut statement = connection.prepare(
                    "SELECT id, event_type, payload_json, created_at
                     FROM audit_events
                     WHERE event_type = ?1
                     ORDER BY id DESC
                     LIMIT ?2",
                )?;
                let rows = statement.query_map(params![event_type, limit], row_to_audit_event)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
            }
            None => {
                let mut statement = connection.prepare(
                    "SELECT id, event_type, payload_json, created_at
                     FROM audit_events
                     ORDER BY id DESC
                     LIMIT ?1",
                )?;
                let rows = statement.query_map(params![limit], row_to_audit_event)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
            }
        }
    }

    pub fn search(
        vault_path: &str,
        query: AuditSearchQuery,
    ) -> ServiceResult<AuditEventSearchResult> {
        let connection = open_index_for_vault(vault_path)?;
        let applied_limit = query.limit.unwrap_or(50).clamp(1, 200);
        let mut statement = connection.prepare(
            "SELECT id, event_type, payload_json, created_at
             FROM audit_events
             ORDER BY id DESC",
        )?;
        let rows = statement.query_map([], row_to_audit_event)?;
        let event_types = query
            .event_types
            .unwrap_or_default()
            .into_iter()
            .map(|event_type| event_type.trim().to_string())
            .filter(|event_type| !event_type.is_empty())
            .collect::<Vec<_>>();
        let text = query
            .text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let source_relative_path = normalized_optional_path(query.source_relative_path)?;
        let target_relative_path = normalized_optional_path(query.target_relative_path)?;
        let status = query
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let mut events = Vec::new();
        for row in rows {
            let event = row?;
            if let Some(before_id) = query.before_id {
                if event.id >= before_id {
                    continue;
                }
            }
            if !event_types.is_empty()
                && !event_types
                    .iter()
                    .any(|event_type| event_type == &event.event_type)
            {
                continue;
            }
            if let Some(created_after) = query.created_after.as_deref() {
                if event.created_at.as_str() < created_after {
                    continue;
                }
            }
            if let Some(created_before) = query.created_before.as_deref() {
                if event.created_at.as_str() > created_before {
                    continue;
                }
            }
            if let Some(source) = source_relative_path.as_deref() {
                if !payload_string_matches(
                    &event.payload,
                    &["sourceRelativePath", "sourcePath", "relativePath"],
                    source,
                ) {
                    continue;
                }
            }
            if let Some(target) = target_relative_path.as_deref() {
                if !payload_string_matches(
                    &event.payload,
                    &["targetRelativePath", "targetPath", "restoreRelativePath"],
                    target,
                ) {
                    continue;
                }
            }
            if let Some(queue_id) = query.queue_id {
                if !payload_i64_matches(&event.payload, &["queueId", "queueItemId"], queue_id) {
                    continue;
                }
            }
            if let Some(movement_id) = query.movement_id {
                if !payload_i64_matches(&event.payload, &["movementId"], movement_id) {
                    continue;
                }
            }
            if let Some(status) = status.as_deref() {
                if !payload_string_matches(&event.payload, &["status"], status) {
                    continue;
                }
            }
            if let Some(text) = text.as_deref() {
                let payload_text = serde_json::to_string(&event.payload).unwrap_or_default();
                let haystack = format!(
                    "{} {} {} {}",
                    event.id, event.event_type, event.created_at, payload_text
                )
                .to_ascii_lowercase();
                if !haystack.contains(text) {
                    continue;
                }
            }

            events.push(event);
            if events.len() >= applied_limit {
                break;
            }
        }
        let next_before_id = events.last().map(|event| event.id);
        Ok(AuditEventSearchResult {
            events,
            next_before_id,
            applied_limit,
        })
    }

    pub fn latest_by_type(vault_path: &str, event_type: &str) -> ServiceResult<Option<AuditEvent>> {
        let connection = open_index_for_vault(vault_path)?;
        connection
            .query_row(
                "SELECT id, event_type, payload_json, created_at
                 FROM audit_events
                 WHERE event_type = ?1
                 ORDER BY id DESC
                 LIMIT 1",
                params![event_type],
                row_to_audit_event,
            )
            .optional()
            .map_err(Into::into)
    }
}

fn normalized_optional_path(value: Option<String>) -> ServiceResult<Option<String>> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(crate::services::vault::normalize_relative_path)
        .transpose()
}

fn payload_string_matches(payload: &Value, keys: &[&str], expected: &str) -> bool {
    keys.iter().any(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .map(|value| value == expected)
            .unwrap_or(false)
    })
}

fn payload_i64_matches(payload: &Value, keys: &[&str], expected: i64) -> bool {
    keys.iter().any(|key| {
        payload
            .get(*key)
            .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
            .map(|value| value == expected)
            .unwrap_or(false)
    })
}

fn row_to_audit_event(row: &Row<'_>) -> rusqlite::Result<AuditEvent> {
    let payload_json: String = row.get(2)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
    Ok(AuditEvent {
        id: row.get(0)?,
        event_type: row.get(1)?,
        payload,
        created_at: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;
    use serde_json::json;

    #[test]
    fn audit_events_are_persisted() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let event = AuditService::record(
            temp.path().to_str().unwrap(),
            "import",
            json!({"relativePath": "000-收集箱/a.md"}),
        )
        .unwrap();
        assert!(event.id > 0);

        let latest = AuditService::latest_by_type(temp.path().to_str().unwrap(), "import")
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, event.id);
        assert_eq!(latest.payload["relativePath"], "000-收集箱/a.md");
    }

    #[test]
    fn audit_events_can_be_listed_recent_first_with_limit() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        AuditService::record(temp.path().to_str().unwrap(), "first", json!({})).unwrap();
        let second =
            AuditService::record(temp.path().to_str().unwrap(), "second", json!({})).unwrap();
        let third =
            AuditService::record(temp.path().to_str().unwrap(), "second", json!({})).unwrap();

        let recent = AuditService::list(temp.path().to_str().unwrap(), None, Some(2)).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, third.id);
        assert_eq!(recent[1].id, second.id);

        let filtered =
            AuditService::list(temp.path().to_str().unwrap(), Some("second"), Some(10)).unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|event| event.event_type == "second"));
    }

    #[test]
    fn audit_events_can_be_searched_by_type_text_path_and_cursor() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let first = AuditService::record(
            temp.path().to_str().unwrap(),
            "worker_failed",
            json!({
                "queueId": 7,
                "sourceRelativePath": "000-收集箱/a.md",
                "status": "failed",
                "message": "missing key"
            }),
        )
        .unwrap();
        let second = AuditService::record(
            temp.path().to_str().unwrap(),
            "move",
            json!({
                "movementId": 3,
                "sourceRelativePath": "000-收集箱/b.md",
                "targetRelativePath": "100-School/b.md",
                "status": "moved"
            }),
        )
        .unwrap();

        let search = AuditService::search(
            temp.path().to_str().unwrap(),
            AuditSearchQuery {
                event_types: Some(vec!["worker_failed".to_string()]),
                text: Some("missing".to_string()),
                source_relative_path: Some("000-收集箱/a.md".to_string()),
                queue_id: Some(7),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(search.events.len(), 1);
        assert_eq!(search.events[0].id, first.id);

        let cursor = AuditService::search(
            temp.path().to_str().unwrap(),
            AuditSearchQuery {
                before_id: Some(second.id),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(cursor.events.len(), 1);
        assert_eq!(cursor.events[0].id, first.id);

        let moved = AuditService::search(
            temp.path().to_str().unwrap(),
            AuditSearchQuery {
                target_relative_path: Some("100-School/b.md".to_string()),
                movement_id: Some(3),
                status: Some("moved".to_string()),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(moved.events.len(), 1);
        assert_eq!(moved.events[0].id, second.id);
    }
}
