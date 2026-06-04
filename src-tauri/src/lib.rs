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
            commands::get_ai_usage,
            commands::rebuild_rag_index,
            commands::get_rag_index_status,
            commands::ask_rag,
            commands::get_latest_rag_trace,
            commands::get_app_settings,
            commands::save_app_settings,
            commands::get_queue_status,
            commands::get_worker_status,
            commands::run_inbox_worker,
            commands::pause_inbox_worker,
            commands::resume_inbox_worker,
            commands::scan_inbox_queue,
            commands::pause_queue,
            commands::resume_queue,
            commands::start_inbox_watcher,
            commands::stop_inbox_watcher,
            commands::get_budget_status,
            commands::save_budget_settings,
            commands::get_mimo_status,
            commands::extract_with_mimo,
            commands::plan_inbox_item,
            commands::plan_ai_organize,
            commands::run_ai_organize,
            commands::move_inbox_item,
            commands::rollback_move,
            commands::list_move_logs,
            commands::list_todo_schedule_candidates,
            commands::create_todo_schedule_candidate,
            commands::confirm_todo_schedule_candidate,
            commands::dismiss_todo_schedule_candidate,
            commands::list_sticky_notes,
            commands::save_sticky_note,
            commands::delete_sticky_note,
            commands::autosave_sticky_note,
            commands::prewarm_sticky_windows,
            commands::register_global_shortcut,
            commands::list_conflicts,
            commands::resolve_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running TheBrain");
}
