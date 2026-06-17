//! Audit Log.
//!
//! Every execution is recorded locally: the user's intent, the plan, the risk
//! review, the confirmation, the actual commands, exit codes, **sanitized**
//! output, and a summary (docs/SECURITY_MODEL.zh-Hans.md). Secrets are never
//! recorded — executions already carry sanitized output, and plans/intents must
//! not contain secrets. This module only builds records; persistence lives in
//! the store.

use crate::core::types::{
    new_id, now, AuditRecord, DoctorReport, Plan, RiskReview, TaskStatus,
};

/// Build an audit record for a completed (read-only) doctor run.
pub fn record_for_doctor(
    server_id: &str,
    plan: Plan,
    review: RiskReview,
    report: &DoctorReport,
) -> AuditRecord {
    let succeeded = report.executions.iter().any(|e| e.exit_code == 0);
    let status = if succeeded { TaskStatus::Completed } else { TaskStatus::Failed };
    let summary = doctor_summary(report);
    let ts = now();
    AuditRecord {
        id: new_id(),
        server_id: Some(server_id.to_string()),
        intent: "只读服务器体检".to_string(),
        plan: Some(plan),
        risk_review: Some(review),
        confirmed_at: Some(ts),
        executions: report.executions.clone(),
        summary: Some(summary),
        status,
        created_at: ts,
        updated_at: ts,
    }
}

/// Build an audit record for a confirmed plan execution.
pub fn record_for_plan(
    server_id: Option<&str>,
    intent: &str,
    plan: Plan,
    review: RiskReview,
    executions: Vec<crate::core::types::CommandExecution>,
    status: TaskStatus,
) -> AuditRecord {
    let ok = executions.iter().filter(|e| e.exit_code == 0).count();
    let summary = format!("{}/{} 步成功", ok, executions.len());
    let ts = now();
    AuditRecord {
        id: new_id(),
        server_id: server_id.map(|s| s.to_string()),
        intent: intent.to_string(),
        plan: Some(plan),
        risk_review: Some(review),
        confirmed_at: Some(ts),
        executions,
        summary: Some(summary),
        status,
        created_at: ts,
        updated_at: ts,
    }
}

fn doctor_summary(report: &DoctorReport) -> String {
    let mut parts = Vec::new();
    if let Some(os) = &report.os {
        parts.push(os.clone());
    }
    let ok = report.executions.iter().filter(|e| e.exit_code == 0).count();
    parts.push(format!("{}/{} 项检查成功", ok, report.executions.len()));
    if !report.warnings.is_empty() {
        parts.push(format!("{} 条告警", report.warnings.len()));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{CommandExecution, RiskLevel};

    fn report(exit: i32) -> DoctorReport {
        DoctorReport {
            server_id: "s".into(),
            os: Some("Ubuntu 22.04".into()),
            kernel: None,
            arch: None,
            uptime: None,
            load: None,
            memory: None,
            disk: None,
            ports: vec![],
            services: vec![],
            docker: None,
            warnings: vec!["磁盘偏高".into()],
            executions: vec![CommandExecution {
                command: "uname -a".into(),
                exit_code: exit,
                stdout: "Linux".into(),
                stderr: String::new(),
                duration_ms: 10,
                started_at: now(),
            }],
            created_at: now(),
        }
    }

    fn review() -> RiskReview {
        RiskReview {
            overall: RiskLevel::Low,
            requires_confirmation: false,
            requires_double_confirmation: false,
            blocked: false,
            findings: vec![],
            step_levels: vec![RiskLevel::Low],
        }
    }

    #[test]
    fn records_completed_doctor() {
        let rec = record_for_doctor("s", crate::doctor::doctor_plan("s"), review(), &report(0));
        assert_eq!(rec.status, TaskStatus::Completed);
        assert!(rec.plan.is_some());
        assert!(rec.confirmed_at.is_some());
        assert!(rec.summary.unwrap().contains("Ubuntu"));
    }

    #[test]
    fn records_failed_when_no_success() {
        let rec = record_for_doctor("s", crate::doctor::doctor_plan("s"), review(), &report(1));
        assert_eq!(rec.status, TaskStatus::Failed);
    }
}
