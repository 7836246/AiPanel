//! AiPanel Tools — the ONLY way the agent reaches server capability.
//!
//! Codex never gets raw SSH or an unrestricted shell; it calls these vetted
//! tools instead (CLAUDE.md, docs/SECURITY_MODEL.zh-Hans.md). Each tool declares
//! its permission and audit policy:
//!
//! - read-only tools are available by default;
//! - write/execute tools require an explicit user confirmation that the agent
//!   CANNOT mint itself — `task.execute_confirmed` refuses unless the call
//!   carries a confirmation that originated from the user via the app;
//! - every execution is audited locally.
//!
//! Dispatch is internal JSON-RPC-shaped (`name` + JSON `args` → JSON result);
//! an MCP adapter can wrap this later without changing the boundary.

use serde::Serialize;
use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::core::types::{Plan, TaskStatus};
use crate::AppState;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolPermission {
    /// Inspection only; never changes server state. Available by default.
    ReadOnly,
    /// Changes state or executes a plan; requires user confirmation.
    Write,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub permission: ToolPermission,
    /// Whether invoking this tool writes an audit record.
    pub audited: bool,
    /// A short example of the expected args, for the agent.
    pub args_example: Value,
}

/// The full tool surface exposed to the agent.
pub fn registry() -> Vec<ToolSpec> {
    use ToolPermission::*;
    vec![
        ToolSpec { name: "server.list", description: "列出本地保存的服务器", permission: ReadOnly, audited: false, args_example: json!({}) },
        ToolSpec { name: "server.info", description: "读取单个服务器的基础信息", permission: ReadOnly, audited: false, args_example: json!({"id": "<serverId>"}) },
        ToolSpec { name: "server.doctor.readonly", description: "对服务器执行只读体检", permission: ReadOnly, audited: true, args_example: json!({"serverId": "<serverId>"}) },
        ToolSpec { name: "ssh.run_readonly", description: "执行单条只读 SSH 命令（经风险审查为 Low）", permission: ReadOnly, audited: true, args_example: json!({"serverId": "<serverId>", "command": "df -h"}) },
        ToolSpec { name: "task.plan", description: "把自然语言任务转换为结构化计划", permission: ReadOnly, audited: false, args_example: json!({"intent": "检查网站为什么打不开", "serverId": "<serverId>"}) },
        ToolSpec { name: "task.review", description: "审查计划的风险等级", permission: ReadOnly, audited: false, args_example: json!({"plan": {}, "readOnlyMode": false}) },
        ToolSpec { name: "task.execute_confirmed", description: "执行用户已确认的计划（需用户确认，Agent 不能自行授权）", permission: Write, audited: true, args_example: json!({"plan": {}, "confirmed": true}) },
        ToolSpec { name: "audit.write", description: "向审计日志追加一条记录", permission: Write, audited: true, args_example: json!({"intent": "...", "summary": "..."}) },
    ]
}

fn arg_str(args: &Value, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Validation(format!("missing string arg `{key}`")))
}

fn server_secret(state: &AppState, server: &crate::core::types::ServerProfile) -> AppResult<Option<String>> {
    match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference),
        None => Ok(None),
    }
}

