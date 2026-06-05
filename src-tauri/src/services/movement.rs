use crate::services::audit::AuditService;
use crate::services::index::open_index_for_vault;
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, resolve_existing_file, INBOX_DIR, LEDGER_FILE,
};
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRequest {
    pub source_relative_path: String,
    pub target_relative_path: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveLog {
    pub id: i64,
    pub operation: String,
    pub source_relative_path: String,
    pub target_relative_path: String,
    pub reason: Option<String>,
    pub status: String,
    pub created_at: String,
    pub moved_at: Option<String>,
    pub rolled_back_at: Option<String>,
    pub error: Option<String>,
}

pub struct MovementService;

impl MovementService {
    pub fn move_from_inbox(vault_path: &str, request: MoveRequest) -> ServiceResult<MoveLog> {
        let source = normalize_relative_path(&request.source_relative_path)?;
        let target = normalize_relative_path(&request.target_relative_path)?;
        ensure_movable_inbox_source(&source)?;

        let root = canonical_vault_root(vault_path)?;
        let source_path = resolve_existing_file(vault_path, &source)?;
        let target_path = resolve_new_vault_file(&root, &target)?;
        if target_path.exists() {
            record_movement_conflict(
                vault_path,
                "move",
                &source,
                &target,
                "target already exists",
            );
            return Err(ServiceError::Conflict(format!(
                "target already exists: {target}"
            )));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut log = Self::insert_log(
            vault_path,
            "move",
            &source,
            &target,
            request.reason.as_deref(),
            "started",
            None,
        )?;
        match fs::rename(&source_path, &target_path) {
            Ok(()) => {
                append_ledger_entry(&root, &target, request.reason.as_deref())?;
                let _ = prune_empty_inbox_parents(&root, source_path.parent());
                log = Self::update_log(
                    vault_path,
                    log.id,
                    "moved",
                    Some(Utc::now().to_rfc3339()),
                    None,
                    None,
                )?;
                AuditService::record(
                    vault_path,
                    "move",
                    json!({
                        "movementId": log.id,
                        "sourceRelativePath": &log.source_relative_path,
                        "targetRelativePath": &log.target_relative_path,
                        "status": &log.status,
                        "reason": &log.reason
                    }),
                )?;
                Ok(log)
            }
            Err(error) => {
                let message = error.to_string();
                let _ = Self::update_log(vault_path, log.id, "failed", None, None, Some(&message));
                Err(ServiceError::Io(error))
            }
        }
    }

    pub fn rollback(vault_path: &str, movement_id: i64) -> ServiceResult<MoveLog> {
        let log = Self::get(vault_path, movement_id)?;
        if log.status != "moved" {
            return Err(ServiceError::InvalidState(format!(
                "movement {movement_id} is not rollbackable"
            )));
        }
        let root = canonical_vault_root(vault_path)?;
        let source_path = resolve_existing_file(vault_path, &log.target_relative_path)?;
        let target_path = resolve_new_vault_file(&root, &log.source_relative_path)?;
        if target_path.exists() {
            record_movement_conflict(
                vault_path,
                "rollback",
                &log.target_relative_path,
                &log.source_relative_path,
                "rollback target already exists",
            );
            return Err(ServiceError::Conflict(format!(
                "rollback target already exists: {}",
                log.source_relative_path
            )));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source_path, target_path)?;
        append_rollback_entry(&root, &log.target_relative_path, &log.source_relative_path)?;
        let rolled_back = Self::update_log(
            vault_path,
            movement_id,
            "rolled_back",
            log.moved_at,
            Some(Utc::now().to_rfc3339()),
            None,
        )?;
        AuditService::record(
            vault_path,
            "rollback",
            json!({
                "movementId": rolled_back.id,
                "sourceRelativePath": &rolled_back.source_relative_path,
                "targetRelativePath": &rolled_back.target_relative_path,
                "status": &rolled_back.status
            }),
        )?;
        Ok(rolled_back)
    }

    pub fn list(vault_path: &str) -> ServiceResult<Vec<MoveLog>> {
        let connection = open_index_for_vault(vault_path)?;
        let mut statement = connection.prepare(
            "SELECT id, operation, source_relative_path, target_relative_path, reason, status,
                    created_at, moved_at, rolled_back_at, error
             FROM movement_log ORDER BY id DESC",
        )?;
        let rows = statement.query_map([], row_to_move_log)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn get(vault_path: &str, movement_id: i64) -> ServiceResult<MoveLog> {
        let connection = open_index_for_vault(vault_path)?;
        connection
            .query_row(
                "SELECT id, operation, source_relative_path, target_relative_path, reason, status,
                        created_at, moved_at, rolled_back_at, error
                 FROM movement_log WHERE id = ?1",
                params![movement_id],
                row_to_move_log,
            )
            .optional()?
            .ok_or_else(|| {
                ServiceError::InvalidState(format!("movement {movement_id} does not exist"))
            })
    }

    fn insert_log(
        vault_path: &str,
        operation: &str,
        source: &str,
        target: &str,
        reason: Option<&str>,
        status: &str,
        error: Option<&str>,
    ) -> ServiceResult<MoveLog> {
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO movement_log
                (operation, source_relative_path, target_relative_path, reason, status, created_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![operation, source, target, reason, status, now, error],
        )?;
        Self::get(vault_path, connection.last_insert_rowid())
    }

    fn update_log(
        vault_path: &str,
        id: i64,
        status: &str,
        moved_at: Option<String>,
        rolled_back_at: Option<String>,
        error: Option<&str>,
    ) -> ServiceResult<MoveLog> {
        let connection = open_index_for_vault(vault_path)?;
        connection.execute(
            "UPDATE movement_log
             SET status = ?1,
                 moved_at = COALESCE(?2, moved_at),
                 rolled_back_at = COALESCE(?3, rolled_back_at),
                 error = ?4
             WHERE id = ?5",
            params![status, moved_at, rolled_back_at, error, id],
        )?;
        Self::get(vault_path, id)
    }
}

fn resolve_new_vault_file(root: &Path, relative_path: &str) -> ServiceResult<std::path::PathBuf> {
    let normalized = normalize_relative_path(relative_path)?;
    let target_path = root.join(normalized);
    let parent = target_path
        .parent()
        .ok_or_else(|| ServiceError::InvalidRelativePath(relative_path.to_string()))?;
    let mut existing_ancestor = parent;
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| ServiceError::EscapedVault(parent.to_string_lossy().to_string()))?;
    }
    let canonical_ancestor = existing_ancestor.canonicalize()?;
    if canonical_ancestor.starts_with(root) {
        Ok(target_path)
    } else {
        Err(ServiceError::EscapedVault(
            parent.to_string_lossy().to_string(),
        ))
    }
}

pub fn ensure_movable_inbox_source(relative_path: &str) -> ServiceResult<()> {
    let normalized = normalize_relative_path(relative_path)?;
    let ledger = format!("{INBOX_DIR}/{LEDGER_FILE}");
    if normalized == ledger {
        return Err(ServiceError::InvalidState(
            "ledger file cannot be moved by AI".to_string(),
        ));
    }
    if normalized.starts_with(&format!("{INBOX_DIR}/")) {
        Ok(())
    } else {
        Err(ServiceError::InvalidState(
            "AI movement source must be inside 000-收集箱".to_string(),
        ))
    }
}

fn record_movement_conflict(
    vault_path: &str,
    operation: &str,
    source: &str,
    target: &str,
    message: &str,
) {
    let _ = AuditService::record(
        vault_path,
        "conflict",
        json!({
            "kind": operation,
            "sourceRelativePath": source,
            "targetRelativePath": target,
            "status": "open",
            "message": message
        }),
    );
}

fn prune_empty_inbox_parents(root: &Path, start: Option<&Path>) -> ServiceResult<()> {
    let inbox_root = root.join(INBOX_DIR);
    let Some(mut current) = start.map(Path::to_path_buf) else {
        return Ok(());
    };

    loop {
        if current == inbox_root || !current.starts_with(&inbox_root) {
            break;
        }
        match fs::remove_dir(&current) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::DirectoryNotEmpty | ErrorKind::NotFound
                ) =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        }
        if !current.pop() {
            break;
        }
    }
    Ok(())
}

