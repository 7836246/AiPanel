//! 任务/运行历史命令 —— [`Store`](crate::store) 的薄封装。
//!
//! 它们支撑侧边栏里每次运行的列表，以及「恢复某次运行」的流程。和这里所有命令
//! 一样，只委托给 Core 并返回 serde 类型；持久化的 [`TaskRecord`] 已脱敏（不含密钥）。

use tauri::State;

use crate::core::error::{AppError, AppResult};
use crate::core::types::TaskRecord;
use crate::AppState;

/// 最近的运行记录，最新在前。设置 `serverId` 时只列该服务器的记录。
#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    server_id: Option<String>,
    limit: Option<u32>,
) -> AppResult<Vec<TaskRecord>> {
    state
        .store
        .list_tasks(server_id.as_deref(), crate::commands::search::normalize_limit(limit))
}

/// 按 id 取单次运行，用于恢复其完整细节。
#[tauri::command]
pub fn get_task(state: State<'_, AppState>, id: String) -> AppResult<TaskRecord> {
    state.store.get_task(&id)
}

/// 新建或更新一条运行记录（按 id upsert）。
#[tauri::command]
pub fn save_task(state: State<'_, AppState>, task: TaskRecord) -> AppResult<()> {
    let task = validate_task_record(task)?;
    validate_task_server_exists(&state, &task)?;
    state.store.upsert_task(&task)
}

/// 从历史中删除某次运行。
#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_task(&id)
}

fn validate_task_record(mut task: TaskRecord) -> AppResult<TaskRecord> {
    task.id = normalize_required_text(task.id, "task id")?;
    task.title = normalize_required_text(task.title, "task title")?;
    task.intent = normalize_required_text(task.intent, "task intent")?;
    task.server_id = normalize_optional_text(task.server_id, "server id")?;
    task.summary = normalize_optional_text(task.summary, "task summary")?;

    if task.updated_at < task.created_at {
        return Err(AppError::Validation(
            "task updatedAt must not be earlier than createdAt".into(),
        ));
    }
    if let (Some(task_server), Some(plan_server)) =
        (task.server_id.as_deref(), task.plan.as_ref().and_then(|p| p.server_id.as_deref()))
    {
        if task_server != plan_server {
            return Err(AppError::Validation(
                "task serverId must match plan serverId".into(),
            ));
        }
    }
    validate_task_kind_shape(&task)?;
    validate_task_status_shape(&task)?;
    Ok(task)
}

fn validate_task_server_exists(state: &AppState, task: &TaskRecord) -> AppResult<()> {
    if let Some(server_id) = task.server_id.as_deref() {
        state
            .store
            .get_server(server_id)
            .map_err(|_| AppError::Validation("task serverId does not reference an existing server".into()))?;
    }
    Ok(())
}

