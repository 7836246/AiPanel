//! AiPanel MCP 服务器(`aipanel mcp-server`)——给 Codex 一个**标准 MCP 工具面**。
//!
//! Codex 官方的外部工具机制是 MCP。AiPanel 以本模块作为一个 stdio MCP 服务器,把
//! **只读** server-ops 工具(server.list/info、server.doctor.readonly、ssh.run_readonly、
//! task.plan/review)暴露给 codex;codex 在一次 turn 里调这些工具收集事实、做诊断。
//!
//! 安全(见 docs/SECURITY_MODEL.zh-Hans.md):
//! - **只暴露只读工具**;写/执行类(task.execute_confirmed、audit.write)绝不列出,即便被
//!   调用也拒绝——服务器变更仍只能走「计划 → 用户确认 → 执行」链路。
//! - 工具结果离开本进程前经 `sanitize` 脱敏。
//! - 本进程用独立的数据目录(`AIPANEL_DATA_DIR`)复用同一份 SQLite/Keychain(跨进程),
//!   由 codex 按注入的 `mcp_servers` 配置拉起;它不读用户 `~/.codex`。

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::tools::{registry, ToolPermission};
use crate::AppState;

/// MCP 协议版本(与 codex 客户端协商用)。
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// 某工具是否为只读(决定是否对自动回路开放)。
fn is_read_only(name: &str) -> bool {
    registry()
        .iter()
        .any(|t| t.name == name && matches!(t.permission, ToolPermission::ReadOnly))
}

/// 把某工具的入参约束表达成 JSON Schema(供 MCP `tools/list`)。
fn input_schema(name: &str) -> Value {
    match name {
        "server.info" => {
            json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]})
        }
        "server.doctor.readonly" => {
            json!({"type":"object","properties":{"serverId":{"type":"string"}},"required":["serverId"]})
        }
        "ssh.run_readonly" => {
            json!({"type":"object","properties":{"serverId":{"type":"string"},"command":{"type":"string"}},"required":["serverId","command"]})
        }
        "task.plan" => {
            json!({"type":"object","properties":{"intent":{"type":"string"},"serverId":{"type":"string"}},"required":["intent"]})
        }
        "task.review" => {
            json!({"type":"object","properties":{"plan":{"type":"object"},"readOnlyMode":{"type":"boolean"}},"required":["plan"]})
        }
        // server.list(无参数)
        _ => json!({"type":"object","properties":{}}),
    }
}

/// 暴露给 codex 的 MCP 工具清单(**仅只读**)。
pub fn mcp_tool_specs() -> Vec<Value> {
    registry()
        .into_iter()
        .filter(|t| matches!(t.permission, ToolPermission::ReadOnly))
        .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": input_schema(t.name) }))
        .collect()
}

fn ok_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "...(truncated)"
    }
}

fn trace_item(name: &str, args: &Value, ok: bool, text: &str) -> Value {
    let args_summary = truncate(&crate::core::sanitize::sanitize(&args.to_string()), 120);
    let sanitized_text = crate::core::sanitize::sanitize(text);
    if ok {
        json!({
            "name": name,
            "ok": true,
            "argsSummary": args_summary,
            "resultPreview": truncate(&sanitized_text, 200),
        })
    } else {
        json!({
            "name": name,
            "ok": false,
            "argsSummary": args_summary,
            "error": truncate(&sanitized_text, 200),
        })
    }
}

fn append_trace(name: &str, args: &Value, ok: bool, text: &str) {
    let Ok(path) = std::env::var("AIPANEL_TRACE_PATH") else {
        return;
    };
    let item = trace_item(name, args, ok, text);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{item}");
    }
}

/// 处理一条 MCP JSON-RPC 消息。请求返回响应;通知(无 id / 非处理方法)返回 None。
pub async fn handle_message(state: &AppState, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str())?;
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => Some(ok_result(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "aipanel", "version": env!("CARGO_PKG_VERSION") },
            }),
        )),
        "tools/list" => Some(ok_result(id, json!({ "tools": mcp_tool_specs() }))),
        "tools/call" => {
            let params = msg.get("params");
            let name = params
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            // 只放行只读工具——写/执行类绝不在此执行。
            if !is_read_only(name) {
                let text = format!("工具 `{name}` 不对自动回路开放(写操作需用户确认)");
                append_trace(name, &args, false, &text);
                return Some(ok_result(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": true,
                    }),
                ));
            }
            let (text, is_error) = match crate::tools::dispatch(state, name, args.clone()).await {
                Ok(v) => (crate::core::sanitize::sanitize(&v.to_string()), false),
                Err(e) => (crate::core::sanitize::sanitize(&e.to_string()), true),
            };
            append_trace(name, &args, !is_error, &text);
            Some(ok_result(
                id,
                json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
            ))
        }
        // 未知方法:带 id 的是**请求**,须回 method-not-found 错误,否则对端会一直等待(挂起);
        // 无 id 的是通知(如 notifications/initialized),按 JSON-RPC 不回。
        _ => {
            if msg.get("id").is_some() {
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("method not found: {method}") },
                }))
            } else {
                None
            }
        }
    }
}

