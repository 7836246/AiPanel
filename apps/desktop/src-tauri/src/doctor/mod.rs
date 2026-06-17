//! Server Doctor: a read-only health check.
//!
//! Runs a fixed set of inspection commands (all `Low` per the Risk Reviewer)
//! over SSH and assembles a [`DoctorReport`]. Diagnosis mode never changes
//! server state (docs/SECURITY_MODEL.zh-Hans.md). The commands here ARE the
//! read-only allowlist for the doctor flow.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::core::error::AppResult;
use crate::core::types::{
    new_id, now, CommandExecution, DoctorReport, Plan, PlanStep, RiskLevel, ServerProfile,
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// (key, summary, command) for each read-only probe.
const PROBES: &[(&str, &str, &str)] = &[
    ("os", "操作系统", "cat /etc/os-release"),
    ("kernel", "内核", "uname -rs"),
    ("arch", "架构", "uname -m"),
    ("uptime", "运行时长", "uptime -p"),
    ("load", "负载", "cat /proc/loadavg"),
    ("memory", "内存", "free -m"),
    ("disk", "磁盘", "df -h"),
    ("ports", "监听端口", "ss -ltn"),
    (
        "services",
        "运行中的服务",
        "systemctl list-units --type=service --state=running --no-pager --no-legend",
    ),
    ("docker", "Docker", "docker ps --format '{{.Names}} {{.Status}}'"),
];

/// The read-only plan the doctor will execute, for showing the user before running.
pub fn doctor_plan(server_id: &str) -> Plan {
    Plan {
        id: new_id(),
        server_id: Some(server_id.to_string()),
        goal: "只读服务器体检：采集系统、资源、端口、服务与容器状态".to_string(),
        steps: PROBES
            .iter()
            .map(|(_, summary, cmd)| PlanStep {
                summary: summary.to_string(),
                command: cmd.to_string(),
                risk: RiskLevel::Low,
                read_only: true,
                tool: None,
            })
            .collect(),
        created_at: now(),
    }
}

/// Run the doctor over SSH. Returns the assembled report plus every execution.
pub async fn run_doctor(
    server: &ServerProfile,
    secret: Option<&str>,
) -> AppResult<DoctorReport> {
    let mut executions: Vec<CommandExecution> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut out: BTreeMap<&str, String> = BTreeMap::new();

    for (key, _summary, cmd) in PROBES {
        match crate::ssh::run_readonly(server, secret, cmd, PROBE_TIMEOUT).await {
            Ok(exec) => {
                if exec.exit_code == 0 {
                    out.insert(key, exec.stdout.trim().to_string());
                } else if *key != "docker" {
                    // docker often absent — not worth a warning
                    warnings.push(format!("{key} 探测失败 (exit {})", exec.exit_code));
                }
                executions.push(exec);
            }
            Err(e) => warnings.push(format!("{key} 探测错误: {}", e.code())),
        }
    }

    if let Some(disk) = out.get("disk") {
        if disk_under_pressure(disk) {
            warnings.push("磁盘使用率偏高（≥90%）".to_string());
        }
    }

    let os = out.get("os").and_then(|s| parse_pretty_name(s));
    let report = DoctorReport {
        server_id: server.id.clone(),
        os: os.clone(),
        kernel: out.get("kernel").cloned(),
        arch: out.get("arch").cloned(),
        uptime: out.get("uptime").cloned(),
        load: out.get("load").cloned(),
        memory: out.get("memory").cloned(),
        disk: out.get("disk").cloned(),
        ports: out.get("ports").map(|s| split_lines(s)).unwrap_or_default(),
        services: out.get("services").map(|s| split_lines(s)).unwrap_or_default(),
        docker: out.get("docker").cloned(),
        warnings,
        executions,
        created_at: now(),
    };
    Ok(report)
}

/// Compact facts for caching on the server card.
pub fn facts_from_report(report: &DoctorReport) -> BTreeMap<String, String> {
    let mut facts = BTreeMap::new();
    if let Some(os) = &report.os {
        facts.insert("OS".to_string(), os.clone());
    }
    if let Some(arch) = &report.arch {
        facts.insert("Arch".to_string(), arch.clone());
    }
    if let Some(uptime) = &report.uptime {
        facts.insert("Uptime".to_string(), uptime.clone());
    }
    facts
}

fn parse_pretty_name(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        line.strip_prefix("PRETTY_NAME=")
            .map(|v| v.trim_matches('"').to_string())
    })
}

fn split_lines(s: &str) -> Vec<String> {
    s.lines().filter(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).collect()
}

fn disk_under_pressure(df_output: &str) -> bool {
    df_output.split_whitespace().any(|tok| {
        tok.strip_suffix('%')
            .and_then(|n| n.parse::<u32>().ok())
            .map(|pct| pct >= 90)
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_steps_are_all_read_only_low() {
        let p = doctor_plan("s1");
        assert_eq!(p.steps.len(), PROBES.len());
        assert!(p.steps.iter().all(|s| s.read_only && s.risk == RiskLevel::Low));
        // every probe must classify Low, or run_readonly would block it
        for s in &p.steps {
            assert_eq!(crate::risk::classify_command(&s.command).level, RiskLevel::Low);
        }
    }

    #[test]
    fn parses_pretty_name() {
        let s = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 22.04.3 LTS\"\nID=ubuntu";
        assert_eq!(parse_pretty_name(s).as_deref(), Some("Ubuntu 22.04.3 LTS"));
    }

    #[test]
    fn detects_disk_pressure() {
        assert!(disk_under_pressure("/dev/sda1  50G  47G  3G  94% /"));
        assert!(!disk_under_pressure("/dev/sda1  50G  20G  30G  40% /"));
    }

    #[test]
    fn facts_extracted() {
        let mut r = DoctorReport {
            server_id: "s".into(),
            os: Some("Ubuntu 22.04".into()),
            kernel: None,
            arch: Some("x86_64".into()),
            uptime: Some("up 2 hours".into()),
            load: None,
            memory: None,
            disk: None,
            ports: vec![],
            services: vec![],
            docker: None,
            warnings: vec![],
            executions: vec![],
            created_at: now(),
        };
        let f = facts_from_report(&r);
        assert_eq!(f.get("OS").unwrap(), "Ubuntu 22.04");
        assert_eq!(f.get("Arch").unwrap(), "x86_64");
        r.os = None;
        assert!(!facts_from_report(&r).contains_key("OS"));
    }
}
