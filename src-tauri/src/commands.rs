use crate::services::ai::{
    MimoExtractInput, MimoExtractResult, MimoKeyInput, MimoProvider, MimoStatus, OrganizeDecision,
};
use crate::services::archive_map::{ArchiveMapService, ArchiveMapSnapshot};
use crate::services::audit::{AuditEvent, AuditService};
use crate::services::budget::{BudgetService, BudgetSettingsInput, BudgetStatus};
use crate::services::candidates::{ActionCandidate, CandidateInput, CandidateService};
use crate::services::conflict_rules::{
    ApplyConflictRuleResult, ConflictAnswerInput, ConflictAnswerResult, ConflictDetail,
    ConflictRule, ConflictRuleMatch, ConflictRuleService, ConflictRuleUpdateInput,
    ConflictRuleUpdateResult, RenameSuggestion,
};
use crate::services::importer::{ImportService, InboxImportResult};
use crate::services::inbox::{InboxItem, InboxService};
use crate::services::ledger::{LedgerItem, LedgerService};
use crate::services::listener::{ListenerService, DEFAULT_LISTENER_STABLE_WAIT_MS};
use crate::services::markdown::{MarkdownDocument, MarkdownExport, MarkdownService};
use crate::services::movement::{MoveLog, MoveRequest, MovementService};
use crate::services::queue::{ListenerState, QueueItem, QueueService};
use crate::services::rag::{RagAnswer, RagIndexRun, RagIndexStatus, RagService};
use crate::services::rag_trace::RagTraceRun;
use crate::services::settings::{AppSettings, AppSettingsInput, SettingsService};
use crate::services::sticky::{StickyNote, StickyNoteInput, StickyService};
use crate::services::usage::{UsageService, UsageSummary};
use crate::services::vault::{
    canonical_vault_root, VaultInitResult, VaultService, VaultTreeNode, INBOX_DIR,
};
use crate::services::worker::{WorkerRunOptions, WorkerRunResult, WorkerService, WorkerStatus};
use chrono::Utc;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Default)]
pub struct WatcherRegistry {
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
}

#[derive(Default)]
pub struct ResidentWorkerRegistry {
    workers: Mutex<HashMap<String, ResidentWorkerHandle>>,
}

