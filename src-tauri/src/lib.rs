//! raitei バックエンドのエントリポイント。モジュール構成は docs/design.md を参照。

pub mod agent;
pub mod commands;
pub mod error;
pub mod git;
pub mod github;
pub mod models;
pub mod shell_env;
pub mod state;
pub mod store;

use std::sync::Arc;

use tauri::Manager;

use crate::agent::AgentManager;
use crate::shell_env::ShellEnv;
use crate::state::{AppState, TauriSink};
use crate::store::Store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = Store::open(&data_dir.join("raitei.db"))?;
            app.manage(AppState {
                env: ShellEnv::resolve(),
                store: Arc::new(store),
                agents: Arc::new(AgentManager::new()),
                sink: Arc::new(TauriSink(app.handle().clone())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_environment,
            commands::project::list_projects,
            commands::project::add_project,
            commands::project::create_project,
            commands::project::remove_project,
            commands::task::list_tasks,
            commands::task::get_task,
            commands::task::create_task,
            commands::task::update_task,
            commands::task::delete_task,
            commands::git::list_worktrees,
            commands::git::get_git_status,
            commands::git::start_base_merge,
            commands::git::get_conflict_state,
            commands::git::read_conflict_file,
            commands::git::resolve_conflict_file,
            commands::git::abort_base_merge,
            commands::git::complete_base_merge,
            commands::git::request_agent_conflict_resolution,
            commands::pr::get_pull_request,
            commands::pr::create_pull_request,
            commands::pr::merge_pull_request,
            commands::agent::send_agent_message,
            commands::agent::cancel_agent_run,
            commands::agent::get_agent_history,
            commands::agent::get_agent_run_state,
            commands::agent::reset_agent_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
