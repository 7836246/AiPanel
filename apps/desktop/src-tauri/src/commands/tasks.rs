//! Task/run history commands — thin wrappers over the [`Store`](crate::store).
//!
//! These back the sidebar's list of every run and the restore-a-run flow. Like
//! all commands here they only delegate to Core and return serde types; the
//! persisted [`TaskRecord`] is already sanitized (no secrets).

use tauri::State;

use crate::core::error::AppResult;
use crate::core::types::TaskRecord;
use crate::AppState;

/// Recent runs, newest first. Filtered to one server when `serverId` is set.
#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    server_id: Option<String>,
    limit: Option<u32>,
) -> AppResult<Vec<TaskRecord>> {
    state.store.list_tasks(server_id.as_deref(), limit.unwrap_or(100))
}

/// One run by id, for restoring its full detail.
#[tauri::command]
pub fn get_task(state: State<'_, AppState>, id: String) -> AppResult<TaskRecord> {
    state.store.get_task(&id)
}

/// Create or update a run record (upsert keyed by id).
#[tauri::command]
pub fn save_task(state: State<'_, AppState>, task: TaskRecord) -> AppResult<()> {
    state.store.upsert_task(&task)
}

/// Remove a run from the history.
#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_task(&id)
}
