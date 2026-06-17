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

use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::core::error::{AppError, AppResult};
use crate::core::sanitize::sanitize;
use crate::core::types::{AuthKind, CommandExecution, RiskLevel, ServerProfile};
use crate::risk::classify_command;

/// 单条命令的默认超时时间。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// 默认连接超时（同时传给 ssh 的 ConnectTimeout）。
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

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
fn build_invocation(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
) -> AppResult<Invocation> {
    let mut keyfile: Option<KeyFile> = None;
    let (program, args, env) = match server.auth_kind {
        AuthKind::Agent => {
            let mut a = vec!["-o".into(), "BatchMode=yes".into()];
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
    let inv = build_invocation(server, secret, command)?;
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
    // 与 run_readonly 相同的安全闸门：只有检查类命令才能流式执行。
    if classify_command(command).level != RiskLevel::Low {
        return Err(AppError::Blocked(format!(
            "command is not read-only and cannot run in inspection mode: {command}"
        )));
    }

    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` 在整个调用生命周期内持有密钥文件（如果有）。
    let inv = build_invocation(server, secret, command)?;
    let mut child = spawn_child(&inv)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stderr".into()))?;

    // 并发读取两个流，通过回调转发每一条脱敏后的行，同时累积完整输出。
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
                line = out_reader.next_line(), if !out_done => match line {
                    Ok(Some(line)) => {
                        // 实时回调拿到的是按行（行内范围）脱敏的结果，用于临时展示；
                        // 存储的缓冲区保留原始行，以便最后做一次整缓冲区脱敏
                        //（私钥等跨行的敏感信息只有跨行才能匹配——见 core::sanitize）。
                        if let Some(t) = redact_live_line(&line, &mut in_key_block) { on_line(&t, false); }
                        out_buf.push_str(&line);
                        out_buf.push('\n');
                    }
                    Ok(None) => out_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
                line = err_reader.next_line(), if !err_done => match line {
                    Ok(Some(line)) => {
                        if let Some(t) = redact_live_line(&line, &mut in_key_block) { on_line(&t, true); }
                        err_buf.push_str(&line);
                        err_buf.push('\n');
                    }
                    Ok(None) => err_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
            }
        }
        let status = child.wait().await.map_err(|e| AppError::Ssh(e.to_string()))?;
        Ok::<_, AppError>((status, out_buf, err_buf))
    };

    let (status, out_buf, err_buf) = match timeout(duration, stream).await {
        Ok(res) => res?,
        Err(_) => {
            // `inv`（及其密钥文件）在返回时 drop；kill_on_drop 会回收子进程。
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: status.code().unwrap_or(-1),
        // 对整个累积缓冲区做一次脱敏（与 run_command 一致），确保跨行的敏感
        // 信息在输出被存储/审计前已被脱敏。上面的按行回调只对实时流做了弱脱敏。
        stdout: sanitize(out_buf.trim_end_matches('\n')),
        stderr: sanitize(err_buf.trim_end_matches('\n')),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    })
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
    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` 在整个调用生命周期内持有密钥文件（如果有）。
    let inv = build_invocation(server, secret, command)?;
    let mut child = spawn_child(&inv)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Ssh("failed to capture ssh stderr".into()))?;

    // 并发读取两个流，通过回调转发每一条脱敏后的行，同时累积完整输出。
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
                line = out_reader.next_line(), if !out_done => match line {
                    Ok(Some(line)) => {
                        // 实时回调拿到的是按行（行内范围）脱敏的结果，用于临时展示；
                        // 存储的缓冲区保留原始行，以便最后做一次整缓冲区脱敏
                        //（私钥等跨行的敏感信息只有跨行才能匹配——见 core::sanitize）。
                        if let Some(t) = redact_live_line(&line, &mut in_key_block) { on_line(&t, false); }
                        out_buf.push_str(&line);
                        out_buf.push('\n');
                    }
                    Ok(None) => out_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
                line = err_reader.next_line(), if !err_done => match line {
                    Ok(Some(line)) => {
                        if let Some(t) = redact_live_line(&line, &mut in_key_block) { on_line(&t, true); }
                        err_buf.push_str(&line);
                        err_buf.push('\n');
                    }
                    Ok(None) => err_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
            }
        }
        let status = child.wait().await.map_err(|e| AppError::Ssh(e.to_string()))?;
        Ok::<_, AppError>((status, out_buf, err_buf))
    };

    let (status, out_buf, err_buf) = match timeout(duration, stream).await {
        Ok(res) => res?,
        Err(_) => {
            // `inv`（及其密钥文件）在返回时 drop；kill_on_drop 会回收子进程。
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: status.code().unwrap_or(-1),
        // 对整个累积缓冲区做一次脱敏（与 run_command 一致），确保跨行的敏感
        // 信息在输出被存储/审计前已被脱敏。上面的按行回调只对实时流做了弱脱敏。
        stdout: sanitize(out_buf.trim_end_matches('\n')),
        stderr: sanitize(err_buf.trim_end_matches('\n')),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    })
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
