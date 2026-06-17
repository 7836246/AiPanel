//! Codex app-server bridge transport.
//!
//! Launches `codex app-server` as a subprocess and speaks **newline-delimited
//! JSON-RPC 2.0** over its stdio. AiPanel drives the agent through this; the
//! agent reaches servers ONLY via AiPanel Tools (advertised at `initialize`),
//! never raw SSH/shell (docs/SECURITY_MODEL.zh-Hans.md).
//!
//! Scope of this module: the transport (spawn, framed request/response, the
//! `initialize` handshake) — implemented and unit-tested at the framing level.
//! The higher-level turn / tool-call loop builds on this; until it is verified
//! against an installed `codex app-server`, `CodexAppServerProvider`'s chat/plan
//! return a clear, documented error and `test()` performs a real `initialize`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};

/// Build a single JSON-RPC request line (newline-terminated).
pub fn build_request(id: u64, method: &str, params: Value) -> String {
    let mut line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    line.push('\n');
    line
}

/// Given a JSON-RPC response value, extract `result` or turn `error` into AppError.
pub fn parse_response(value: &Value, id: u64) -> Option<AppResult<Value>> {
    if value.get("id").and_then(|v| v.as_u64()) != Some(id) {
        return None; // not our response (notification or a different id)
    }
    if let Some(err) = value.get("error") {
        let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("unknown error");
        return Some(Err(AppError::Provider(format!("codex JSON-RPC error: {msg}"))));
    }
    Some(Ok(value.get("result").cloned().unwrap_or(Value::Null)))
}

/// A live connection to a `codex app-server` subprocess.
pub struct CodexClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: u64,
}

impl CodexClient {
    /// Spawn `<codex_path> app-server` and start reading its stdout.
    pub fn start(codex_path: &str) -> AppResult<Self> {
        let mut child = Command::new(codex_path)
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Provider(format!("无法启动 codex app-server（{codex_path}）: {e}")))?;

        let stdin = child.stdin.take().ok_or_else(|| AppError::Provider("no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::Provider("no stdout".into()))?;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(CodexClient { child, stdin, rx, next_id: 1 })
    }

    /// Send a request and wait (bounded) for the matching response.
    pub fn request(&mut self, method: &str, params: Value, timeout: Duration) -> AppResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.stdin
            .write_all(build_request(id, method, params).as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|e| AppError::Provider(format!("写入 codex 失败: {e}")))?;

        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(AppError::Provider("codex app-server 响应超时".into()));
            }
            match self.rx.recv_timeout(remaining) {
                Ok(line) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        if let Some(result) = parse_response(&v, id) {
                            return result;
                        }
                        // else: a notification or other id — keep waiting.
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(AppError::Provider("codex app-server 响应超时".into()))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(AppError::Provider("codex app-server 已退出".into()))
                }
            }
        }
    }

    /// The JSON-RPC `initialize` handshake. `tools` is the AiPanel Tools surface
    /// advertised to the agent — the only capability it may call.
    pub fn initialize(&mut self, tools: Value) -> AppResult<Value> {
        self.request(
            "initialize",
            json!({
                "clientInfo": { "name": "AiPanel", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "tools": tools },
            }),
            Duration::from_secs(15),
        )
    }
}

impl Drop for CodexClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_line_is_valid_jsonrpc() {
        let line = build_request(7, "initialize", json!({"a": 1}));
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 7);
        assert_eq!(v["method"], "initialize");
        assert_eq!(v["params"]["a"], 1);
    }

    #[test]
    fn parse_response_matches_id_and_extracts_result() {
        let v = json!({"jsonrpc": "2.0", "id": 3, "result": {"ok": true}});
        let got = parse_response(&v, 3).unwrap().unwrap();
        assert_eq!(got["ok"], true);
    }

    #[test]
    fn parse_response_ignores_other_ids_and_notifications() {
        assert!(parse_response(&json!({"id": 9, "result": {}}), 3).is_none());
        assert!(parse_response(&json!({"method": "event", "params": {}}), 3).is_none());
    }

    #[test]
    fn parse_response_surfaces_errors() {
        let v = json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -1, "message": "boom"}});
        let err = parse_response(&v, 1).unwrap().unwrap_err();
        assert_eq!(err.code(), "provider");
        assert!(err.to_string().contains("boom"));
    }
}
