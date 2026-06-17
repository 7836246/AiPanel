//! Codex app-server 桥接的传输层。
//!
//! 把 `codex app-server` 作为子进程启动，并在其 stdio 上以**换行分隔的
//! JSON-RPC 2.0** 通信。AiPanel 通过它驱动 agent；agent 只能经由 AiPanel Tools
//! （在 `initialize` 时声明）触达服务器，绝不走裸 SSH/shell
//! （见 docs/SECURITY_MODEL.zh-Hans.md）。
//!
//! 本模块范围：传输层（启动、带帧的请求/响应、`initialize` 握手）——已实现
//! 并在分帧层面有单元测试。更上层的 turn / 工具调用回路在此之上构建；在它
//! 对照已安装的 `codex app-server` 验证通过之前，`CodexAppServerProvider` 的
//! chat/plan 返回明确且有文档说明的错误，而 `test()` 会执行一次真实的
//! `initialize`。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};

/// 构造单行 JSON-RPC 请求（以换行符结尾）。
pub fn build_request(id: u64, method: &str, params: Value) -> String {
    let mut line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    line.push('\n');
    line
}

/// 给定一个 JSON-RPC 响应值，提取 `result`，或把 `error` 转成 AppError。
pub fn parse_response(value: &Value, id: u64) -> Option<AppResult<Value>> {
    if value.get("id").and_then(|v| v.as_u64()) != Some(id) {
        return None; // 不是我们要的响应（通知，或别的 id）
    }
    if let Some(err) = value.get("error") {
        let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("unknown error");
        return Some(Err(AppError::Provider(format!("codex JSON-RPC error: {msg}"))));
    }
    Some(Ok(value.get("result").cloned().unwrap_or(Value::Null)))
}

/// 到 `codex app-server` 子进程的一个活动连接。
pub struct CodexClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: u64,
}

impl CodexClient {
    /// 启动 `<codex_path> app-server` 并开始读取其 stdout。
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

    /// 发送一个请求，并（带超时地）等待匹配的响应。
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
                        // 否则：是通知或别的 id —— 继续等。
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

    /// JSON-RPC `initialize` 握手。`tools` 是向 agent 声明的 AiPanel Tools
    /// 能力清单——也是它唯一可调用的能力。
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