/// Dispatch a tool call. The agent runtime routes every tool invocation here.
pub async fn dispatch(state: &AppState, name: &str, args: Value) -> AppResult<Value> {
    match name {
        "server.list" => Ok(json!(state.store.list_servers()?)),

        "server.info" => {
            let id = arg_str(&args, "id")?;
            Ok(json!(state.store.get_server(&id)?))
        }

        "server.doctor.readonly" => {
            let id = arg_str(&args, "serverId")?;
            let server = state.store.get_server(&id)?;
            let secret = server_secret(state, &server)?;
            let plan = crate::doctor::doctor_plan(&id);
            let review = crate::risk::review_plan(&plan, true);
            let report = crate::doctor::run_doctor(&server, secret.as_deref()).await?;
            let record = crate::audit::record_for_doctor(&id, plan, review, &report);
            state.store.insert_audit_record(&record)?;
            Ok(json!(report))
        }

        "ssh.run_readonly" => {
            let id = arg_str(&args, "serverId")?;
            let command = arg_str(&args, "command")?;
            let server = state.store.get_server(&id)?;
            let secret = server_secret(state, &server)?;
            // run_readonly enforces the Low classification; this is the boundary.
            let exec = crate::ssh::run_readonly(&server, secret.as_deref(), &command, crate::ssh::DEFAULT_TIMEOUT).await?;
            Ok(json!(exec))
        }

        "task.plan" => {
            let intent = arg_str(&args, "intent")?;
            let server_id = args.get("serverId").and_then(|v| v.as_str());
            Ok(json!(state.plan_engine.create_plan(&intent, server_id)?))
        }

        "task.review" => {
            let plan: Plan = serde_json::from_value(args.get("plan").cloned().unwrap_or(Value::Null))?;
            let read_only_mode = args.get("readOnlyMode").and_then(|v| v.as_bool()).unwrap_or(false);
            Ok(json!(crate::risk::review_plan(&plan, read_only_mode)))
        }

        "task.execute_confirmed" => {
            // The agent CANNOT self-authorize execution: confirmation must come
            // from the user via the app. We refuse unless the call carries it.
            let confirmed = args.get("confirmed").and_then(|v| v.as_bool()).unwrap_or(false);
            if !confirmed {
                return Err(AppError::Blocked(
                    "执行需要用户确认；Agent 不能自行授权写操作".into(),
                ));
            }
            let plan: Plan = serde_json::from_value(args.get("plan").cloned().unwrap_or(Value::Null))?;
            let server_id = plan
                .server_id
                .clone()
                .ok_or_else(|| AppError::Validation("plan has no target server".into()))?;
            let server = state.store.get_server(&server_id)?;
            let secret = server_secret(state, &server)?;
            let review = crate::risk::review_plan(&plan, false);
            if review.blocked {
                return Err(AppError::Blocked("plan contains blocked steps".into()));
            }
            let double = args.get("doubleConfirmed").and_then(|v| v.as_bool()).unwrap_or(false);
            if review.requires_double_confirmation && !double {
                return Err(AppError::Blocked("plan requires a second confirmation".into()));
            }
            let mut executions = Vec::new();
            let mut failed = false;
            for step in &plan.steps {
                let res = if step.read_only {
                    crate::ssh::run_readonly(&server, secret.as_deref(), &step.command, crate::ssh::DEFAULT_TIMEOUT).await
                } else {
                    crate::ssh::run_command(&server, secret.as_deref(), &step.command, crate::ssh::DEFAULT_TIMEOUT).await
                };
                match res {
                    Ok(e) => {
                        let bad = e.exit_code != 0;
                        executions.push(e);
                        if bad { failed = true; break; }
                    }
                    Err(_) => { failed = true; break; }
                }
            }
            let status = if failed { TaskStatus::Failed } else { TaskStatus::Completed };
            let intent = plan.goal.clone();
            let record = crate::audit::record_for_plan(Some(&server_id), &intent, plan, review, executions, status);
            state.store.insert_audit_record(&record)?;
            Ok(json!(record))
        }

        "audit.write" => {
            let intent = arg_str(&args, "intent")?;
            let summary = args.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());
            let server_id = args.get("serverId").and_then(|v| v.as_str());
            let ts = crate::core::types::now();
            let record = crate::core::types::AuditRecord {
                id: crate::core::types::new_id(),
                server_id: server_id.map(|s| s.to_string()),
                intent,
                plan: None,
                risk_review: None,
                confirmed_at: None,
                executions: vec![],
                summary,
                status: TaskStatus::Completed,
                created_at: ts,
                updated_at: ts,
            };
            state.store.insert_audit_record(&record)?;
            Ok(json!({ "id": record.id }))
        }

        other => Err(AppError::Validation(format!("unknown tool `{other}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_all_tools_with_policy() {
        let r = registry();
        assert_eq!(r.len(), 8);
        let exec = r.iter().find(|t| t.name == "task.execute_confirmed").unwrap();
        assert!(matches!(exec.permission, ToolPermission::Write));
        assert!(exec.audited);
        let list = r.iter().find(|t| t.name == "server.list").unwrap();
        assert!(matches!(list.permission, ToolPermission::ReadOnly));
    }

    #[tokio::test]
    async fn execute_requires_confirmation() {
        // Build a minimal AppState with in-memory store + mock credentials/engine.
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let plan = state.plan_engine.create_plan("检查磁盘", Some("s")).unwrap();
        let err = dispatch(&state, "task.execute_confirmed", json!({ "plan": plan, "confirmed": false }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "blocked");
    }

    #[tokio::test]
    async fn unknown_tool_is_validation_error() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let err = dispatch(&state, "shell.exec", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "validation");
    }

    #[tokio::test]
    async fn server_list_and_plan_work_through_tools() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let servers = dispatch(&state, "server.list", json!({})).await.unwrap();
        assert!(servers.is_array());
        let plan = dispatch(&state, "task.plan", json!({"intent": "网站打不开"})).await.unwrap();
        assert!(plan["steps"].is_array());
    }
}
