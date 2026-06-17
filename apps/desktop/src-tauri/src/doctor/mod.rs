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

    // 从原始探测输出解析结构化指标（Doctor v2）。任一解析失败都安全降级为 None，
    // 报告其余部分不受影响。
    let mem = out.get("memory").and_then(|s| parse_mem_mb(s));
    let (mem_used_mb, mem_total_mb) = match mem {
        Some((used, total)) => (Some(used), Some(total)),
        None => (None, None),
    };
    let disk_used_percent = out.get("disk").and_then(|s| parse_root_disk_percent(s));
    let ports: Vec<String> = out.get("ports").map(|s| parse_listen_ports(s)).unwrap_or_default();
    let services: Vec<String> =
        out.get("services").map(|s| split_lines(s)).unwrap_or_default();
    let service_count = out.get("services").map(|_| services.len());
    let port_count = out.get("ports").map(|_| ports.len());
    let container_count = out.get("docker").map(|s| count_containers(s));

    DoctorReport {
        server_id: server_id.to_string(),
        os,
        kernel: out.get("kernel").cloned(),
        arch: out.get("arch").cloned(),
        uptime: out.get("uptime").cloned(),
        load: out.get("load").cloned(),
        memory: out.get("memory").cloned(),
        disk: out.get("disk").cloned(),
        ports,
        services,
        docker: out.get("docker").cloned(),
        // CPU 使用率需要两次采样 /proc/stat 才能算出，过重；这里不采集，
        // 改为在 facts 中展示 /proc/loadavg 的 1 分钟负载。
        cpu_percent: None,
        mem_used_mb,
        mem_total_mb,
        disk_used_percent,
        service_count,
        container_count,
        port_count,
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
        // 只依据已解析的「根分区」使用率判定，避免扫描所有分区（含 tmpfs / 只读层）
        // 造成的误报。阈值沿用既有 ≥90%。
        if let Some(pct) = parse_root_disk_percent(disk) {
            if disk_under_pressure(pct) {
                let w = "磁盘使用率偏高（≥90%）".to_string();
                warnings.push(w.clone());
                return Some(w);
            }
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
/// 走 [`crate::ssh::run_readonly_streamed_cancellable`]。
///
/// 取消是协作式的，并被**下推到探针循环**：一旦某次探针在执行期间被取消（返回
/// `Ok(None)`），就停止后续探针，并用「已执行的部分」构建报告**正常返回**——
/// 取消不是错误，已执行的探针照常被调用方落库 / 审计。阻塞版 [`run_doctor`] 不
/// 关心取消，使用一个永不唤醒的 Notify 即可保持兼容。
pub async fn run_doctor_streamed(
    server: &ServerProfile,
    secret: Option<&str>,
    on_event: &(dyn Fn(DoctorStreamEvent) + Sync + Send),
    cancel: &tokio::sync::Notify,
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

        let result = crate::ssh::run_readonly_streamed_cancellable(
            server,
            secret,
            cmd,
            PROBE_TIMEOUT,
            &|line: &str, stderr: bool| {
                on_event(DoctorStreamEvent::Line { text: line.to_string(), stderr });
            },
            cancel,
        )
        .await;

        // 本次探针被取消（Ok(None)）：停止后续探针，用已执行的部分正常返回。
        if matches!(&result, Ok(None)) {
            on_event(DoctorStreamEvent::Step {
                index,
                total,
                summary: summary.to_string(),
                status: "failed".to_string(),
            });
            break;
        }

        // 把 Ok(Some(exec)) 归一为 record_probe 期望的 AppResult<CommandExecution>。
        // 此处 result 必为 Ok(Some(_)) 或 Err(_)（Ok(None) 已在上方处理）。
        let probe_result = match result {
            Ok(Some(exec)) => Ok(exec),
            Ok(None) => unreachable!("Ok(None) handled above"),
            Err(e) => Err(e),
        };

        // 本次探针是否成功？沿用 run_doctor 的记账逻辑：docker 非零退出是良性的
        //（docker 经常缺失），因此不将其标记为失败。
        let ok = matches!(&probe_result, Ok(exec) if exec.exit_code == 0) || *key == "docker";
        let emitted = record_probe(*key, probe_result, &mut out, &mut warnings, &mut executions);
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
    // 负载：取 /proc/loadavg 第一列（1 分钟负载）作为更友好的展示。
    if let Some(load1) = report.load.as_deref().and_then(parse_loadavg_1m) {
        facts.insert("Load".to_string(), load1);
    }
    // 内存：以「已用/总量 MB」展示，若同时有总量则附百分比。
    if let (Some(used), Some(total)) = (report.mem_used_mb, report.mem_total_mb) {
        let mut mem = format!("{used}/{total} MB");
        if total > 0 {
            let pct = (used as f64 / total as f64 * 100.0).round() as u64;
            mem.push_str(&format!(" ({pct}%)"));
        }
        facts.insert("Memory".to_string(), mem);
    }
    // 磁盘：根分区使用率百分比。
    if let Some(pct) = report.disk_used_percent {
        facts.insert("Disk".to_string(), format!("{pct}%"));
    }
    // 服务数 / 容器数 / 端口数。
    if let Some(n) = report.service_count {
        facts.insert("Services".to_string(), n.to_string());
    }
    if let Some(n) = report.container_count {
        facts.insert("Containers".to_string(), n.to_string());
    }
    if let Some(n) = report.port_count {
        facts.insert("Ports".to_string(), n.to_string());
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

/// 从 `cat /proc/loadavg` 输出中取第一列（1 分钟平均负载）。
/// 例如 "0.52 0.58 0.59 1/234 5678" -> Some("0.52")。
fn parse_loadavg_1m(loadavg: &str) -> Option<String> {
    loadavg
        .split_whitespace()
        .next()
        .filter(|t| t.parse::<f64>().is_ok())
        .map(|t| t.to_string())
}

/// 解析 `free -m` 输出，返回 (已用 MB, 总量 MB)。
/// 定位以 "Mem:" 开头的行，取第 2 列（total）与第 3 列（used）。
/// 兼容中英文 free（首列可能是 "Mem:" 或本地化名称），优先匹配 "Mem:"。
fn parse_mem_mb(free_output: &str) -> Option<(u64, u64)> {
    let line = free_output
        .lines()
        .find(|l| l.trim_start().to_lowercase().starts_with("mem:"))?;
    let cols: Vec<&str> = line.split_whitespace().collect();
    // 形如：Mem: <total> <used> <free> ...
    let total = cols.get(1)?.parse::<u64>().ok()?;
    let used = cols.get(2)?.parse::<u64>().ok()?;
    Some((used, total))
}

/// 从 df 输出中解析根分区（挂载点为 "/"）的使用率百分比。
/// 兼容 `df` 与 `df -h`：定位最后一列为 "/" 的数据行，取其中带 '%' 的字段。
fn parse_root_disk_percent(df_output: &str) -> Option<u64> {
    for line in df_output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 数据行最后一列是挂载点；仅匹配根分区 "/"。
        if cols.last() == Some(&"/") {
            // 在该行中找带 '%' 的字段（Use% 列）。
            return cols.iter().find_map(|tok| {
                tok.strip_suffix('%').and_then(|n| n.trim().parse::<u64>().ok())
            });
        }
    }
    None
}

/// 解析 `ss -ltn` 输出中的监听端口行数（跳过表头）。
/// 表头以 "State" 开头；其余非空行视为一条监听 socket。
fn parse_listen_ports(ss_output: &str) -> Vec<String> {
    ss_output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("State"))
        .map(|l| l.to_string())
        .collect()
}

/// 统计 `docker ps --format '{{.Names}} {{.Status}}'` 输出的容器行数。
/// 输出无表头，每个非空行对应一个运行中的容器。
fn count_containers(docker_output: &str) -> usize {
    docker_output.lines().filter(|l| !l.trim().is_empty()).count()
}

/// 判断根分区使用率是否达到或超过 90%。入参为已解析出的根分区使用率百分比，
/// 避免扫描所有分区（含 tmpfs / 只读层）导致误报。
fn disk_under_pressure(root_percent: u64) -> bool {
    root_percent >= 90
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
    fn parses_loadavg_1m() {
        assert_eq!(parse_loadavg_1m("0.52 0.58 0.59 1/234 5678").as_deref(), Some("0.52"));
        assert_eq!(parse_loadavg_1m(""), None);
        assert_eq!(parse_loadavg_1m("notanumber 0.1"), None);
    }

    #[test]
    fn parses_mem_mb() {
        // free -m：表头 + Mem 行 + Swap 行
        let out = "               total        used        free      shared  buff/cache   available\n\
                   Mem:            7900        3200         900         120        3800        4300\n\
                   Swap:           2047           0        2047";
        assert_eq!(parse_mem_mb(out), Some((3200, 7900)));
        assert_eq!(parse_mem_mb("no mem line here"), None);
    }

    #[test]
    fn parses_root_disk_percent() {
        let dfh = "Filesystem      Size  Used Avail Use% Mounted on\n\
                   /dev/sda1        50G   24G   24G  51% /\n\
                   tmpfs           1.6G     0  1.6G   0% /run";
        assert_eq!(parse_root_disk_percent(dfh), Some(51));
        // 没有根分区时返回 None
        let none = "Filesystem Size Used Avail Use% Mounted on\n/dev/sdb1 10G 1G 9G 10% /data";
        assert_eq!(parse_root_disk_percent(none), None);
    }

    #[test]
    fn counts_listen_ports_skips_header() {
        let ss = "State   Recv-Q  Send-Q  Local Address:Port  Peer Address:Port\n\
                  LISTEN  0       128     0.0.0.0:22          0.0.0.0:*\n\
                  LISTEN  0       511     0.0.0.0:80          0.0.0.0:*";
        assert_eq!(parse_listen_ports(ss).len(), 2);
        assert!(parse_listen_ports("State   Recv-Q").is_empty());
    }

    #[test]
    fn counts_containers() {
        assert_eq!(count_containers("web Up 2 hours\ndb Up 3 days\n"), 2);
        assert_eq!(count_containers(""), 0);
    }

    #[test]
    fn new_probes_classify_low() {
        // 新增的只读探针必须被风险审查判定为 Low，否则会被 run_readonly 拦截。
        for (_, _, cmd) in PROBES {
            assert_eq!(
                crate::risk::classify_command(cmd).level,
                RiskLevel::Low,
                "探针命令应为 Low: {cmd}"
            );
        }
        assert_eq!(crate::risk::classify_command("who").level, RiskLevel::Low);
        assert_eq!(
            crate::risk::classify_command("cat /proc/loadavg").level,
            RiskLevel::Low
        );
    }

    #[test]
    fn build_report_populates_structured_fields() {
        let mut out: BTreeMap<&str, String> = BTreeMap::new();
        out.insert(
            "memory",
            "               total used free\nMem:            7900 3200 900".to_string(),
        );
        out.insert(
            "disk",
            "Filesystem Size Used Avail Use% Mounted on\n/dev/sda1 50G 24G 24G 51% /".to_string(),
        );
        out.insert(
            "ports",
            "State Recv-Q Send-Q Local Peer\nLISTEN 0 128 0.0.0.0:22 0.0.0.0:*".to_string(),
        );
        out.insert("services", "a.service running\nb.service running".to_string());
        out.insert("docker", "web Up 1h".to_string());
        let r = build_report("s1", &out, vec![], vec![]);
        assert_eq!(r.mem_used_mb, Some(3200));
        assert_eq!(r.mem_total_mb, Some(7900));
        assert_eq!(r.disk_used_percent, Some(51));
        assert_eq!(r.port_count, Some(1));
        assert_eq!(r.service_count, Some(2));
        assert_eq!(r.container_count, Some(1));
        assert_eq!(r.cpu_percent, None);
    }

    #[test]
    fn detects_disk_pressure() {
        // 现在只依据根分区使用率判定。
        assert!(disk_under_pressure(94));
        assert!(disk_under_pressure(90));
        assert!(!disk_under_pressure(40));
    }

    #[test]
    fn disk_pressure_uses_root_only() {
        // tmpfs / 只读层即使满载也不应触发告警；仅看根分区。
        let mut out: BTreeMap<&str, String> = BTreeMap::new();
        out.insert(
            "disk",
            "Filesystem Size Used Avail Use% Mounted on\n\
             /dev/sda1 50G 20G 30G 40% /\n\
             overlay 1G 1G 0 100% /var/lib/docker/overlay2\n\
             tmpfs 1G 1G 0 99% /run".to_string(),
        );
        let mut warnings = Vec::new();
        assert_eq!(check_disk_pressure(&out, &mut warnings), None);
        assert!(warnings.is_empty());

        // 根分区告急时应触发。
        let mut out2: BTreeMap<&str, String> = BTreeMap::new();
        out2.insert(
            "disk",
            "Filesystem Size Used Avail Use% Mounted on\n/dev/sda1 50G 47G 3G 94% /".to_string(),
        );
        let mut warnings2 = Vec::new();
        assert!(check_disk_pressure(&out2, &mut warnings2).is_some());
        assert_eq!(warnings2.len(), 1);
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
            cpu_percent: None,
            mem_used_mb: None,
            mem_total_mb: None,
            disk_used_percent: None,
            service_count: None,
            container_count: None,
            port_count: None,
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