fn append_ledger_entry(
    root: &Path,
    target_relative_path: &str,
    reason: Option<&str>,
) -> ServiceResult<()> {
    let ledger = root.join(INBOX_DIR).join(LEDGER_FILE);
    let reason = reason.unwrap_or("AI 整理");
    let line = format!(
        "- {} [[../{}]] - {}\n",
        Utc::now().to_rfc3339(),
        target_relative_path,
        reason
    );
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger)?
        .write_all(line.as_bytes())?;
    Ok(())
}

fn append_rollback_entry(
    root: &Path,
    old_target: &str,
    restored_source: &str,
) -> ServiceResult<()> {
    let ledger = root.join(INBOX_DIR).join(LEDGER_FILE);
    let line = format!(
        "- {} rollback [[../{}]] -> [[{}]]\n",
        Utc::now().to_rfc3339(),
        old_target,
        restored_source
    );
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger)?
        .write_all(line.as_bytes())?;
    Ok(())
}

fn row_to_move_log(row: &Row<'_>) -> rusqlite::Result<MoveLog> {
    Ok(MoveLog {
        id: row.get(0)?,
        operation: row.get(1)?,
        source_relative_path: row.get(2)?,
        target_relative_path: row.get(3)?,
        reason: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        moved_at: row.get(7)?,
        rolled_back_at: row.get(8)?,
        error: row.get(9)?,
    })
}

