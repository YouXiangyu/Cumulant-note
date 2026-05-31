use crate::services::index::open_index_for_vault;
use crate::services::vault::normalize_relative_path;
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupeRecordInput {
    pub dedupe_key: String,
    pub content_hash: Option<String>,
    pub relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupeRecord {
    pub dedupe_key: String,
    pub content_hash: Option<String>,
    pub relative_path: Option<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub hit_count: i64,
    pub is_duplicate: bool,
}

pub struct DedupeService;

impl DedupeService {
    pub fn remember(vault_path: &str, input: DedupeRecordInput) -> ServiceResult<DedupeRecord> {
        let dedupe_key = input.dedupe_key.trim().to_string();
        if dedupe_key.is_empty() {
            return Err(ServiceError::InvalidState(
                "dedupe_key must not be empty".to_string(),
            ));
        }
        let relative_path = input
            .relative_path
            .as_deref()
            .map(normalize_relative_path)
            .transpose()?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO dedupe_records
                (dedupe_key, content_hash, relative_path, first_seen_at, last_seen_at, hit_count)
             VALUES (?1, ?2, ?3, ?4, ?4, 1)
             ON CONFLICT(dedupe_key) DO UPDATE SET
                content_hash = COALESCE(excluded.content_hash, dedupe_records.content_hash),
                relative_path = COALESCE(excluded.relative_path, dedupe_records.relative_path),
                last_seen_at = excluded.last_seen_at,
                hit_count = dedupe_records.hit_count + 1",
            params![dedupe_key, input.content_hash, relative_path, now],
        )?;
        Self::get(vault_path, &dedupe_key)
    }

    pub fn get(vault_path: &str, dedupe_key: &str) -> ServiceResult<DedupeRecord> {
        let connection = open_index_for_vault(vault_path)?;
        connection
            .query_row(
                "SELECT dedupe_key, content_hash, relative_path, first_seen_at, last_seen_at, hit_count
                 FROM dedupe_records
                 WHERE dedupe_key = ?1",
                params![dedupe_key],
                row_to_dedupe_record,
            )
            .map_err(Into::into)
    }
}

fn row_to_dedupe_record(row: &Row<'_>) -> rusqlite::Result<DedupeRecord> {
    let hit_count = row.get(5)?;
    Ok(DedupeRecord {
        dedupe_key: row.get(0)?,
        content_hash: row.get(1)?,
        relative_path: row.get(2)?,
        first_seen_at: row.get(3)?,
        last_seen_at: row.get(4)?,
        hit_count,
        is_duplicate: hit_count > 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn remembers_caller_provided_dedupe_key_without_hashing() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let first = DedupeService::remember(
            temp.path().to_str().unwrap(),
            DedupeRecordInput {
                dedupe_key: "caller-key".to_string(),
                content_hash: Some("caller-hash".to_string()),
                relative_path: Some("000-收集箱/a.md".to_string()),
            },
        )
        .unwrap();
        let second = DedupeService::remember(
            temp.path().to_str().unwrap(),
            DedupeRecordInput {
                dedupe_key: "caller-key".to_string(),
                content_hash: None,
                relative_path: None,
            },
        )
        .unwrap();

        assert!(!first.is_duplicate);
        assert!(second.is_duplicate);
        assert_eq!(second.hit_count, 2);
        assert_eq!(second.content_hash.as_deref(), Some("caller-hash"));
    }
}
