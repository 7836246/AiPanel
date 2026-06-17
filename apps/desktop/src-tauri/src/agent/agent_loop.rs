//! 自主 agent 回合：模型通过 AiPanel Tools 自行调查，然后给出回答。
//!
//! 只向 agent 提供**只读**工具（server.list/info、server.doctor.readonly、
//! ssh.run_readonly、task.plan、task.review）。它能调用这些工具收集事实再做总结
//! ——但绝不能借此修改服务器：写/执行类工具不会暴露给自主回路，因此任何变更
//! 仍走显式的 计划 → 用户确认 → 执行 链路（见 docs/SECURITY_MODEL.zh-Hans.md）。
//! 工具结果已由 `tools::dispatch` 完成脱敏 + 审计。
//!
//! 使用异步 reqwest 客户端（回路需 await 异步的工具分发），与 provider 那套
//! 阻塞式的一次性调用相互独立。

use serde::Serialize;
use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::core::types::ProviderConfig;
use crate::tools::{registry, ToolPermission};
use crate::AppState;

const MAX_ITERS: usize = 6;

/// 单次工具调用的轨迹（工具名 + 是否成功），用于回放本回合调用了什么。
///
/// 额外携带脱敏后的入参摘要 / 错误 / 结果预览，方便前端展示更丰富的调用细节。
/// 所有文本均经 `crate::core::sanitize::sanitize` 脱敏后才放入，绝不泄露密钥。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTrace {
    pub name: String,
    pub ok: bool,
    /// 本次工具调用入参的简短摘要（截断到 ~120 字符的脱敏 JSON）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_summary: Option<String>,
    /// 失败时的脱敏错误信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 成功结果的前 ~200 字符脱敏预览。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
}

/// 一次自主回合的结果：最终总结，以及途中调用过的工具轨迹。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnResult {
    pub summary: String,
    pub tool_calls: Vec<ToolTrace>,
}

/// 把只读的 AiPanel Tools 转成 OpenAI 的 `tools` 数组。
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
        // server.list（无参数）
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

/// 执行一次自主的、只读的调查回合。
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
                // 把带 tool_calls 的 assistant 消息回放进上下文。
                messages.push(msg.clone());
                for call in calls {
                    let id = call["id"].as_str().unwrap_or_default().to_string();
                    let name = call["function"]["name"].as_str().unwrap_or_default().to_string();
                    let args: Value = call["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_else(|| json!({}));

                    // 入参摘要：脱敏后截断到 ~120 字符，供轨迹回放展示。
                    let args_summary = Some(truncate(
                        &crate::core::sanitize::sanitize(&args.to_string()),
                        120,
                    ));

                    // 虽然只提供了只读工具，这里再强制校验一次。
                    let is_read_only = read_only_tool_specs()
                        .iter()
                        .any(|t| t["function"]["name"] == name);
                    // content 回灌给模型；result_preview / error 仅用于轨迹展示（均已脱敏）。
                    let (content, ok, error, result_preview) = if !is_read_only {
                        let msg = "该工具需用户确认,自动诊断回路不执行写操作。".to_string();
                        (msg.clone(), false, Some(msg), None)
                    } else {
                        match crate::tools::dispatch(state, &name, args).await {
                            // 工具结果回灌给模型前先脱敏
                            // （在 server.list / server.info / 输出中抹掉 IP / 密钥）。
                            Ok(v) => {
                                let sanitized = crate::core::sanitize::sanitize(&v.to_string());
                                let preview = truncate(&sanitized, 200);
                                (truncate(&sanitized, 4000), true, None, Some(preview))
                            }
                            Err(e) => {
                                let err = crate::core::sanitize::sanitize(&e.to_string());
                                (format!("工具错误: {}", err), false, Some(err), None)
                            }
                        }
                    };
                    trace.push(ToolTrace {
                        name,
                        ok,
                        args_summary,
                        error,
                        result_preview,
                    });
                    messages.push(json!({"role":"tool","tool_call_id": id,"content": content}));
                }
                // 再循环一轮，让模型用上这些结果
            }
            _ => {
                let summary = msg["content"].as_str().unwrap_or("").to_string();
                // 空总结时前端会退化为空白：返回明确的 Provider 错误让前端 catch 展示。
                if summary.trim().is_empty() {
                    return Err(AppError::Provider("诊断未返回任何结论".into()));
                }
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
        // 写/执行类工具绝不能暴露给自主回路
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
