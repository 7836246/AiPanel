//! SSH Executor.
//!
//! Runs commands on remote servers through the **system OpenSSH client** — we do
//! not implement the SSH protocol. AiPanel owns this path entirely; the agent
//! never gets raw SSH access (see docs/SECURITY_MODEL.zh-Hans.md).
//!
//! Safety properties:
//! - read-only execution is gated on the Risk Reviewer classifying the command
//!   as `Low` (the inspection allowlist);
//! - every command has a timeout and the child is killed if it elapses;
//! - stdout/stderr are sanitized before they leave this module;
//! - private keys are written to a 0600 temp file used only for the call and
//!   deleted immediately after.

use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::core::error::{AppError, AppResult};
use crate::core::sanitize::sanitize;
use crate::core::types::{AuthKind, CommandExecution, RiskLevel, ServerProfile};
use crate::risk::classify_command;

/// Default per-command timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default connection timeout (also passed to ssh's ConnectTimeout).
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Temp private-key file, deleted on drop.
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

/// Shared ssh hardening options.
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

/// A fully-built ssh invocation: the program to run, its argv, and an optional
/// env var (for `sshpass -e`). The [`KeyFile`] is kept alive for the duration of
/// the call and deleted on drop.
struct Invocation {
    program: String,
    args: Vec<String>,
    env: Option<(String, String)>,
    /// Held only to keep the temp key alive for the call; deleted on drop.
    _keyfile: Option<KeyFile>,
}

/// Build the ssh invocation for a server + command, per auth method. Shared by
/// the blocking and streaming executors so both speak ssh identically.
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
            // sshpass -e reads the password from the SSHPASS env var (not argv,
            // so it isn't visible in `ps`).
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

/// Spawn an ssh invocation as a child process with stdio piped and kill-on-drop.
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

/// Run a raw command on the server. Callers are responsible for risk review —
/// use [`run_readonly`] for anything driven by the agent or untrusted input.
pub async fn run_command(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
) -> AppResult<CommandExecution> {
    let started_at = crate::core::types::now();
    let start = Instant::now();

    // Build the invocation per auth method. `inv` owns the keyfile (if any), so
    // the key is deleted on drop — including the timeout path below, where `inv`
    // goes out of scope on return.
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

/// Run a command only if the Risk Reviewer classifies it as read-only (`Low`).
/// This is the safe entry point for anything not already user-confirmed.
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

/// Streaming counterpart to [`run_readonly`]: runs a `Low`-classified command and
/// invokes `on_line` for each line as it arrives, so the UI can fill in live.
/// The line is sanitized before the callback sees it; `stderr` flags whether the
/// line came from stderr. The returned [`CommandExecution`] carries the full
/// sanitized output, identical in shape to [`run_command`].
pub async fn run_readonly_streamed(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
) -> AppResult<CommandExecution> {
    // Same safety gate as run_readonly: only inspection commands may stream.
    if classify_command(command).level != RiskLevel::Low {
        return Err(AppError::Blocked(format!(
            "command is not read-only and cannot run in inspection mode: {command}"
        )));
    }

    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` owns the keyfile (if any) for the lifetime of the call.
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

    // Read both streams concurrently, forwarding each sanitized line via the
    // callback and accumulating the full output.
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
                        // Live callback gets per-line (line-scoped) redaction for
                        // ephemeral display; the stored buffer keeps the RAW line
                        // so it can be whole-buffer sanitized once at the end
                        // (multi-line secrets like private keys only match across
                        // lines — see core::sanitize).
                        on_line(&sanitize(&line), false);
                        out_buf.push_str(&line);
                        out_buf.push('\n');
                    }
                    Ok(None) => out_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
                line = err_reader.next_line(), if !err_done => match line {
                    Ok(Some(line)) => {
                        on_line(&sanitize(&line), true);
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
            // `inv` (and its keyfile) drops on return; kill_on_drop reaps the child.
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: status.code().unwrap_or(-1),
        // Sanitize the WHOLE accumulated buffer once (matching run_command), so
        // multi-line secrets are redacted before the output is stored/audited.
        // The per-line callback above only weakly redacts the live stream.
        stdout: sanitize(out_buf.trim_end_matches('\n')),
        stderr: sanitize(err_buf.trim_end_matches('\n')),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    })
}

/// Streaming counterpart to [`run_command`]: runs an **already-confirmed** step
/// (including writes) and invokes `on_line` for each line as it arrives, so the
/// console can fill in live. Identical to [`run_readonly_streamed`] EXCEPT there
/// is no `Low`-classification gate — confirmation/double-confirmation has already
/// been enforced by the caller (execute_confirmed_plan / run_confirmed_plan_stream).
/// The line is sanitized before the callback sees it; `stderr` flags whether the
/// line came from stderr. The returned [`CommandExecution`] carries the full
/// sanitized output, identical in shape to [`run_command`].
pub async fn run_command_streamed(
    server: &ServerProfile,
    secret: Option<&str>,
    command: &str,
    duration: Duration,
    on_line: &(dyn Fn(&str, bool) + Sync + Send),
) -> AppResult<CommandExecution> {
    let started_at = crate::core::types::now();
    let start = Instant::now();

    // `inv` owns the keyfile (if any) for the lifetime of the call.
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

    // Read both streams concurrently, forwarding each sanitized line via the
    // callback and accumulating the full output.
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
                        // Live callback gets per-line (line-scoped) redaction for
                        // ephemeral display; the stored buffer keeps the RAW line
                        // so it can be whole-buffer sanitized once at the end
                        // (multi-line secrets like private keys only match across
                        // lines — see core::sanitize).
                        on_line(&sanitize(&line), false);
                        out_buf.push_str(&line);
                        out_buf.push('\n');
                    }
                    Ok(None) => out_done = true,
                    Err(e) => return Err(AppError::Ssh(e.to_string())),
                },
                line = err_reader.next_line(), if !err_done => match line {
                    Ok(Some(line)) => {
                        on_line(&sanitize(&line), true);
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
            // `inv` (and its keyfile) drops on return; kill_on_drop reaps the child.
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: status.code().unwrap_or(-1),
        // Sanitize the WHOLE accumulated buffer once (matching run_command), so
        // multi-line secrets are redacted before the output is stored/audited.
        // The per-line callback above only weakly redacts the live stream.
        stdout: sanitize(out_buf.trim_end_matches('\n')),
        stderr: sanitize(err_buf.trim_end_matches('\n')),
        duration_ms: start.elapsed().as_millis() as u64,
        started_at,
    })
}

/// Connectivity + auth probe. Returns Ok(true) when a trivial remote command
/// succeeds.
pub async fn check_connection(server: &ServerProfile, secret: Option<&str>) -> AppResult<bool> {
    let exec = run_command(server, secret, "true", Duration::from_secs(CONNECT_TIMEOUT_SECS + 5)).await?;
    Ok(exec.exit_code == 0)
}

/// Locate an executable on PATH (used to detect optional `sshpass`).
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
