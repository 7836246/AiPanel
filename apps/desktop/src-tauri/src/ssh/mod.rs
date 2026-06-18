//! SSH 执行器。
//!
//! 通过**系统自带的 OpenSSH 客户端**在远程服务器上执行命令——我们不自己
//! 实现 SSH 协议。这条链路完全由 AiPanel 掌控；Agent 永远拿不到裸 SSH 访问
//! 权限（见 docs/SECURITY_MODEL.zh-Hans.md）。
//!
//! 安全特性：
//! - 只读执行的前提是 Risk Reviewer 将命令判定为 `Low`（检查类白名单）；
//! - 每条命令都有超时，超时后会杀掉子进程；
//! - stdout/stderr 离开本模块前都会经过脱敏；
//! - 私钥被写入仅本次调用使用的 0600 权限临时文件，调用结束立即删除。

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;
use tokio::time::timeout;

use crate::core::error::{AppError, AppResult};
use crate::core::sanitize::sanitize;
use crate::core::types::{AuthKind, CommandExecution, RiskLevel, ServerProfile};
use crate::risk::classify_command;

/// 单条命令的默认超时时间。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// 默认连接超时（同时传给 ssh 的 ConnectTimeout）。
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

/// 取消注册表：把每次流式运行的 `run_id` 映射到一个 [`Notify`]。前端可以通过
/// `cancel_run(run_id)` 唤醒对应的 [`Notify`]，正在运行的流式循环 select 到它后
/// 立即跳出并 drop 子进程（kill_on_drop 已设），从而中断本地 ssh 以及——配合
/// `-tt` 强制分配的 tty——中断远端命令（远端收到 SIGHUP）。
///
/// 只依赖标准库 + 已有的 tokio，不引入新依赖。
#[derive(Clone)]
struct CancelEntry {
    server_id: Option<String>,
    notify: Arc<Notify>,
}

static CANCEL_REGISTRY: OnceLock<Mutex<HashMap<String, CancelEntry>>> = OnceLock::new();

/// 取回（必要时初始化）全局取消注册表。
fn registry() -> &'static Mutex<HashMap<String, CancelEntry>> {
    CANCEL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn registry_lock() -> Option<std::sync::MutexGuard<'static, HashMap<String, CancelEntry>>> {
    registry().lock().ok()
}

/// 为一次运行登记取消句柄并返回它。流式命令在开始执行前调用，把返回的
/// [`Notify`] 传给可取消的流式执行器；运行结束后必须调用 [`unregister`]。
pub fn register(run_id: &str, server_id: Option<&str>) -> Arc<Notify> {
    let notify = Arc::new(Notify::new());
    if let Some(mut guard) = registry_lock() {
        guard.insert(
            run_id.to_string(),
            CancelEntry {
                server_id: server_id.map(str::to_string),
                notify: notify.clone(),
            },
        );
    } else {
        eprintln!("[ssh] cancel registry lock poisoned; run cancellation disabled for {run_id}");
    }
    notify
}

/// 注销一次运行的取消句柄。无论成功、失败还是被取消都应调用，避免句柄泄漏。
pub fn unregister(run_id: &str) {
    if let Some(mut guard) = registry_lock() {
        guard.remove(run_id);
    }
}

/// 请求取消指定运行。若该 `run_id` 仍在注册表中，唤醒其 [`Notify`]。
/// 找不到（已结束 / 从未登记）时静默忽略。
///
/// 用 `notify_one` 而非 `notify_waiters`：前者会在当前没有等待者时**存下一个许可**，
/// 这样即便取消请求恰好落在流式循环两次迭代之间（此刻没有 `.notified()` 在等待），
/// 下一次 `.notified()` 也会立刻返回，不会漏掉取消。
pub fn cancel(run_id: &str) {
    if let Some(notify) = registry_lock()
        .and_then(|guard| guard.get(run_id).map(|entry| entry.notify.clone()))
    {
        notify.notify_one();
    }
}

fn run_ids_for_server(
    entries: &HashMap<String, CancelEntry>,
    server_id: &str,
) -> Vec<String> {
    matching_run_ids(
        entries
            .iter()
            .map(|(run_id, entry)| (run_id.as_str(), entry.server_id.as_deref())),
        server_id,
    )
}

