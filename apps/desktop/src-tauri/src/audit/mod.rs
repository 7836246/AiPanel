//! 审计日志。
//!
//! 每次执行都在本地记录：用户意图、计划、风险审查、确认、实际命令、退出码、
//! **脱敏后的**输出，以及总结（docs/SECURITY_MODEL.zh-Hans.md）。绝不记录密钥
//! ——执行记录携带的已是脱敏输出，而计划/意图本身也不得包含密钥。本模块只负责
//! 构建记录；持久化在 store 中完成。

use crate::core::types::{
    new_id, now, AuditRecord, DoctorReport, Plan, RiskReview, TaskStatus,
};

/// 为一次完成的（只读）体检运行构建审计记录。
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

/// 为一次已确认的计划执行构建审计记录。
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

/// 根据体检报告生成一行简短总结（操作系统 · 成功项数 · 告警数）。
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
            cpu_percent: None,
            mem_used_mb: None,
            mem_total_mb: None,
            disk_used_percent: None,
            service_count: None,
            container_count: None,
            port_count: None,
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
