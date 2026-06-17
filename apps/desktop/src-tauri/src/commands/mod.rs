//! Tauri command handlers — the thin boundary between the frontend and Core.
//!
//! Commands validate, delegate to Core modules, and return serde types. They
//! never embed business logic and never log or return secrets.

use tauri::State;

use crate::core::error::AppResult;
use crate::core::types::{ServerInput, ServerProfile};
use crate::AppState;

#[tauri::command]
pub fn list_servers(state: State<'_, AppState>) -> AppResult<Vec<ServerProfile>> {
    state.store.list_servers()
}

#[tauri::command]
pub fn get_server(state: State<'_, AppState>, id: String) -> AppResult<ServerProfile> {
    state.store.get_server(&id)
}

#[tauri::command]
pub fn create_server(state: State<'_, AppState>, input: ServerInput) -> AppResult<ServerProfile> {
    state.store.create_server(input)
}

#[tauri::command]
pub fn update_server(
    state: State<'_, AppState>,
    id: String,
    input: ServerInput,
) -> AppResult<ServerProfile> {
    state.store.update_server(&id, input)
}

#[tauri::command]
pub fn delete_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_server(&id)
}
