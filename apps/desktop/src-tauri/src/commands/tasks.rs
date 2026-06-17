//! 任务/运行历史命令 —— [`Store`](crate::store) 的薄封装。
//!
//! 它们支撑侧边栏里每次运行的列表，以及「恢复某次运行」的流程。和这里所有命令
//! 一样，只委托给 Core 并返回 serde 类型；持久化的 [`TaskRecord`] 已脱敏（不含密钥）。

use tauri::State;

use crate::core::error::AppResult;
use crate::core::types::TaskRecord;
use crate::AppState;

/// 最近的运行记录，最新在前。设置 `serverId` 时只列该服务器的记录。
#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    server_id: Option<String>,
    limit: Option<u32>,
) -> AppResult<Vec<TaskRecord>> {
    state.store.list_tasks(server_id.as_deref(), limit.unwrap_or(100))
}

/// 按 id 取单次运行，用于恢复其完整细节。
#[tauri::command]
pub fn get_task(state: State<'_, AppState>, id: String) -> AppResult<TaskRecord> {
    state.store.get_task(&id)
}

/// 新建或更新一条运行记录（按 id upsert）。
#[tauri::command]
pub fn save_task(state: State<'_, AppState>, task: TaskRecord) -> AppResult<()> {
    state.store.upsert_task(&task)
}

/// 从历史中删除某次运行。
#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_task(&id)
}
