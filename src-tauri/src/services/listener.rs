use crate::services::index::open_index_for_vault;
use crate::services::queue::{ListenerState, ListenerStateUpdate, QueueItemInput, QueueService};
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, INBOX_DIR, LEDGER_FILE,
};
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

pub const DEFAULT_LISTENER_STABLE_WAIT_MS: u64 = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerEventOutcome {
    pub status: String,
    pub relative_path: Option<String>,
    pub queue_item_id: Option<i64>,
    pub dedupe_key: Option<String>,
    pub reason: Option<String>,
}

pub struct ListenerService;

impl ListenerService {
    pub fn status(vault_path: &str) -> ServiceResult<ListenerState> {
        QueueService::get_listener_state(vault_path)
    }

    pub fn mark_running(vault_path: &str, watch_path: String) -> ServiceResult<ListenerState> {
        QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: true,
                status: "running".to_string(),
                watch_path: Some(watch_path),
                last_event: Some("listener started".to_string()),
                last_enqueued_count: Some(0),
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
        )
    }

    pub fn mark_stopped(vault_path: &str) -> ServiceResult<ListenerState> {
        QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: false,
                status: "paused".to_string(),
                watch_path: None,
                last_event: Some("listener stopped".to_string()),
                last_enqueued_count: Some(0),
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
        )
    }

    pub fn mark_error(vault_path: &str, error: &str) -> ServiceResult<ListenerState> {
        QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: false,
                status: "error".to_string(),
                watch_path: None,
                last_event: Some("listener error".to_string()),
                last_enqueued_count: Some(0),
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: Some(error.to_string()),
            },
        )
    }

    pub fn scan_inbox(vault_path: &str, stable_wait_ms: u64) -> ServiceResult<usize> {
        let root = canonical_vault_root(vault_path)?;
        let inbox = root.join(INBOX_DIR);
        if !inbox.exists() {
            return Ok(0);
        }
        let mut enqueued: usize = 0;
        scan_directory(vault_path, &root, &inbox, stable_wait_ms, &mut enqueued)?;
        update_listener_event(vault_path, "manual inbox scan", enqueued as i64, None)?;
        Ok(enqueued)
    }

    pub fn process_path(
        vault_path: &str,
        root: &Path,
        path: PathBuf,
        stable_wait_ms: u64,
    ) -> ServiceResult<ListenerEventOutcome> {
        let candidate = classify_candidate(root, &path)?;
        if let Some(reason) = candidate.skip_reason {
            update_listener_event(vault_path, &reason, 0, None)?;
            return Ok(skipped(reason));
        }

        let relative_path = candidate
            .relative_path
            .ok_or_else(|| ServiceError::InvalidRelativePath(path.to_string_lossy().to_string()))?;
        if !is_file_stable(&path, stable_wait_ms)? {
            let reason = "file is still stabilizing".to_string();
            update_listener_event(vault_path, &reason, 0, None)?;
            return Ok(ListenerEventOutcome {
                status: "waiting".to_string(),
                relative_path: Some(relative_path),
                queue_item_id: None,
                dedupe_key: None,
                reason: Some(reason),
            });
        }

        let signature = file_signature(&path)?;
        let dedupe_key = format!("listener:{relative_path}:{signature}");
        if remember_dedupe(vault_path, &dedupe_key, &relative_path, &signature)? {
            let item = QueueService::enqueue(
                vault_path,
                QueueItemInput {
                    kind: "inbox_file_changed".to_string(),
                    relative_path: relative_path.clone(),
                    dedupe_key: Some(dedupe_key.clone()),
                    payload: Some(json!({
                        "source": "listener",
                        "relativePath": relative_path,
                        "signature": signature
                    })),
                    max_attempts: Some(3),
                    run_after: None,
                },
            )?;
            update_listener_event(vault_path, &format!("enqueued {relative_path}"), 1, None)?;
            Ok(ListenerEventOutcome {
                status: "enqueued".to_string(),
                relative_path: Some(relative_path),
                queue_item_id: Some(item.id),
                dedupe_key: Some(dedupe_key),
                reason: None,
            })
        } else {
            let reason = "duplicate listener event".to_string();
            update_listener_event(vault_path, &reason, 0, None)?;
            Ok(ListenerEventOutcome {
                status: "duplicate".to_string(),
                relative_path: Some(relative_path),
                queue_item_id: None,
                dedupe_key: Some(dedupe_key),
                reason: Some(reason),
            })
        }
    }

    pub fn process_path_after_wait(
        vault_path: &str,
        root: &Path,
        path: PathBuf,
        stable_wait_ms: u64,
    ) -> ServiceResult<ListenerEventOutcome> {
        let first = Self::process_path(vault_path, root, path.clone(), stable_wait_ms)?;
        if first.status != "waiting" || stable_wait_ms == 0 {
            return Ok(first);
        }
        thread::sleep(Duration::from_millis(stable_wait_ms));
        Self::process_path(vault_path, root, path, stable_wait_ms)
    }
}

