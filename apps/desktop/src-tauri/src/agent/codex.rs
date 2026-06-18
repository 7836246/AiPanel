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

/// 把一个 JSON-RPC 值序列化为单行(带换行)写入子进程 stdin。
fn write_line(stdin: &mut ChildStdin, v: &Value) -> AppResult<()> {
    let mut s = v.to_string();
    s.push('\n');
    stdin
        .write_all(s.as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|e| AppError::Provider(format!("写入 codex 失败: {e}")))
}

/// 一次 turn 中,从 Codex app-server 收到的一个语义事件(从 JSON-RPC 通知里解析)。
///
/// Codex 以通知流的形式推进一个 turn:文本增量、工具调用请求、完成、错误。
/// 这里把原始 JSON 归一化为这几类,真正的协议字段差异都收敛在 [`classify_event`]。
#[derive(Debug, PartialEq)]
pub enum TurnEvent {
    /// agent 想调用一个 AiPanel 工具(我们分发后必须把结果回灌)。
    ToolCall { call_id: String, name: String, args: Value },
    /// agent 的文本增量(累计成最终回答)。
    Text(String),
    /// 本 turn 结束;可能附带最终消息文本。
    Complete(Option<String>),
    /// agent / 服务端报错。
    Error(String),
    /// 与本回路无关的通知(忽略)。
    Other,
}