/// 以 stdio 跑 MCP 服务器循环。复用 `AIPANEL_DATA_DIR` 指向的同一份 SQLite/Keychain
/// (由 codex 按注入的 mcp_servers 配置在启动时设好)。阻塞式读取 stdin,逐条用一个
/// 本地 tokio 运行时 `block_on` 异步分发,再把响应写回 stdout。
pub fn serve() -> AppResult<()> {
    use std::io::{BufRead, Write};

    let data_dir = std::env::var("AIPANEL_DATA_DIR")
        .map_err(|_| AppError::Provider("mcp-server 需要 AIPANEL_DATA_DIR 环境变量".into()))?;
    std::fs::create_dir_all(&data_dir).ok();
    let store =
        crate::store::Store::open(&std::path::Path::new(&data_dir).join("aipanel.sqlite3"))?;
    let state = AppState {
        store,
        credentials: crate::credentials::default_credential_store(),
        plan_engine: Box::new(crate::plan::MockPlanEngine),
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AppError::Provider(e.to_string()))?;

    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // 跳过非法行
        };
        if let Some(resp) = rt.block_on(handle_message(&state, &msg)) {
            let mut s = resp.to_string();
            s.push('\n');
            if out
                .write_all(s.as_bytes())
                .and_then(|_| out.flush())
                .is_err()
            {
                break; // 对端关闭
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_state() -> AppState {
        AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        }
    }

    #[test]
    fn tools_list_exposes_only_read_only() {
        let names: Vec<String> = mcp_tool_specs()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"server.list".to_string()));
        assert!(names.contains(&"ssh.run_readonly".to_string()));
        assert!(names.contains(&"task.plan".to_string()));
        // 写/执行类绝不暴露
        assert!(!names.contains(&"task.execute_confirmed".to_string()));
        assert!(!names.contains(&"audit.write".to_string()));
    }

    #[tokio::test]
    async fn initialize_reports_server_info() {
        let st = mem_state();
        let resp = handle_message(
            &st,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .await
        .unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["serverInfo"]["name"], "aipanel");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn tools_list_via_message() {
        let st = mem_state();
        let resp = handle_message(&st, &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .await
            .unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "server.list"));
        assert!(tools.iter().all(|t| t["name"] != "task.execute_confirmed"));
    }

    #[tokio::test]
    async fn tools_call_read_only_dispatches() {
        let st = mem_state();
        let resp = handle_message(
            &st,
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"server.list","arguments":{}}}),
        )
        .await
        .unwrap();
        assert_eq!(resp["result"]["isError"], false);
        // server.list 在空 store 上返回 "[]"(脱敏后不变)。
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with('['));
    }

    #[test]
    fn trace_item_matches_frontend_shape() {
        let trace = trace_item("server.list", &json!({}), true, "[]");
        assert_eq!(trace["name"], "server.list");
        assert_eq!(trace["ok"], true);
        assert!(trace["resultPreview"].as_str().unwrap().starts_with('['));
    }

    #[tokio::test]
    async fn tools_call_refuses_write_tool() {
        let st = mem_state();
        let resp = handle_message(
            &st,
            &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"task.execute_confirmed","arguments":{"confirmed":true}}}),
        )
        .await
        .unwrap();
        // 写工具不对自动回路开放:isError=true,且绝不真的执行。
        assert_eq!(resp["result"]["isError"], true);
    }

    #[tokio::test]
    async fn notifications_get_no_response() {
        let st = mem_state();
        assert!(handle_message(
            &st,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .await
        .is_none());
    }

    #[tokio::test]
    async fn unknown_request_method_returns_method_not_found() {
        // 带 id 的未知方法是**请求**:必须回 JSON-RPC -32601 错误,否则对端会一直等待挂起。
        let st = mem_state();
        let resp = handle_message(&st, &json!({"jsonrpc":"2.0","id":7,"method":"no/such/method"}))
            .await
            .expect("带 id 的未知请求必须返回响应");
        assert_eq!(resp["id"], 7);
        assert_eq!(resp["error"]["code"], -32601);
        assert!(resp.get("result").is_none(), "错误响应不应同时带 result");
    }
}
