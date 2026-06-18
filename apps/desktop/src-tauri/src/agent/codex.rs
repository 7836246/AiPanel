//! Codex app-server 桥接的传输层 + turn / 工具回路。
//!
//! 把 `codex app-server` 作为子进程启动，并在其 stdio 上以**换行分隔 JSON**
//! 通信（与 Codex 桌面 app 同一引擎:`codex-cli`）。AiPanel 通过它
//! 驱动 agent；agent 只能经由 AiPanel 审核过的工具触达服务器,绝不走裸 SSH/shell
//! （见 docs/SECURITY_MODEL.zh-Hans.md）。
//!
//! 协议字段对齐**真实** app-server（由 `codex app-server generate-json-schema` 导出):
//! - 握手:`initialize` 请求 + `initialized` 通知;
//! - `thread/start`(带 `sandbox`/`approvalPolicy`)→ 响应 `.thread.id`;
//! - `turn/start`(`input:[{type:"text",text,text_elements:[]}]`)→ 响应 `.turn.id`;
//! - 事件流(通知):`item/agentMessage/delta` 累计文本、`turn/completed` 收尾、`error`;
//! - **服务端请求**(需回同 id 响应):`item/tool/call`(客户端工具,经 `on_tool`
//!   分发并回 `DynamicToolCallResponse`)、各 `*Approval`(codex 原生本地 shell/文件
//!   操作的审批——**一律拒绝**,服务器只能经 AiPanel 工具触达)。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};

/// 构造单行 app-server 请求（以换行符结尾）。
///
/// codex app-server 的 stdio transport 使用 newline-delimited JSON。生成的
/// protocol schema 把消息命名为 JSON-RPC，但真实 `ClientRequest` 不接受
/// `jsonrpc: "2.0"` 字段；带上该字段会让 0.141.0 sidecar 忽略请求。
pub fn build_request(id: u64, method: &str, params: Value) -> String {
    let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
    line.push('\n');
    line
}

fn text_input(text: &str) -> Value {
    json!({ "type": "text", "text": text, "text_elements": [] })
}

/// 给定一个 app-server 响应值，提取 `result`，或把 `error` 转成 AppError。
pub fn parse_response(value: &Value, id: u64) -> Option<AppResult<Value>> {
    if value.get("id").and_then(|v| v.as_u64()) != Some(id) {
        return None; // 不是我们要的响应（通知，或别的 id）
    }
    if let Some(err) = value.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Some(Err(AppError::Provider(format!(
            "codex app-server 错误: {}",
            user_facing_provider_error(msg)
        ))));
    }
    Some(Ok(value.get("result").cloned().unwrap_or(Value::Null)))
}

/// 把一个消息值序列化为单行(带换行)写入子进程 stdin。
fn write_line(stdin: &mut ChildStdin, v: &Value) -> AppResult<()> {
    let mut s = v.to_string();
    s.push('\n');
    stdin
        .write_all(s.as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|e| AppError::Provider(format!("写入 codex 失败: {e}")))
}

/// codex 在一个 turn 中可能要求客户端「审批」的服务端请求方法。这些都对应
/// codex **原生、在本机沙箱里**执行命令/改文件/提权——对 AiPanel 的远端 SSH 运维
/// 既不适用也不安全,因此一律拒绝;服务器访问只能经 AiPanel 工具。
const APPROVAL_METHODS: &[&str] = &[
    "execCommandApproval",
    "applyPatchApproval",
    "item/commandExecution/requestApproval",
    "item/fileChange/requestApproval",
    "item/permissions/requestApproval",
    "item/tool/requestUserInput",
];

/// 从 app-server 收到的一条消息,归一化为本回路关心的语义。请求(带 `id`)必须回
/// 同 id response;通知(无 `id`)只观察。
#[derive(Debug, PartialEq)]
pub enum Msg {
    /// 服务端请求:agent 调用客户端工具(`item/tool/call` / `DynamicToolCallParams`)。
    ToolCall {
        id: Value,
        tool: String,
        args: Value,
    },
    /// 服务端请求:原生 shell/文件/提权审批 → 一律拒绝。
    Approval { id: Value },
    /// 其它未支持的服务端请求 → 回错误,避免 codex 卡住等待。
    UnknownRequest { id: Value, method: String },
    /// 通知:agent 文本增量(累计成最终回答)。
    Text(String),
    /// 通知:turn 结束(可能附带最终消息文本)。
    Complete(Option<String>),
    /// 通知:错误。
    Error(String),
    /// 与本回路无关(其它通知 / 我方请求的响应)。
    Other,
}

