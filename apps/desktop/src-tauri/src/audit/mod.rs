//! 审计日志。
//!
//! 每次执行都在本地记录：用户意图、计划、风险审查、确认、实际命令、退出码、
//! **脱敏后的**输出，以及总结（docs/SECURITY_MODEL.zh-Hans.md）。绝不记录密钥
//! ——执行记录携带的已是脱敏输出，而计划/意图本身也不得包含密钥。本模块只负责
//! 构建记录；持久化在 store 中完成。

use crate::core::types::{
    new_id, now, AuditRecord, CommandExecution, DoctorReport, Plan, RiskReview, TaskStatus,
};

/// 构造一条“命令未能形成正常执行结果”的失败记录。
///
/// 例如 ssh 客户端启动失败、连接建立前出错、stdin/stdout 捕获失败等场景，本来不会
/// 产生 [`CommandExecution`]。审计仍需要保留失败命令和脱敏错误，否则只能看到
/// “0/N 步成功”，缺少复盘证据。
pub fn record_failed_command(command: &str, error: &str) -> CommandExecution {
    let ts = now();
    CommandExecution {
        command: command.to_string(),
        exit_code: -1,
        stdout: String::new(),
        stderr: crate::core::sanitize::sanitize(error),
        duration_ms: 0,
        started_at: ts,
    }
}

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
    let total = plan.steps.len();
    let attempted = executions.len();
    let summary = if attempted < total {
        format!("{ok}/{total} 步成功（已执行 {attempted}/{total} 步）")
    } else {
        format!("{ok}/{total} 步成功")
    };
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

/// 为一次交互式终端会话构建元数据审计记录。
///
/// 终端是逐键交互流，不把按键、屏幕输出或远端命令内容写入审计；这里只记录会话
/// 打开这类元数据，满足可追踪性，同时避免把敏感终端内容落库。
pub fn record_for_terminal_open(
    server_id: &str,
    session_id: &str,
    cols: u16,
    rows: u16,
) -> AuditRecord {
    let ts = now();
    let session_hint: String = session_id.chars().take(8).collect();
    AuditRecord {
        id: new_id(),
        server_id: Some(server_id.to_string()),
        intent: "打开交互式 SSH 终端".to_string(),
        plan: None,
        risk_review: None,
        confirmed_at: None,
        executions: vec![],
        summary: Some(format!(
            "已打开交互式终端会话 {session_hint}...，初始尺寸 {cols}x{rows}；未记录按键或终端输出"
        )),
        status: TaskStatus::Completed,
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
    use crate::core::types::{CommandExecution, PlanStep, RiskLevel};

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

    fn plan_with_steps(count: usize) -> Plan {
        Plan {
            id: new_id(),
            server_id: Some("s".into()),
            goal: "测试计划".into(),
            steps: (0..count)
                .map(|i| PlanStep {
                    summary: format!("step {i}"),
                    command: "true".into(),
                    risk: RiskLevel::Low,
                    read_only: true,
                    tool: None,
                })
                .collect(),
            created_at: now(),
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

    #[test]
    fn plan_record_summary_uses_total_plan_steps() {
        let exec = CommandExecution {
            command: "true".into(),
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 1,
            started_at: now(),
        };
        let rec = record_for_plan(
            Some("s"),
            "执行测试",
            plan_with_steps(2),
            review(),
            vec![exec],
            TaskStatus::Failed,
        );
        assert_eq!(rec.summary.as_deref(), Some("1/2 步成功（已执行 1/2 步）"));
    }

    #[test]
    fn plan_record_summary_does_not_report_zero_total_when_no_execution_started() {
        let rec = record_for_plan(
            Some("s"),
            "执行测试",
            plan_with_steps(2),
            review(),
            vec![],
            TaskStatus::Failed,
        );
        assert_eq!(rec.summary.as_deref(), Some("0/2 步成功（已执行 0/2 步）"));
    }

    #[test]
    fn failed_command_record_preserves_command_and_sanitizes_error() {
        let exec = record_failed_command(
            "ssh root@example 'true'",
            "failed with bearer sk-secret-token from 10.0.0.4",
        );
        assert_eq!(exec.command, "ssh root@example 'true'");
        assert_eq!(exec.exit_code, -1);
        assert!(exec.stdout.is_empty());
        assert!(exec.stderr.contains("Bearer [redacted]"));
        assert!(exec.stderr.contains("[redacted-ip]"));
        assert!(!exec.stderr.contains("sk-secret-token"));
        assert!(!exec.stderr.contains("10.0.0.4"));
    }

    #[test]
    fn terminal_open_record_contains_only_session_metadata() {
        let rec = record_for_terminal_open("s", "12345678-secret-rest", 100, 30);
        assert_eq!(rec.server_id.as_deref(), Some("s"));
        assert_eq!(rec.intent, "打开交互式 SSH 终端");
        assert_eq!(rec.status, TaskStatus::Completed);
        assert!(rec.plan.is_none());
        assert!(rec.risk_review.is_none());
        assert!(rec.executions.is_empty());
        let summary = rec.summary.unwrap();
        assert!(summary.contains("12345678"));
        assert!(summary.contains("100x30"));
        assert!(summary.contains("未记录按键或终端输出"));
        assert!(!summary.contains("secret-rest"));
    }
}
