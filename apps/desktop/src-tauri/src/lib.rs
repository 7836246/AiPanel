//! AiPanel desktop backend.
//!
//! `lib.rs` only assembles application state and registers Tauri commands. All
//! business logic lives in the Core layer and its modules — AiPanel owns the
//! security boundary (SSH execution, risk review, audit), never the agent. See
//! CLAUDE.md and docs/SECURITY_MODEL.zh-Hans.md.

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
pub mod tools;

use tauri::Manager;

use credentials::{default_credential_store, CredentialStore};
use store::Store;

/// Shared application state, managed by Tauri and injected into commands.
pub struct AppState {
    pub store: Store,
    pub credentials: Box<dyn CredentialStore>,
    pub plan_engine: Box<dyn plan::PlanEngine>,
}

/// Backend version, surfaced to the frontend so the UI can show what it's talking to.
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
            commands::update_server,
            commands::delete_server,
            commands::set_server_secret,
            commands::credential_backend,
            commands::review_plan,
            commands::check_ssh_connection,
            commands::run_readonly_command,
            commands::server_doctor_plan,
            commands::run_server_doctor,
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