/// 把一行 JSON-RPC 消息归一化为 [`Msg`]。
pub fn classify(v: &Value) -> Msg {
    let method = v.get("method").and_then(|m| m.as_str());
    let id = v.get("id");
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    match (method, id) {
        // ---- 服务端请求(带 id,需回应)----
        (Some("item/tool/call"), Some(id)) => Msg::ToolCall {
            id: id.clone(),
            tool: params
                .get("tool")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            args: params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| params.get("args").cloned().unwrap_or(Value::Null)),
        },
        (Some(m), Some(id)) if APPROVAL_METHODS.contains(&m) => Msg::Approval { id: id.clone() },
        (Some(m), Some(id)) => Msg::UnknownRequest {
            id: id.clone(),
            method: m.to_string(),
        },
        // ---- 通知(无 id,只观察)----
        (Some("item/agentMessage/delta"), None) => Msg::Text(
            params
                .get("delta")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        (Some("rawResponseItem/completed"), None) => params
            .get("item")
            .and_then(extract_item_text)
            .map(Msg::Text)
            .unwrap_or(Msg::Other),
        (Some("turn/completed"), None) => {
            if let Some(m) = completed_error(&params) {
                Msg::Error(format!("codex turn 失败: {m}"))
            } else {
                Msg::Complete(completed_text(&params))
            }
        }
        (Some("error"), None) => {
            if params.get("willRetry").and_then(|x| x.as_bool()) == Some(true) {
                Msg::Other
            } else {
                Msg::Error(error_message(&params))
            }
        }
        _ => Msg::Other,
    }
}

fn error_message(params: &Value) -> String {
    let message = params
        .get("message")
        .and_then(|x| x.as_str())
        .or_else(|| {
            params
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|x| x.as_str())
        })
        .unwrap_or("unknown");
    let detail = params
        .get("additionalDetails")
        .and_then(|x| x.as_str())
        .or_else(|| {
            params
                .get("error")
                .and_then(|e| e.get("additionalDetails"))
                .and_then(|x| x.as_str())
        });
    let raw = match detail {
        Some(d) if !d.is_empty() => format!("{message}: {d}"),
        _ => message.to_string(),
    };
    user_facing_provider_error(&raw)
}

fn user_facing_provider_error(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("invalid api key")
    {
        return "模型供应商认证失败：请在设置里检查该供应商的 API Key、Base URL 与模型是否匹配。"
            .to_string();
    }
    if lower.contains("api key")
        || lower.contains("apikey")
        || lower.contains("missing key")
        || lower.contains("no auth")
    {
        return "模型供应商缺少 API Key：请在设置里为当前供应商保存有效密钥后重试。".to_string();
    }
    crate::core::sanitize::sanitize(message)
}

fn completed_error(params: &Value) -> Option<String> {
    let turn = params.get("turn")?;
    let status_type = turn
        .get("status")
        .and_then(|s| s.get("type"))
        .and_then(|s| s.as_str());
    if status_type != Some("failed") {
        return None;
    }
    turn.get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .map(|s| s.to_string())
        .or_else(|| Some("unknown failure".to_string()))
}

