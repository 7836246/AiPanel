//! Streaming Tauri command(s).
//!
//! These mirror the blocking commands in [`super`] but push live progress to the
//! frontend over a [`tauri::ipc::Channel`] as the work happens. The security
//! boundary is unchanged — streaming goes through the same read-only SSH path,
//! caches status/facts, and writes the same local audit record.

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::core::error::{AppError, AppResult};
use crate::core::types::{AuditRecord, DoctorReport, Plan, ServerStatus, TaskStatus};
use crate::AppState;

/// Run the read-only server doctor, streaming per-step / per-line events to the
/// frontend as they happen. Returns the same [`DoctorReport`] as
/// [`super::run_server_doctor`], and mirrors its tail exactly: cache status +
/// quick facts on the server, and write a local audit record.
#[tauri::command]
pub async fn run_server_doctor_stream(
    state: tauri::State<'_, AppState>,
    id: String,
    on_event: Channel<crate::doctor::DoctorStreamEvent>,
) -> AppResult<DoctorReport> {
    // Resolve the server and its SSH secret (if its auth method stores one).
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };

    let report = crate::doctor::run_doctor_streamed(&server, secret.as_deref(), &|ev| {
        // Send failures (e.g. the channel was dropped) must not abort the run.
        let _ = on_event.send(ev);
    })
    .await?;

    let succeeded = report.executions.iter().any(|e| e.exit_code == 0);
    let status = if succeeded { ServerStatus::Online } else { ServerStatus::Offline };
    let facts = crate::doctor::facts_from_report(&report);
    state.store.set_server_status(&id, status, Some(&facts))?;

    // Every execution is audited locally.
    let plan = crate::doctor::doctor_plan(&id);
    let review = crate::risk::review_plan(&plan, true); // doctor runs in read-only mode
    let record = crate::audit::record_for_doctor(&id, plan, review, &report);
    state.store.insert_audit_record(&record)?;

    Ok(report)
}

/// A streaming event emitted while a confirmed plan executes, so the console can
/// fill in live as each step runs instead of all-at-once.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlanExecEvent {
    /// A step started / finished. `status` is "running" | "done" | "failed".
    Step {
        index: usize,
        total: usize,
        summary: String,
        status: String,
    },
    /// A single line of (sanitized) output from the current step.
    Line { text: String, stderr: bool },
    /// The whole run finished. `status` is "done" | "failed"; `exit_code` is the
    /// last step's exit code (or -1 if a step errored before producing one).
    Done { status: String, exit_code: i32 },
}

/// Execute a user-confirmed plan, streaming per-step / per-line events to the
/// frontend as each step runs. Mirrors [`super::execute_confirmed_plan`]'s
/// security boundary EXACTLY: the plan is ALWAYS re-reviewed server-side (never
/// trust the client), blocked steps are rejected, and the required confirmation
/// level is enforced before anything runs. Returns the same [`AuditRecord`].
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
    // Resolve the server and its SSH secret (if its auth method stores one).
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

        // Read-only steps still go through the Low-classification gate; everything
        // else has already cleared confirmation above and runs via the ungated
        // streaming executor.
        let on_line = |line: &str, stderr: bool| {
            // Send failures (e.g. the channel was dropped) must not abort the run.
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

    // Every execution is audited locally.
    let intent = plan.goal.clone();
    let record =
        crate::audit::record_for_plan(Some(&server_id), &intent, plan, review, executions, status);
    state.store.insert_audit_record(&record)?;
    Ok(record)
}
