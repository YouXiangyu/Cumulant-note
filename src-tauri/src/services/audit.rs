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
}
