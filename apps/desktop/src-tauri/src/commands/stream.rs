//! 流式 Tauri 命令。
//!
//! 这些命令对应 [`super`] 里的阻塞版，但在执行过程中通过
//! [`tauri::ipc::Channel`] 把实时进度推送给前端。安全边界不变——流式仍走相同的
//! 只读 SSH 路径，缓存状态/事实，并写入相同的本地审计记录。

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::core::error::{AppError, AppResult};
use crate::core::types::{AuditRecord, DoctorReport, Plan, ServerStatus, TaskStatus};
use crate::AppState;

/// 执行只读的服务器 doctor，并在过程中按步 / 按行把事件流式推给前端。返回与
/// [`super::run_server_doctor`] 相同的 [`DoctorReport`]，收尾逻辑也完全一致：
/// 把状态 + 快速事实缓存到服务器上，并写入一条本地审计记录。
#[tauri::command]
pub async fn run_server_doctor_stream(
    state: tauri::State<'_, AppState>,
    id: String,
    on_event: Channel<crate::doctor::DoctorStreamEvent>,
) -> AppResult<DoctorReport> {
    // 取出服务器及其 SSH 密钥（若其认证方式存有密钥）。
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };

    let report = crate::doctor::run_doctor_streamed(&server, secret.as_deref(), &|ev| {
        // 发送失败（例如通道被丢弃）不能中断本次执行。
        let _ = on_event.send(ev);
    })
    .await?;

    let succeeded = report.executions.iter().any(|e| e.exit_code == 0);
    let status = if succeeded { ServerStatus::Online } else { ServerStatus::Offline };
    let facts = crate::doctor::facts_from_report(&report);
    state.store.set_server_status(&id, status, Some(&facts))?;

    // 每次执行都在本地审计。
    let plan = crate::doctor::doctor_plan(&id);
    let review = crate::risk::review_plan(&plan, true); // doctor 以只读模式运行
    let record = crate::audit::record_for_doctor(&id, plan, review, &report);
    state.store.insert_audit_record(&record)?;

    Ok(report)
}

/// 已确认计划执行过程中发出的流式事件，让控制台能随每一步实时填充，而非一次性出结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlanExecEvent {
    /// 某一步开始 / 结束。`status` 取 "running" | "done" | "failed"。
    Step {
        index: usize,
        total: usize,
        summary: String,
        status: String,
    },
    /// 当前步骤输出的单行（已脱敏）内容。
    Line { text: String, stderr: bool },
    /// 整个执行结束。`status` 取 "done" | "failed"；`exit_code` 是最后一步的
    /// 退出码（若某步在产生退出码前就出错，则为 -1）。
    Done { status: String, exit_code: i32 },
}

/// 执行用户已确认的计划，并在每一步运行时按步 / 按行把事件流式推给前端。
/// 安全边界与 [`super::execute_confirmed_plan`] **完全一致**：计划总是在服务端
/// 重新审查（绝不信任客户端），拒绝被 Blocked 的步骤，并在任何命令运行前
/// 强制要求达到所需确认级别。返回相同的 [`AuditRecord`]。
#[tauri::command]
pub async fn run_confirmed_plan_stream(
    state: tauri::State<'_, AppState>,
    plan: Plan,
    confirmed: bool,
    double_confirmed: bool,
    read_only_mode: bool,
    on_event: Channel<PlanExecEvent>,
) -> AppResult<AuditRecord> {
    let server_id = plan
        .server_id
        .clone()
        .ok_or_else(|| AppError::Validation("plan has no target server".into()))?;
    // 取出服务器及其 SSH 密钥（若其认证方式存有密钥）。
    let server = state.store.get_server(&server_id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };

    let review = crate::risk::review_plan(&plan, read_only_mode);
    if review.blocked {
        return Err(AppError::Blocked("plan contains blocked steps".into()));
    }
    if review.requires_confirmation && !confirmed {
        return Err(AppError::Blocked("plan requires confirmation".into()));
    }
    if review.requires_double_confirmation && !double_confirmed {
        return Err(AppError::Blocked("plan requires a second confirmation".into()));
    }

    let total = plan.steps.len();
    let mut executions = Vec::new();
    let mut failed = false;
    for (index, step) in plan.steps.iter().enumerate() {
        on_event
            .send(PlanExecEvent::Step {
                index,
                total,
                summary: step.summary.clone(),
                status: "running".to_string(),
            })
            .ok();

        // 只读步骤仍要过 Low 等级的校验门；其余步骤已在上方通过确认，走不带
        // 校验门的流式执行器。
        let on_line = |line: &str, stderr: bool| {
            // 发送失败（例如通道被丢弃）不能中断本次执行。
            on_event.send(PlanExecEvent::Line { text: line.to_string(), stderr }).ok();
        };
        let res = if step.read_only {
            crate::ssh::run_readonly_streamed(
                &server,
                secret.as_deref(),
                &step.command,
                crate::ssh::DEFAULT_TIMEOUT,
                &on_line,
            )
            .await
        } else {
            crate::ssh::run_command_streamed(
                &server,
                secret.as_deref(),
                &step.command,
                crate::ssh::DEFAULT_TIMEOUT,
                &on_line,
            )
            .await
        };

        match res {
            Ok(exec) => {
                let bad = exec.exit_code != 0;
                executions.push(exec);
                on_event
                    .send(PlanExecEvent::Step {
                        index,
                        total,
                        summary: step.summary.clone(),
                        status: if bad { "failed".to_string() } else { "done".to_string() },
                    })
                    .ok();
                if bad {
                    failed = true;
                    break;
                }
            }
            Err(_) => {
                on_event
                    .send(PlanExecEvent::Step {
                        index,
                        total,
                        summary: step.summary.clone(),
                        status: "failed".to_string(),
                    })
                    .ok();
                failed = true;
                break;
            }
        }
    }

    let status = if failed { TaskStatus::Failed } else { TaskStatus::Completed };
    let exit_code = executions.last().map(|e| e.exit_code).unwrap_or(-1);
    on_event
        .send(PlanExecEvent::Done {
            status: if failed { "failed".to_string() } else { "done".to_string() },
            exit_code,
        })
        .ok();

    // 每次执行都在本地审计。
    let intent = plan.goal.clone();
    let record =
        crate::audit::record_for_plan(Some(&server_id), &intent, plan, review, executions, status);
    state.store.insert_audit_record(&record)?;
    Ok(record)
}
