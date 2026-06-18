//! 服务器监控指标采集（SSH 只读，服务器零 agent）。
//!
//! 用**一条**复合只读命令（经系统 OpenSSH 执行）一次性采集 CPU / 负载 / 内存 /
//! 磁盘 / 网络 / 运行时长 / 容器·服务·端口·进程数量，再在本地解析为
//! [`ServerMetrics`]。网络与磁盘 I/O 的**速率**由前端跨两次轮询求差得到，
//! 后端只回累计值（除 CPU 外不做 sleep 测速）。
//!
//! CPU 使用率需要两次采样 /proc/stat 取增量才能算出，因此命令里夹了一个
//! `sleep 1` 取前后两帧；其余指标都是单帧绝对值。
//!
//! 解析必须**稳健**：任何字段缺失、命令不存在、非 Linux 输出，都安全降级为 0
//! 或合理缺省，绝不 panic（不在解析路径上 unwrap）。所有输出在离开 ssh 模块前
//! 已脱敏。

use crate::core::error::AppResult;
use crate::core::types::{now, ServerMetrics, ServerProfile};

/// 一次性采集所需的复合只读命令。用清晰的 `##XXX` 分隔标记便于按段解析；
/// 缺失的命令（docker / systemctl / ss）用子 shell + `2>/dev/null` 兜底，
/// 失败时该段为空 / wc 输出 0，解析侧统一降级。
const COLLECT_CMD: &str = "echo '##LOAD'; cat /proc/loadavg; \
echo '##NPROC'; nproc; \
echo '##MEM'; free -b; \
echo '##DISK'; df -B1 -P /; \
echo '##UP'; cat /proc/uptime; \
echo '##NET'; cat /proc/net/dev; \
echo '##DOCKER'; (docker ps -q 2>/dev/null | wc -l); \
echo '##SVC'; (systemctl list-units --type=service --state=running --no-legend --no-pager 2>/dev/null | wc -l); \
echo '##PORTS'; (ss -H -ltun 2>/dev/null | wc -l); \
echo '##PROCS'; (ps -e --no-headers 2>/dev/null | wc -l); \
echo '##CPU1'; grep '^cpu ' /proc/stat; sleep 1; \
echo '##CPU2'; grep '^cpu ' /proc/stat";

/// 通过 SSH 执行复合命令并解析为一份 [`ServerMetrics`] 快照。
pub async fn collect(server: &ServerProfile, secret: Option<&str>) -> AppResult<ServerMetrics> {
    // 命令里含 `sleep 1`，整体耗时略高于 1s；用 DEFAULT_TIMEOUT（30s）足够宽裕。
    let exec = crate::ssh::run_command(server, secret, COLLECT_CMD, crate::ssh::DEFAULT_TIMEOUT).await?;
    Ok(parse_metrics(&exec.stdout))
}

/// 把复合命令的 stdout 解析为 [`ServerMetrics`]。纯函数、无 I/O，便于单测。
fn parse_metrics(stdout: &str) -> ServerMetrics {
    let sections = split_sections(stdout);
    let get = |name: &str| sections.get(name).map(String::as_str).unwrap_or("");

    let (load1, load5, load15) = parse_loadavg(get("LOAD"));
    let cpu_cores = parse_u32_first(get("NPROC")).unwrap_or(0);
    let (mem_used_bytes, mem_total_bytes, swap_used_bytes, swap_total_bytes) = parse_free(get("MEM"));
    let (disk_used_bytes, disk_total_bytes, disk_path) = parse_df(get("DISK"));
    let (net_rx_bytes, net_tx_bytes) = parse_net_dev(get("NET"));
    let uptime_secs = parse_uptime(get("UP"));
    let containers = parse_u32_first(get("DOCKER")).unwrap_or(0);
    let services = parse_u32_first(get("SVC")).unwrap_or(0);
    let listening_ports = parse_u32_first(get("PORTS")).unwrap_or(0);
    let procs = parse_u32_first(get("PROCS")).unwrap_or(0);
    let cpu_percent = parse_cpu_percent(get("CPU1"), get("CPU2"));

    ServerMetrics {
        cpu_percent,
        cpu_cores,
        load1,
        load5,
        load15,
        mem_used_bytes,
        mem_total_bytes,
        swap_used_bytes,
        swap_total_bytes,
        disk_used_bytes,
        disk_total_bytes,
        disk_path,
        net_rx_bytes,
        net_tx_bytes,
        uptime_secs,
        containers,
        services,
        listening_ports,
        procs,
        sampled_at: now().to_rfc3339(),
    }
}

