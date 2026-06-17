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

/// Backend version, surfaced to the frontend so the UI can show what it's talking to.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
