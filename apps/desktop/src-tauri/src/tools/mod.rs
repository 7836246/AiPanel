//! AiPanel Tools —— agent 触达服务器能力的**唯一**通道。
//!
//! Codex 永远拿不到裸 SSH 或不受限的 shell，只能调用这些经过审核的工具
//! （见 CLAUDE.md、docs/SECURITY_MODEL.zh-Hans.md）。每个工具都声明自己的
//! 权限与审计策略：
//!
//! - 只读工具默认可用；
//! - 写/执行类工具需要用户明确确认，且 agent **无法**自行伪造——
//!   `task.execute_confirmed` 除非调用携带了来自用户、经由 app 产生的确认，
//!   否则一律拒绝；
//! - 每次执行都在本地审计。
//!
//! 分发采用内部的类 JSON-RPC 形态（`name` + JSON `args` → JSON 结果）；
//! 以后可在不改动安全边界的前提下，用一个 MCP 适配器把它包起来。

use serde::Serialize;
use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::core::types::{Plan, TaskStatus};
use crate::AppState;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolPermission {
    /// 仅检查；绝不改变服务器状态。默认可用。
    ReadOnly,
    /// 改变状态或执行计划；需要用户确认。
    Write,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub permission: ToolPermission,
    /// 调用此工具是否写入审计记录。
    pub audited: bool,
    /// 给 agent 看的、预期参数的简短示例。
    pub args_example: Value,
}

/// 暴露给 agent 的完整工具清单。
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

/// 分发一次工具调用。agent 运行时把每一次工具调用都路由到这里。
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
            // run_readonly 会强制要求命令被判为 Low —— 这就是安全边界。
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
            // agent **不能**自行授权执行：确认必须来自用户、经由 app。
            // 调用不携带确认就拒绝。
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
            if plan.steps.is_empty() {
                return Err(AppError::Validation("plan has no steps".into()));
            }
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
            for (index, step) in plan.steps.iter().enumerate() {
                let res = if review.step_levels[index] == crate::core::types::RiskLevel::Low {
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
                    Err(e) => {
                        executions.push(crate::audit::record_failed_command(&step.command, &e.to_string()));
                        failed = true;
                        break;
                    }
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
        // 用内存 store + mock 凭据/引擎构造一个最小可用的 AppState。
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
    async fn execute_rejects_empty_plan() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let server = state
            .store
            .create_server(crate::core::types::ServerInput {
                name: "web".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "root".into(),
                auth_kind: crate::core::types::AuthKind::Agent,
            })
            .unwrap();
        let plan = crate::core::types::Plan {
            id: crate::core::types::new_id(),
            server_id: Some(server.id),
            goal: "空计划".into(),
            steps: vec![],
            created_at: crate::core::types::now(),
        };

        let err = dispatch(
            &state,
            "task.execute_confirmed",
            json!({ "plan": plan, "confirmed": true }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code(), "validation");
        assert!(err.to_string().contains("no steps"));
    }

    #[tokio::test]
    async fn execute_uses_server_risk_review_not_client_read_only_flag() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let server = state
            .store
            .create_server(crate::core::types::ServerInput {
                name: "web".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "root".into(),
                auth_kind: crate::core::types::AuthKind::Agent,
            })
            .unwrap();
        let plan = crate::core::types::Plan {
            id: crate::core::types::new_id(),
            server_id: Some(server.id),
            goal: "伪造只读".into(),
            steps: vec![crate::core::types::PlanStep {
                summary: "delete files".into(),
                command: "rm -rf /var/www/old".into(),
                risk: crate::core::types::RiskLevel::Low,
                read_only: true,
                tool: None,
            }],
            created_at: crate::core::types::now(),
        };

        let err = dispatch(
            &state,
            "task.execute_confirmed",
            json!({ "plan": plan, "confirmed": true }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code(), "blocked");
        assert!(err.to_string().contains("second confirmation"));
    }

    #[tokio::test]
    async fn execute_records_failed_command_when_ssh_call_errors() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let server = state
            .store
            .create_server(crate::core::types::ServerInput {
                name: "web".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "root".into(),
                auth_kind: crate::core::types::AuthKind::Password,
            })
            .unwrap();
        let plan = crate::core::types::Plan {
            id: crate::core::types::new_id(),
            server_id: Some(server.id),
            goal: "失败审计".into(),
            steps: vec![crate::core::types::PlanStep {
                summary: "执行检查".into(),
                command: "uptime".into(),
                risk: crate::core::types::RiskLevel::Low,
                read_only: true,
                tool: None,
            }],
            created_at: crate::core::types::now(),
        };

        let value = dispatch(
            &state,
            "task.execute_confirmed",
            json!({ "plan": plan, "confirmed": true }),
        )
        .await
        .unwrap();
        let record: crate::core::types::AuditRecord = serde_json::from_value(value).unwrap();

        assert_eq!(record.status, crate::core::types::TaskStatus::Failed);
        assert_eq!(record.executions.len(), 1);
        assert_eq!(record.executions[0].command, "uptime");
        assert_eq!(record.executions[0].exit_code, -1);
        assert!(record.executions[0].stderr.contains("no SSH password stored"));
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