#[derive(Clone)]
struct ResidentWorkerHandle {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<ResidentWorkerStatus>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentWorkerOptions {
    pub interval_ms: Option<u64>,
    pub max_items_per_tick: Option<usize>,
    pub stable_wait_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedResidentWorkerOptions {
    interval_ms: u64,
    max_items_per_tick: usize,
    stable_wait_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentWorkerStatus {
    pub running: bool,
    pub vault_path: String,
    pub interval_ms: u64,
    pub max_items_per_tick: usize,
    pub stable_wait_ms: u64,
    pub started_at: Option<String>,
    pub last_tick_at: Option<String>,
    pub last_status: String,
    pub last_processed: usize,
    pub last_moved: usize,
    pub last_failed: usize,
    pub last_conflicts: usize,
    pub last_error: Option<String>,
    pub updated_at: String,
}

const RESIDENT_WORKER_DEFAULT_INTERVAL_MS: u64 = 5_000;
const RESIDENT_WORKER_MIN_INTERVAL_MS: u64 = 1_000;
const RESIDENT_WORKER_MAX_INTERVAL_MS: u64 = 60_000;
const RESIDENT_WORKER_DEFAULT_STABLE_WAIT_MS: u64 = 1_000;
const RESIDENT_WORKER_MIN_STABLE_WAIT_MS: u64 = 250;
const RESIDENT_WORKER_MAX_STABLE_WAIT_MS: u64 = 30_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub listener: ListenerState,
    pub pending: Vec<QueueItem>,
    pub running: Vec<QueueItem>,
    pub failed: Vec<QueueItem>,
    pub conflicts: Vec<QueueItem>,
    pub completed: Vec<QueueItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlanResult {
    pub extraction: MimoExtractResult,
    pub plan: OrganizeDecision,
    pub candidates: Vec<ActionCandidate>,
    pub budget: BudgetStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchOperationError {
    pub id: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchQueueResult {
    pub requested: usize,
    pub succeeded: Vec<QueueItem>,
    pub failed: Vec<BatchOperationError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRollbackResult {
    pub requested: usize,
    pub rolled_back: Vec<MoveLog>,
    pub failed: Vec<BatchOperationError>,
}

fn into_command_result<T>(result: crate::services::ServiceResult<T>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}

fn sanitize_batch_ids(ids: Vec<i64>) -> Result<Vec<i64>, String> {
    let mut cleaned = Vec::new();
    for id in ids {
        if id <= 0 {
            return Err(format!("invalid batch id: {id}"));
        }
        if !cleaned.contains(&id) {
            cleaned.push(id);
        }
    }
    if cleaned.is_empty() {
        return Err("batch ids must not be empty".to_string());
    }
    if cleaned.len() > 50 {
        return Err("batch operations are limited to 50 items".to_string());
    }
    Ok(cleaned)
}

fn resolve_resident_worker_options(
    options: Option<ResidentWorkerOptions>,
) -> ResolvedResidentWorkerOptions {
    let interval_ms = options
        .as_ref()
        .and_then(|options| options.interval_ms)
        .unwrap_or(RESIDENT_WORKER_DEFAULT_INTERVAL_MS)
        .clamp(
            RESIDENT_WORKER_MIN_INTERVAL_MS,
            RESIDENT_WORKER_MAX_INTERVAL_MS,
        );
    let stable_wait_ms = options
        .as_ref()
        .and_then(|options| options.stable_wait_ms)
        .unwrap_or(RESIDENT_WORKER_DEFAULT_STABLE_WAIT_MS)
        .clamp(
            RESIDENT_WORKER_MIN_STABLE_WAIT_MS,
            RESIDENT_WORKER_MAX_STABLE_WAIT_MS,
        );

    ResolvedResidentWorkerOptions {
        interval_ms,
        max_items_per_tick: 1,
        stable_wait_ms,
    }
}

fn default_resident_worker_status(
    vault_path: String,
    options: ResolvedResidentWorkerOptions,
) -> ResidentWorkerStatus {
    ResidentWorkerStatus {
        running: false,
        vault_path,
        interval_ms: options.interval_ms,
        max_items_per_tick: options.max_items_per_tick,
        stable_wait_ms: options.stable_wait_ms,
        started_at: None,
        last_tick_at: None,
        last_status: "stopped".to_string(),
        last_processed: 0,
        last_moved: 0,
        last_failed: 0,
        last_conflicts: 0,
        last_error: None,
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn clone_resident_status(handle: &ResidentWorkerHandle) -> Result<ResidentWorkerStatus, String> {
    let mut status = handle
        .status
        .lock()
        .map_err(|_| "resident worker status poisoned".to_string())?
        .clone();
    if handle.stop.load(Ordering::SeqCst) {
        status.running = false;
    }
    Ok(status)
}

fn update_resident_status<F>(status: &Arc<Mutex<ResidentWorkerStatus>>, update: F)
where
    F: FnOnce(&mut ResidentWorkerStatus),
{
    if let Ok(mut status) = status.lock() {
        update(&mut status);
        status.updated_at = Utc::now().to_rfc3339();
    }
}

fn sleep_resident_interval(stop: &AtomicBool, interval_ms: u64) {
    let mut remaining = interval_ms;
    while remaining > 0 && !stop.load(Ordering::SeqCst) {
        let chunk = remaining.min(250);
        thread::sleep(Duration::from_millis(chunk));
        remaining -= chunk;
    }
}

fn run_resident_worker_loop(
    vault_path: String,
    options: ResolvedResidentWorkerOptions,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<ResidentWorkerStatus>>,
) {
    while !stop.load(Ordering::SeqCst) {
        let tick_at = Utc::now().to_rfc3339();
        update_resident_status(&status, |status| {
            status.last_tick_at = Some(tick_at);
            status.last_status = "running".to_string();
            status.last_error = None;
        });

        match WorkerService::drain(
            &vault_path,
            WorkerRunOptions {
                max_items: Some(options.max_items_per_tick),
                stable_wait_ms: Some(options.stable_wait_ms),
                force_mock: Some(false),
            },
        ) {
            Ok(result) => update_resident_status(&status, |status| {
                status.last_status = result.status;
                status.last_processed = result.processed;
                status.last_moved = result.moved;
                status.last_failed = result.failed;
                status.last_conflicts = result.conflicts;
                status.last_error = None;
            }),
            Err(error) => update_resident_status(&status, |status| {
                status.last_status = "error".to_string();
                status.last_error = Some(error.to_string());
            }),
        }

        sleep_resident_interval(&stop, options.interval_ms);
    }

    update_resident_status(&status, |status| {
        status.running = false;
        status.last_status = "stopped".to_string();
    });
}

#[tauri::command]
pub fn select_vault(app: AppHandle) -> Result<Option<String>, String> {
    let selected = app.dialog().file().blocking_pick_folder();
    Ok(selected.map(|path| path.to_string()))
}

#[tauri::command]
pub fn init_vault(vault_path: String) -> Result<VaultInitResult, String> {
    into_command_result(VaultService::init(&vault_path))
}

#[tauri::command]
pub fn list_vault_tree(vault_path: String) -> Result<Vec<VaultTreeNode>, String> {
    into_command_result(VaultService::list_tree(&vault_path))
}

#[tauri::command]
pub fn read_markdown(
    vault_path: String,
    relative_path: String,
) -> Result<MarkdownDocument, String> {
    into_command_result(MarkdownService::read(&vault_path, &relative_path))
}

#[tauri::command]
pub fn save_markdown(
    vault_path: String,
    relative_path: String,
    content: String,
    frontmatter: Option<Value>,
) -> Result<MarkdownDocument, String> {
    into_command_result(MarkdownService::save(
        &vault_path,
        &relative_path,
        &content,
        frontmatter,
    ))
}

#[tauri::command]
pub fn export_markdown(
    vault_path: String,
    relative_path: String,
) -> Result<MarkdownExport, String> {
    into_command_result(MarkdownService::export(&vault_path, &relative_path))
}

#[tauri::command]
pub fn list_inbox(vault_path: String) -> Result<Vec<InboxItem>, String> {
    into_command_result(InboxService::list(&vault_path))
}

#[tauri::command]
pub fn parse_inbox_ledger(vault_path: String) -> Result<Vec<LedgerItem>, String> {
    into_command_result(LedgerService::parse_inbox_ledger(&vault_path))
}

#[tauri::command]
pub fn import_to_inbox(
    vault_path: String,
    source_paths: Vec<String>,
    mode: Option<String>,
) -> Result<Vec<InboxImportResult>, String> {
    into_command_result(ImportService::import_to_inbox(
        &vault_path,
        source_paths,
        mode,
    ))
}

#[tauri::command]
pub fn get_archive_map(vault_path: String) -> Result<ArchiveMapSnapshot, String> {
    into_command_result(ArchiveMapService::latest_or_rebuild(&vault_path))
}

#[tauri::command]
pub fn rebuild_archive_map(vault_path: String) -> Result<ArchiveMapSnapshot, String> {
    into_command_result(ArchiveMapService::rebuild(&vault_path))
}

#[tauri::command]
pub fn get_ai_usage(vault_path: String) -> Result<UsageSummary, String> {
    into_command_result(UsageService::summary(&vault_path))
}

#[tauri::command]
pub fn rebuild_rag_index(vault_path: String) -> Result<RagIndexRun, String> {
    into_command_result(RagService::rebuild_index(&vault_path))
}

#[tauri::command]
pub fn get_rag_index_status(vault_path: String) -> Result<RagIndexStatus, String> {
    into_command_result(RagService::status(&vault_path))
}

#[tauri::command]
pub async fn ask_rag(
    vault_path: String,
    question: String,
    top_k: Option<usize>,
    conversation_id: Option<i64>,
) -> Result<RagAnswer, String> {
    let task = tauri::async_runtime::spawn_blocking(move || {
        RagService::ask(&vault_path, &question, top_k, conversation_id)
    });
    match task.await {
        Ok(result) => into_command_result(result),
        Err(error) => Err(format!("RAG task failed: {error}")),
    }
}

#[tauri::command]
pub fn get_latest_rag_trace(vault_path: String) -> Result<Option<RagTraceRun>, String> {
    into_command_result(RagService::latest_trace(&vault_path))
}

#[tauri::command]
pub fn get_app_settings(vault_path: String) -> Result<AppSettings, String> {
    into_command_result(SettingsService::get(&vault_path))
}

#[tauri::command]
pub fn save_app_settings(
    vault_path: String,
    settings: AppSettingsInput,
) -> Result<AppSettings, String> {
    into_command_result(SettingsService::save(&vault_path, settings))
}

#[tauri::command]
pub fn get_queue_status(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(queue_status(&vault_path))
}

#[tauri::command]
pub fn get_inbox_listener_status(vault_path: String) -> Result<ListenerState, String> {
    into_command_result(ListenerService::status(&vault_path))
}

#[tauri::command]
pub fn get_worker_status(vault_path: String) -> Result<WorkerStatus, String> {
    into_command_result(WorkerService::status(&vault_path))
}

#[tauri::command]
pub fn get_resident_worker_status(
    vault_path: String,
    registry: tauri::State<'_, ResidentWorkerRegistry>,
) -> Result<ResidentWorkerStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let key = root.to_string_lossy().to_string();
    let options = resolve_resident_worker_options(None);
    let workers = registry
        .workers
        .lock()
        .map_err(|_| "resident worker registry poisoned".to_string())?;
    match workers.get(&key) {
        Some(handle) => clone_resident_status(handle),
        None => Ok(default_resident_worker_status(key, options)),
    }
}

#[tauri::command]
pub fn start_resident_worker(
    vault_path: String,
    options: Option<ResidentWorkerOptions>,
    registry: tauri::State<'_, ResidentWorkerRegistry>,
) -> Result<ResidentWorkerStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let key = root.to_string_lossy().to_string();
    let resolved = resolve_resident_worker_options(options);

    {
        let mut workers = registry
            .workers
            .lock()
            .map_err(|_| "resident worker registry poisoned".to_string())?;
        if let Some(handle) = workers.get(&key) {
            let status = clone_resident_status(handle)?;
            if status.running {
                return Ok(status);
            }
        }
        workers.remove(&key);
    }

    into_command_result(WorkerService::resume(&key))?;
    let _ = AuditService::record(
        &key,
        "resident_worker_started",
        json!({
            "intervalMs": resolved.interval_ms,
            "maxItemsPerTick": resolved.max_items_per_tick,
            "stableWaitMs": resolved.stable_wait_ms
        }),
    );

    let now = Utc::now().to_rfc3339();
    let status = Arc::new(Mutex::new(ResidentWorkerStatus {
        running: true,
        vault_path: key.clone(),
        interval_ms: resolved.interval_ms,
        max_items_per_tick: resolved.max_items_per_tick,
        stable_wait_ms: resolved.stable_wait_ms,
        started_at: Some(now.clone()),
        last_tick_at: None,
        last_status: "starting".to_string(),
        last_processed: 0,
        last_moved: 0,
        last_failed: 0,
        last_conflicts: 0,
        last_error: None,
        updated_at: now,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = ResidentWorkerHandle {
        stop: Arc::clone(&stop),
        status: Arc::clone(&status),
    };

    thread::spawn({
        let vault_path = key.clone();
        move || run_resident_worker_loop(vault_path, resolved, stop, status)
    });

    registry
        .workers
        .lock()
        .map_err(|_| "resident worker registry poisoned".to_string())?
        .insert(key, handle.clone());
    clone_resident_status(&handle)
}

#[tauri::command]
pub fn stop_resident_worker(
    vault_path: String,
    registry: tauri::State<'_, ResidentWorkerRegistry>,
) -> Result<ResidentWorkerStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let key = root.to_string_lossy().to_string();
    let options = resolve_resident_worker_options(None);
    let handle = registry
        .workers
        .lock()
        .map_err(|_| "resident worker registry poisoned".to_string())?
        .get(&key)
        .cloned();

    match handle {
        Some(handle) => {
            handle.stop.store(true, Ordering::SeqCst);
            update_resident_status(&handle.status, |status| {
                status.running = false;
                status.last_status = "stopping".to_string();
            });
            let _ = AuditService::record(&key, "resident_worker_stopped", json!({}));
            clone_resident_status(&handle)
        }
        None => Ok(default_resident_worker_status(key, options)),
    }
}

#[tauri::command]
pub fn run_inbox_worker(
    vault_path: String,
    options: Option<WorkerRunOptions>,
) -> Result<WorkerRunResult, String> {
    into_command_result(WorkerService::drain(
        &vault_path,
        options.unwrap_or(WorkerRunOptions {
            max_items: None,
            stable_wait_ms: None,
            force_mock: None,
        }),
    ))
}

#[tauri::command]
pub fn pause_inbox_worker(vault_path: String) -> Result<WorkerStatus, String> {
    into_command_result(WorkerService::pause(&vault_path))
}

#[tauri::command]
pub fn resume_inbox_worker(vault_path: String) -> Result<WorkerStatus, String> {
    into_command_result(WorkerService::resume(&vault_path))
}

#[tauri::command]
pub fn scan_inbox_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(
        ListenerService::scan_inbox(&vault_path, DEFAULT_LISTENER_STABLE_WAIT_MS)
            .and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn pause_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(
        ListenerService::mark_stopped(&vault_path).and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn resume_queue(vault_path: String) -> Result<QueueStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let inbox = root.join(INBOX_DIR);
    into_command_result(
        ListenerService::mark_running(&vault_path, inbox.to_string_lossy().to_string())
            .and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn retry_queue_item(vault_path: String, queue_id: i64) -> Result<QueueItem, String> {
    into_command_result(QueueService::retry_item(&vault_path, queue_id))
}

#[tauri::command]
pub fn skip_queue_item(
    vault_path: String,
    queue_id: i64,
    reason: Option<String>,
) -> Result<QueueItem, String> {
    into_command_result(QueueService::skip_item(
        &vault_path,
        queue_id,
        reason.as_deref(),
    ))
}

#[tauri::command]
pub fn retry_queue_items(
    vault_path: String,
    queue_ids: Vec<i64>,
) -> Result<BatchQueueResult, String> {
    let ids = sanitize_batch_ids(queue_ids)?;
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for id in &ids {
        match QueueService::retry_item(&vault_path, *id) {
            Ok(item) => succeeded.push(item),
            Err(error) => failed.push(BatchOperationError {
                id: *id,
                message: error.to_string(),
            }),
        }
    }
    let result = BatchQueueResult {
        requested: ids.len(),
        succeeded,
        failed,
    };
    let _ = AuditService::record(
        &vault_path,
        "queue_batch_retry",
        json!({
            "requested": result.requested,
            "succeededIds": result.succeeded.iter().map(|item| item.id).collect::<Vec<_>>(),
            "failed": &result.failed
        }),
    );
    Ok(result)
}

#[tauri::command]
pub fn skip_queue_items(
    vault_path: String,
    queue_ids: Vec<i64>,
    reason: Option<String>,
) -> Result<BatchQueueResult, String> {
    let ids = sanitize_batch_ids(queue_ids)?;
    let reason = reason.unwrap_or_else(|| "batch skipped from inbox recovery panel".to_string());
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for id in &ids {
        match QueueService::skip_item(&vault_path, *id, Some(&reason)) {
            Ok(item) => succeeded.push(item),
            Err(error) => failed.push(BatchOperationError {
                id: *id,
                message: error.to_string(),
            }),
        }
    }
    let result = BatchQueueResult {
        requested: ids.len(),
        succeeded,
        failed,
    };
    let _ = AuditService::record(
        &vault_path,
        "queue_batch_skip",
        json!({
            "requested": result.requested,
            "reason": reason,
            "succeededIds": result.succeeded.iter().map(|item| item.id).collect::<Vec<_>>(),
            "failed": &result.failed
        }),
    );
    Ok(result)
}

#[tauri::command]
pub fn start_inbox_watcher(
    vault_path: String,
    registry: tauri::State<'_, WatcherRegistry>,
) -> Result<QueueStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    let inbox = root.join(INBOX_DIR);
    let watch_key = root.to_string_lossy().to_string();
    let vault_for_callback = vault_path.clone();
    let root_for_callback = root.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            for path in event.paths {
                let _ = ListenerService::process_path_after_wait(
                    &vault_for_callback,
                    &root_for_callback,
                    path,
                    DEFAULT_LISTENER_STABLE_WAIT_MS,
                );
            }
        }
    })
    .map_err(|error| error.to_string())?;
    watcher
        .watch(&inbox, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    registry
        .watchers
        .lock()
        .map_err(|_| "watcher registry poisoned".to_string())?
        .insert(watch_key, watcher);
    into_command_result(
        ListenerService::mark_running(&vault_path, inbox.to_string_lossy().to_string())
            .and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn stop_inbox_watcher(
    vault_path: String,
    registry: tauri::State<'_, WatcherRegistry>,
) -> Result<QueueStatus, String> {
    let root = canonical_vault_root(&vault_path).map_err(|error| error.to_string())?;
    registry
        .watchers
        .lock()
        .map_err(|_| "watcher registry poisoned".to_string())?
        .remove(&root.to_string_lossy().to_string());
    into_command_result(
        ListenerService::mark_stopped(&vault_path).and_then(|_| queue_status(&vault_path)),
    )
}

#[tauri::command]
pub fn get_budget_status(vault_path: String) -> Result<BudgetStatus, String> {
    into_command_result(BudgetService::status(&vault_path))
}

#[tauri::command]
pub fn save_budget_settings(vault_path: String, settings: Value) -> Result<BudgetStatus, String> {
    let input = BudgetSettingsInput {
        scope: None,
        monthly_limit_cents: settings
            .get("monthlyLimitCents")
            .or_else(|| settings.get("monthly_limit_cents"))
            .and_then(Value::as_i64),
        daily_limit_cents: settings
            .get("hardStopCents")
            .or_else(|| settings.get("dailyLimitCents"))
            .and_then(Value::as_i64),
        paused: settings
            .get("paused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        retry_limit: settings
            .get("retryLimit")
            .and_then(Value::as_i64)
            .unwrap_or(3),
        cooldown_seconds: settings
            .get("cooldownSeconds")
            .or_else(|| settings.get("cooldown_seconds"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    };
    into_command_result(
        BudgetService::save_settings(&vault_path, input)
            .and_then(|_| BudgetService::status(&vault_path)),
    )
}

#[tauri::command]
pub fn get_mimo_status(vault_path: String) -> Result<MimoStatus, String> {
    into_command_result(MimoProvider::status(&vault_path))
}

#[tauri::command]
pub fn save_mimo_api_key(vault_path: String, input: MimoKeyInput) -> Result<MimoStatus, String> {
    into_command_result(MimoProvider::save_api_key(&vault_path, input))
}

#[tauri::command]
pub fn extract_with_mimo(
    vault_path: String,
    input: MimoExtractInput,
) -> Result<MimoExtractResult, String> {
    into_command_result(MimoProvider::extract_file(&vault_path, input))
}

#[tauri::command]
pub fn plan_inbox_item(
    vault_path: String,
    source_relative_path: String,
    force_mock: Option<bool>,
) -> Result<InboxPlanResult, String> {
    let extraction = MimoProvider::extract_file(
        &vault_path,
        MimoExtractInput {
            relative_path: source_relative_path.clone(),
            force_mock: force_mock.unwrap_or(false),
        },
    )
    .map_err(|error| error.to_string())?;
    let plan = MimoProvider::organize_decision(
        &vault_path,
        &source_relative_path,
        Some(&extraction.text),
        force_mock.unwrap_or(false),
    )
    .map_err(|error| error.to_string())?;
    let candidates =
        create_candidates_from_decision(&vault_path, &plan).map_err(|error| error.to_string())?;
    let budget = BudgetService::status(&vault_path).map_err(|error| error.to_string())?;
    Ok(InboxPlanResult {
        extraction,
        plan,
        candidates,
        budget,
    })
}

#[tauri::command]
pub fn plan_ai_organize(
    vault_path: String,
    source_relative_path: Option<String>,
    extracted_text: Option<String>,
    force_mock: Option<bool>,
) -> Result<OrganizeDecision, String> {
    into_command_result(MimoProvider::organize_decision(
        &vault_path,
        source_relative_path
            .as_deref()
            .unwrap_or("000-收集箱/示例笔记.md"),
        extracted_text.as_deref(),
        force_mock.unwrap_or(false),
    ))
}

#[tauri::command]
pub fn run_ai_organize(
    vault_path: String,
    plan_id: Option<String>,
    source_relative_path: Option<String>,
    target_relative_path: Option<String>,
    reason: Option<String>,
) -> Result<Value, String> {
    if let (Some(source), Some(target)) = (source_relative_path, target_relative_path) {
        let log = MovementService::move_from_inbox(
            &vault_path,
            MoveRequest {
                source_relative_path: source,
                target_relative_path: target,
                reason,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(json!({
            "moved": 1,
            "skipped": 0,
            "conflicts": 0,
            "auditId": log.id.to_string(),
            "movement": &log,
            "message": "AI 整理移动已执行"
        }))
    } else {
        Ok(json!({
            "moved": 0,
            "skipped": 0,
            "conflicts": 0,
            "auditId": plan_id.unwrap_or_else(|| "plan-only".to_string()),
            "message": "Plan recorded; select a concrete target before moving"
        }))
    }
}

#[tauri::command]
pub fn move_inbox_item(
    vault_path: String,
    source_relative_path: String,
    target_relative_path: String,
    reason: Option<String>,
) -> Result<MoveLog, String> {
    into_command_result(MovementService::move_from_inbox(
        &vault_path,
        MoveRequest {
            source_relative_path,
            target_relative_path,
            reason,
        },
    ))
}

#[tauri::command]
pub fn rollback_move(
    vault_path: String,
    movement_id: Option<i64>,
    audit_id: Option<String>,
) -> Result<MoveLog, String> {
    let movement_id = movement_id
        .or_else(|| audit_id.and_then(|id| id.parse::<i64>().ok()))
        .ok_or_else(|| "movement_id or audit_id is required".to_string())?;
    into_command_result(MovementService::rollback(&vault_path, movement_id))
}

#[tauri::command]
pub fn list_move_logs(vault_path: String) -> Result<Vec<MoveLog>, String> {
    into_command_result(MovementService::list(&vault_path))
}

#[tauri::command]
pub fn rollback_moves(
    vault_path: String,
    movement_ids: Vec<i64>,
) -> Result<BatchRollbackResult, String> {
    let ids = sanitize_batch_ids(movement_ids)?;
    let mut rolled_back = Vec::new();
    let mut failed = Vec::new();
    for id in &ids {
        match MovementService::rollback(&vault_path, *id) {
            Ok(log) => rolled_back.push(log),
            Err(error) => failed.push(BatchOperationError {
                id: *id,
                message: error.to_string(),
            }),
        }
    }
    let result = BatchRollbackResult {
        requested: ids.len(),
        rolled_back,
        failed,
    };
    let _ = AuditService::record(
        &vault_path,
        "movement_batch_rollback",
        json!({
            "requested": result.requested,
            "rolledBackIds": result.rolled_back.iter().map(|log| log.id).collect::<Vec<_>>(),
            "failed": &result.failed
        }),
    );
    Ok(result)
}

#[tauri::command]
pub fn list_audit_events(
    vault_path: String,
    event_type: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<AuditEvent>, String> {
    into_command_result(AuditService::list(
        &vault_path,
        event_type.as_deref(),
        limit,
    ))
}

#[tauri::command]
pub fn list_todo_schedule_candidates(vault_path: String) -> Result<Vec<ActionCandidate>, String> {
    into_command_result(CandidateService::list_pending(&vault_path))
}

#[tauri::command]
pub fn create_todo_schedule_candidate(
    vault_path: String,
    candidate: CandidateInput,
) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::create(&vault_path, candidate))
}

#[tauri::command]
pub fn confirm_todo_schedule_candidate(
    vault_path: String,
    candidate_id: i64,
) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::confirm(&vault_path, candidate_id))
}

#[tauri::command]
pub fn dismiss_todo_schedule_candidate(
    vault_path: String,
    candidate_id: i64,
) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::reject(&vault_path, candidate_id))
}

#[tauri::command]
pub fn list_sticky_notes(
    vault_path: String,
    include_archived: Option<bool>,
) -> Result<Vec<StickyNote>, String> {
    into_command_result(StickyService::list(
        &vault_path,
        include_archived.unwrap_or(false),
    ))
}

#[tauri::command]
pub fn save_sticky_note(vault_path: String, note: StickyNoteInput) -> Result<StickyNote, String> {
    into_command_result(StickyService::save(&vault_path, note))
}

#[tauri::command]
pub fn delete_sticky_note(vault_path: String, note_id: i64) -> Result<(), String> {
    into_command_result(StickyService::delete(&vault_path, note_id))
}

#[tauri::command]
pub fn autosave_sticky_note(vault_path: String, note_id: i64) -> Result<String, String> {
    into_command_result(StickyService::autosave_to_inbox(&vault_path, note_id))
}

#[tauri::command]
pub async fn prewarm_sticky_windows(app: AppHandle, count: i64) -> Result<Vec<String>, String> {
    let mut labels = Vec::new();
    for index in 0..count.clamp(0, 4) {
        let label = format!("sticky-prewarm-{index}");
        if app.get_webview_window(&label).is_none() {
            WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
                .title("TheBrain Sticky")
                .inner_size(320.0, 240.0)
                .visible(false)
                .build()
                .map_err(|error| error.to_string())?;
        }
        labels.push(label);
    }
    Ok(labels)
}

#[tauri::command]
pub fn register_global_shortcut(app: AppHandle, shortcut: String) -> Result<String, String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| error.to_string())?;
    let registered = shortcut.clone();
    let parsed: Shortcut = shortcut
        .parse()
        .map_err(|error| format!("invalid shortcut: {error}"))?;
    app.global_shortcut()
        .on_shortcut(parsed, move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ =
                        window.emit("thebrain-sticky-shortcut", json!({"shortcut": registered}));
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(shortcut)
}

#[tauri::command]
pub fn list_conflicts(vault_path: String) -> Result<Vec<ConflictDetail>, String> {
    into_command_result(ConflictRuleService::list_open_conflicts(&vault_path))
}

#[tauri::command]
pub fn get_conflict_detail(
    vault_path: String,
    conflict_id: String,
) -> Result<ConflictDetail, String> {
    into_command_result(ConflictRuleService::get_conflict(&vault_path, &conflict_id))
}

#[tauri::command]
pub fn submit_conflict_answer(
    vault_path: String,
    input: ConflictAnswerInput,
) -> Result<ConflictAnswerResult, String> {
    into_command_result(ConflictRuleService::submit_answer(&vault_path, input))
}

#[tauri::command]
pub fn match_conflict_rules(
    vault_path: String,
    source_relative_path: String,
    target_relative_path: String,
    message: Option<String>,
) -> Result<Vec<ConflictRuleMatch>, String> {
    into_command_result(ConflictRuleService::match_rules(
        &vault_path,
        source_relative_path,
        target_relative_path,
        message,
    ))
}

#[tauri::command]
pub fn suggest_conflict_rename_targets(
    vault_path: String,
    target_relative_path: String,
    limit: Option<usize>,
) -> Result<Vec<RenameSuggestion>, String> {
    into_command_result(ConflictRuleService::suggest_rename_targets(
        &vault_path,
        target_relative_path,
        limit,
    ))
}

#[tauri::command]
pub fn list_conflict_rules(
    vault_path: String,
    include_disabled: Option<bool>,
) -> Result<Vec<ConflictRule>, String> {
    into_command_result(ConflictRuleService::list_rules(
        &vault_path,
        include_disabled.unwrap_or(false),
    ))
}

#[tauri::command]
pub fn set_conflict_rule_status(
    vault_path: String,
    rule_id: i64,
    status: String,
) -> Result<ConflictRuleUpdateResult, String> {
    into_command_result(ConflictRuleService::set_rule_status(
        &vault_path,
        rule_id,
        status,
    ))
}

#[tauri::command]
pub fn update_conflict_rule(
    vault_path: String,
    input: ConflictRuleUpdateInput,
) -> Result<ConflictRuleUpdateResult, String> {
    into_command_result(ConflictRuleService::update_rule(&vault_path, input))
}

#[tauri::command]
pub fn apply_conflict_rule(
    vault_path: String,
    conflict_id: String,
    rule_id: i64,
) -> Result<ApplyConflictRuleResult, String> {
    into_command_result(ConflictRuleService::apply_rule(
        &vault_path,
        conflict_id,
        rule_id,
    ))
}

#[tauri::command]
pub fn resolve_conflict(
    vault_path: String,
    conflict_id: String,
    action: String,
) -> Result<Value, String> {
    let parsed_id = conflict_id
        .parse::<i64>()
        .map_err(|_| "conflict_id must be an audit event id".to_string())?;
    let event = AuditService::record(
        &vault_path,
        "conflict_resolved",
        json!({
            "conflictId": parsed_id,
            "action": action,
            "status": "resolved"
        }),
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "resolved",
        "conflictId": parsed_id,
        "auditId": event.id
    }))
}

fn create_candidates_from_decision(
    vault_path: &str,
    decision: &OrganizeDecision,
) -> crate::services::ServiceResult<Vec<ActionCandidate>> {
    let mut created = Vec::new();
    for item in &decision.todo_candidates {
        created.push(CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: Some(decision.source_relative_path.clone()),
                title: candidate_title(item, "Untitled todo"),
                payload: Some(candidate_payload(item, decision)),
            },
        )?);
    }
    for item in &decision.schedule_candidates {
        created.push(CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "schedule".to_string(),
                source_relative_path: Some(decision.source_relative_path.clone()),
                title: candidate_title(item, "Untitled schedule item"),
                payload: Some(candidate_payload(item, decision)),
            },
        )?);
    }
    Ok(created)
}

fn candidate_title(item: &Value, fallback: &str) -> String {
    item.get("title")
        .or_else(|| item.get("text"))
        .or_else(|| item.get("summary"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn candidate_payload(item: &Value, decision: &OrganizeDecision) -> Value {
    let mut payload = match item {
        Value::Object(object) => object.clone(),
        other => {
            let mut object = Map::new();
            object.insert("value".to_string(), other.clone());
            object
        }
    };
    payload.insert(
        "sourceRelativePath".to_string(),
        Value::String(decision.source_relative_path.clone()),
    );
    payload.insert(
        "targetRelativePath".to_string(),
        Value::String(decision.target_relative_path.clone()),
    );
    payload.insert(
        "planStatus".to_string(),
        Value::String(decision.status.clone()),
    );
    payload.insert(
        "planProvider".to_string(),
        Value::String(decision.provider.clone()),
    );
    Value::Object(payload)
}

fn queue_status(vault_path: &str) -> crate::services::ServiceResult<QueueStatus> {
    Ok(QueueStatus {
        listener: QueueService::get_listener_state(vault_path)?,
        pending: QueueService::list_by_status(vault_path, "pending")?,
        running: QueueService::list_by_status(vault_path, "running")?,
        failed: QueueService::list_by_status(vault_path, "failed")?,
        conflicts: QueueService::list_by_status(vault_path, "conflict")?,
        completed: QueueService::list_by_status(vault_path, "completed")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_worker_options_stay_conservative() {
        let options = resolve_resident_worker_options(Some(ResidentWorkerOptions {
            interval_ms: Some(100),
            max_items_per_tick: Some(99),
            stable_wait_ms: Some(10),
        }));

        assert_eq!(options.interval_ms, RESIDENT_WORKER_MIN_INTERVAL_MS);
        assert_eq!(options.max_items_per_tick, 1);
        assert_eq!(options.stable_wait_ms, RESIDENT_WORKER_MIN_STABLE_WAIT_MS);
    }

    #[test]
    fn batch_ids_are_deduped_and_limited() {
        assert_eq!(sanitize_batch_ids(vec![3, 3, 2]).unwrap(), vec![3, 2]);
        assert!(sanitize_batch_ids(vec![]).is_err());
        assert!(sanitize_batch_ids(vec![0]).is_err());
        assert!(sanitize_batch_ids((1..=51).collect()).is_err());
    }
}
