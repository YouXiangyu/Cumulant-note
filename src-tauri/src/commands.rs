use crate::services::ai::{MimoExtractInput, MimoExtractResult, MimoProvider, OrganizeDecision};
use crate::services::budget::{BudgetService, BudgetSettingsInput, BudgetStatus};
use crate::services::candidates::{ActionCandidate, CandidateInput, CandidateService};
use crate::services::inbox::{InboxItem, InboxService};
use crate::services::ledger::{LedgerItem, LedgerService};
use crate::services::markdown::{MarkdownDocument, MarkdownExport, MarkdownService};
use crate::services::movement::{MoveLog, MoveRequest, MovementService};
use crate::services::queue::{ListenerState, ListenerStateUpdate, QueueItem, QueueItemInput, QueueService};
use crate::services::settings::{AppSettings, AppSettingsInput, SettingsService};
use crate::services::sticky::{StickyNote, StickyNoteInput, StickyService};
use crate::services::usage::{UsageService, UsageSummary};
use crate::services::vault::{canonical_vault_root, normalize_relative_path, VaultInitResult, VaultService, VaultTreeNode, INBOX_DIR, LEDGER_FILE};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct WatcherRegistry {
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub listener: ListenerState,
    pub pending: Vec<QueueItem>,
    pub running: Vec<QueueItem>,
    pub failed: Vec<QueueItem>,
}

fn into_command_result<T>(result: crate::services::ServiceResult<T>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
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
pub fn get_ai_usage(vault_path: String) -> Result<UsageSummary, String> {
    into_command_result(UsageService::summary(&vault_path))
}

#[tauri::command]
pub fn get_app_settings(vault_path: String) -> Result<AppSettings, String> {
    into_command_result(SettingsService::get(&vault_path))
}

#[tauri::command]
pub fn save_app_settings(vault_path: String, settings: AppSettingsInput) -> Result<AppSettings, String> {
    into_command_result(SettingsService::save(&vault_path, settings))
}

#[tauri::command]
pub fn get_queue_status(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(queue_status(&vault_path))
}

#[tauri::command]
pub fn scan_inbox_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(scan_inbox(&vault_path).and_then(|_| queue_status(&vault_path)))
}

#[tauri::command]
pub fn pause_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(QueueService::set_listener_state(
        &vault_path,
        ListenerStateUpdate {
            enabled: false,
            status: "paused".to_string(),
            last_event_at: None,
            last_error: None,
        },
    ).and_then(|_| queue_status(&vault_path)))
}