/// 按 `##XXX` 标记行把输出切成「段名 -> 段内容（已 trim）」。标记行本身不计入内容。
fn split_sections(stdout: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut current: Option<String> = None;
    let mut buf = String::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("##") {
            // 收尾上一段，开启新段。
            if let Some(key) = current.take() {
                map.insert(key, buf.trim().to_string());
            }
            current = Some(name.trim().to_string());
            buf = String::new();
        } else if current.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if let Some(key) = current.take() {
        map.insert(key, buf.trim().to_string());
    }
    map
}

/// /proc/loadavg 前三列：1 / 5 / 15 分钟平均负载。任一缺失/非数字记为 0.0。
fn parse_loadavg(s: &str) -> (f64, f64, f64) {
    let mut it = s.split_whitespace();
    let a = it.next().and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
    let b = it.next().and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
    let c = it.next().and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
    (a, b, c)
}

/// 取段内第一个 token 解析为 u32（用于 nproc / wc -l 的纯数字段）。
fn parse_u32_first(s: &str) -> Option<u32> {
    s.split_whitespace().next()?.parse::<u32>().ok()
}

/// 解析 `free -b` 输出，返回 (mem_used, mem_total, swap_used, swap_total)，单位字节。
///
/// 内存：`Mem:` 行 `total used free shared buff/cache available`。已用优先用
/// `total - available`（更贴近实际可用，available 在第 7 列）；若无 available 列
/// 则回退到 `used` 列（第 3 列）。
/// 交换：`Swap:` 行 `total used free`。
fn parse_free(s: &str) -> (u64, u64, u64, u64) {
    let mut mem_used = 0u64;
    let mut mem_total = 0u64;
    let mut swap_used = 0u64;
    let mut swap_total = 0u64;
    for line in s.lines() {
        let lower = line.trim_start().to_lowercase();
        let cols: Vec<&str> = line.split_whitespace().collect();
        if lower.starts_with("mem:") {
            // 形如：Mem: <total> <used> <free> <shared> <buff/cache> <available>
            let total = cols.get(1).and_then(|t| t.parse::<u64>().ok()).unwrap_or(0);
            let used_col = cols.get(2).and_then(|t| t.parse::<u64>().ok());
            let available = cols.get(6).and_then(|t| t.parse::<u64>().ok());
            mem_total = total;
            mem_used = match available {
                // total - available 更准；用 saturating_sub 防止异常输出下溢。
                Some(av) => total.saturating_sub(av),
                None => used_col.unwrap_or(0),
            };
        } else if lower.starts_with("swap:") {
            // 形如：Swap: <total> <used> <free>
            swap_total = cols.get(1).and_then(|t| t.parse::<u64>().ok()).unwrap_or(0);
            swap_used = cols.get(2).and_then(|t| t.parse::<u64>().ok()).unwrap_or(0);
        }
    }
    (mem_used, mem_total, swap_used, swap_total)
}

/// 解析 `df -B1 -P /` 输出，返回 (used_bytes, total_bytes, mount_path)。
/// `-P` 保证每条记录一行、列稳定：`Filesystem 1B-blocks Used Available Capacity Mounted`。
/// `-B1` 使容量单位为字节。取首个数据行（跳过表头），挂载点为最后一列。
fn parse_df(s: &str) -> (u64, u64, String) {
    for line in s.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 跳过表头（首列为 "Filesystem"）与空行；数据行至少 6 列。
        if cols.len() < 6 || cols[0].eq_ignore_ascii_case("filesystem") {
            continue;
        }
        let total = cols[1].parse::<u64>().ok();
        let used = cols[2].parse::<u64>().ok();
        // 仅当容量字段确实是数字时才认作有效数据行。
        if let (Some(total), Some(used)) = (total, used) {
            let path = cols.last().copied().unwrap_or("/").to_string();
            return (used, total, path);
        }
    }
    (0, 0, "/".to_string())
}

/// 解析 /proc/net/dev，累加所有**非 loopback**（跳过 `lo`）网卡的 rx/tx 字节。
/// 格式：`iface: rx_bytes rx_packets ... tx_bytes tx_packets ...`。冒号后第 1 个
/// 数字为 rx 字节、第 9 个为 tx 字节。
fn parse_net_dev(s: &str) -> (u64, u64) {
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    for line in s.lines() {
        // 只看含 ':' 的数据行（前两行是表头）。
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface.is_empty() || iface == "lo" {
            continue;
        }
        let nums: Vec<u64> = rest
            .split_whitespace()
            .map(|t| t.parse::<u64>().unwrap_or(0))
            .collect();
        // rx 字节 = 第 1 个数字；tx 字节 = 第 9 个数字（索引 8）。
        if let Some(rx) = nums.first() {
            rx_total = rx_total.saturating_add(*rx);
        }
        if let Some(tx) = nums.get(8) {
            tx_total = tx_total.saturating_add(*tx);
        }
    }
    (rx_total, tx_total)
}