use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn moves_only_inbox_files_and_rolls_back_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join(INBOX_DIR).join("nested")).unwrap();
        fs::write(
            temp.path().join(INBOX_DIR).join("nested").join("a.md"),
            "hello",
        )
        .unwrap();

        let log = MovementService::move_from_inbox(
            temp.path().to_str().unwrap(),
            MoveRequest {
                source_relative_path: "000-收集箱/nested/a.md".to_string(),
                target_relative_path: "100-School/a.md".to_string(),
                reason: Some("test".to_string()),
            },
        )
        .unwrap();
        assert_eq!(log.status, "moved");
        assert!(temp.path().join("100-School").join("a.md").exists());
        assert!(!temp.path().join(INBOX_DIR).join("a.md").exists());

        let rolled = MovementService::rollback(temp.path().to_str().unwrap(), log.id).unwrap();
        assert_eq!(rolled.status, "rolled_back");
        assert!(temp
            .path()
            .join(INBOX_DIR)
            .join("nested")
            .join("a.md")
            .exists());
    }

    #[test]
    fn move_prunes_empty_inbox_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let source_dir = temp.path().join(INBOX_DIR).join("nested").join("deep");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("a.md"), "hello").unwrap();

        MovementService::move_from_inbox(
            temp.path().to_str().unwrap(),
            MoveRequest {
                source_relative_path: format!("{INBOX_DIR}/nested/deep/a.md"),
                target_relative_path: "100-School/a.md".to_string(),
                reason: Some("test".to_string()),
            },
        )
        .unwrap();

        assert!(temp.path().join(INBOX_DIR).exists());
        assert!(!source_dir.exists());
        assert!(!temp.path().join(INBOX_DIR).join("nested").exists());
    }

    #[test]
    fn move_keeps_non_empty_inbox_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let source_dir = temp.path().join(INBOX_DIR).join("nested");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("a.md"), "hello").unwrap();
        fs::write(source_dir.join("keep.md"), "keep").unwrap();

        MovementService::move_from_inbox(
            temp.path().to_str().unwrap(),
            MoveRequest {
                source_relative_path: format!("{INBOX_DIR}/nested/a.md"),
                target_relative_path: "100-School/a.md".to_string(),
                reason: Some("test".to_string()),
            },
        )
        .unwrap();

        assert!(source_dir.exists());
        assert!(source_dir.join("keep.md").exists());
    }

    #[test]
    fn move_and_rollback_refuse_to_overwrite_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        fs::write(temp.path().join(INBOX_DIR).join("a.md"), "source").unwrap();
        fs::write(temp.path().join("100-School").join("a.md"), "target").unwrap();

        let move_conflict = MovementService::move_from_inbox(
            temp.path().to_str().unwrap(),
            MoveRequest {
                source_relative_path: "000-收集箱/a.md".to_string(),
                target_relative_path: "100-School/a.md".to_string(),
                reason: None,
            },
        )
        .unwrap_err();
        assert!(matches!(move_conflict, ServiceError::Conflict(_)));
        assert_eq!(
            fs::read_to_string(temp.path().join("100-School").join("a.md")).unwrap(),
            "target"
        );

        let log = MovementService::move_from_inbox(
            temp.path().to_str().unwrap(),
            MoveRequest {
                source_relative_path: "000-收集箱/a.md".to_string(),
                target_relative_path: "100-School/b.md".to_string(),
                reason: None,
            },
        )
        .unwrap();
        fs::write(temp.path().join(INBOX_DIR).join("a.md"), "new source").unwrap();

        let rollback_conflict =
            MovementService::rollback(temp.path().to_str().unwrap(), log.id).unwrap_err();
        assert!(matches!(rollback_conflict, ServiceError::Conflict(_)));
        assert_eq!(
            fs::read_to_string(temp.path().join(INBOX_DIR).join("a.md")).unwrap(),
            "new source"
        );
    }

    #[test]
    fn rejects_non_inbox_and_ledger_moves() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        assert!(ensure_movable_inbox_source("100-School/a.md").is_err());
        assert!(ensure_movable_inbox_source("000-收集箱/收集箱-已整理.md").is_err());
        assert!(ensure_movable_inbox_source("../outside.md").is_err());
    }
}