fn matching_run_ids<'a>(
    entries: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    server_id: &str,
) -> Vec<String> {
    entries
        .into_iter()
        .filter(|(_, entry_server_id)| *entry_server_id == Some(server_id))
        .map(|(run_id, _)| run_id.to_string())
        .collect()
}

/// 请求取消某台服务器下当前登记的所有流式运行。删除服务器时调用，避免服务器
/// 档案已删除但旧 SSH 流式命令仍继续运行。
pub fn cancel_for_server(server_id: &str) -> usize {
    let entries = match registry_lock() {
        Some(guard) => guard,
        None => return 0,
    };
    let run_ids = run_ids_for_server(&entries, server_id);
    for run_id in &run_ids {
        if let Some(entry) = entries.get(run_id) {
            entry.notify.notify_one();
        }
    }
    run_ids.len()
}

/// 对流式输出的单行做脱敏。私钥块只有在 `sanitize` 对整个缓冲区匹配时才能命中，
/// 按行流式输出时其内容会泄露；这里抑制内容、只输出一个占位符。存储的输出仍会
/// 另行做整缓冲区脱敏。
fn redact_live_line(line: &str, in_key_block: &mut bool) -> Option<String> {
    let upper = line.to_uppercase();
    if *in_key_block {
        if upper.contains("END") && upper.contains("PRIVATE KEY") {
            *in_key_block = false;
        }
        return None;
    }
    if upper.contains("BEGIN") && upper.contains("PRIVATE KEY") {
        *in_key_block = true;
        return Some("[redacted-private-key]".to_string());
    }
    Some(sanitize(line))
}

/// 判断是否为 ssh 客户端自身在连接结束时打印的噪声行（如
/// `Connection to host closed.`）。因为流式执行用 `-tt` 强制分配 tty，
/// ssh 会把这类提示写到合并的 tty 流里——它不属于远端命令的真实输出，
/// 既不应回调给 `on_line`，也不应计入累积缓冲区/审计。
fn is_ssh_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("Connection to ") && trimmed.contains("closed")
}

/// 临时私钥文件，在 drop 时删除。
struct KeyFile {
    path: std::path::PathBuf,
}

impl KeyFile {
    fn write(secret: &str) -> AppResult<Self> {
        let path = std::env::temp_dir().join(format!("aipanel-key-{}", crate::core::types::new_id()));
        std::fs::write(&path, secret)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(KeyFile { path })
    }
}

impl Drop for KeyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 共用的 ssh 安全加固选项。
fn common_opts(connect_timeout_secs: u64) -> Vec<String> {
    vec![
        "-o".into(),
        format!("ConnectTimeout={connect_timeout_secs}"),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ServerAliveInterval=5".into(),
    ]
}

/// 一次完整构建好的 ssh 调用：要运行的程序、它的 argv，以及一个可选的
/// 环境变量（供 `sshpass -e` 使用）。[`KeyFile`] 在调用期间保持存活，
/// 在 drop 时删除。
struct Invocation {
    program: String,
    args: Vec<String>,
    env: Option<(String, String)>,
    /// 仅用于在调用期间保持临时密钥存活；drop 时删除。
    _keyfile: Option<KeyFile>,
}

