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

/// 在作用域结束（含 `?`/`return`/panic 提前退出）时自动注销取消句柄，避免句柄泄漏。
struct RunGuard(String);

impl Drop for RunGuard {
    fn drop(&mut self) {
        crate::ssh::unregister(&self.0);
    }
}

/// 请求中断某次正在运行的流式命令（doctor 或已确认计划）。`run_id` 必须与启动该次
/// 运行时传入的一致。找不到（已结束 / 从未登记）时静默忽略。取消是协作式的：正在
/// 运行的流式循环会 select 到取消信号，杀掉本地 ssh 子进程并提前结束；已执行的步骤
/// 照常落库 / 审计。
#[tauri::command]
pub fn cancel_run(run_id: String) {
    crate::ssh::cancel(&run_id);
}

/// 执行只读的服务器 doctor，并在过程中按步 / 按行把事件流式推给前端。返回与
/// [`super::run_server_doctor`] 相同的 [`DoctorReport`]，收尾逻辑也完全一致：
/// 把状态 + 快速事实缓存到服务器上，并写入一条本地审计记录。
#[tauri::command]
pub async fn run_server_doctor_stream(
    state: tauri::State<'_, AppState>,
    id: String,
    run_id: String,
    on_event: Channel<crate::doctor::DoctorStreamEvent>,
) -> AppResult<DoctorReport> {
    // 登记取消句柄；无论成功/失败/取消都要注销，这里用一个 drop guard 兜底。
    let cancel = crate::ssh::register(&run_id);
    let _guard = RunGuard(run_id.clone());

    // 取出服务器及其 SSH 密钥（若其认证方式存有密钥）。
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };

    // 取消被下推到 doctor 的探针循环：把已登记的 cancel 直接传给 run_doctor_streamed。
    // 取消时它会停止后续探针，并用「已执行的部分」构建报告正常返回——因此无论正常
    // 完成还是中途取消，都用返回的（可能是部分的）报告统一收尾：设置服务器状态、
    // 构建并写入审计记录（已执行探针照常入审计）。
    let on_doctor_event = |ev| {
        // 发送失败（例如通道被丢弃）不能中断本次执行。
        let _ = on_event.send(ev);
    };
    let report =
        crate::doctor::run_doctor_streamed(&server, secret.as_deref(), &on_doctor_event, &cancel)
            .await?;

    let succeeded = report.executions.iter().any(|e| e.exit_code == 0);
    let status = if succeeded { ServerStatus::Online } else { ServerStatus::Offline };
    let facts = crate::doctor::facts_from_report(&report);
    // 取消或全失败的体检可能产出空/部分 facts；为空时传 None 以保留上次缓存的完整 facts，
    // 仅在 facts 非空时才覆盖。
    let facts_arg = if facts.is_empty() { None } else { Some(&facts) };
    state.store.set_server_status(&id, status, facts_arg)?;

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
    run_id: String,
    on_event: Channel<PlanExecEvent>,
) -> AppResult<AuditRecord> {
    // 登记取消句柄；RunGuard 在任意退出路径上注销它。
    let cancel = crate::ssh::register(&run_id);
    let _guard = RunGuard(run_id.clone());

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

    // 空计划没有任何步骤可执行；提前拒绝，避免产生一条无内容的审计记录。
    if plan.steps.is_empty() {
        return Err(AppError::Validation("plan has no steps".into()));
    }

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
    let mut cancelled = false;
    for (index, step) in plan.steps.iter().enumerate() {
        on_event
            .send(PlanExecEvent::Step {
                index,
                total,
                summary: step.summary.clone(),
                status: "running".to_string(),
            })
            .ok();

        // 按「服务端重判的等级」而非过时的客户端 step.read_only 路由：用户编辑某步后，
        // step.read_only 可能已过时。Low 等级步骤仍要过 Low 校验门；其余步骤已在上方
        // 通过确认，走不带校验门的流式执行器。两者都用可取消版本，把 cancel 句柄透传进流式循环。
        let on_line = |line: &str, stderr: bool| {
            // 发送失败（例如通道被丢弃）不能中断本次执行。
            on_event.send(PlanExecEvent::Line { text: line.to_string(), stderr }).ok();
        };
        let res = if review.step_levels[index] == crate::core::types::RiskLevel::Low {
            crate::ssh::run_readonly_streamed_cancellable(
                &server,
                secret.as_deref(),
                &step.command,
                crate::ssh::DEFAULT_TIMEOUT,
                &on_line,
                &cancel,
            )
            .await
        } else {
            crate::ssh::run_command_streamed_cancellable(
                &server,
                secret.as_deref(),
                &step.command,
                crate::ssh::DEFAULT_TIMEOUT,
                &on_line,
                &cancel,
            )
            .await
        };

        match res {
            // 正常完成一步。
            Ok(Some(exec)) => {
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
            // 本步被用户取消：停止后续步骤，已完成的步骤照常落库 / 审计。
            Ok(None) => {
                on_event
                    .send(PlanExecEvent::Step {
                        index,
                        total,
                        summary: step.summary.clone(),
                        status: "failed".to_string(),
                    })
                    .ok();
                cancelled = true;
                break;
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

    // 取消与失败一样视为未成功完成（已执行的步骤仍记入审计）。
    let status = if failed || cancelled { TaskStatus::Failed } else { TaskStatus::Completed };
    let exit_code = executions.last().map(|e| e.exit_code).unwrap_or(-1);
    on_event
        .send(PlanExecEvent::Done {
            status: if failed || cancelled { "failed".to_string() } else { "done".to_string() },
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