fn completed_text(params: &Value) -> Option<String> {
    params
        .get("turn")
        .and_then(|t| t.get("items"))
        .and_then(|items| items.as_array())
        .and_then(|items| items.iter().rev().find_map(extract_item_text))
        .or_else(|| params.get("item").and_then(extract_item_text))
        .or_else(|| {
            params
                .get("text")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
}

/// 从一个 turn item 里尽力抽取可读文本(用于 `turn/completed` 没有走增量时兜底)。
fn extract_item_text(item: &Value) -> Option<String> {
    if item.get("type").and_then(|t| t.as_str()) == Some("agentMessage") {
        return item
            .get("text")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    }
    if item.get("type").and_then(|t| t.as_str()) == Some("message") {
        return item
            .get("content")
            .and_then(|c| c.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("output_text"))
                    .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty());
    }
    item.get("text")
        .and_then(|x| x.as_str())
        .or_else(|| {
            item.get("message")
                .and_then(|m| m.get("text"))
                .and_then(|x| x.as_str())
        })
        .map(|s| s.to_string())
}

/// 事件回路从传输层取到的下一条输入。
pub enum Incoming {
    Line(Value),
    Timeout,
    Closed,
}

/// 与传输无关的 **turn 事件回路**——可注入式,因而可对模拟的 JSON-RPC 事件流做单测。
///
/// 反复读取消息:文本增量累计;`item/tool/call` 交给 `on_tool` 分发并回
/// `DynamicToolCallResponse`;各审批请求一律拒绝;直到 `turn/completed` 返回最终文本,
/// 或 `error`/超时/连接断开报错。
///
/// 安全:`on_tool` 是唯一的工具入口(其背后是 `tools::dispatch`,写操作授权由工具层
/// 把关);审批请求被硬拒绝,绝不替 agent 放宽权限。
pub fn drive_turn(
    mut send: impl FnMut(Value) -> AppResult<()>,
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
                return Err(AppError::Provider(
                    "codex app-server 已退出（turn 未完成）".into(),
                ))
            }
            Incoming::Timeout => return Err(AppError::Provider("codex turn 响应超时".into())),
            Incoming::Line(v) => match classify(&v) {
                Msg::ToolCall { id, tool, args } => {
                    let (success, text) = match on_tool(&tool, &args) {
                        Ok(v) => (true, v.to_string()),
                        Err(e) => (false, e.to_string()),
                    };
                    send(json!({
                        "id": id,
                        "result": {
                            "contentItems": [{ "type": "inputText", "text": text }],
                            "success": success,
                        },
                    }))?;
                }
                Msg::Approval { id } => {
                    // codex 原生 shell/文件/提权审批:一律拒绝。
                    send(json!({ "id": id, "result": { "decision": "denied" } }))?;
                }
                Msg::UnknownRequest { id, method } => {
                    send(json!({
                        "id": id,
                        "error": { "code": -32601, "message": format!("AiPanel 不支持该请求: {method}") },
                    }))?;
                }
                Msg::Text(t) => acc.push_str(&t),
                Msg::Complete(final_msg) => {
                    if !acc.is_empty() {
                        return Ok(acc);
                    }
                    return Ok(final_msg.unwrap_or_default());
                }
                Msg::Error(m) => return Err(AppError::Provider(format!("codex turn 错误: {m}"))),
                Msg::Other => {}
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
    codex_home: PathBuf,
    stdout_tail: Arc<Mutex<Vec<String>>>,
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

/// codex 的私有 `CODEX_HOME`——每个子进程使用独立临时目录，把 codex 的会话/配置
/// 隔离到 AiPanel 专属空间，并避免不同测试/会话复用旧 session、插件缓存而互相污染。
/// **绝不读取用户的 `~/.codex`**(否则会加载用户个人的 MCP 服务器等,既污染又有
/// 安全风险;已对真实二进制验证:设了它就不会启动用户的 MCP)。
fn isolated_codex_home() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let home = std::env::temp_dir().join(format!(
        "aipanel-codex-home-{}-{nonce}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&home);
    home
}

/// codex 读取 API Key 的环境变量名(配进 model_providers.env_key,密钥经 env 传入、不进 argv)。
const KEY_ENV: &str = "AIPANEL_OAI_KEY";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn push_stderr_tail(buf: &Arc<Mutex<Vec<String>>>, line: String) {
    push_tail(buf, line);
}

fn push_stdout_tail(buf: &Arc<Mutex<Vec<String>>>, line: String) {
    push_tail(buf, line);
}

fn push_tail(buf: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut guard) = buf.lock() {
        guard.push(line);
        if guard.len() > 20 {
            guard.remove(0);
        }
    }
}

fn stderr_tail(buf: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    tail(buf)
}

fn stdout_tail(buf: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    tail(buf)
}

fn tail(buf: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    let guard = buf.lock().ok()?;
    if guard.is_empty() {
        return None;
    }
    Some(crate::core::sanitize::sanitize(&guard.join("\n")))
}

/// 把 AiPanel 自己作为 MCP 服务器注入 codex,让 codex 经 MCP 调用 AiPanel 的只读
/// server-ops 工具。codex 会按此拉起 `<aipanel_exe> mcp-server`(带 `AIPANEL_DATA_DIR`
/// 复用同一份 SQLite/Keychain)。
pub struct McpBridge {
    pub aipanel_exe: String,
    pub data_dir: String,
    pub trace_path: Option<String>,
}

/// 启动 codex-app-server 所需的配置。base/key/model 复用用户的 OpenAI 兼容供应商配置。
pub struct CodexLaunch {
    /// 解析好的二进制路径(见 [`resolve_codex_bin`])。
    pub program: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    /// 设置后:注入 AiPanel MCP 工具面(用于带工具的自动诊断);None 则纯对话/规划。
    pub mcp: Option<McpBridge>,
}

/// 当前运行平台的 target triple(用于定位打包的 sidecar 文件名)。
fn target_triple() -> String {
    let arch = std::env::consts::ARCH; // aarch64 / x86_64
    if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else if cfg!(target_os = "windows") {
        format!("{arch}-pc-windows-msvc")
    } else {
        format!("{arch}-unknown-linux-musl")
    }
}

/// 解析要用的 codex-app-server 二进制路径(始终优先用打包的那份):
/// 1) 显式配置且存在;2) `AIPANEL_CODEX_BIN` 环境变量;3) 与主程序同目录的 sidecar;
/// 4) 开发期 `src-tauri/binaries/`;5) 回退 PATH 上的 `codex-app-server`。
pub fn resolve_codex_bin(configured: Option<&str>) -> String {
    use std::path::Path;
    if let Some(p) = configured {
        if !p.is_empty() && Path::new(p).exists() {
            return p.to_string();
        }
    }
    if let Ok(p) = std::env::var("AIPANEL_CODEX_BIN") {
        if Path::new(&p).exists() {
            return p;
        }
    }
    let triple = target_triple();
    let exe_name = if cfg!(target_os = "windows") {
        format!("codex-app-server-{triple}.exe")
    } else {
        format!("codex-app-server-{triple}")
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [
                exe_name.as_str(),
                "codex-app-server",
                "codex-app-server.exe",
            ] {
                let c = dir.join(name);
                if c.exists() {
                    return c.display().to_string();
                }
            }
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(&exe_name);
    if dev.exists() {
        return dev.display().to_string();
    }
    "codex-app-server".to_string()
}

impl CodexClient {
    /// 启动打包的 `codex-app-server`(它**直接**就是 app-server,无需子命令),并开始读取
    /// 其 stdout。用隔离的 `CODEX_HOME` 防止加载用户 `~/.codex`;把用户的 OpenAI 兼容
    /// base/key/model 配成 codex 的 `model_providers`(0.141 仅支持 `responses` 线协议),
    /// 密钥经环境变量传入(不进 argv)。
    pub fn launch(cfg: &CodexLaunch) -> AppResult<Self> {
        let mut cmd = Command::new(&cfg.program);
        if let Some(base) = &cfg.base_url {
            // 与探测/chat 共用同一套智能 /v1 规整(用户只填 host 时自动补 /v1)。
            let base = super::normalize_openai_base(base)?;
            cmd.arg("-c")
                .arg("model_providers.aipanel.name=\"AiPanel\"");
            cmd.arg("-c")
                .arg(format!("model_providers.aipanel.base_url=\"{base}\""));
            cmd.arg("-c")
                .arg(format!("model_providers.aipanel.env_key=\"{KEY_ENV}\""));
            cmd.arg("-c")
                .arg("model_providers.aipanel.wire_api=\"responses\"");
            cmd.arg("-c")
                .arg("model_providers.aipanel.requires_openai_auth=false");
            cmd.arg("-c").arg("model_provider=\"aipanel\"");
        }
        if let Some(model) = &cfg.model {
            cmd.arg("-c").arg(format!("model=\"{model}\""));
        }
        if let Some(key) = &cfg.api_key {
            cmd.env(KEY_ENV, key);
        }
        if let Some(b) = &cfg.mcp {
            // 把 AiPanel 注册成 codex 的 MCP 服务器。路径/env 值按 TOML 字符串编码，
            // 避免空格、反斜杠或引号破坏 `-c key=value` 配置。
            cmd.arg("-c").arg(format!(
                "mcp_servers.aipanel.command={}",
                toml_string(&b.aipanel_exe)
            ));
            cmd.arg("-c")
                .arg("mcp_servers.aipanel.args=[\"mcp-server\"]");
            cmd.arg("-c").arg(format!(
                "mcp_servers.aipanel.env.AIPANEL_DATA_DIR={}",
                toml_string(&b.data_dir)
            ));
            if let Some(trace_path) = &b.trace_path {
                cmd.arg("-c").arg(format!(
                    "mcp_servers.aipanel.env.AIPANEL_TRACE_PATH={}",
                    toml_string(trace_path)
                ));
            }
            // 我方工具均为安全只读,自动批准,避免 codex 走审批(审批会被我们拒绝)。
            cmd.arg("-c")
                .arg("mcp_servers.aipanel.default_tools_approval_mode=\"auto\"");
        }
        let codex_home = isolated_codex_home();
        cmd.env("CODEX_HOME", &codex_home);
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&codex_home);
                AppError::Provider(format!("无法启动 codex-app-server（{}）: {e}", cfg.program))
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| {
                let _ = std::fs::remove_dir_all(&codex_home);
                AppError::Provider("codex app-server stdin 不可用".into())
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| {
                let _ = std::fs::remove_dir_all(&codex_home);
                AppError::Provider("codex app-server stdout 不可用".into())
            })?;
        let stderr = child.stderr.take();

        let stdout_tail = Arc::new(Mutex::new(Vec::new()));
        let stdout_tail_for_thread = Arc::clone(&stdout_tail);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        push_stdout_tail(&stdout_tail_for_thread, l.clone());
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        if let Some(stderr) = stderr {
            let stderr_tail_for_thread = Arc::clone(&stderr_tail);
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    push_stderr_tail(&stderr_tail_for_thread, line);
                }
            });
        }

        Ok(CodexClient {
            child,
            stdin,
            rx,
            next_id: 1,
            codex_home,
            stdout_tail,
            stderr_tail,
        })
    }

    fn provider_error_with_stderr(&self, message: &str) -> AppError {
        let stdout = stdout_tail(&self.stdout_tail);
        let stderr = stderr_tail(&self.stderr_tail);
        match (stdout, stderr) {
            (Some(stdout), Some(stderr)) => {
                AppError::Provider(format!("{message}; codex stdout: {stdout}; codex stderr: {stderr}"))
            }
            (Some(stdout), None) => {
                AppError::Provider(format!("{message}; codex stdout: {stdout}"))
            }
            (None, Some(stderr)) => {
                AppError::Provider(format!("{message}; codex stderr: {stderr}"))
            }
            (None, None) => AppError::Provider(message.to_string()),
        }
    }

    /// 发送一个请求，并（带超时地）等待匹配的响应。期间到达的通知会被跳过
    /// （`thread/start` 等握手阶段尚无事件流,安全）。
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
                return Err(self.provider_error_with_stderr("codex app-server 响应超时"));
            }
            match self.rx.recv_timeout(remaining) {
                Ok(line) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        if let Some(result) = parse_response(&v, id) {
                            return result;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(self.provider_error_with_stderr("codex app-server 响应超时"))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(self.provider_error_with_stderr("codex app-server 已退出"))
                }
            }
        }
    }

    /// JSON-RPC `initialize` 握手 + `initialized` 通知。声明 experimental API 以使用
    /// app-server 的(实验)thread/turn 方法。
    pub fn initialize(&mut self) -> AppResult<Value> {
        let result = self.request(
            "initialize",
            json!({
                "clientInfo": { "name": "AiPanel", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "experimentalApi": true },
            }),
            REQUEST_TIMEOUT,
        )?;
        // 通知服务端握手完成。
        write_line(&mut self.stdin, &json!({ "method": "initialized" }))?;
        Ok(result)
    }

    /// 跑完整的一个 turn:开 thread(只读沙箱)、发 `turn/start`、消费事件流。
    ///
    /// `turn/start` **不**走 [`request`](Self::request)(那会丢弃后续以通知形式到达的
    /// 事件)——写出后直接进入 [`drive_turn`]:工具调用经 `on_tool` 分发并回灌,最终返回
    /// agent 的回答文本。
    pub fn run_turn(
        &mut self,
        user_msg: &str,
        on_tool: impl FnMut(&str, &Value) -> AppResult<Value>,
        timeout: Duration,
    ) -> AppResult<String> {
        // 1) 开会话线程:只读沙箱 + on-request 审批(我们对审批一律拒绝)。
        let thread = self.request(
            "thread/start",
            json!({ "sandbox": "read-only", "approvalPolicy": "on-request" }),
            REQUEST_TIMEOUT,
        )?;
        let thread_id = thread
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Provider("codex thread/start 未返回 thread.id".into()))?
            .to_string();

        // 2) 发起 turn(写出即可,响应/事件随后以通知形式到来)。
        let id = self.next_id;
        self.next_id += 1;
        let turn = json!({
            "id": id, "method": "turn/start",
            "params": {
                "threadId": thread_id,
                "input": [text_input(user_msg)],
            },
        });
        write_line(&mut self.stdin, &turn)?;

        // 3) 事件回路。分别借用 stdin / rx(不同字段,借用不冲突)。
        let stdin = &mut self.stdin;
        let rx = &self.rx;
        drive_turn(
            |v| write_line(stdin, &v),
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
        let _ = std::fs::remove_dir_all(&self.codex_home);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_line_matches_app_server_protocol() {
        let line = build_request(7, "initialize", json!({"a": 1}));
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert!(v.get("jsonrpc").is_none());
        assert_eq!(v["id"], 7);
        assert_eq!(v["method"], "initialize");
        assert_eq!(v["params"]["a"], 1);
    }

    #[test]
    fn text_input_matches_v2_schema() {
        let input = text_input("hi");
        assert_eq!(input["type"], "text");
        assert_eq!(input["text"], "hi");
        assert!(input["text_elements"].as_array().unwrap().is_empty());
    }

    #[test]
    fn toml_string_escapes_paths_for_cli_config() {
        assert_eq!(toml_string("/tmp/Ai Panel"), "\"/tmp/Ai Panel\"");
        assert_eq!(toml_string("/tmp/a'b\"c"), "\"/tmp/a'b\\\"c\"");
    }

    #[test]
    fn stderr_tail_is_bounded_and_sanitized() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        for i in 0..25 {
            push_stderr_tail(&buf, format!("line {i} Bearer abcdefgh ip=10.0.0.{i}"));
        }
        let tail = stderr_tail(&buf).unwrap();
        assert!(!tail.contains("line 0"));
        assert!(tail.contains("line 24"));
        assert!(!tail.contains("Bearer abcdefgh"));
        assert!(tail.contains("Bearer [redacted]"));
        assert!(tail.contains("[redacted-ip]"));
    }

    #[test]
    fn stdout_tail_is_bounded_and_sanitized() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        for i in 0..25 {
            push_stdout_tail(&buf, format!("out {i} Bearer abcdefgh ip=10.0.0.{i}"));
        }
        let tail = stdout_tail(&buf).unwrap();
        assert!(!tail.contains("out 0"));
        assert!(tail.contains("out 24"));
        assert!(!tail.contains("Bearer abcdefgh"));
        assert!(tail.contains("Bearer [redacted]"));
        assert!(tail.contains("[redacted-ip]"));
    }

    #[test]
    fn invalid_launch_config_does_not_create_codex_home() {
        let result = CodexClient::launch(&CodexLaunch {
            program: "/definitely/missing/codex-app-server".into(),
            base_url: Some("notaurl".into()),
            api_key: None,
            model: None,
            mcp: None,
        });
        let err = match result {
            Ok(_) => panic!("launch unexpectedly succeeded"),
            Err(err) => err,
        };
        assert_eq!(err.code(), "provider");
    }

    #[test]
    #[ignore = "requires the bundled codex-app-server sidecar"]
    fn bundled_sidecar_initializes_with_real_protocol() {
        let program = resolve_codex_bin(None);
        assert!(
            std::path::Path::new(&program).is_file(),
            "missing sidecar: {program}"
        );
        let mut client = CodexClient::launch(&CodexLaunch {
            program,
            base_url: None,
            api_key: None,
            model: None,
            mcp: None,
        })
        .unwrap();
        let result = client
            .initialize()
            .unwrap_or_else(|e| panic!("bundled sidecar initialize failed: {e}"));
        assert!(result["userAgent"]
            .as_str()
            .unwrap_or_default()
            .contains("Codex"));
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

    // ---- turn / 工具回路(对真实形态的模拟 JSON-RPC 事件流单测)----

    /// 把一串预设消息做成 `recv` 闭包;耗尽后返回 Closed。
    fn scripted(events: Vec<Incoming>) -> impl FnMut(Duration) -> Incoming {
        let mut it = events.into_iter();
        move |_dur| it.next().unwrap_or(Incoming::Closed)
    }

    #[test]
    fn classify_recognizes_real_protocol_shapes() {
        assert!(matches!(
            classify(
                &json!({"id": 5, "method": "item/tool/call", "params": {"tool": "server.list", "callId": "c1", "arguments": {}}})
            ),
            Msg::ToolCall { .. }
        ));
        assert!(matches!(
            classify(
                &json!({"id": 6, "method": "item/commandExecution/requestApproval", "params": {}})
            ),
            Msg::Approval { .. }
        ));
        assert!(matches!(
            classify(&json!({"id": 7, "method": "some/unknown/request", "params": {}})),
            Msg::UnknownRequest { .. }
        ));
        assert_eq!(
            classify(&json!({"method": "item/agentMessage/delta", "params": {"delta": "hi"}})),
            Msg::Text("hi".into())
        );
        assert_eq!(
            classify(&json!({"method": "turn/completed", "params": {}})),
            Msg::Complete(None)
        );
        assert!(matches!(
            classify(&json!({"method": "error", "params": {"message": "boom"}})),
            Msg::Error(_)
        ));
        assert_eq!(
            classify(
                &json!({"method": "error", "params": {"willRetry": true, "error": {"message": "Reconnecting"}}})
            ),
            Msg::Other
        );
        assert_eq!(
            classify(
                &json!({"method": "error", "params": {"error": {"message": "Unauthorized", "additionalDetails": "missing key"}}})
            ),
            Msg::Error(
                "模型供应商认证失败：请在设置里检查该供应商的 API Key、Base URL 与模型是否匹配。"
                    .into()
            )
        );
        assert_eq!(
            classify(&json!({"method": "thread/started", "params": {}})),
            Msg::Other
        );
    }

    #[test]
    fn classify_extracts_v2_completed_agent_message() {
        let msg = json!({
            "method": "turn/completed",
            "params": {
                "turn": {
                    "status": {"type": "completed"},
                    "items": [
                        {"type": "userMessage", "content": []},
                        {"type": "agentMessage", "text": "最终结论", "phase": null, "memoryCitation": null}
                    ]
                }
            }
        });
        assert_eq!(classify(&msg), Msg::Complete(Some("最终结论".into())));
    }

    #[test]
    fn classify_extracts_raw_response_output_text() {
        let msg = json!({
            "method": "rawResponseItem/completed",
            "params": {
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "A"},
                        {"type": "output_text", "text": "B"}
                    ]
                }
            }
        });
        assert_eq!(classify(&msg), Msg::Text("AB".into()));
    }

    #[test]
    fn classify_turn_completed_failed_as_error() {
        let msg = json!({
            "method": "turn/completed",
            "params": {
                "turn": {
                    "status": {"type": "failed"},
                    "error": {"message": "unauthorized"}
                }
            }
        });
        assert_eq!(
            classify(&msg),
            Msg::Error("codex turn 失败: unauthorized".into())
        );
    }

    #[test]
    fn provider_errors_are_actionable_and_sanitized() {
        assert!(user_facing_provider_error("401 Unauthorized").contains("模型供应商认证失败"));
        assert!(user_facing_provider_error("missing API key").contains("模型供应商缺少 API Key"));
        let sanitized = user_facing_provider_error("connect to 10.0.0.4 failed");
        assert!(sanitized.contains("[redacted-ip]"));
    }

    #[test]
    fn drive_turn_dispatches_tool_then_completes() {
        let events = vec![
            Incoming::Line(
                json!({"id": 11, "method": "item/tool/call", "params": {"tool": "server.list", "callId": "c1", "arguments": {}}}),
            ),
            Incoming::Line(
                json!({"method": "item/agentMessage/delta", "params": {"delta": "已检查"}}),
            ),
            Incoming::Line(json!({"method": "turn/completed", "params": {}})),
        ];
        let mut sent: Vec<Value> = vec![];
        let mut tools: Vec<String> = vec![];
        let out = drive_turn(
            |v| {
                sent.push(v);
                Ok(())
            },
            scripted(events),
            |name, _args| {
                tools.push(name.to_string());
                Ok(json!({ "ok": true }))
            },
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out, "已检查");
        assert_eq!(tools, vec!["server.list".to_string()]);
        // 回了一条 JSON-RPC response:id 对上、success=true、含 contentItems。
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["id"], 11);
        assert_eq!(sent[0]["result"]["success"], true);
        assert!(sent[0]["result"]["contentItems"][0]["text"]
            .as_str()
            .unwrap()
            .contains("ok"));
    }

    #[test]
    fn drive_turn_denies_native_approval() {
        let events = vec![
            Incoming::Line(
                json!({"id": 22, "method": "execCommandApproval", "params": {"command": "rm -rf /"}}),
            ),
            Incoming::Line(json!({"method": "item/agentMessage/delta", "params": {"delta": "ok"}})),
            Incoming::Line(json!({"method": "turn/completed", "params": {}})),
        ];
        let mut sent: Vec<Value> = vec![];
        let out = drive_turn(
            |v| {
                sent.push(v);
                Ok(())
            },
            scripted(events),
            |_, _| Ok(json!(null)),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out, "ok");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["id"], 22);
        assert_eq!(sent[0]["result"]["decision"], "denied"); // 原生 shell 审批被硬拒绝
    }

    #[test]
    fn drive_turn_answers_unknown_request_with_error() {
        let events = vec![
            Incoming::Line(json!({"id": 33, "method": "some/unknown", "params": {}})),
            Incoming::Line(
                json!({"method": "turn/completed", "params": {"turn": {"items": [{"text": "done"}]}}}),
            ),
        ];
        let mut sent: Vec<Value> = vec![];
        let out = drive_turn(
            |v| {
                sent.push(v);
                Ok(())
            },
            scripted(events),
            |_, _| Ok(json!(null)),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out, "done"); // 没有增量时用 turn.items 文本兜底
        assert_eq!(sent[0]["id"], 33);
        assert!(sent[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("some/unknown"));
    }

    #[test]
    fn drive_turn_accumulates_text_deltas() {
        let events = vec![
            Incoming::Line(
                json!({"method": "item/agentMessage/delta", "params": {"delta": "foo"}}),
            ),
            Incoming::Line(
                json!({"method": "item/agentMessage/delta", "params": {"delta": "bar"}}),
            ),
            Incoming::Line(json!({"method": "turn/completed", "params": {}})),
        ];
        let out = drive_turn(
            |_| Ok(()),
            scripted(events),
            |_, _| Ok(json!(null)),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out, "foobar");
    }

    #[test]
    fn drive_turn_surfaces_error_and_disconnect() {
        let e1 = drive_turn(
            |_| Ok(()),
            scripted(vec![Incoming::Line(
                json!({"method": "error", "params": {"message": "boom"}}),
            )]),
            |_, _| Ok(json!(null)),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(e1.code(), "provider");
        assert!(e1.to_string().contains("boom"));
        let e2 = drive_turn(
            |_| Ok(()),
            scripted(vec![Incoming::Closed]),
            |_, _| Ok(json!(null)),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(e2.code(), "provider");
        let e3 = drive_turn(
            |_| Ok(()),
            scripted(vec![Incoming::Timeout]),
            |_, _| Ok(json!(null)),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(e3.code(), "provider");
    }
}