/// 把一行 JSON-RPC 通知归一化为 [`TurnEvent`]。事件体可能直接在顶层,也可能在
/// `params` 下;字段名按常见形态做了容错(arguments/args、text/delta、message.text)。
pub fn classify_event(v: &Value) -> TurnEvent {
    let p = v.get("params").unwrap_or(v);
    let t = p.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match t {
        "tool_call" => TurnEvent::ToolCall {
            call_id: p.get("callId").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            name: p.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            args: p
                .get("arguments")
                .cloned()
                .or_else(|| p.get("args").cloned())
                .unwrap_or(Value::Null),
        },
        "agent_message" | "agent_message_delta" | "message" => TurnEvent::Text(
            p.get("text")
                .and_then(|x| x.as_str())
                .or_else(|| p.get("delta").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string(),
        ),
        "turn_completed" | "turn_complete" => TurnEvent::Complete(
            p.get("message")
                .and_then(|m| m.get("text"))
                .and_then(|x| x.as_str())
                .or_else(|| p.get("text").and_then(|x| x.as_str()))
                .map(|s| s.to_string()),
        ),
        "error" => TurnEvent::Error(
            p.get("message").and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
        ),
        _ => TurnEvent::Other,
    }
}

/// 事件回路从传输层取到的下一条输入。
pub enum Incoming {
    Line(Value),
    Timeout,
    Closed,
}

/// 与传输无关的 **turn 事件回路**——可注入式,因而可对模拟的 JSON-RPC 事件流做单测。
///
/// 反复读取事件:文本累计;遇到工具调用就交给 `on_tool` 分发,并把结果(或错误)
/// 经 `send_tool_result` 回灌给 agent;直到 turn 完成返回最终文本,或报错/超时/连接断开。
///
/// 安全:`on_tool` 是唯一的工具入口。写操作的授权完全由 `on_tool` 背后的
/// `tools::dispatch` 把关(`task.execute_confirmed` 无用户确认即拒绝);本回路只是
/// 忠实地分发并把结果回灌,绝不替 agent 放宽权限。
pub fn drive_turn(
    mut send_tool_result: impl FnMut(&str, &AppResult<Value>) -> AppResult<()>,
    mut recv: impl FnMut(Duration) -> Incoming,
    mut on_tool: impl FnMut(&str, &Value) -> AppResult<Value>,
    timeout: Duration,
) -> AppResult<String> {
    let deadline = std::time::Instant::now() + timeout;
    let mut acc = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(AppError::Provider("codex turn 响应超时".into()));
        }
        match recv(remaining) {
            Incoming::Closed => {
                return Err(AppError::Provider("codex app-server 已退出（turn 未完成）".into()))
            }
            Incoming::Timeout => return Err(AppError::Provider("codex turn 响应超时".into())),
            Incoming::Line(v) => match classify_event(&v) {
                TurnEvent::ToolCall { call_id, name, args } => {
                    let res = on_tool(&name, &args);
                    send_tool_result(&call_id, &res)?;
                }
                TurnEvent::Text(t) => acc.push_str(&t),
                TurnEvent::Complete(final_msg) => {
                    if !acc.is_empty() {
                        return Ok(acc);
                    }
                    return Ok(final_msg.unwrap_or_default());
                }
                TurnEvent::Error(m) => {
                    return Err(AppError::Provider(format!("codex turn 错误: {m}")))
                }
                TurnEvent::Other => {}
            },
        }
    }
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

    /// 跑完整的一个 turn:开 thread、发 `turn/start`、消费事件流。
    ///
    /// `turn/start` **不**走 [`request`](Self::request)(那会丢弃后续以通知形式
    /// 到达的事件)——而是写出后直接进入 [`drive_turn`] 事件回路:工具调用交给
    /// `on_tool` 分发并把结果回灌,最终返回 agent 的回答文本。
    pub fn run_turn(
        &mut self,
        user_msg: &str,
        on_tool: impl FnMut(&str, &Value) -> AppResult<Value>,
        timeout: Duration,
    ) -> AppResult<String> {
        // 1) 开一个会话线程。
        let thread = self.request("thread/start", json!({}), Duration::from_secs(15))?;
        let thread_id = thread.get("threadId").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // 2) 发起 turn(写出即可,响应/事件随后以通知形式到来)。
        let id = self.next_id;
        self.next_id += 1;
        let turn = json!({
            "jsonrpc": "2.0", "id": id, "method": "turn/start",
            "params": { "threadId": thread_id, "input": user_msg },
        });
        write_line(&mut self.stdin, &turn)?;

        // 3) 事件回路。分别借用 stdin / rx(不同字段,借用不冲突)。
        let stdin = &mut self.stdin;
        let rx = &self.rx;
        drive_turn(
            |call_id, res| {
                let params = match res {
                    Ok(v) => json!({ "callId": call_id, "output": v }),
                    Err(e) => json!({ "callId": call_id, "error": e.to_string() }),
                };
                write_line(stdin, &json!({ "jsonrpc": "2.0", "method": "tool/result", "params": params }))
            },
            |dur| match rx.recv_timeout(dur) {
                Ok(line) => Incoming::Line(serde_json::from_str(&line).unwrap_or(Value::Null)),
                Err(RecvTimeoutError::Timeout) => Incoming::Timeout,
                Err(RecvTimeoutError::Disconnected) => Incoming::Closed,
            },
            on_tool,
            timeout,
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

    // ---- turn / tool-call 事件回路（对模拟 JSON-RPC 事件流的单测）----

    /// 把一串预设事件做成 `recv` 闭包;耗尽后返回 Closed。
    fn scripted(events: Vec<Incoming>) -> impl FnMut(Duration) -> Incoming {
        let mut it = events.into_iter();
        move |_dur| it.next().unwrap_or(Incoming::Closed)
    }

    #[test]
    fn classify_event_recognizes_variants() {
        assert!(matches!(
            classify_event(&json!({"params":{"type":"tool_call","callId":"c1","name":"server.list","arguments":{"a":1}}})),
            TurnEvent::ToolCall { .. }
        ));
        assert_eq!(
            classify_event(&json!({"params":{"type":"agent_message","text":"hi"}})),
            TurnEvent::Text("hi".into())
        );
        assert_eq!(classify_event(&json!({"params":{"type":"turn_completed"}})), TurnEvent::Complete(None));
        assert!(matches!(classify_event(&json!({"params":{"type":"error","message":"boom"}})), TurnEvent::Error(_)));
        assert_eq!(classify_event(&json!({"params":{"type":"whatever"}})), TurnEvent::Other);
    }

    #[test]
    fn drive_turn_dispatches_tool_then_completes() {
        let events = vec![
            Incoming::Line(json!({"params":{"type":"tool_call","callId":"c1","name":"server.list","arguments":{}}})),
            Incoming::Line(json!({"params":{"type":"agent_message","text":"已检查"}})),
            Incoming::Line(json!({"params":{"type":"turn_completed"}})),
        ];
        let mut sent: Vec<(String, bool)> = vec![];
        let mut tools_called: Vec<String> = vec![];
        let out = drive_turn(
            |call_id, res| {
                sent.push((call_id.to_string(), res.is_ok()));
                Ok(())
            },
            scripted(events),
            |name, _args| {
                tools_called.push(name.to_string());
                Ok(json!({ "ok": true }))
            },
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out, "已检查");
        assert_eq!(tools_called, vec!["server.list".to_string()]);
        assert_eq!(sent, vec![("c1".to_string(), true)]);
    }

    #[test]
    fn drive_turn_accumulates_text_deltas() {
        let events = vec![
            Incoming::Line(json!({"params":{"type":"agent_message_delta","delta":"foo"}})),
            Incoming::Line(json!({"params":{"type":"agent_message_delta","delta":"bar"}})),
            Incoming::Line(json!({"params":{"type":"turn_completed","text":"ignored"}})),
        ];
        let out = drive_turn(|_, _| Ok(()), scripted(events), |_, _| Ok(json!(null)), Duration::from_secs(5)).unwrap();
        assert_eq!(out, "foobar");
    }

    #[test]
    fn drive_turn_uses_final_message_when_no_stream() {
        let events = vec![Incoming::Line(json!({"params":{"type":"turn_completed","message":{"text":"final"}}}))];
        let out = drive_turn(|_, _| Ok(()), scripted(events), |_, _| Ok(json!(null)), Duration::from_secs(5)).unwrap();
        assert_eq!(out, "final");
    }

    #[test]
    fn drive_turn_surfaces_error_event() {
        let events = vec![Incoming::Line(json!({"params":{"type":"error","message":"boom"}}))];
        let err = drive_turn(|_, _| Ok(()), scripted(events), |_, _| Ok(json!(null)), Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.code(), "provider");
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn drive_turn_errors_on_closed_and_timeout() {
        let e1 = drive_turn(|_, _| Ok(()), scripted(vec![Incoming::Closed]), |_, _| Ok(json!(null)), Duration::from_secs(5)).unwrap_err();
        assert_eq!(e1.code(), "provider");
        let e2 = drive_turn(|_, _| Ok(()), scripted(vec![Incoming::Timeout]), |_, _| Ok(json!(null)), Duration::from_secs(5)).unwrap_err();
        assert_eq!(e2.code(), "provider");
    }

    #[test]
    fn drive_turn_relays_tool_error_without_crashing() {
        // 模拟 agent 调用写工具但未带确认:on_tool 返回 Blocked,回路应把错误回灌,turn 仍能完成。
        // 这验证了「写操作不由 Agent 自行授权」的边界由工具层把关,回路忠实转达拒绝。
        let events = vec![
            Incoming::Line(json!({"params":{"type":"tool_call","callId":"w1","name":"task.execute_confirmed","arguments":{"confirmed":false}}})),
            Incoming::Line(json!({"params":{"type":"agent_message","text":"已被拒绝"}})),
            Incoming::Line(json!({"params":{"type":"turn_completed"}})),
        ];
        let mut relayed_error = false;
        let out = drive_turn(
            |_call_id, res| {
                if res.is_err() {
                    relayed_error = true;
                }
                Ok(())
            },
            scripted(events),
            |name, args| {
                if name == "task.execute_confirmed"
                    && !args.get("confirmed").and_then(|v| v.as_bool()).unwrap_or(false)
                {
                    Err(AppError::Blocked("需要用户确认".into()))
                } else {
                    Ok(json!({ "ok": true }))
                }
            },
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(relayed_error);
        assert_eq!(out, "已被拒绝");
    }
}
