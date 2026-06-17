//! AiPanel 桌面端后端。
//!
//! `lib.rs` 只负责组装应用状态、注册 Tauri 命令。所有业务逻辑都在 Core 层及其
//! 子模块里——安全边界（SSH 执行、风险审查、审计）由 AiPanel 自己掌控，绝不
//! 交给 agent。详见 CLAUDE.md 与 docs/SECURITY_MODEL.zh-Hans.md。

pub mod core;

pub mod agent;
pub mod audit;
pub mod commands;
pub mod credentials;
pub mod doctor;
pub mod plan;
pub mod risk;
pub mod ssh;
pub mod store;
pub mod terminal;
pub mod tools;

use tauri::Manager;

use credentials::{default_credential_store, CredentialStore};
use store::Store;

/// 共享的应用状态，由 Tauri 托管并注入到各命令中。
pub struct AppState {
    pub store: Store,
    pub credentials: Box<dyn CredentialStore>,
    pub plan_engine: Box<dyn plan::PlanEngine>,
}

/// 后端版本号，暴露给前端，让 UI 能显示自己正在对接的版本。
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(&dir.join("aipanel.sqlite3"))?;
            let credentials = default_credential_store();
            let plan_engine: Box<dyn plan::PlanEngine> = Box::new(plan::MockPlanEngine);
            app.manage(AppState { store, credentials, plan_engine });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::list_servers,
            commands::get_server,
            commands::create_server,
            commands::set_server_favorite,
            commands::refresh_all_servers,
            commands::update_server,
            commands::delete_server,
            commands::set_server_secret,
            commands::credential_backend,
            commands::review_plan,
            commands::check_ssh_connection,
            commands::run_readonly_command,
            commands::server_doctor_plan,
            commands::run_server_doctor,
            commands::stream::run_server_doctor_stream,
            commands::stream::run_confirmed_plan_stream,
            commands::stream::cancel_run,
            commands::terminal::terminal_open,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_close,
            commands::tasks::list_tasks,
            commands::tasks::get_task,
            commands::tasks::save_task,
            commands::tasks::delete_task,
            commands::search::search_audit_records,
            commands::search::search_tasks,
            commands::search::export_audit_json,
            commands::list_audit_records,
            commands::get_audit_record,
            commands::create_plan,
            commands::execute_confirmed_plan,
            commands::test_provider,
            commands::run_agent_turn,
            commands::list_tools,
            commands::list_providers,
            commands::save_provider,
            commands::delete_provider,
            commands::get_model_selection_policy,
            commands::save_model_selection_policy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
