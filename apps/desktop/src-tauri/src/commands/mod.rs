//! Tauri command handlers — the thin boundary between the frontend and Core.
//!
//! Commands validate, delegate to Core modules, and return serde types. They
//! never embed business logic and never log or return secrets.

use tauri::State;

use crate::core::error::AppResult;
use crate::core::types::{Plan, RiskReview, ServerInput, ServerProfile};
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
    // Remove the secret first so deleting a server never orphans a credential.
    if let Ok(profile) = state.store.get_server(&id) {
        if let Some(reference) = &profile.credential_ref {
            let _ = state.credentials.delete_secret(reference);
        }
    }
    state.store.delete_server(&id)
}

/// Store an SSH secret (password or private key) for a server. The secret goes
/// straight to the credential store and is never logged, persisted to SQLite, or
/// written to the audit log.
#[tauri::command]
pub fn set_server_secret(state: State<'_, AppState>, id: String, secret: String) -> AppResult<()> {
    let profile = state.store.get_server(&id)?;
    let reference = profile.credential_ref.ok_or_else(|| {
        crate::core::error::AppError::Validation(
            "this server's auth method does not use a stored secret".into(),
        )
    })?;
    state.credentials.put_secret(&reference, &secret)
}

/// Which credential backend is active ("keychain" or "mock"), so the UI can warn
/// when secrets are only in memory.
#[tauri::command]
pub fn credential_backend(state: State<'_, AppState>) -> String {
    state.credentials.backend().to_string()
}

/// Review a plan's risk. `readOnlyMode` escalates any non-inspection step to
/// Blocked. Pure function — no side effects, no state needed.
#[tauri::command]
pub fn review_plan(plan: Plan, read_only_mode: bool) -> RiskReview {
    crate::risk::review_plan(&plan, read_only_mode)
}
