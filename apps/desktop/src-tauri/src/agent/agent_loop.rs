//! Autonomous agent turn: the model investigates via AiPanel Tools, then answers.
//!
//! The agent is offered ONLY the **read-only** tools (server.list/info,
//! server.doctor.readonly, ssh.run_readonly, task.plan, task.review). It can call
//! them to gather facts and then summarize — but it can NEVER change a server
//! this way: write/execute tools are not exposed to the autonomous loop, so any
//! mutation still goes through the explicit plan → user-confirm → execute path
//! (docs/SECURITY_MODEL.zh-Hans.md). Tool results are already sanitized + audited
//! by `tools::dispatch`.
//!
//! Uses the async reqwest client (the loop awaits async tool dispatch), separate
//! from the provider's blocking one-shot calls.

use serde::Serialize;
use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::core::types::ProviderConfig;
use crate::tools::{registry, ToolPermission};
use crate::AppState;

const MAX_ITERS: usize = 6;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTrace {
    pub name: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnResult {
    pub summary: String,
    pub tool_calls: Vec<ToolTrace>,
}

/// OpenAI `tools` array for the read-only AiPanel Tools.
pub fn read_only_tool_specs() -> Vec<Value> {
    registry()
        .into_iter()
        .filter(|t| matches!(t.permission, ToolPermission::ReadOnly))
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": params_schema(t.name),
                }
            })
        })
        .collect()
}

fn params_schema(name: &str) -> Value {
    match name {
        "server.info" => json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
        "server.doctor.readonly" => json!({"type":"object","properties":{"serverId":{"type":"string"}},"required":["serverId"]}),
        "ssh.run_readonly" => json!({"type":"object","properties":{"serverId":{"type":"string"},"command":{"type":"string"}},"required":["serverId","command"]}),
        "task.plan" => json!({"type":"object","properties":{"intent":{"type":"string"},"serverId":{"type":"string"}},"required":["intent"]}),
        "task.review" => json!({"type":"object","properties":{"plan":{"type":"object"},"readOnlyMode":{"type":"boolean"}},"required":["plan"]}),
        // server.list
        _ => json!({"type":"object","properties":{}}),
    }
}

fn base_url(provider: &ProviderConfig) -> AppResult<String> {
    match &provider.base_url {
        Some(u) if u.starts_with("http") => Ok(u.trim_end_matches('/').to_string()),
        _ => Err(AppError::Provider("base_url 缺失或不是 http(s) URL".into())),
    }
}

async fn chat(
    client: &reqwest::Client,
    base: &str,
    api_key: &Option<String>,
    body: &Value,
) -> AppResult<Value> {
    let mut req = client.post(format!("{base}/chat/completions")).json(body);
    if let Some(k) = api_key {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await.map_err(|e| AppError::Provider(format!("请求失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| AppError::Provider(e.to_string()))?;
    if !status.is_success() {
        return Err(AppError::Provider(format!("HTTP {status}: {}", crate::core::sanitize::sanitize(&text))));
    }
    serde_json::from_str(&text).map_err(|e| AppError::Provider(format!("响应解析失败: {e}")))
}

/// Run one autonomous, read-only investigation turn.
pub async fn run_turn(
    state: &AppState,
    provider: &ProviderConfig,
    api_key: Option<String>,
    intent: &str,
    server_id: Option<&str>,
) -> AppResult<AgentTurnResult> {
    let base = base_url(provider)?;
    let model = provider.model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Provider(e.to_string()))?;

    let system = format!(
        "你是 AiPanel 的 Linux 运维助手。你只能调用提供的【只读】工具来收集事实(系统信息、端口、服务、日志等),\
然后用简体中文给出诊断结论和建议。绝不假设、不要编造命令输出。如果需要修改服务器,只在结论里描述建议的操作,\
由用户在确认流程里执行——你自己不能执行任何写操作。当前目标服务器 serverId={}。",
        server_id.unwrap_or("(未指定)")
    );

    let tools = read_only_tool_specs();
    let mut messages = vec![
        json!({"role":"system","content": system}),
        json!({"role":"user","content": intent}),
    ];
    let mut trace: Vec<ToolTrace> = Vec::new();

    for _ in 0..MAX_ITERS {
        let body = json!({
            "model": model,
            "temperature": 0.2,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
        });
        let resp = chat(&client, &base, &api_key, &body).await?;
        let msg = &resp["choices"][0]["message"];
        let tool_calls = msg.get("tool_calls").and_then(|v| v.as_array()).cloned();

        match tool_calls {
            Some(calls) if !calls.is_empty() => {
                // Echo the assistant message (with tool_calls) back into context.
                messages.push(msg.clone());
                for call in calls {
                    let id = call["id"].as_str().unwrap_or_default().to_string();
                    let name = call["function"]["name"].as_str().unwrap_or_default().to_string();
                    let args: Value = call["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_else(|| json!({}));

                    // Only read-only tools are offered, but enforce again here.
                    let is_read_only = read_only_tool_specs()
                        .iter()
                        .any(|t| t["function"]["name"] == name);
                    let (content, ok) = if !is_read_only {
                        ("该工具需用户确认,自动诊断回路不执行写操作。".to_string(), false)
                    } else {
                        match crate::tools::dispatch(state, &name, args).await {
                            Ok(v) => (truncate(&v.to_string(), 4000), true),
                            Err(e) => (format!("工具错误: {}", e), false),
                        }
                    };
                    trace.push(ToolTrace { name, ok });
                    messages.push(json!({"role":"tool","tool_call_id": id,"content": content}));
                }
                // loop again so the model can use the results
            }
            _ => {
                let summary = msg["content"].as_str().unwrap_or("").to_string();
                return Ok(AgentTurnResult { summary, tool_calls: trace });
            }
        }
    }
    Err(AppError::Provider("达到最大工具调用轮数仍未得出结论".into()))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…(已截断)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_read_only_tools_are_offered() {
        let specs = read_only_tool_specs();
        let names: Vec<String> = specs
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"server.list".to_string()));
        assert!(names.contains(&"ssh.run_readonly".to_string()));
        // write/execute tools must NOT be exposed to the autonomous loop
        assert!(!names.contains(&"task.execute_confirmed".to_string()));
        assert!(!names.contains(&"audit.write".to_string()));
    }

    #[test]
    fn schemas_require_expected_args() {
        let s = params_schema("ssh.run_readonly");
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "serverId"));
        assert!(req.iter().any(|v| v == "command"));
    }

    #[test]
    fn truncate_caps_length() {
        let long = "x".repeat(5000);
        assert!(truncate(&long, 100).chars().count() <= 110);
    }
}