/// /proc/uptime 第一个数（浮点秒），截断为整数秒。
fn parse_uptime(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|t| t.parse::<f64>().ok())
        .map(|f| f.max(0.0) as u64)
        .unwrap_or(0)
}

/// 由两帧 `cpu ...`（/proc/stat 聚合行）算 CPU 使用率百分比（0-100，一位小数）。
///
/// 行格式：`cpu user nice system idle iowait irq softirq steal guest guest_nice`。
/// busy = user+nice+system+irq+softirq+steal；total = 所有列之和。
/// 占比 = (busy2-busy1) / (total2-total1) * 100。
/// 解析失败 / 总增量 <= 0（无变化或异常）时返回 0.0。
fn parse_cpu_percent(cpu1: &str, cpu2: &str) -> f64 {
    let (busy1, total1) = match parse_cpu_line(cpu1) {
        Some(v) => v,
        None => return 0.0,
    };
    let (busy2, total2) = match parse_cpu_line(cpu2) {
        Some(v) => v,
        None => return 0.0,
    };
    let total_delta = total2.saturating_sub(total1);
    if total_delta == 0 {
        return 0.0;
    }
    let busy_delta = busy2.saturating_sub(busy1);
    let pct = busy_delta as f64 / total_delta as f64 * 100.0;
    // 钳到 0-100 并保留一位小数。
    let pct = pct.clamp(0.0, 100.0);
    (pct * 10.0).round() / 10.0
}

