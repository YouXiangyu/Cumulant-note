pub mod commands;
pub mod services;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::WatcherRegistry::default())
        .manage(commands::ResidentWorkerRegistry::default())
        .invoke_handler(tauri::generate_handler![
            commands::select_vault,
            commands::init_vault,
            commands::list_vault_tree,
            commands::read_markdown,
            commands::save_markdown,
            commands::export_markdown,
            commands::list_inbox,
            commands::parse_inbox_ledger,
            commands::import_to_inbox,
            commands::get_archive_map,
            commands::rebuild_archive_map,
            commands::get_ai_usage,
            commands::get_workspace_insights,
            commands::rebuild_rag_index,
            commands::get_rag_index_status,
            commands::list_rag_conversations,
            commands::create_rag_conversation,
            commands::get_rag_conversation,
            commands::ask_rag,
            commands::get_latest_rag_trace,
            commands::get_app_settings,
            commands::save_app_settings,
            commands::get_queue_status,
            commands::get_inbox_listener_status,
            commands::get_worker_status,
            commands::get_resident_worker_status,
            commands::start_resident_worker,
            commands::stop_resident_worker,
            commands::run_inbox_worker,
            commands::pause_inbox_worker,
            commands::resume_inbox_worker,
            commands::scan_inbox_queue,
            commands::pause_queue,
            commands::resume_queue,
            commands::retry_queue_item,
            commands::skip_queue_item,
            commands::preview_queue_recovery,
            commands::retry_queue_items,
            commands::skip_queue_items,
            commands::start_inbox_watcher,
            commands::stop_inbox_watcher,
            commands::get_budget_status,
            commands::save_budget_settings,
            commands::get_mimo_status,
            commands::save_mimo_api_key,
            commands::extract_with_mimo,
            commands::plan_inbox_item,
            commands::plan_ai_organize,
            commands::run_ai_organize,
            commands::move_inbox_item,
            commands::rollback_move,
            commands::list_move_logs,
            commands::rollback_moves,
            commands::preview_rollback_moves,
            commands::list_audit_events,
            commands::search_audit_events,
            commands::list_todo_schedule_candidates,
            commands::create_todo_schedule_candidate,
            commands::confirm_todo_schedule_candidate,
            commands::dismiss_todo_schedule_candidate,
            commands::promote_todo_schedule_candidate,
            commands::list_todo_items,
            commands::list_schedule_items,
            commands::set_todo_item_status,
            commands::set_schedule_item_status,
            commands::list_sticky_notes,
            commands::save_sticky_note,
            commands::delete_sticky_note,
            commands::autosave_sticky_note,
            commands::prewarm_sticky_windows,
            commands::register_global_shortcut,
            commands::list_conflicts,
            commands::get_conflict_detail,
            commands::submit_conflict_answer,
            commands::match_conflict_rules,
            commands::suggest_conflict_rename_targets,
            commands::list_conflict_rules,
            commands::set_conflict_rule_status,
            commands::update_conflict_rule,
            commands::apply_conflict_rule,
            commands::resolve_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running TheBrain");
}
