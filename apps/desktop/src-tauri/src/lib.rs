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
pub mod risk;
pub mod ssh;
pub mod store;
pub mod tools;

use tauri::Manager;

use store::Store;

/// Shared application state, managed by Tauri and injected into commands.
pub struct AppState {
    pub store: Store,
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
            app.manage(AppState { store });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::list_servers,
            commands::get_server,
            commands::create_server,
            commands::update_server,
            commands::delete_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