fn validate_task_kind_shape(task: &TaskRecord) -> AppResult<()> {
    match task.kind {
        crate::core::types::TaskKind::Plan => {
            if task.plan.is_none() {
                return Err(AppError::Validation(
                    "plan task records must include a plan".into(),
                ));
            }
            if !task.tool_calls.is_empty() {
                return Err(AppError::Validation(
                    "plan task records must not include diagnose tool calls".into(),
                ));
            }
        }
        crate::core::types::TaskKind::Diagnose => {
            if task.plan.is_some() || !task.executions.is_empty() {
                return Err(AppError::Validation(
                    "diagnose task records must not include plan executions".into(),
                ));
            }
        }
        crate::core::types::TaskKind::Doctor => {
            if !task.tool_calls.is_empty() {
                return Err(AppError::Validation(
                    "doctor task records must not include diagnose tool calls".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_task_status_shape(task: &TaskRecord) -> AppResult<()> {
    match task.status {
        crate::core::types::TaskStatus::AwaitingConfirmation
        | crate::core::types::TaskStatus::Pending
        | crate::core::types::TaskStatus::Planning => {
            if !task.executions.is_empty() || task.summary.is_some() {
                return Err(AppError::Validation(
                    "pending task records must not include execution results".into(),
                ));
            }
        }
        crate::core::types::TaskStatus::Running => {
            if !task.executions.is_empty() || task.summary.is_some() {
                return Err(AppError::Validation(
                    "running task records must not include final execution results".into(),
                ));
            }
        }
        crate::core::types::TaskStatus::Completed => {
            if task.executions.iter().any(|e| e.exit_code != 0) {
                return Err(AppError::Validation(
                    "completed task records must not include failed executions".into(),
                ));
            }
            if task.executions.is_empty() && task.summary.is_none() && task.tool_calls.is_empty() {
                return Err(AppError::Validation(
                    "completed task records must include executions, tool calls, or summary".into(),
                ));
            }
        }
        crate::core::types::TaskStatus::Failed => {
            if task.executions.is_empty() && task.summary.is_none() && task.tool_calls.is_empty() {
                return Err(AppError::Validation(
                    "failed task records must include executions, tool calls, or summary".into(),
                ));
            }
        }
        crate::core::types::TaskStatus::Blocked => {}
    }
    Ok(())
}

fn normalize_required_text(value: String, field: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(format!("{field} is required")));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_text(value: Option<String>, field: &str) -> AppResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{validate_task_record, validate_task_server_exists};
    use crate::core::types::{now, AuthKind, Plan, ServerInput, TaskKind, TaskRecord, TaskStatus};

    fn task() -> TaskRecord {
        let ts = now();
        TaskRecord {
            id: "task-1".into(),
            server_id: Some("srv".into()),
            title: "检查磁盘".into(),
            intent: "检查磁盘".into(),
            kind: TaskKind::Plan,
            plan: None,
            risk_review: None,
            executions: vec![],
            tool_calls: vec![],
            summary: None,
            status: TaskStatus::Pending,
            created_at: ts,
            updated_at: ts,
        }
    }

    #[test]
    fn task_record_validation_normalizes_display_fields() {
        let mut t = task();
        t.kind = TaskKind::Doctor;
        t.status = TaskStatus::Completed;
        t.id = "  task-1  ".into();
        t.title = "  检查磁盘  ".into();
        t.intent = "  df -h  ".into();
        t.server_id = Some("  srv  ".into());
        t.summary = Some("  ok  ".into());

        let got = validate_task_record(t).unwrap();
        assert_eq!(got.id, "task-1");
        assert_eq!(got.title, "检查磁盘");
        assert_eq!(got.intent, "df -h");
        assert_eq!(got.server_id.as_deref(), Some("srv"));
        assert_eq!(got.summary.as_deref(), Some("ok"));
    }

    #[test]
    fn task_record_validation_rejects_bad_required_fields() {
        let mut blank_id = task();
        blank_id.id = "  ".into();
        assert_eq!(validate_task_record(blank_id).unwrap_err().code(), "validation");

        let mut bad_title = task();
        bad_title.title = "检查\n磁盘".into();
        assert_eq!(validate_task_record(bad_title).unwrap_err().code(), "validation");

        let mut bad_intent = task();
        bad_intent.intent = "\0".into();
        assert_eq!(validate_task_record(bad_intent).unwrap_err().code(), "validation");
    }

    #[test]
    fn task_record_validation_rejects_inconsistent_times_and_plan_server() {
        let mut bad_time = task();
        bad_time.updated_at = bad_time.created_at - chrono::Duration::seconds(1);
        assert_eq!(validate_task_record(bad_time).unwrap_err().code(), "validation");

        let mut bad_plan = task();
        bad_plan.plan = Some(Plan {
            id: "plan-1".into(),
            server_id: Some("other".into()),
            goal: "检查磁盘".into(),
            steps: vec![],
            created_at: bad_plan.created_at,
        });
        assert_eq!(validate_task_record(bad_plan).unwrap_err().code(), "validation");
    }

    #[test]
    fn task_record_validation_rejects_inconsistent_kind_and_status_shape() {
        let mut plan_missing = task();
        plan_missing.kind = TaskKind::Plan;
        plan_missing.plan = None;
        assert_eq!(validate_task_record(plan_missing).unwrap_err().code(), "validation");

        let mut waiting_with_execution = task();
        waiting_with_execution.kind = TaskKind::Doctor;
        waiting_with_execution.status = TaskStatus::AwaitingConfirmation;
        waiting_with_execution.executions.push(crate::core::types::CommandExecution {
            command: "df -h".into(),
            exit_code: 0,
            stdout: "ok".into(),
            stderr: String::new(),
            duration_ms: 1,
            started_at: waiting_with_execution.created_at,
        });
        assert_eq!(validate_task_record(waiting_with_execution).unwrap_err().code(), "validation");

        let mut completed_with_failure = task();
        completed_with_failure.kind = TaskKind::Doctor;
        completed_with_failure.status = TaskStatus::Completed;
        completed_with_failure.executions.push(crate::core::types::CommandExecution {
            command: "false".into(),
            exit_code: 1,
            stdout: String::new(),
            stderr: "failed".into(),
            duration_ms: 1,
            started_at: completed_with_failure.created_at,
        });
        assert_eq!(validate_task_record(completed_with_failure).unwrap_err().code(), "validation");

        let mut empty_completed = task();
        empty_completed.kind = TaskKind::Doctor;
        empty_completed.status = TaskStatus::Completed;
        empty_completed.summary = None;
        assert_eq!(validate_task_record(empty_completed).unwrap_err().code(), "validation");
    }

    #[test]
    fn task_server_reference_must_exist_when_present() {
        let state = crate::AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let mut no_server = task();
        no_server.server_id = None;
        assert!(validate_task_server_exists(&state, &no_server).is_ok());

        let server = state
            .store
            .create_server(ServerInput {
                name: "srv".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "root".into(),
                auth_kind: AuthKind::Agent,
            })
            .unwrap();
        let mut existing = task();
        existing.server_id = Some(server.id.clone());
        assert!(validate_task_server_exists(&state, &existing).is_ok());

        let mut missing = task();
        missing.server_id = Some("missing-server".into());
        assert_eq!(
            validate_task_server_exists(&state, &missing)
                .unwrap_err()
                .code(),
            "validation"
        );
    }
}