/// 按认证方式为「服务器 + 命令」构建 ssh 调用。阻塞式与流式执行器共用此函数，
/// 以保证两者与 ssh 的交互方式完全一致。
///
/// `force_tty` 为 `true` 时追加 `-tt`，强制 ssh 分配伪终端。这样当本地 ssh 进程
/// 被杀（取消 / 超时触发 kill_on_drop）时，远端会收到 SIGHUP，从而把远端命令也
/// 一并终止——而不是留下一个孤儿进程继续跑。仅流式执行器使用。
fn build_invocation(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    force_tty: bool,
) -> AppResult<Invocation> {
    // 强制 tty 的两个 `-tt`。放在 ssh 自身参数最前面，对三种认证方式都适用
    //（password 分支里 ssh 的参数从第 2 项起，见下方）。
    let tty_opts: &[&str] = if force_tty { &["-tt"] } else { &[] };
    let mut keyfile: Option<KeyFile> = None;
    let (program, args, env) = match server.auth_kind {
        AuthKind::Agent => {
            let mut a = vec!["-o".into(), "BatchMode=yes".into()];
            a.extend(tty_opts.iter().map(|s| s.to_string()));
            a.extend(common_opts(CONNECT_TIMEOUT_SECS));
            a.push("-p".into());
            a.push(server.port.to_string());
            a.push(format!("{}@{}", server.username, server.host));
            a.push(command.to_string());
            ("ssh".to_string(), a, None)
        }
        AuthKind::Key => {
            let secret = secret.ok_or_else(|| AppError::Credential("no SSH key stored".into()))?;
            let kf = KeyFile::write(secret)?;
            let mut a = vec![
                "-o".into(),
                "BatchMode=yes".into(),
                "-o".into(),
                "IdentitiesOnly=yes".into(),
                "-i".into(),
                kf.path.display().to_string(),
            ];
            a.extend(tty_opts.iter().map(|s| s.to_string()));
            a.extend(common_opts(CONNECT_TIMEOUT_SECS));
            a.push("-p".into());
            a.push(server.port.to_string());
            a.push(format!("{}@{}", server.username, server.host));
            a.push(command.to_string());
            keyfile = Some(kf);
            ("ssh".to_string(), a, None)
        }
        AuthKind::Password => {
            let secret = secret.ok_or_else(|| AppError::Credential("no SSH password stored".into()))?;
            if which("sshpass").is_none() {
                return Err(AppError::Ssh(
                    "password auth needs `sshpass` installed; prefer key or agent auth".into(),
                ));
            }
            // sshpass -e 从 SSHPASS 环境变量读取密码（而非 argv，
            // 这样密码不会在 `ps` 中可见）。
            let mut a = vec!["-e".into(), "ssh".into(), "-o".into(), "PreferredAuthentications=password".into(), "-o".into(), "PubkeyAuthentication=no".into()];
            a.extend(tty_opts.iter().map(|s| s.to_string()));
            a.extend(common_opts(CONNECT_TIMEOUT_SECS));
            a.push("-p".into());
            a.push(server.port.to_string());
            a.push(format!("{}@{}", server.username, server.host));
            a.push(command.to_string());
            ("sshpass".to_string(), a, Some(("SSHPASS".to_string(), secret.to_string())))
        }
    };

    Ok(Invocation { program, args, env, _keyfile: keyfile })
}