#[derive(Debug)]
struct Candidate {
    relative_path: Option<String>,
    skip_reason: Option<String>,
}

fn classify_candidate(root: &Path, path: &Path) -> ServiceResult<Candidate> {
    if !path.exists() {
        return Ok(skip("path no longer exists"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Ok(skip("listener ignores directories and non-files"));
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ServiceError::EscapedVault(path.to_string_lossy().to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    let relative = normalize_relative_path(&relative)?;
    if !relative.starts_with(&format!("{INBOX_DIR}/")) {
        return Ok(skip("listener ignores non-inbox paths"));
    }
    if relative == format!("{INBOX_DIR}/{LEDGER_FILE}") {
        return Ok(skip("listener ignores inbox ledger"));
    }
    let segments: Vec<&str> = relative.split('/').collect();
    if segments.iter().any(|segment| is_blocked_segment(segment)) {
        return Ok(skip("listener ignores internal directories"));
    }
    if path
        .file_name()
        .and_then(OsStr::to_str)
        .map(is_temporary_or_hidden_name)
        .unwrap_or(true)
    {
        return Ok(skip("listener ignores hidden or temporary files"));
    }
    if !is_supported_listener_file(path) {
        return Ok(skip("listener ignores unsupported file type"));
    }
    Ok(Candidate {
        relative_path: Some(relative),
        skip_reason: None,
    })
}

fn scan_directory(
    vault_path: &str,
    root: &Path,
    directory: &Path,
    stable_wait_ms: u64,
    enqueued: &mut usize,
) -> ServiceResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            if path
                .file_name()
                .and_then(OsStr::to_str)
                .map(|name| is_blocked_segment(name) || is_temporary_or_hidden_name(name))
                .unwrap_or(true)
            {
                continue;
            }
            scan_directory(vault_path, root, &path, stable_wait_ms, enqueued)?;
            continue;
        }
        let outcome = ListenerService::process_path(vault_path, root, path, stable_wait_ms)?;
        if outcome.status == "enqueued" {
            *enqueued += 1;
        }
    }
    Ok(())
}

fn skip(reason: &str) -> Candidate {
    Candidate {
        relative_path: None,
        skip_reason: Some(reason.to_string()),
    }
}

fn skipped(reason: String) -> ListenerEventOutcome {
    ListenerEventOutcome {
        status: "skipped".to_string(),
        relative_path: None,
        queue_item_id: None,
        dedupe_key: None,
        reason: Some(reason),
    }
}

fn is_blocked_segment(segment: &str) -> bool {
    matches!(
        segment,
        ".thebrain" | ".secrets" | ".git" | "node_modules" | "target" | "dist"
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

fn is_supported_listener_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "md" | "markdown" | "txt" | "mp3" | "wav" | "m4a" | "aac" | "png" | "jpg" | "jpeg"
    )
}

fn is_file_stable(path: &Path, stable_wait_ms: u64) -> ServiceResult<bool> {
    if stable_wait_ms == 0 {
        return Ok(true);
    }
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified()?;
    let elapsed = modified.elapsed().unwrap_or_default();
    Ok(elapsed.as_millis() >= stable_wait_ms as u128)
}

fn file_signature(path: &Path) -> ServiceResult<String> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(format!("{}:{modified}", metadata.len()))
}

fn remember_dedupe(
    vault_path: &str,
    dedupe_key: &str,
    relative_path: &str,
    signature: &str,
) -> ServiceResult<bool> {
    let connection = open_index_for_vault(vault_path)?;
    let existing: Option<String> = connection
        .query_row(
            "SELECT dedupe_key FROM dedupe_records WHERE dedupe_key = ?1",
            params![dedupe_key],
            |row| row.get(0),
        )
        .optional()?;
    let now = Utc::now().to_rfc3339();
    if existing.is_some() {
        connection.execute(
            "UPDATE dedupe_records
             SET last_seen_at = ?1, hit_count = hit_count + 1
             WHERE dedupe_key = ?2",
            params![now, dedupe_key],
        )?;
        return Ok(false);
    }
    connection.execute(
        "INSERT INTO dedupe_records
            (dedupe_key, content_hash, relative_path, first_seen_at, last_seen_at, hit_count)
         VALUES (?1, ?2, ?3, ?4, ?4, 1)",
        params![dedupe_key, signature, relative_path, now],
    )?;
    Ok(true)
}