#[tauri::command]
pub fn resume_queue(vault_path: String) -> Result<QueueStatus, String> {
    into_command_result(QueueService::set_listener_state(
        &vault_path,
        ListenerStateUpdate {
            enabled: true,
            status: "watching".to_string(),
            last_event_at: None,
            last_error: None,
        },
    ).and_then(|_| queue_status(&vault_path)))
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
                let _ = enqueue_path_event(&vault_for_callback, &root_for_callback, path);
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
    into_command_result(QueueService::set_listener_state(
        &vault_path,
        ListenerStateUpdate {
            enabled: true,
            status: "watching".to_string(),
            last_event_at: Some(chrono::Utc::now().to_rfc3339()),
            last_error: None,
        },
    ).and_then(|_| queue_status(&vault_path)))
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
    into_command_result(QueueService::set_listener_state(
        &vault_path,
        ListenerStateUpdate {
            enabled: false,
            status: "stopped".to_string(),
            last_event_at: Some(chrono::Utc::now().to_rfc3339()),
            last_error: None,
        },
    ).and_then(|_| queue_status(&vault_path)))
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
        paused: settings.get("paused").and_then(Value::as_bool).unwrap_or(false),
        retry_limit: settings.get("retryLimit").and_then(Value::as_i64).unwrap_or(3),
        cooldown_seconds: settings
            .get("cooldownSeconds")
            .or_else(|| settings.get("cooldown_seconds"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    };
    into_command_result(BudgetService::save_settings(&vault_path, input).and_then(|_| BudgetService::status(&vault_path)))
}

#[tauri::command]
pub fn extract_with_mimo(vault_path: String, input: MimoExtractInput) -> Result<MimoExtractResult, String> {
    into_command_result(MimoProvider::extract_file(&vault_path, input))
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
        source_relative_path.as_deref().unwrap_or("000-收集箱/示例笔记.md"),
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
            "message": "AI 整理移动已执行"
        }))
    } else {
        Ok(json!({
            "moved": 0,
            "skipped": 0,
            "conflicts": 0,
            "auditId": plan_id.unwrap_or_else(|| "plan-only".to_string()),
            "message": "整理计划已记录；请选择具体候选后执行移动"
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
pub fn rollback_move(vault_path: String, movement_id: Option<i64>, audit_id: Option<String>) -> Result<MoveLog, String> {
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
pub fn list_todo_schedule_candidates(vault_path: String) -> Result<Vec<ActionCandidate>, String> {
    into_command_result(CandidateService::list_pending(&vault_path))
}

#[tauri::command]
pub fn create_todo_schedule_candidate(vault_path: String, candidate: CandidateInput) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::create(&vault_path, candidate))
}

#[tauri::command]
pub fn confirm_todo_schedule_candidate(vault_path: String, candidate_id: i64) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::confirm(&vault_path, candidate_id))
}

#[tauri::command]
pub fn dismiss_todo_schedule_candidate(vault_path: String, candidate_id: i64) -> Result<ActionCandidate, String> {
    into_command_result(CandidateService::reject(&vault_path, candidate_id))
}

#[tauri::command]
pub fn list_sticky_notes(vault_path: String, include_archived: Option<bool>) -> Result<Vec<StickyNote>, String> {
    into_command_result(StickyService::list(&vault_path, include_archived.unwrap_or(false)))
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
                    let _ = window.emit("thebrain-sticky-shortcut", json!({"shortcut": registered}));
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(shortcut)
}

#[tauri::command]
pub fn list_conflicts() -> Vec<Value> {
    Vec::new()
}

#[tauri::command]
pub fn resolve_conflict(_conflict_id: String, _strategy: String) -> Value {
    json!({"status": "noop"})
}

fn queue_status(vault_path: &str) -> crate::services::ServiceResult<QueueStatus> {
    Ok(QueueStatus {
        listener: QueueService::get_listener_state(vault_path)?,
        pending: QueueService::list_by_status(vault_path, "pending")?,
        running: QueueService::list_by_status(vault_path, "running")?,
        failed: QueueService::list_by_status(vault_path, "failed")?,
    })
}

fn scan_inbox(vault_path: &str) -> crate::services::ServiceResult<()> {
    let root = canonical_vault_root(vault_path)?;
    let inbox = root.join(INBOX_DIR);
    if !inbox.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(inbox)? {
        let path = entry?.path();
        enqueue_path_event(vault_path, &root, path)?;
    }
    Ok(())
}

fn enqueue_path_event(vault_path: &str, root: &std::path::Path, path: PathBuf) -> crate::services::ServiceResult<()> {
    if !path.is_file() {
        return Ok(());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| crate::services::ServiceError::EscapedVault(path.to_string_lossy().to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    let relative = normalize_relative_path(&relative)?;
    if !relative.starts_with(&format!("{INBOX_DIR}/")) || relative == format!("{INBOX_DIR}/{LEDGER_FILE}") {
        return Ok(());
    }
    let dedupe_key = format!("file:{relative}");
    let _ = QueueService::enqueue(
        vault_path,
        QueueItemInput {
            kind: "inbox_file_changed".to_string(),
            relative_path: relative.clone(),
            dedupe_key: Some(dedupe_key),
            payload: Some(json!({"source": "watcher", "relativePath": relative})),
            max_attempts: Some(3),
            run_after: None,
        },
    )?;
    Ok(())
}