/// 以子进程方式启动一次 ssh 调用，stdio 全部接管管道，并启用 kill-on-drop。
fn spawn_child(inv: &Invocation) -> AppResult<tokio::process::Child> {
    let mut cmd = Command::new(&inv.program);
    cmd.args(&inv.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some((k, v)) = &inv.env {
        cmd.env(k, v);
    }
    cmd.spawn().map_err(|e| AppError::Ssh(format!("failed to launch ssh: {e}")))
}

/// 在服务器上执行一条原始命令。风险审查由调用方负责——任何由 Agent 驱动
/// 或来自不可信输入的命令，都应使用 [`run_readonly`]。
pub async fn run_command(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
) -> AppResult<CommandExecution> {
    let started_at = crate::core::types::now();
    let start = Instant::now();

    // 按认证方式构建调用。`inv` 持有密钥文件（如果有），因此密钥会在 drop 时
    // 删除——包括下面的超时分支，那里 `inv` 在返回时离开作用域。
    // 阻塞式执行不需要强制 tty。
    let inv = build_invocation(server, secret, command, false)?;
    let child = spawn_child(&inv)?;

    let output = match timeout(duration, child.wait_with_output()).await {
        Ok(res) => res.map_err(|e| AppError::Ssh(e.to_string()))?,
        Err(_) => {
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: sanitize(&String::from_utf8_lossy(&output.stdout)),
        stderr: sanitize(&String::from_utf8_lossy(&output.stderr)),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    })
}

/// 与 [`run_command`] 行为一致，但在执行前把 `input` 写入子进程的 stdin，
/// 然后关闭 stdin（drop），再收集输出。供文件管理的「写文件」（`cat > 路径`）
/// 使用：内容经 stdin 传入，绝不进入 argv（避免在 `ps` 中可见、也避免超长 argv）。
///
/// 复用 [`build_invocation`]（不强制 tty）与既有的认证/脱敏逻辑；不复用
/// [`spawn_child`]（后者把 stdin 设为 null），而是单独构建一个 stdin 接管管道的子进程。
/// 风险审查由调用方负责。
pub async fn run_command_with_input(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    input: &str,
    duration: Duration,
) -> AppResult<CommandExecution> {
    use tokio::io::AsyncWriteExt;

    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` 持有临时密钥文件（如果有），在 drop 时删除。阻塞式执行不需要强制 tty。
    let inv = build_invocation(server, secret, command, false)?;

    // 与 spawn_child 一致，但 stdin 接管为管道，以便写入内容。
    let mut cmd = Command::new(&inv.program);
    cmd.args(&inv.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some((k, v)) = &inv.env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Ssh(format!("failed to launch ssh: {e}")))?;

    // 取出 stdin，写入内容后 drop（关闭），让远端的 `cat` 收到 EOF 并落盘。
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stdin".into()))?;

    let output = match timeout(duration, async {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))?;
        drop(stdin); // 关闭 stdin，发送 EOF
        child
            .wait_with_output()
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))
    })
    .await
    {
        Ok(res) => res?,
        Err(_) => {
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: sanitize(&String::from_utf8_lossy(&output.stdout)),
        stderr: sanitize(&String::from_utf8_lossy(&output.stderr)),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    })
}

/// 仅当 Risk Reviewer 将命令判定为只读（`Low`）时才执行。
/// 这是任何尚未经用户确认的命令的安全入口。
pub async fn run_readonly(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
) -> AppResult<CommandExecution> {
    if classify_command(command).level != RiskLevel::Low {
        return Err(AppError::Blocked(format!(
            "command is not read-only and cannot run in inspection mode: {command}"
        )));
    }
    run_command(server, secret, command, duration).await
}

/// [`run_readonly`] 的流式版本：执行被判定为 `Low` 的命令，并在每一行到达时
/// 调用 `on_line`，以便 UI 实时填充。回调看到的行已经过脱敏；`stderr` 标记该行
/// 是否来自 stderr。返回的 [`CommandExecution`] 携带完整的脱敏输出，结构与
/// [`run_command`] 完全一致。
pub async fn run_readonly_streamed(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
) -> AppResult<CommandExecution> {
    // 不可取消的兼容入口：传入一个永远不会被唤醒的 Notify，因此结果必然是
    // `Some`（取消是唯一会产生 `None` 的分支）。
    let never = Notify::new();
    let res =
        run_readonly_streamed_cancellable(server, secret, command, duration, on_line, &never)
            .await?;
    res.ok_or_else(|| AppError::Ssh("stream ended unexpectedly".into()))
}

/// [`run_readonly_streamed`] 的可取消版本。除了多一个 `cancel` 参数外行为完全一致：
/// 流式循环在读取每一行的同时还 select `cancel.notified()`。一旦被唤醒就跳出循环
/// 并返回 `Ok(None)` 表示「被用户取消」（不是错误——调用方应把已产生的部分照常
/// 落库/审计）。函数返回时 `inv` 离开作用域，其子进程被 drop，kill_on_drop 杀掉
/// 本地 ssh；配合 `-tt` 强制的 tty，远端命令也会随之收到 SIGHUP。
pub async fn run_readonly_streamed_cancellable(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
    cancel: &Notify,
) -> AppResult<Option<CommandExecution>> {
    // 与 run_readonly 相同的安全闸门：只有检查类命令才能流式执行。
    if classify_command(command).level != RiskLevel::Low {
        return Err(AppError::Blocked(format!(
            "command is not read-only and cannot run in inspection mode: {command}"
        )));
    }

    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` 在整个调用生命周期内持有密钥文件（如果有）。强制分配 tty，使本地 ssh
    // 被杀时远端命令能随之终止。
    let inv = build_invocation(server, secret, command, true)?;
    let child = spawn_child(&inv)?;

    stream_child(child, command, started_at, start, duration, on_line, cancel).await
}

/// [`run_command`] 的流式版本：执行**已确认**的步骤（包括写操作），并在每一行
/// 到达时调用 `on_line`，以便控制台实时填充。与 [`run_readonly_streamed`] 完全
/// 相同，唯一区别是没有 `Low` 判定闸门——确认/二次确认已由调用方强制执行
///（execute_confirmed_plan / run_confirmed_plan_stream）。回调看到的行已经过脱敏；
/// `stderr` 标记该行是否来自 stderr。返回的 [`CommandExecution`] 携带完整的脱敏
/// 输出，结构与 [`run_command`] 完全一致。
pub async fn run_command_streamed(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
) -> AppResult<CommandExecution> {
    // 不可取消的兼容入口：传入一个永远不会被唤醒的 Notify，因此结果必然是 `Some`。
    let never = Notify::new();
    let res =
        run_command_streamed_cancellable(server, secret, command, duration, on_line, &never)
            .await?;
    res.ok_or_else(|| AppError::Ssh("stream ended unexpectedly".into()))
}

/// [`run_command_streamed`] 的可取消版本——见 [`run_readonly_streamed_cancellable`]
/// 的取消语义（取消返回 `Ok(None)`）。同样没有 `Low` 等级闸门（确认 / 二次确认
/// 已由调用方强制执行）。
pub async fn run_command_streamed_cancellable(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
    cancel: &Notify,
) -> AppResult<Option<CommandExecution>> {
    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` 在整个调用生命周期内持有密钥文件（如果有）。强制分配 tty，使本地 ssh
    // 被杀时远端命令能随之终止。
    let inv = build_invocation(server, secret, command, true)?;
    let child = spawn_child(&inv)?;

    stream_child(child, command, started_at, start, duration, on_line, cancel).await
}

/// 流式执行的共享核心：并发按行读取子进程的 stdout/stderr，通过 `on_line` 转发
/// 每一条脱敏后的行并累积完整输出，同时受超时与 `cancel` 约束。
///
/// 注意（合并 tty 流的限制）：流式执行用 `-tt` 强制分配伪终端（取消传播 / 远端
/// SIGHUP 需要它），而 tty 会把远端的 stderr 合并进 stdout。因此**流式输出本质上
/// 是一条合并的 tty 流，逐行的 stderr 标记对流式步骤并不保证精确**——例如远端
/// 写到 stderr 的内容会从 stdout 这一路读出、被标记为非 stderr。这是 `-tt` 的固有
/// 取舍；保留 `-tt` 是为了取消时能可靠地把远端命令一并终止。`run_command`
/// 的阻塞式路径不加 `-tt`，stdout/stderr 仍是分开的、标记可靠。
///
/// 三种结束方式：
/// - 正常结束：两个流都读完，等待子进程退出并返回 `Ok(Some(execution))`；
/// - 超时：返回 `Ssh` 错误（沿用既有文案）；
/// - 取消：`cancel` 被唤醒，返回 `Ok(None)`（不是错误，调用方照常处理已产生的部分）。
///
/// 后两种情况下 `child` 随函数返回而 drop，kill_on_drop 杀掉本地 ssh；配合 `-tt`
/// 强制的 tty，远端命令也会收到 SIGHUP。
async fn stream_child(
    mut child: tokio::process::Child,
    command: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    start: Instant,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
    cancel: &Notify,
) -> AppResult<Option<CommandExecution>> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stderr".into()))?;

    // 并发读取两个流，通过回调转发每一条脱敏后的行，同时累积完整输出。
    // 内层 future 返回 `Ok(None)` 表示被取消，`Ok(Some(..))` 表示正常读完。
    // 私钥块屏蔽状态按路独立维护：stdout 与 stderr 各用一个布尔，避免两路
    // 交错时一路的 BEGIN/END 影响另一路的判定而漏过某一行私钥内容。
    let mut out_key_block = false;
    let mut err_key_block = false;
    let stream = async {
        let mut out_reader = BufReader::new(stdout).lines();
        let mut err_reader = BufReader::new(stderr).lines();
        let mut out_buf = String::new();
        let mut err_buf = String::new();
        let mut out_done = false;
        let mut err_done = false;
        while !out_done || !err_done {
            tokio::select! {
                // 取消优先：被唤醒后返回 None，让上层把它当作「正常取消」处理。
                _ = cancel.notified() => {
                    return Ok::<_, AppError>(None);
                }
                line = out_reader.next_line(), if !out_done => match line {
                    Ok(Some(line)) => {
                        // 因为加了 `-tt`（tty），行尾可能多出 \r；按行展示前去掉它。
                        let line = line.strip_suffix('\r').unwrap_or(&line);
                        // 过滤 ssh 客户端自身在连接结束时打印的噪声行（合并 tty 流的产物）：
                        // 既不回调、也不计入缓冲区。
                        if is_ssh_noise_line(line) { continue; }
                        // 实时回调拿到的是按行（行内范围）脱敏的结果，用于临时展示；
                        // 存储的缓冲区保留原始行，以便最后做一次整缓冲区脱敏
                        //（私钥等跨行的敏感信息只有跨行才能匹配——见 core::sanitize）。
                        if let Some(t) = redact_live_line(line, &mut out_key_block) { on_line(&t, false); }
                        out_buf.push_str(line);
                        out_buf.push('\n');
                    }
                    Ok(None) => out_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
                line = err_reader.next_line(), if !err_done => match line {
                    Ok(Some(line)) => {
                        let line = line.strip_suffix('\r').unwrap_or(&line);
                        if is_ssh_noise_line(line) { continue; }
                        if let Some(t) = redact_live_line(line, &mut err_key_block) { on_line(&t, true); }
                        err_buf.push_str(line);
                        err_buf.push('\n');
                    }
                    Ok(None) => err_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
            }
        }
        let status = child.wait().await.map_err(|e| AppError::Ssh(e.to_string()))?;
        Ok::<_, AppError>(Some((status, out_buf, err_buf)))
    };

    let (status, out_buf, err_buf) = match timeout(duration, stream).await {
        // 正常读完。
        Ok(Ok(Some(triple))) => triple,
        // 被取消：返回 None，`child` 随之 drop 被杀。
        Ok(Ok(None)) => return Ok(None),
        // 流式读取过程中出错。
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            // `child` 在返回时 drop；kill_on_drop 会回收子进程。
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(Some(CommandExecution {
        command: command.to_string(),
        exit_code: status.code().unwrap_or(-1),
        // 对整个累积缓冲区做一次脱敏（与 run_command 一致），确保跨行的敏感
        // 信息在输出被存储/审计前已被脱敏。上面的按行回调只对实时流做了弱脱敏。
        stdout: sanitize(out_buf.trim_end_matches('\n')),
        stderr: sanitize(err_buf.trim_end_matches('\n')),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    }))
}

/// 连通性 + 认证探测。返回 [`ConnCheck`]:连上则 `ok=true`,否则 `message` 给出
/// 可读原因(认证失败 / 超时 / 连接被拒 / host key / 解析失败 / 未装 sshpass 等)。
pub async fn check_connection(
    server: &ServerProfile,
    secret: Option<&str>,
) -> AppResult<crate::core::types::ConnCheck> {
    use crate::core::types::ConnCheck;
    match run_command(server, secret, "true", Duration::from_secs(CONNECT_TIMEOUT_SECS + 5)).await {
        Ok(exec) if exec.exit_code == 0 => Ok(ConnCheck { ok: true, message: "连接成功".into() }),
        Ok(exec) => Ok(ConnCheck { ok: false, message: classify_ssh_failure(&exec.stderr, exec.exit_code) }),
        // run_command 本身报错(如未装 sshpass、无密钥、spawn 失败):错误信息本就可操作,直接透出。
        Err(e) => Ok(ConnCheck { ok: false, message: e.to_string() }),
    }
}

/// 把 ssh 的 stderr/退出码归类成一句可读的中文原因(stderr 已脱敏)。
fn classify_ssh_failure(stderr: &str, exit_code: i32) -> String {
    let s = stderr.to_lowercase();
    let hit = |needles: &[&str]| needles.iter().any(|n| s.contains(n));
    if hit(&["permission denied", "authentication failed", "too many authentication"]) {
        "认证失败:用户名 / 密码 / 私钥 不正确,或服务器不接受该认证方式(可在「编辑服务器」更新凭据)".into()
    } else if hit(&["timed out", "timeout", "operation timed out"]) {
        "连接超时:网络不通、被防火墙拦截,或本机代理 / VPN 拦截了到该服务器的连接".into()
    } else if hit(&["connection refused"]) {
        "连接被拒绝:目标端口未开放 SSH(检查端口与 SSH 服务是否在运行)".into()
    } else if hit(&["host key verification failed", "remote host identification has changed"]) {
        "主机密钥校验失败:服务器密钥已变更(可能换机或中间人;确认无误后清理 known_hosts)".into()
    } else if hit(&["could not resolve", "name or service not known", "nodename nor servname"]) {
        "无法解析主机名:检查 Host 是否填写正确".into()
    } else if hit(&["no route to host", "network is unreachable"]) {
        "无法路由到主机:网络不可达".into()
    } else {
        let detail = stderr.trim();
        if detail.is_empty() {
            format!("SSH 连接失败(退出码 {exit_code})")
        } else {
            let short: String = detail.chars().take(160).collect();
            format!("SSH 连接失败:{short}")
        }
    }
}

/// 在 PATH 中定位可执行文件（用于探测可选的 `sshpass`）。
fn which(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|p| p.join(bin))
        .find(|p| p.is_file())
}

/// 用系统自带的 `scp` 在本地与远程之间传输文件——**用户直接操作**的文件
/// 上传/下载（见 `files/mod.rs`），这条能力**绝不暴露给 AI / Agent**。
///
/// 认证方式与 [`build_invocation`] 等价（agent=BatchMode；key=`-i` 临时 0600
/// 密钥文件 + IdentitiesOnly；password=`sshpass -e scp ...` + `SSHPASS` 环境变量），
/// 但运行的程序是 `scp` 而不是 `ssh`，且 **scp 的端口选项是大写 `-P`**（ssh 用小写 `-p`）。
///
/// `args_after_opts` 是拼好的源/目的参数（含 `user@host:remote`，由调用方按
/// 上传 / 下载方向组装）。它们直接进 argv、不经本地 shell——但 scp 的**远端**
/// 路径会被远端 shell 再次解析，因此调用方需自行对远端路径做 shell 转义（见 `files/mod.rs`）。
///
/// 受 `timeout(duration)` 约束：超时则杀掉子进程（`kill_on_drop`）并返回 `Ssh` 错误；
/// scp 退出码非 0 时，把脱敏后的 stderr 纳入 `Ssh` 错误返回。
pub async fn run_scp(
    server: &ServerProfile,
    secret: Option<&str>,
    args_after_opts: Vec<String>,
    duration: Duration,
) -> AppResult<()> {
    let port = server.port.to_string();
    // `_keyfile` 在本函数返回前保持存活——临时 0600 密钥文件在 drop 时删除，
    // 因此必须持有到 spawn + wait 全部结束。
    let mut _keyfile: Option<KeyFile> = None;
    let mut env: Option<(String, String)> = None;

    let (program, args) = match server.auth_kind {
        AuthKind::Agent => {
            let mut a = vec!["-o".into(), "BatchMode=yes".into()];
            a.extend(common_opts(CONNECT_TIMEOUT_SECS));
            a.push("-P".into());
            a.push(port);
            a.extend(args_after_opts);
            ("scp".to_string(), a)
        }
        AuthKind::Key => {
            let secret = secret.ok_or_else(|| AppError::Credential("no SSH key stored".into()))?;
            let kf = KeyFile::write(secret)?;
            let mut a = vec![
                "-o".into(),
                "BatchMode=yes".into(),
                "-o".into(),
                "IdentitiesOnly=yes".into(),
                "-i".into(),
                kf.path.display().to_string(),
            ];
            a.extend(common_opts(CONNECT_TIMEOUT_SECS));
            a.push("-P".into());
            a.push(port);
            a.extend(args_after_opts);
            _keyfile = Some(kf);
            ("scp".to_string(), a)
        }
        AuthKind::Password => {
            let secret =
                secret.ok_or_else(|| AppError::Credential("no SSH password stored".into()))?;
            if which("sshpass").is_none() {
                return Err(AppError::Ssh(
                    "password auth needs `sshpass` installed; prefer key or agent auth".into(),
                ));
            }
            // sshpass -e 从 SSHPASS 环境变量读取密码（绝不进 argv，避免在 `ps` 中可见）。
            let mut a = vec![
                "-e".into(),
                "scp".into(),
                "-o".into(),
                "PreferredAuthentications=password".into(),
                "-o".into(),
                "PubkeyAuthentication=no".into(),
            ];
            a.extend(common_opts(CONNECT_TIMEOUT_SECS));
            a.push("-P".into());
            a.push(port);
            a.extend(args_after_opts);
            env = Some(("SSHPASS".to_string(), secret.to_string()));
            ("sshpass".to_string(), a)
        }
    };

    // 以子进程方式启动 scp：stdin 设为 null，捕获 stdout/stderr，并启用 kill-on-drop。
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some((k, v)) = &env {
        cmd.env(k, v);
    }
    let child = cmd
        .spawn()
        .map_err(|e| AppError::Ssh(format!("failed to launch scp: {e}")))?;

    // 受超时约束：超时则子进程随 `child` drop 被杀（kill_on_drop）。
    let output = match timeout(duration, child.wait_with_output()).await {
        Ok(res) => res.map_err(|e| AppError::Ssh(e.to_string()))?,
        Err(_) => {
            return Err(AppError::Ssh(format!(
                "scp timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    if !output.status.success() {
        // 非 0 退出码：把脱敏后的 stderr 纳入错误（私钥/IP/token 等会被改写）。
        let stderr = sanitize(&String::from_utf8_lossy(&output.stderr));
        let stderr = stderr.trim();
        let msg = if stderr.is_empty() {
            format!("scp failed with exit code {}", output.status.code().unwrap_or(-1))
        } else {
            stderr.to_string()
        };
        return Err(AppError::Ssh(msg));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{new_id, now, ServerStatus};

    fn server(auth: AuthKind) -> ServerProfile {
        ServerProfile {
            id: new_id(),
            name: "t".into(),
            host: "127.0.0.1".into(),
            port: 22,
            username: "nobody".into(),
            auth_kind: auth,
            credential_ref: None,
            status: ServerStatus::Unknown,
            favorite: false,
            facts: Default::default(),
            created_at: now(),
            updated_at: now(),
        }
    }

    #[tokio::test]
    async fn readonly_blocks_non_inspection_commands() {
        let s = server(AuthKind::Agent);
        let err = run_readonly(&s, None, "rm -rf /var/www", DEFAULT_TIMEOUT).await.unwrap_err();
        assert_eq!(err.code(), "blocked");
    }

    #[tokio::test]
    async fn key_auth_without_secret_errors() {
        let s = server(AuthKind::Key);
        let err = run_command(&s, None, "true", DEFAULT_TIMEOUT).await.unwrap_err();
        assert_eq!(err.code(), "credential");
    }

    #[test]
    fn keyfile_is_deleted_on_drop() {
        let path;
        {
            let kf = KeyFile::write("secret-key-material").unwrap();
            path = kf.path.clone();
            assert!(path.exists());
        }
        assert!(!path.exists());
    }

    #[test]
    fn matching_run_ids_returns_only_target_server_runs() {
        let entries = [
            ("run-1", Some("server-a")),
            ("run-2", Some("server-b")),
            ("run-3", None),
            ("run-4", Some("server-a")),
        ];

        assert_eq!(
            matching_run_ids(entries, "server-a"),
            vec!["run-1".to_string(), "run-4".to_string()]
        );
        assert!(matching_run_ids(entries, "missing").is_empty());
    }
}
