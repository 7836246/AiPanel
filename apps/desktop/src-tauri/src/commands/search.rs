//! 审计 / 任务的搜索与导出命令 —— [`Store`](crate::store) 的薄封装。
//!
//! 它们支撑审计/历史界面的搜索框与「导出」按钮。和这里所有命令一样，只委托给
//! Core 并返回 serde 类型；持久化的记录已脱敏（执行输出不含密钥），因此搜索结果
//! 与导出内容都不会引入任何明文密钥。

use tauri::State;

use crate::core::error::AppResult;
use crate::core::types::{AuditRecord, TaskRecord};
use crate::AppState;

/// 在审计记录中按子串搜索（意图 / 总结 / 命令，不区分大小写）。
///
/// `query` 为空时退化为「列出最近记录」。`limit` 缺省为 100。
#[tauri::command]
pub fn search_audit_records(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<AuditRecord>> {
    state.store.search_audit_records(&query, limit.unwrap_or(100))
}

/// 在运行历史中按子串搜索（标题 / 意图，不区分大小写），可按服务器过滤。
///
/// `query` 为空时退化为「列出最近运行」。`limit` 缺省为 100。
#[tauri::command]
pub fn search_tasks(
    state: State<'_, AppState>,
    server_id: Option<String>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<TaskRecord>> {
    state
        .store
        .search_tasks(server_id.as_deref(), &query, limit.unwrap_or(100))
}

/// 把全部审计记录导出为格式化 JSON 字符串（最新在前）。
///
/// 返回的内容已脱敏、不含密钥，前端可直接写盘或分享。
#[tauri::command]
pub fn export_audit_json(state: State<'_, AppState>) -> AppResult<String> {
    state.store.export_audit_records_json()
}
