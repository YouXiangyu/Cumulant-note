use crate::services::ai::{MimoExtractInput, MimoProvider};
use crate::services::audit::AuditService;
use crate::services::budget::BudgetService;
use crate::services::conflict_rules::ConflictRuleService;
use crate::services::movement::{MoveRequest, MovementService};
use crate::services::queue::{ListenerState, ListenerStateUpdate, QueueItem, QueueService};
use crate::services::vault::{
    normalize_relative_path, resolve_existing_file, INBOX_DIR, LEDGER_FILE,
};
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const DEFAULT_MAX_ITEMS: usize = 8;
const DEFAULT_STABLE_WAIT_MS: u64 = 1_000;
const DEFAULT_RETRY_DELAY_SECONDS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRunOptions {
    pub max_items: Option<usize>,
    pub stable_wait_ms: Option<u64>,
    pub force_mock: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerItemResult {
    pub queue_id: i64,
    pub relative_path: String,
    pub status: String,
    pub message: String,
    pub movement_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRunResult {
    pub status: String,
    pub processed: usize,
    pub moved: usize,
    pub skipped: usize,
    pub failed: usize,
    pub conflicts: usize,
    pub items: Vec<WorkerItemResult>,
    pub listener: ListenerState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatus {
    pub listener: ListenerState,
    pub pending: usize,
    pub running: usize,
    pub failed: usize,
    pub conflicts: usize,
}

pub struct WorkerService;

impl WorkerService {
    pub fn status(vault_path: &str) -> ServiceResult<WorkerStatus> {
        Ok(WorkerStatus {
            listener: QueueService::get_listener_state(vault_path)?,
            pending: QueueService::list_by_status(vault_path, "pending")?.len(),
            running: QueueService::list_by_status(vault_path, "running")?.len(),
            failed: QueueService::list_by_status(vault_path, "failed")?.len(),
            conflicts: QueueService::list_by_status(vault_path, "conflict")?.len(),
        })
    }

    pub fn pause(vault_path: &str) -> ServiceResult<WorkerStatus> {
        QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: false,
                status: "paused".to_string(),
                watch_path: None,
                last_event: Some("worker paused".to_string()),
                last_enqueued_count: None,
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
        )?;
        Self::status(vault_path)
    }

    pub fn resume(vault_path: &str) -> ServiceResult<WorkerStatus> {
        QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: true,
                status: "idle".to_string(),
                watch_path: None,
                last_event: Some("worker resumed".to_string()),
                last_enqueued_count: None,
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
        )?;
        Self::status(vault_path)
    }

    pub fn drain(vault_path: &str, options: WorkerRunOptions) -> ServiceResult<WorkerRunResult> {
        let max_items = options.max_items.unwrap_or(DEFAULT_MAX_ITEMS).clamp(1, 50);
        let stable_wait_ms = options.stable_wait_ms.unwrap_or(DEFAULT_STABLE_WAIT_MS);
        let force_mock = options.force_mock.unwrap_or(false);

        let listener = QueueService::get_listener_state(vault_path)?;
        if !listener.enabled || listener.status == "paused" {
            AuditService::record(
                vault_path,
                "worker_paused",
                json!({"status": listener.status, "enabled": listener.enabled}),
            )?;
            return Ok(WorkerRunResult {
                status: "paused".to_string(),
                processed: 0,
                moved: 0,
                skipped: 0,
                failed: 0,
                conflicts: 0,
                items: Vec::new(),
                listener,
            });
        }

        QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: true,
                status: "running".to_string(),
                watch_path: None,
                last_event: Some("worker running".to_string()),
                last_enqueued_count: None,
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
        )?;

        let mut run = WorkerRunResult {
            status: "ok".to_string(),
            processed: 0,
            moved: 0,
            skipped: 0,
            failed: 0,
            conflicts: 0,
            items: Vec::new(),
            listener: QueueService::get_listener_state(vault_path)?,
        };

        for _ in 0..max_items {
            let Some(item) = QueueService::claim_next(vault_path)? else {
                break;
            };
            let result = Self::process_item(vault_path, &item, stable_wait_ms, force_mock)?;
            run.processed += 1;
            match result.status.as_str() {
                "moved" => run.moved += 1,
                "skipped" | "pending" => run.skipped += 1,
                "conflict" => run.conflicts += 1,
                _ => run.failed += 1,
            }
            run.items.push(result);
        }

        let final_status = if run.failed > 0 || run.conflicts > 0 {
            "attention"
        } else {
            "idle"
        };
        run.listener = QueueService::set_listener_state(
            vault_path,
            ListenerStateUpdate {
                enabled: true,
                status: final_status.to_string(),
                watch_path: None,
                last_event: Some("worker drain finished".to_string()),
                last_enqueued_count: None,
                last_event_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
        )?;
        run.status = final_status.to_string();
        Ok(run)
    }

    fn process_item(
        vault_path: &str,
        item: &QueueItem,
        stable_wait_ms: u64,
        force_mock: bool,
    ) -> ServiceResult<WorkerItemResult> {
        if !matches!(
            item.kind.as_str(),
            "inbox_file_changed" | "file_changed" | "imported_inbox_file"
        ) {
            let message = format!("unsupported queue item kind: {}", item.kind);
            QueueService::finish(vault_path, item.id, "skipped", Some(&message))?;
            AuditService::record(
                vault_path,
                "worker_skipped",
                json!({"queueId": item.id, "relativePath": item.relative_path, "reason": message}),
            )?;
            return Ok(item_result(item, "skipped", message, None));
        }

        let relative_path = match ensure_worker_source(&item.relative_path) {
            Ok(path) => path,
            Err(error) => {
                let message = error.to_string();
                QueueService::finish(vault_path, item.id, "failed", Some(&message))?;
                AuditService::record(
                    vault_path,
                    "worker_failed",
                    json!({"queueId": item.id, "relativePath": item.relative_path, "reason": message}),
                )?;
                return Ok(item_result(item, "failed", message, None));
            }
        };

        let source_path = match resolve_existing_file(vault_path, &relative_path) {
            Ok(path) => path,
            Err(error) => {
                let message = error.to_string();
                QueueService::finish(vault_path, item.id, "failed", Some(&message))?;
                AuditService::record(
                    vault_path,
                    "worker_failed",
                    json!({"queueId": item.id, "relativePath": relative_path, "reason": message}),
                )?;
                return Ok(item_result(item, "failed", message, None));
            }
        };

        if !is_stable(&source_path, stable_wait_ms)? {
            let message = "file is still stabilizing".to_string();
            QueueService::retry_later(vault_path, item.id, 2, &message)?;
            AuditService::record(
                vault_path,
                "worker_waiting",
                json!({"queueId": item.id, "relativePath": relative_path, "reason": message}),
            )?;
            return Ok(item_result(item, "pending", message, None));
        }

        let budget = BudgetService::status(vault_path)?;
        if !budget.can_run {
            let message = "budget is paused or exhausted".to_string();
            QueueService::finish(vault_path, item.id, "failed", Some(&message))?;
            AuditService::record(
                vault_path,
                "worker_failed",
                json!({"queueId": item.id, "relativePath": relative_path, "reason": message}),
            )?;
            return Ok(item_result(item, "failed", message, None));
        }

        let extraction = MimoProvider::extract_file(
            vault_path,
            MimoExtractInput {
                relative_path: relative_path.clone(),
                force_mock,
            },
        )?;
        if extraction.status != "ok" || extraction.is_mock {
            let message = extraction
                .error
                .clone()
                .unwrap_or_else(|| format!("extract status {}", extraction.status));
            QueueService::finish(vault_path, item.id, "failed", Some(&message))?;
            AuditService::record(
                vault_path,
                "worker_failed",
                json!({
                    "queueId": item.id,
                    "relativePath": relative_path,
                    "stage": "extract",
                    "status": extraction.status,
                    "isMock": extraction.is_mock,
                    "reason": message
                }),
            )?;
            return Ok(item_result(item, "failed", message, None));
        }

        let decision = MimoProvider::organize_decision(
            vault_path,
            &relative_path,
            Some(&extraction.text),
            force_mock,
        )?;
        if decision.status != "ok" || decision.is_mock {
            let message = decision
                .error
                .clone()
                .unwrap_or_else(|| format!("plan status {}", decision.status));
            let queue_status = if message.contains("conflict") || decision.status == "pending" {
                "conflict"
            } else {
                "failed"
            };
            QueueService::finish(vault_path, item.id, queue_status, Some(&message))?;
            let recommendations = conflict_recommendations(
                vault_path,
                &relative_path,
                &decision.target_relative_path,
                &message,
            );
            AuditService::record(
                vault_path,
                if queue_status == "conflict" {
                    "conflict"
                } else {
                    "worker_failed"
                },
                json!({
                    "queueId": item.id,
                    "sourceRelativePath": relative_path,
                    "targetRelativePath": decision.target_relative_path,
                    "status": "open",
                    "stage": "plan",
                    "isMock": decision.is_mock,
                    "reason": message,
                    "recommendedRules": recommendations
                }),
            )?;
            return Ok(item_result(item, queue_status, message, None));
        }

        match MovementService::move_from_inbox(
            vault_path,
            MoveRequest {
                source_relative_path: relative_path.clone(),
                target_relative_path: decision.target_relative_path.clone(),
                reason: Some(format!("worker: {}", decision.reason)),
            },
        ) {
            Ok(log) => {
                QueueService::finish(vault_path, item.id, "completed", None)?;
                AuditService::record(
                    vault_path,
                    "worker_moved",
                    json!({
                        "queueId": item.id,
                        "movementId": log.id,
                        "sourceRelativePath": relative_path,
                        "targetRelativePath": decision.target_relative_path,
                        "confidence": decision.confidence
                    }),
                )?;
                Ok(item_result(
                    item,
                    "moved",
                    "worker moved inbox item".to_string(),
                    Some(log.id),
                ))
            }
            Err(ServiceError::Conflict(message)) => {
                QueueService::finish(vault_path, item.id, "conflict", Some(&message))?;
                let recommendations = conflict_recommendations(
                    vault_path,
                    &relative_path,
                    &decision.target_relative_path,
                    &message,
                );
                AuditService::record(
                    vault_path,
                    "conflict",
                    json!({
                        "queueId": item.id,
                        "sourceRelativePath": relative_path,
                        "targetRelativePath": decision.target_relative_path,
                        "status": "open",
                        "stage": "move",
                        "reason": message,
                        "recommendedRules": recommendations
                    }),
                )?;
                Ok(item_result(item, "conflict", message, None))
            }
            Err(error @ ServiceError::Io(_)) => {
                let message = error.to_string();
                QueueService::retry_later(
                    vault_path,
                    item.id,
                    DEFAULT_RETRY_DELAY_SECONDS,
                    &message,
                )?;
                AuditService::record(
                    vault_path,
                    "worker_retry",
                    json!({"queueId": item.id, "relativePath": relative_path, "reason": message}),
                )?;
                Ok(item_result(item, "pending", message, None))
            }
            Err(error) => {
                let message = error.to_string();
                QueueService::finish(vault_path, item.id, "failed", Some(&message))?;
                AuditService::record(
                    vault_path,
                    "worker_failed",
                    json!({"queueId": item.id, "relativePath": relative_path, "reason": message}),
                )?;
                Ok(item_result(item, "failed", message, None))
            }
        }
    }
}

pub fn ensure_worker_source(relative_path: &str) -> ServiceResult<String> {
    let normalized = normalize_relative_path(relative_path)?;
    let ledger = format!("{INBOX_DIR}/{LEDGER_FILE}");
    if normalized == ledger {
        return Err(ServiceError::InvalidState(
            "worker skips inbox ledger".to_string(),
        ));
    }
    if !normalized.starts_with(&format!("{INBOX_DIR}/")) {
        return Err(ServiceError::InvalidState(
            "worker source must be inside 000-收集箱".to_string(),
        ));
    }
    if normalized
        .split('/')
        .any(|segment| segment == ".thebrain" || segment == ".secrets")
    {
        return Err(ServiceError::InvalidState(
            "worker skips internal or secret paths".to_string(),
        ));
    }
    Ok(normalized)
}

fn is_stable(path: &Path, stable_wait_ms: u64) -> ServiceResult<bool> {
    if stable_wait_ms == 0 {
        return Ok(true);
    }
    let modified = fs::metadata(path)?.modified()?;
    let elapsed = modified.elapsed().unwrap_or_default();
    Ok(elapsed.as_millis() >= stable_wait_ms as u128)
}

fn item_result(
    item: &QueueItem,
    status: &str,
    message: String,
    movement_id: Option<i64>,
) -> WorkerItemResult {
    WorkerItemResult {
        queue_id: item.id,
        relative_path: item.relative_path.clone(),
        status: status.to_string(),
        message,
        movement_id,
    }
}

fn conflict_recommendations(
    vault_path: &str,
    source_relative_path: &str,
    target_relative_path: &str,
    message: &str,
) -> Value {
    match ConflictRuleService::match_rules(
        vault_path,
        source_relative_path.to_string(),
        target_relative_path.to_string(),
        Some(message.to_string()),
    ) {
        Ok(matches) => serde_json::to_value(matches).unwrap_or_else(|_| json!([])),
        Err(_) => json!([]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::queue::{QueueItemInput, QueueService};
    use crate::services::vault::VaultService;
    use serde_json::json;

    #[test]
    fn worker_source_rejects_non_inbox_internal_secret_and_ledger_paths() {
        assert!(ensure_worker_source("100-School/a.md").is_err());
        assert!(ensure_worker_source("000-收集箱/收集箱-已整理.md").is_err());
        assert!(ensure_worker_source("000-收集箱/.secrets/key.txt").is_err());
        assert!(ensure_worker_source("000-收集箱/.thebrain/index.sqlite").is_err());
        assert!(ensure_worker_source("../outside.md").is_err());
        assert_eq!(
            ensure_worker_source("000-收集箱/a.md").unwrap(),
            "000-收集箱/a.md"
        );
    }

    #[test]
    fn paused_worker_does_not_claim_pending_items() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "inbox_file_changed".to_string(),
                relative_path: "000-收集箱/a.md".to_string(),
                dedupe_key: Some("file:000-收集箱/a.md".to_string()),
                payload: Some(json!({"source": "test"})),
                max_attempts: Some(3),
                run_after: None,
            },
        )
        .unwrap();
        WorkerService::pause(temp.path().to_str().unwrap()).unwrap();
        let run = WorkerService::drain(
            temp.path().to_str().unwrap(),
            WorkerRunOptions {
                max_items: Some(1),
                stable_wait_ms: Some(0),
                force_mock: Some(false),
            },
        )
        .unwrap();

        assert_eq!(run.status, "paused");
        assert_eq!(run.processed, 0);
        assert_eq!(
            QueueService::list_by_status(temp.path().to_str().unwrap(), "pending")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn imported_inbox_file_queue_items_are_processed_by_worker() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        std::fs::write(temp.path().join(INBOX_DIR).join("a.md"), "hello").unwrap();
        QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "imported_inbox_file".to_string(),
                relative_path: format!("{INBOX_DIR}/a.md"),
                dedupe_key: Some(format!("import:{INBOX_DIR}/a.md")),
                payload: Some(json!({"source": "import"})),
                max_attempts: Some(3),
                run_after: None,
            },
        )
        .unwrap();
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
        assert!(!run.items[0].message.contains("unsupported queue item kind"));
    }

    #[test]
    fn fallback_ai_failure_does_not_move_or_overwrite_files() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        std::fs::write(temp.path().join(INBOX_DIR).join("a.md"), "hello").unwrap();
        QueueService::enqueue(
            temp.path().to_str().unwrap(),
            QueueItemInput {
                kind: "inbox_file_changed".to_string(),
                relative_path: "000-收集箱/a.md".to_string(),
                dedupe_key: Some("file:000-收集箱/a.md".to_string()),
                payload: Some(json!({"source": "test"})),
                max_attempts: Some(3),
                run_after: None,
            },
        )
        .unwrap();
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
        assert!(temp.path().join(INBOX_DIR).join("a.md").exists());
        assert_eq!(
            QueueService::list_by_status(temp.path().to_str().unwrap(), "failed")
                .unwrap()
                .len(),
            1
        );
    }
}
