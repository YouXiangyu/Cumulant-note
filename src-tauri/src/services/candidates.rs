use crate::services::index::open_index_for_vault;
use crate::services::vault::normalize_relative_path;
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateInput {
    pub candidate_type: String,
    pub source_relative_path: Option<String>,
    pub title: String,
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionCandidate {
    pub id: i64,
    pub candidate_type: String,
    pub source_relative_path: Option<String>,
    pub title: String,
    pub payload: Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub confirmed_at: Option<String>,
}

pub struct CandidateService;

impl CandidateService {
    pub fn create(vault_path: &str, input: CandidateInput) -> ServiceResult<ActionCandidate> {
        validate_nonempty("candidate_type", &input.candidate_type)?;
        validate_nonempty("candidate title", &input.title)?;
        let source_relative_path = input
            .source_relative_path
            .as_deref()
            .map(normalize_relative_path)
            .transpose()?;
        let payload = input.payload.unwrap_or_else(|| Value::Object(Map::new()));
        let payload_json = serde_json::to_string(&payload)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO action_candidates
                (candidate_type, source_relative_path, title, payload_json, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)",
            params![
                input.candidate_type,
                source_relative_path,
                input.title,
                payload_json,
                now
            ],
        )?;
        Self::get(vault_path, connection.last_insert_rowid())
    }

    pub fn confirm(vault_path: &str, id: i64) -> ServiceResult<ActionCandidate> {
        Self::set_status(vault_path, id, "confirmed")
    }

    pub fn reject(vault_path: &str, id: i64) -> ServiceResult<ActionCandidate> {
        Self::set_status(vault_path, id, "rejected")
    }

    pub fn set_status(vault_path: &str, id: i64, status: &str) -> ServiceResult<ActionCandidate> {
        validate_candidate_status(status)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let confirmed_at = (status == "confirmed").then_some(now.clone());
        let changed = connection.execute(
            "UPDATE action_candidates
             SET status = ?1, updated_at = ?2, confirmed_at = ?3
             WHERE id = ?4",
            params![status, now, confirmed_at, id],
        )?;
        if changed == 0 {
            return Err(ServiceError::InvalidState(format!(
                "candidate {id} does not exist"
            )));
        }
        get_candidate(&connection, id)
    }

    pub fn get(vault_path: &str, id: i64) -> ServiceResult<ActionCandidate> {
        let connection = open_index_for_vault(vault_path)?;
        get_candidate(&connection, id)
    }

    pub fn list_pending(vault_path: &str) -> ServiceResult<Vec<ActionCandidate>> {
        let connection = open_index_for_vault(vault_path)?;
        let mut statement = connection.prepare(
            "SELECT id, candidate_type, source_relative_path, title, payload_json,
                    status, created_at, updated_at, confirmed_at
             FROM action_candidates
             WHERE status = 'pending'
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], row_to_candidate)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn get_candidate(connection: &rusqlite::Connection, id: i64) -> ServiceResult<ActionCandidate> {
    connection
        .query_row(
            "SELECT id, candidate_type, source_relative_path, title, payload_json,
                    status, created_at, updated_at, confirmed_at
             FROM action_candidates
             WHERE id = ?1",
            params![id],
            row_to_candidate,
        )
        .map_err(Into::into)
}

fn row_to_candidate(row: &Row<'_>) -> rusqlite::Result<ActionCandidate> {
    let payload_json: String = row.get(4)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Object(Map::new()));
    Ok(ActionCandidate {
        id: row.get(0)?,
        candidate_type: row.get(1)?,
        source_relative_path: row.get(2)?,
        title: row.get(3)?,
        payload,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        confirmed_at: row.get(8)?,
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

fn validate_candidate_status(status: &str) -> ServiceResult<()> {
    match status {
        "pending" | "confirmed" | "rejected" => Ok(()),
        _ => Err(ServiceError::InvalidState(format!(
            "unsupported candidate status: {status}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;
    use serde_json::json;

    #[test]
    fn todo_and_schedule_candidates_can_be_confirmed_or_rejected() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let todo = CandidateService::create(
            temp.path().to_str().unwrap(),
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: Some("000-收集箱/a.md".to_string()),
                title: "Buy milk".to_string(),
                payload: Some(json!({ "line": 4 })),
            },
        )
        .unwrap();
        let schedule = CandidateService::create(
            temp.path().to_str().unwrap(),
            CandidateInput {
                candidate_type: "schedule".to_string(),
                source_relative_path: None,
                title: "Meeting".to_string(),
                payload: Some(json!({ "date": "2026-06-01" })),
            },
        )
        .unwrap();

        assert_eq!(
            CandidateService::list_pending(temp.path().to_str().unwrap())
                .unwrap()
                .len(),
            2
        );
        let confirmed = CandidateService::confirm(temp.path().to_str().unwrap(), todo.id).unwrap();
        let rejected =
            CandidateService::reject(temp.path().to_str().unwrap(), schedule.id).unwrap();
        assert_eq!(confirmed.status, "confirmed");
        assert!(confirmed.confirmed_at.is_some());
        assert_eq!(rejected.status, "rejected");
    }
}
