use crate::services::audit::AuditService;
use crate::services::queue::{QueueItemInput, QueueService};
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, VaultService, INBOX_DIR,
};
use crate::services::{ServiceError, ServiceResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxImportResult {
    pub source_path: String,
    pub relative_path: Option<String>,
    pub file_name: Option<String>,
    pub mode: String,
    pub status: String,
    pub bytes: Option<u64>,
    pub audit_id: Option<i64>,
    pub queue_item_id: Option<i64>,
    pub error: Option<String>,
}

pub struct ImportService;

impl ImportService {
    pub fn import_to_inbox(
        vault_path: &str,
        source_paths: Vec<String>,
        mode: Option<String>,
    ) -> ServiceResult<Vec<InboxImportResult>> {
        let mode = normalize_mode(mode)?;
        let _ = VaultService::init(vault_path)?;
        let root = canonical_vault_root(vault_path)?;
        let mut results = Vec::new();

        for source in source_paths {
            results.push(import_one(vault_path, &root, &source, &mode)?);
        }

        Ok(results)
    }
}

fn import_one(
    vault_path: &str,
    root: &Path,
    source: &str,
    mode: &str,
) -> ServiceResult<InboxImportResult> {
    let source_path = PathBuf::from(source);
    let base = InboxImportResult {
        source_path: source.to_string(),
        relative_path: None,
        file_name: source_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        mode: mode.to_string(),
        status: "failed".to_string(),
        bytes: None,
        audit_id: None,
        queue_item_id: None,
        error: None,
    };

    let Ok(source_canonical) = source_path.canonicalize() else {
        return record_result(
            vault_path,
            InboxImportResult {
                status: "failed".to_string(),
                error: Some("source file does not exist".to_string()),
                ..base
            },
        );
    };

    if !source_canonical.is_file() {
        return record_result(
            vault_path,
            InboxImportResult {
                status: "failed".to_string(),
                error: Some("source path is not a file".to_string()),
                ..base
            },
        );
    }

    if !is_supported_import_file(&source_canonical) {
        return record_result(
            vault_path,
            InboxImportResult {
                status: "unsupported".to_string(),
                error: Some("unsupported v0.3 import file type".to_string()),
                ..base
            },
        );
    }

    let file_name = source_canonical
        .file_name()
        .ok_or_else(|| ServiceError::InvalidRelativePath(source.to_string()))?
        .to_string_lossy()
        .to_string();
    let relative_path = normalize_relative_path(&format!("{INBOX_DIR}/{file_name}"))?;
    let target_path = root.join(&relative_path);
    let bytes = source_canonical
        .metadata()
        .ok()
        .map(|metadata| metadata.len());

    if target_path.exists() {
        let same_file = target_path
            .canonicalize()
            .map(|target| target == source_canonical)
            .unwrap_or(false);
        if same_file {
            let queued = enqueue_import(vault_path, &relative_path, mode, "already_inbox")?;
            return record_result(
                vault_path,
                InboxImportResult {
                    relative_path: Some(relative_path),
                    file_name: Some(file_name),
                    status: "already_inbox".to_string(),
                    bytes,
                    queue_item_id: Some(queued.id),
                    ..base
                },
            );
        }

        let mut result = InboxImportResult {
            relative_path: Some(relative_path.clone()),
            file_name: Some(file_name),
            status: "conflict".to_string(),
            bytes,
            error: Some("target file already exists in inbox".to_string()),
            ..base
        };
        result.audit_id = Some(record_conflict(vault_path, &result)?);
        return record_result(vault_path, result);
    }

    if mode == "move" && source_canonical.starts_with(root) {
        return record_result(
            vault_path,
            InboxImportResult {
                relative_path: Some(relative_path),
                file_name: Some(file_name),
                status: "failed".to_string(),
                bytes,
                error: Some("move import refuses to move an existing vault file".to_string()),
                ..base
            },
        );
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if mode == "move" {
        match fs::rename(&source_canonical, &target_path) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(&source_canonical, &target_path)?;
                fs::remove_file(&source_canonical)?;
            }
        }
    } else {
        fs::copy(&source_canonical, &target_path)?;
    }

    let queued = enqueue_import(vault_path, &relative_path, mode, "imported")?;
    record_result(
        vault_path,
        InboxImportResult {
            relative_path: Some(relative_path),
            file_name: Some(file_name),
            status: "imported".to_string(),
            bytes,
            queue_item_id: Some(queued.id),
            ..base
        },
    )
}

