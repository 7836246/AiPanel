//! Streaming Tauri command(s).
//!
//! These mirror the blocking commands in [`super`] but push live progress to the
//! frontend over a [`tauri::ipc::Channel`] as the work happens. The security
//! boundary is unchanged — streaming goes through the same read-only SSH path,
//! caches status/facts, and writes the same local audit record.

use tauri::ipc::Channel;

use crate::core::error::AppResult;
use crate::core::types::{DoctorReport, ServerStatus};
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