/// 解析单行 `cpu ...`，返回 (busy, total)。需要至少 user/nice/system/idle 四列才认为有效。
fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let mut it = line.split_whitespace();
    // 首 token 必须是 "cpu"（聚合行）。
    if it.next()? != "cpu" {
        return None;
    }
    let nums: Vec<u64> = it.map(|t| t.parse::<u64>().unwrap_or(0)).collect();
    if nums.len() < 4 {
        return None;
    }
    // 列序：0 user, 1 nice, 2 system, 3 idle, 4 iowait, 5 irq, 6 softirq, 7 steal, ...
    let at = |i: usize| nums.get(i).copied().unwrap_or(0);
    let busy = at(0) + at(1) + at(2) + at(5) + at(6) + at(7);
    let total: u64 = nums.iter().sum();
    Some((busy, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一段固定的复合命令输出样本，覆盖典型 Linux 主机。
    /// 关键设计：CPU 两帧增量 busy=100、total=900 → 11.1%；net 含 lo（应被跳过）
    /// 与 eth0/eth1（应累加）；free 含 available 列（used = total-available）。
    const SAMPLE: &str = "##LOAD
0.52 0.58 0.59 1/234 5678
##NPROC
4
##MEM
               total        used        free      shared  buff/cache   available
Mem:      16000000000  4000000000  2000000000   100000000  10000000000  11000000000
Swap:      2000000000   500000000  1500000000
##DISK
Filesystem     1B-blocks       Used  Available Capacity Mounted on
/dev/sda1    50000000000 20000000000 30000000000      40% /
##UP
123456.78 987654.32
##NET
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets
    lo: 9999999    100    0    0    0     0          0         0  8888888    100
  eth0: 1000000    500    0    0    0     0          0         0   200000    300
  eth1:  500000    250    0    0    0     0          0         0   100000    150
##DOCKER
3
##SVC
42
##PORTS
7
##PROCS
180
##CPU1
cpu 1000 0 500 8000 100 0 100 0 0 0
##CPU2
cpu 1050 0 550 8800 100 0 100 0 0 0
";

    #[test]
    fn parses_full_sample() {
        let m = parse_metrics(SAMPLE);

        // CPU：帧1 busy=1000+500+100=1600, total=9700; 帧2 busy=1050+550+100=1700, total=10600.
        // busy_delta=100, total_delta=900 → 11.1%（保留一位小数）。
        assert_eq!(m.cpu_percent, 11.1);
        assert_eq!(m.cpu_cores, 4);

        assert_eq!(m.load1, 0.52);
        assert_eq!(m.load5, 0.58);
        assert_eq!(m.load15, 0.59);

        // 内存：used = total(16e9) - available(11e9) = 5e9。
        assert_eq!(m.mem_total_bytes, 16_000_000_000);
        assert_eq!(m.mem_used_bytes, 5_000_000_000);
        assert_eq!(m.swap_total_bytes, 2_000_000_000);
        assert_eq!(m.swap_used_bytes, 500_000_000);

        // 磁盘（-B1 故为字节）。
        assert_eq!(m.disk_total_bytes, 50_000_000_000);
        assert_eq!(m.disk_used_bytes, 20_000_000_000);
        assert_eq!(m.disk_path, "/");

        // 网络：跳过 lo，累加 eth0+eth1。rx=1_000_000+500_000=1_500_000; tx=200_000+100_000=300_000。
        assert_eq!(m.net_rx_bytes, 1_500_000);
        assert_eq!(m.net_tx_bytes, 300_000);

        assert_eq!(m.uptime_secs, 123456);

        assert_eq!(m.containers, 3);
        assert_eq!(m.services, 42);
        assert_eq!(m.listening_ports, 7);
        assert_eq!(m.procs, 180);

        // sampled_at 是合法 rfc3339。
        assert!(chrono::DateTime::parse_from_rfc3339(&m.sampled_at).is_ok());
    }

    #[test]
    fn cpu_percent_basic_increment() {
        // busy_delta=100, total_delta=1000 → 10.0%。
        let c1 = "cpu 100 0 0 900 0 0 0 0";
        let c2 = "cpu 200 0 0 1800 0 0 0 0";
        assert_eq!(parse_cpu_percent(c1, c2), 10.0);
    }

    #[test]
    fn cpu_percent_no_change_is_zero() {
        let c = "cpu 100 0 0 900 0 0 0 0";
        assert_eq!(parse_cpu_percent(c, c), 0.0);
        // 缺帧 / 非法行 → 0.0，不 panic。
        assert_eq!(parse_cpu_percent("", "cpu 1 2 3 4"), 0.0);
        assert_eq!(parse_cpu_percent("garbage", "garbage"), 0.0);
    }

    #[test]
    fn free_falls_back_to_used_column_when_no_available() {
        // 旧 free 只有 total/used/free（无 available 列）→ 用 used 列。
        let s = "              total        used        free\nMem:  8000  3000  5000\nSwap:  0  0  0";
        let (used, total, sw_used, sw_total) = parse_free(s);
        assert_eq!(total, 8000);
        assert_eq!(used, 3000);
        assert_eq!(sw_used, 0);
        assert_eq!(sw_total, 0);
    }

    #[test]
    fn net_skips_loopback() {
        let s = "Inter-|   Receive | Transmit\n face |bytes ...\n    lo: 500 1 0 0 0 0 0 0 500 1\n  eth0: 100 1 0 0 0 0 0 0 200 1";
        let (rx, tx) = parse_net_dev(s);
        assert_eq!(rx, 100);
        assert_eq!(tx, 200);
    }

    #[test]
    fn robust_against_missing_and_empty() {
        // 完全空输入：全部降级为 0 / 缺省，绝不 panic。
        let m = parse_metrics("");
        assert_eq!(m.cpu_percent, 0.0);
        assert_eq!(m.cpu_cores, 0);
        assert_eq!(m.load1, 0.0);
        assert_eq!(m.mem_total_bytes, 0);
        assert_eq!(m.disk_total_bytes, 0);
        assert_eq!(m.disk_path, "/");
        assert_eq!(m.net_rx_bytes, 0);
        assert_eq!(m.uptime_secs, 0);
        assert_eq!(m.containers, 0);
        assert_eq!(m.services, 0);
        assert_eq!(m.listening_ports, 0);
        assert_eq!(m.procs, 0);

        // 命令不存在时 wc 段可能为空或非数字 → 0。
        let partial = "##LOAD\n1.0 2.0 3.0\n##DOCKER\n\n##SVC\nbash: systemctl: command not found";
        let m2 = parse_metrics(partial);
        assert_eq!(m2.load1, 1.0);
        assert_eq!(m2.containers, 0);
        assert_eq!(m2.services, 0);
    }

    #[test]
    fn df_picks_data_row() {
        let s = "Filesystem 1B-blocks Used Available Capacity Mounted on\n/dev/root 100 40 60 40% /";
        let (used, total, path) = parse_df(s);
        assert_eq!(used, 40);
        assert_eq!(total, 100);
        assert_eq!(path, "/");
    }
}