fn update_listener_event(
    vault_path: &str,
    event: &str,
    enqueued_count: i64,
    error: Option<&str>,
) -> ServiceResult<ListenerState> {
    let current = QueueService::get_listener_state(vault_path)?;
    QueueService::set_listener_state(
        vault_path,
        ListenerStateUpdate {
            enabled: current.enabled,
            status: if error.is_some() {
                "error".to_string()
            } else {
                current.status
            },
            watch_path: current.watch_path,
            last_event: Some(event.to_string()),
            last_enqueued_count: Some(enqueued_count),
            last_event_at: Some(Utc::now().to_rfc3339()),
            last_error: error.map(ToOwned::to_owned),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::queue::QueueService;
    use crate::services::vault::{VaultService, INTERNAL_DIR};
    use crate::services::worker::{WorkerRunOptions, WorkerService};

    #[test]
    fn listener_skips_internal_ledger_temp_hidden_dirs_and_non_inbox_paths() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let cases = [
            root.join("100-School").join("a.md"),
            root.join(INBOX_DIR).join(LEDGER_FILE),
            root.join(INBOX_DIR).join(".hidden.md"),
            root.join(INBOX_DIR).join("upload.tmp"),
            root.join(INBOX_DIR).join("draft.md.crdownload"),
            root.join(INBOX_DIR).join(INTERNAL_DIR).join("index.sqlite"),
            root.join(INBOX_DIR).join(".secrets").join("key.txt"),
        ];
        for path in &cases {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, "x").unwrap();
            let outcome = ListenerService::process_path(
                temp.path().to_str().unwrap(),
                &root,
                path.clone(),
                0,
            )
            .unwrap();
            assert_eq!(outcome.status, "skipped", "{path:?}");
        }
        let dir = root.join(INBOX_DIR).join("folder");
        fs::create_dir_all(&dir).unwrap();
        let outcome =
            ListenerService::process_path(temp.path().to_str().unwrap(), &root, dir, 0).unwrap();
        assert_eq!(outcome.status, "skipped");
    }

    #[test]
    fn listener_enqueues_once_for_duplicate_events_with_same_signature() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file = root.join(INBOX_DIR).join("a.md");
        fs::write(&file, "hello").unwrap();

        let first =
            ListenerService::process_path(temp.path().to_str().unwrap(), &root, file.clone(), 0)
                .unwrap();
        let second =
            ListenerService::process_path(temp.path().to_str().unwrap(), &root, file, 0).unwrap();

        assert_eq!(first.status, "enqueued");
        assert_eq!(second.status, "duplicate");
        assert_eq!(
            QueueService::list_by_status(temp.path().to_str().unwrap(), "pending")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn manual_scan_recurses_and_skips_unsupported_files() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let inbox = temp.path().join(INBOX_DIR);
        fs::create_dir_all(inbox.join("nested").join("deep")).unwrap();
        fs::write(inbox.join("root.md"), "root").unwrap();
        fs::write(inbox.join("nested").join("deep").join("note.txt"), "deep").unwrap();
        fs::write(inbox.join("nested").join("slides.pdf"), "unsupported").unwrap();

        let count = ListenerService::scan_inbox(temp.path().to_str().unwrap(), 0).unwrap();
        let pending =
            QueueService::list_by_status(temp.path().to_str().unwrap(), "pending").unwrap();

        assert_eq!(count, 2);
        assert_eq!(pending.len(), 2);
        assert!(pending
            .iter()
            .any(|item| item.relative_path == format!("{INBOX_DIR}/root.md")));
        assert!(pending
            .iter()
            .any(|item| item.relative_path == format!("{INBOX_DIR}/nested/deep/note.txt")));
    }

    #[test]
    fn listener_waits_for_unstable_file_before_enqueueing() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file = root.join(INBOX_DIR).join("copying.md");
        fs::write(&file, "still copying").unwrap();

        let outcome =
            ListenerService::process_path(temp.path().to_str().unwrap(), &root, file, 60_000)
                .unwrap();

        assert_eq!(outcome.status, "waiting");
        assert_eq!(
            QueueService::list_by_status(temp.path().to_str().unwrap(), "pending")
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn listener_enqueued_item_can_be_consumed_by_worker() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file = root.join(INBOX_DIR).join("worker-source.md");
        fs::write(&file, "hello").unwrap();

        let outcome =
            ListenerService::process_path(temp.path().to_str().unwrap(), &root, file.clone(), 0)
                .unwrap();
        assert_eq!(outcome.status, "enqueued");
        WorkerService::resume(temp.path().to_str().unwrap()).unwrap();
        let run = WorkerService::drain(
            temp.path().to_str().unwrap(),
            WorkerRunOptions {
                max_items: Some(1),
                stable_wait_ms: Some(0),
                force_mock: Some(true),
            },
        )
        .unwrap();

        assert_eq!(run.processed, 1);
        assert_eq!(run.failed, 1);
        assert!(file.exists());
    }
}