fn enqueue_import(
    vault_path: &str,
    relative_path: &str,
    mode: &str,
    status: &str,
) -> ServiceResult<crate::services::queue::QueueItem> {
    QueueService::enqueue(
        vault_path,
        QueueItemInput {
            kind: "imported_inbox_file".to_string(),
            relative_path: relative_path.to_string(),
            dedupe_key: Some(format!("import:{relative_path}")),
            payload: Some(json!({
                "source": "import",
                "mode": mode,
                "status": status,
                "relativePath": relative_path
            })),
            max_attempts: Some(3),
            run_after: None,
        },
    )
}

fn record_result(
    vault_path: &str,
    mut result: InboxImportResult,
) -> ServiceResult<InboxImportResult> {
    let event = AuditService::record(
        vault_path,
        "import",
        json!({
            "sourcePath": &result.source_path,
            "relativePath": &result.relative_path,
            "fileName": &result.file_name,
            "mode": &result.mode,
            "status": &result.status,
            "bytes": result.bytes,
            "queueItemId": result.queue_item_id,
            "error": &result.error
        }),
    )?;
    if result.audit_id.is_none() {
        result.audit_id = Some(event.id);
    }
    Ok(result)
}

fn record_conflict(vault_path: &str, result: &InboxImportResult) -> ServiceResult<i64> {
    let event = AuditService::record(
        vault_path,
        "conflict",
        json!({
            "kind": "import",
            "sourcePath": &result.source_path,
            "relativePath": &result.relative_path,
            "mode": &result.mode,
            "status": "open",
            "message": &result.error
        }),
    )?;
    Ok(event.id)
}

fn normalize_mode(mode: Option<String>) -> ServiceResult<String> {
    let mode = mode
        .unwrap_or_else(|| "copy".to_string())
        .to_ascii_lowercase();
    match mode.as_str() {
        "copy" | "move" => Ok(mode),
        _ => Err(ServiceError::InvalidState(format!(
            "unsupported import mode: {mode}"
        ))),
    }
}

fn is_supported_import_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "md" | "markdown" | "txt" | "mp3" | "wav" | "m4a" | "aac" | "png" | "jpg" | "jpeg"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::LEDGER_FILE;

    #[test]
    fn import_copy_is_idempotent_and_does_not_overwrite() {
        let vault = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let source = external.path().join("note.md");
        fs::write(&source, "hello").unwrap();

        let first = ImportService::import_to_inbox(
            vault.path().to_str().unwrap(),
            vec![source.to_string_lossy().to_string()],
            Some("copy".to_string()),
        )
        .unwrap();
        assert_eq!(first[0].status, "imported");

        fs::write(&source, "changed").unwrap();
        let second = ImportService::import_to_inbox(
            vault.path().to_str().unwrap(),
            vec![source.to_string_lossy().to_string()],
            Some("copy".to_string()),
        )
        .unwrap();
        assert_eq!(second[0].status, "conflict");
        assert_eq!(
            fs::read_to_string(vault.path().join(INBOX_DIR).join("note.md")).unwrap(),
            "hello"
        );
        assert!(vault.path().join(INBOX_DIR).join(LEDGER_FILE).exists());
    }

    #[test]
    fn unsupported_import_type_is_reported() {
        let vault = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let source = external.path().join("deck.pdf");
        fs::write(&source, "pdf").unwrap();

        let result = ImportService::import_to_inbox(
            vault.path().to_str().unwrap(),
            vec![source.to_string_lossy().to_string()],
            None,
        )
        .unwrap();
        assert_eq!(result[0].status, "unsupported");
    }
}
