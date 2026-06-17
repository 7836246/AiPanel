//! 服务器体检（Doctor）：只读健康检查。
//!
//! 通过 SSH 执行一组固定的检查类命令（按 Risk Reviewer 判定全部为 `Low`），
//! 并汇总成一份 [`DoctorReport`]。诊断模式永远不改变服务器状态
//!（docs/SECURITY_MODEL.zh-Hans.md）。这里的命令本身就是体检流程的只读白名单。

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::core::error::AppResult;
use crate::core::types::{
    new_id, now, CommandExecution, DoctorReport, Plan, PlanStep, RiskLevel, ServerProfile,
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// 每个只读探针的 (key, 摘要, 命令)。
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

/// 体检将要执行的只读计划，用于在运行前展示给用户。
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

/// 体检运行过程中发出的流式事件，让前端终端可以实时填充而非一次性输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DoctorStreamEvent {
    /// 某个探针开始 / 结束。`status` 为 "running" | "done" | "failed"。
    Step {
        index: usize,
        total: usize,
        summary: String,
        status: String,
    },
    /// 当前探针输出的单行（已脱敏）。
    Line { text: String, stderr: bool },
    /// 从采集到的输出中推导出的健康告警。
    Warning { message: String },
}

/// 从采集到的探针输出 + 告警汇总出一份 [`DoctorReport`]。由 [`run_doctor`] 和
/// [`run_doctor_streamed`] 共用，保证两者产出的结构一致。
fn build_report(
    server_id: &str,
    out: &BTreeMap<&str, String>,
    warnings: Vec<String>,
    executions: Vec<CommandExecution>,
) -> DoctorReport {
    let os = out.get("os").and_then(|s| parse_pretty_name(s));
    DoctorReport {
        server_id: server_id.to_string(),
        os,
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
    }
}

/// 将单个探针的结果记入累积中的 output/warnings/executions，与 run_doctor 的
/// 记账方式完全一致。返回本次探针新增的告警，以便流式调用方能够即时展示。
fn record_probe(
    key: &'static str,
    result: AppResult<CommandExecution>,
    out: &mut BTreeMap<&'static str, String>,
    warnings: &mut Vec<String>,
    executions: &mut Vec<CommandExecution>,
) -> Vec<String> {
    let mut emitted = Vec::new();
    match result {
        Ok(exec) => {
            if exec.exit_code == 0 {
                out.insert(key, exec.stdout.trim().to_string());
            } else if key != "docker" {
                // docker 经常缺失——不值得为此发告警
                emitted.push(format!("{key} 探测失败 (exit {})", exec.exit_code));
            }
            executions.push(exec);
        }
        Err(e) => emitted.push(format!("{key} 探测错误: {}", e.code())),
    }
    warnings.extend(emitted.iter().cloned());
    emitted
}

/// 检查磁盘输出是否使用率过高，若是则追加一条告警。若新增了告警则将其返回，
/// 以便流式调用方能够展示。
fn check_disk_pressure(out: &BTreeMap<&str, String>, warnings: &mut Vec<String>) -> Option<String> {
    if let Some(disk) = out.get("disk") {
        if disk_under_pressure(disk) {
            let w = "磁盘使用率偏高（≥90%）".to_string();
            warnings.push(w.clone());
            return Some(w);
        }
    }
    None
}

/// 通过 SSH 运行体检。返回汇总后的报告以及每一次命令执行记录。
pub async fn run_doctor(
    server: &ServerProfile,
    secret: Option<&str>,
) -> AppResult<DoctorReport> {
    let mut executions: Vec<CommandExecution> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut out: BTreeMap<&str, String> = BTreeMap::new();

    for (key, _summary, cmd) in PROBES {
        let result = crate::ssh::run_readonly(server, secret, cmd, PROBE_TIMEOUT).await;
        // `key` 的类型是 `&&'static str`；record_probe 接收 `&'static str`。
        record_probe(*key, result, &mut out, &mut warnings, &mut executions);
    }

    check_disk_pressure(&out, &mut warnings);

    Ok(build_report(&server.id, &out, warnings, executions))
}

/// [`run_doctor`] 的流式版本：运行时通过 `on_event` 发出按步骤 / 按行的事件，
/// 最后返回与之相同结构的 [`DoctorReport`]。只读安全性不变——每个探针仍然
/// 走 [`crate::ssh::run_readonly_streamed`]。
pub async fn run_doctor_streamed(
    server: &ServerProfile,
    secret: Option<&str>,
    on_event: &(dyn Fn(DoctorStreamEvent) + Sync + Send),
) -> AppResult<DoctorReport> {
    let mut executions: Vec<CommandExecution> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut out: BTreeMap<&str, String> = BTreeMap::new();
    let total = PROBES.len();

    for (index, (key, summary, cmd)) in PROBES.iter().enumerate() {
        on_event(DoctorStreamEvent::Step {
            index,
            total,
            summary: summary.to_string(),
            status: "running".to_string(),
        });

        let result = crate::ssh::run_readonly_streamed(
            server,
            secret,
            cmd,
            PROBE_TIMEOUT,
            &|line: &str, stderr: bool| {
                on_event(DoctorStreamEvent::Line { text: line.to_string(), stderr });
            },
        )
        .await;

        // 本次探针是否成功？沿用 run_doctor 的记账逻辑：docker 非零退出是良性的
        //（docker 经常缺失），因此不将其标记为失败。
        let ok = matches!(&result, Ok(exec) if exec.exit_code == 0) || *key == "docker";
        let emitted = record_probe(*key, result, &mut out, &mut warnings, &mut executions);
        for message in emitted {
            on_event(DoctorStreamEvent::Warning { message });
        }

        on_event(DoctorStreamEvent::Step {
            index,
            total,
            summary: summary.to_string(),
            status: if ok { "done".to_string() } else { "failed".to_string() },
        });
    }

    if let Some(message) = check_disk_pressure(&out, &mut warnings) {
        on_event(DoctorStreamEvent::Warning { message });
    }

    Ok(build_report(&server.id, &out, warnings, executions))
}

/// 用于缓存到服务器卡片上的精简信息（facts）。
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

/// 从 /etc/os-release 内容中解析出 PRETTY_NAME（发行版友好名称）。
fn parse_pretty_name(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        line.strip_prefix("PRETTY_NAME=")
            .map(|v| v.trim_matches('"').to_string())
    })
}

/// 按行拆分，过滤空行并去除每行首尾空白。
fn split_lines(s: &str) -> Vec<String> {
    s.lines().filter(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).collect()
}

/// 判断 df 输出中是否存在任一分区使用率达到或超过 90%。
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
        // 每个探针都必须被判定为 Low，否则 run_readonly 会拦截它
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
