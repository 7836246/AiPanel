//! 审计 / 任务的搜索与导出命令 —— [`Store`](crate::store) 的薄封装。
//!
//! 它们支撑审计/历史界面的搜索框与「导出」按钮。和这里所有命令一样，只委托给
//! Core 并返回 serde 类型；持久化的记录已脱敏（执行输出不含密钥），因此搜索结果
//! 与导出内容都不会引入任何明文密钥。

use tauri::State;

use crate::core::error::{AppError, AppResult};
use crate::core::types::{AuditRecord, TaskRecord};
use crate::AppState;

const DEFAULT_QUERY_LIMIT: u32 = 100;
const MAX_QUERY_LIMIT: u32 = 500;

/// 在审计记录中按子串搜索（意图 / 总结 / 命令，不区分大小写）。
///
/// `query` 为空时退化为「列出最近记录」。`limit` 缺省为 100。
#[tauri::command]
pub fn search_audit_records(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<AuditRecord>> {
    state.store.search_audit_records(&query, normalize_limit(limit))
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
        .search_tasks(server_id.as_deref(), &query, normalize_limit(limit))
}

/// 把全部审计记录导出为格式化 JSON 字符串（最新在前）。
///
/// 返回的内容已脱敏、不含密钥，前端可直接写盘或分享。
#[tauri::command]
pub fn export_audit_json(state: State<'_, AppState>) -> AppResult<String> {
    state.store.export_audit_records_json()
}

/// 把全部审计记录导出到用户选择的本地 JSON 文件。
///
/// 路径来自系统保存对话框，但后端仍做边界校验，避免空路径、目录路径或不存在的父目录
/// 导致导出按钮显示成功但文件实际没有正确落盘。
#[tauri::command]
pub fn export_audit_json_to_path(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<()> {
    validate_export_path(&path)?;
    let json = state.store.export_audit_records_json()?;
    let target = std::path::Path::new(&path);
    std::fs::write(target, json)?;
    Ok(())
}

fn validate_export_path(path: &str) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::Validation("export path is required".into()));
    }
    if path.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "export path must not contain control characters".into(),
        ));
    }
    let target = std::path::Path::new(path);
    if target.is_dir() {
        return Err(AppError::Validation(
            "export path must be a file, not a directory".into(),
        ));
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(AppError::Validation(
                "export parent directory does not exist".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn normalize_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_QUERY_LIMIT)
        .clamp(1, MAX_QUERY_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::{normalize_limit, validate_export_path};

    #[test]
    fn export_path_validation_rejects_unwritable_targets() {
        let dir = std::env::temp_dir().join(format!(
            "aipanel-audit-export-test-{}",
            crate::core::types::new_id()
        ));
        std::fs::create_dir(&dir).unwrap();

        assert_eq!(validate_export_path("  ").unwrap_err().code(), "validation");
        assert_eq!(
            validate_export_path("/tmp/a\nb.json").unwrap_err().code(),
            "validation"
        );
        assert_eq!(
            validate_export_path(dir.to_str().unwrap()).unwrap_err().code(),
            "validation"
        );
        let missing_parent = dir.join("missing").join("audit.json");
        assert_eq!(
            validate_export_path(missing_parent.to_str().unwrap())
                .unwrap_err()
                .code(),
            "validation"
        );

        let file = dir.join("audit.json");
        assert!(validate_export_path(file.to_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn query_limit_is_defaulted_and_capped() {
        assert_eq!(normalize_limit(None), 100);
        assert_eq!(normalize_limit(Some(0)), 1);
        assert_eq!(normalize_limit(Some(1)), 1);
        assert_eq!(normalize_limit(Some(500)), 500);
        assert_eq!(normalize_limit(Some(501)), 500);
        assert_eq!(normalize_limit(Some(u32::MAX)), 500);
    }
}
