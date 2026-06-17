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
static CANCEL_REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Notify>>>> = OnceLock::new();

/// 取回（必要时初始化）全局取消注册表。
fn registry() -> &'static Mutex<HashMap<String, Arc<Notify>>> {
    CANCEL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 为一次运行登记取消句柄并返回它。流式命令在开始执行前调用，把返回的
/// [`Notify`] 传给可取消的流式执行器；运行结束后必须调用 [`unregister`]。
pub fn register(run_id: &str) -> Arc<Notify> {
    let notify = Arc::new(Notify::new());
    registry()
        .lock()
        .unwrap()
        .insert(run_id.to_string(), notify.clone());
    notify
}

/// 注销一次运行的取消句柄。无论成功、失败还是被取消都应调用，避免句柄泄漏。
pub fn unregister(run_id: &str) {
    registry().lock().unwrap().remove(run_id);
}

/// 请求取消指定运行。若该 `run_id` 仍在注册表中，唤醒其 [`Notify`]。
/// 找不到（已结束 / 从未登记）时静默忽略。
///
/// 用 `notify_one` 而非 `notify_waiters`：前者会在当前没有等待者时**存下一个许可**，
/// 这样即便取消请求恰好落在流式循环两次迭代之间（此刻没有 `.notified()` 在等待），
/// 下一次 `.notified()` 也会立刻返回，不会漏掉取消。
pub fn cancel(run_id: &str) {
    if let Some(notify) = registry().lock().unwrap().get(run_id) {
        notify.notify_one();
    }
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
    let mut in_key_block = false;
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
                        // 实时回调拿到的是按行（行内范围）脱敏的结果，用于临时展示；
                        // 存储的缓冲区保留原始行，以便最后做一次整缓冲区脱敏
                        //（私钥等跨行的敏感信息只有跨行才能匹配——见 core::sanitize）。
                        if let Some(t) = redact_live_line(line, &mut in_key_block) { on_line(&t, false); }
                        out_buf.push_str(line);
                        out_buf.push('\n');
                    }
                    Ok(None) => out_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
                line = err_reader.next_line(), if !err_done => match line {
                    Ok(Some(line)) => {
                        let line = line.strip_suffix('\r').unwrap_or(&line);
                        if let Some(t) = redact_live_line(line, &mut in_key_block) { on_line(&t, true); }
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

/// 连通性 + 认证探测。当一条简单的远程命令执行成功时返回 Ok(true)。
pub async fn check_connection(server: &ServerProfile, secret: Option<&str>) -> AppResult<bool> {
    let exec = run_command(server, secret, "true", Duration::from_secs(CONNECT_TIMEOUT_SECS + 5)).await?;
    Ok(exec.exit_code == 0)
}

/// 在 PATH 中定位可执行文件（用于探测可选的 `sshpass`）。
fn which(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|p| p.join(bin))
        .find(|p| p.is_file())
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
}
