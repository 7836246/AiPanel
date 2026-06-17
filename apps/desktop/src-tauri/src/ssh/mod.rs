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

    // Build the invocation per auth method.
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

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some((k, v)) = &env {
        cmd.env(k, v);
    }

    let child = cmd.spawn().map_err(|e| AppError::Ssh(format!("failed to launch ssh: {e}")))?;

    let output = match timeout(duration, child.wait_with_output()).await {
        Ok(res) => res.map_err(|e| AppError::Ssh(e.to_string()))?,
        Err(_) => {
            drop(keyfile); // ensure key removed even on timeout
            return Err(AppError::Ssh(format!(
                "command timed out after {}s",
                duration.as_secs()
            )));
        }
    };
    drop(keyfile);

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
