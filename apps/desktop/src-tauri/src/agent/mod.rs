//! Agent Provider abstraction and the Codex app-server bridge.
//!
//! Codex (or any provider) owns conversation, understanding, planning, model and
//! context. AiPanel owns servers, SSH, permissions, execution, security and
//! audit. The hard boundary (CLAUDE.md, docs/SECURITY_MODEL.zh-Hans.md):
//!
//! - the provider NEVER holds SSH credentials and NEVER runs a raw shell;
//! - it reaches server capability only through AiPanel Tools (see `tools`),
//!   which are vetted, default-read-only, and audited.
//!
//! This module defines the provider trait and three implementations:
//! `MockAgentProvider` (offline, always available), `OpenAiCompatibleProvider`
//! (config + connectivity skeleton), and `CodexAppServerProvider` (the intended
//! runtime, launched as a JSON-RPC/stdio subprocess — entry point + health
//! check here; the full tool-call loop is wired with the tools layer).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, AppResult};
use crate::core::types::{Plan, ProviderConfig, ProviderKind, ProviderTestResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A streamed agent event (subset; expands as the bridge matures).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    Token { text: String },
    ToolCall { name: String, args: serde_json::Value },
    Done,
    Error { message: String },
}

/// What every agent runtime must provide. Kept synchronous for the MVP; the
/// Codex bridge runs its async subprocess work internally.
pub trait AgentProvider: Send + Sync {
    fn name(&self) -> &str;
    fn chat(&self, messages: &[ChatMessage]) -> AppResult<String>;
    fn plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan>;
    fn summarize(&self, context: &str) -> AppResult<String>;
    fn stream_events(&self, intent: &str) -> AppResult<Vec<AgentEvent>>;
    /// Validate config / reachability without committing to a full session.
    fn test(&self) -> ProviderTestResult;
}

/// Build a provider from its stored config plus its secret (API key), resolved
/// from the credential store by the caller. The key lives only in the provider
/// instance for the duration of the call.
pub fn build_provider(config: &ProviderConfig, api_key: Option<String>) -> Box<dyn AgentProvider> {
    match config.kind {
        ProviderKind::CodexAppServer => Box::new(CodexAppServerProvider {
            codex_path: config.codex_path.clone().unwrap_or_else(|| "codex".to_string()),
        }),
        ProviderKind::OpenAiCompatible => {
            Box::new(OpenAiCompatibleProvider { config: config.clone(), api_key })
        }
        ProviderKind::Custom => Box::new(MockAgentProvider),
    }
}

// ---------------------------------------------------------------------------
// Mock — offline, deterministic, always available.
// ---------------------------------------------------------------------------

pub struct MockAgentProvider;

impl AgentProvider for MockAgentProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn chat(&self, messages: &[ChatMessage]) -> AppResult<String> {
        Ok(format!("(mock) 收到 {} 条消息", messages.len()))
    }
    fn plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan> {
        use crate::plan::PlanEngine;
        crate::plan::MockPlanEngine.create_plan(intent, server_id)
    }
    fn summarize(&self, context: &str) -> AppResult<String> {
        Ok(format!("(mock) 摘要：{} 字", context.chars().count()))
    }
    fn stream_events(&self, intent: &str) -> AppResult<Vec<AgentEvent>> {
        Ok(vec![
            AgentEvent::Token { text: format!("正在分析：{intent}") },
            AgentEvent::Token { text: "生成只读诊断计划…".into() },
            AgentEvent::Done,
        ])
    }
    fn test(&self) -> ProviderTestResult {
        ProviderTestResult { ok: true, message: "Mock provider 可用".into(), detail: None }
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible — config + connectivity skeleton (no live request yet).
// ---------------------------------------------------------------------------

pub struct OpenAiCompatibleProvider {
    pub config: ProviderConfig,
    pub api_key: Option<String>,
}

#[derive(Deserialize)]
struct LlmPlan {
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    steps: Vec<LlmStep>,
}

#[derive(Deserialize)]
struct LlmStep {
    summary: String,
    command: String,
}

impl OpenAiCompatibleProvider {
    fn base(&self) -> AppResult<String> {
        match &self.config.base_url {
            Some(u) if u.starts_with("http://") || u.starts_with("https://") => {
                Ok(u.trim_end_matches('/').to_string())
            }
            _ => Err(AppError::Provider("base_url 缺失或不是 http(s) URL".into())),
        }
    }

    fn model(&self) -> String {
        self.config.model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string())
    }

    fn client() -> AppResult<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| AppError::Provider(e.to_string()))
    }

    /// POST /chat/completions and return the assistant message content.
    fn complete(&self, messages: &[ChatMessage], json_mode: bool) -> AppResult<String> {
        let base = self.base()?;
        let url = format!("{base}/chat/completions");
        let mut body = serde_json::json!({
            "model": self.model(),
            "temperature": 0.2,
            "messages": messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
        });
        if json_mode {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        let mut req = Self::client()?.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req.send().map_err(|e| AppError::Provider(format!("请求失败: {e}")))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| AppError::Provider(e.to_string()))?;
        if !status.is_success() {
            return Err(AppError::Provider(format!("HTTP {status}: {}", crate::core::sanitize::sanitize(&text))));
        }
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| AppError::Provider(format!("响应解析失败: {e}")))?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::Provider("响应缺少 message.content".into()))
    }
}

/// Strip ```json … ``` fences if the model wrapped its JSON.
fn unfence(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        return rest.trim().trim_end_matches("```").trim();
    }
    t
}

impl AgentProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        "openai-compatible"
    }

    fn chat(&self, messages: &[ChatMessage]) -> AppResult<String> {
        self.complete(messages, false)
    }

    fn plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan> {
        let system = "你是 Linux 服务器运维规划助手。把用户任务转成一个结构化的诊断/操作计划。\
默认只读优先：除非用户明确要求修改,只生成检查类命令,绝不包含 rm/格式化/改防火墙/改 SSH 配置等危险命令。\
只输出 JSON,不要解释,格式:{\"goal\":\"一句话目标\",\"steps\":[{\"summary\":\"这步做什么\",\"command\":\"可直接在服务器执行的命令\"}]}。";
        let messages = vec![
            ChatMessage { role: "system".into(), content: system.into() },
            ChatMessage { role: "user".into(), content: intent.to_string() },
        ];
        let content = self.complete(&messages, true)?;
        let parsed: LlmPlan = serde_json::from_str(unfence(&content))
            .map_err(|e| AppError::Provider(format!("计划 JSON 解析失败: {e}")))?;
        if parsed.steps.is_empty() {
            return Err(AppError::Provider("模型未返回任何步骤".into()));
        }
        // AiPanel assigns risk — never trust the model's own assessment.
        let steps = parsed
            .steps
            .into_iter()
            .map(|s| {
                let risk = crate::risk::classify_command(&s.command).level;
                make_step(s.summary, s.command, risk)
            })
            .collect();
        Ok(Plan {
            id: crate::core::types::new_id(),
            server_id: server_id.map(|s| s.to_string()),
            goal: parsed.goal.unwrap_or_else(|| format!("诊断：{intent}")),
            steps,
            created_at: crate::core::types::now(),
        })
    }

    fn summarize(&self, context: &str) -> AppResult<String> {
        let messages = vec![
            ChatMessage { role: "system".into(), content: "用简体中文简要总结以下运维执行结果,指出关键发现与下一步建议。".into() },
            ChatMessage { role: "user".into(), content: context.to_string() },
        ];
        self.complete(&messages, false)
    }

    fn stream_events(&self, intent: &str) -> AppResult<Vec<AgentEvent>> {
        // Non-streaming fallback: one shot wrapped as events.
        let reply = self.chat(&[ChatMessage { role: "user".into(), content: intent.to_string() }])?;
        Ok(vec![AgentEvent::Token { text: reply }, AgentEvent::Done])
    }

    fn test(&self) -> ProviderTestResult {
        let base = match self.base() {
            Ok(b) => b,
            Err(e) => return ProviderTestResult { ok: false, message: e.to_string(), detail: None },
        };
        let client = match Self::client() {
            Ok(c) => c,
            Err(e) => return ProviderTestResult { ok: false, message: e.to_string(), detail: None },
        };
        let mut req = client.get(format!("{base}/models"));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        match req.send() {
            Ok(resp) if resp.status().is_success() => {
                ProviderTestResult { ok: true, message: "连接成功".into(), detail: Some(format!("{base}/models")) }
            }
            Ok(resp) => ProviderTestResult {
                ok: false,
                message: format!("HTTP {}", resp.status()),
                detail: Some("检查 base_url / API Key".into()),
            },
            Err(e) => ProviderTestResult { ok: false, message: format!("请求失败: {e}"), detail: None },
        }
    }
}

fn make_step(summary: String, command: String, risk: crate::core::types::RiskLevel) -> crate::core::types::PlanStep {
    crate::core::types::PlanStep {
        summary,
        command,
        read_only: risk == crate::core::types::RiskLevel::Low,
        risk,
        tool: None,
    }
}

// ---------------------------------------------------------------------------
// Codex app-server — the intended runtime. Launched as a JSON-RPC/stdio
// subprocess. This is the entry point + health check; the full tool-call loop
// is wired alongside the AiPanel Tools layer.
// ---------------------------------------------------------------------------

pub struct CodexAppServerProvider {
    pub codex_path: String,
}

impl CodexAppServerProvider {
    /// Run `<codex> --version` to confirm the binary is present and runnable.
    fn version(&self) -> AppResult<String> {
        let output = std::process::Command::new(&self.codex_path)
            .arg("--version")
            .output()
            .map_err(|e| AppError::Provider(format!("无法启动 codex（{}）: {e}", self.codex_path)))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(AppError::Provider(format!(
                "codex --version 退出码 {}",
                output.status.code().unwrap_or(-1)
            )))
        }
    }

    /// Bridge plan/chat go through the app-server's JSON-RPC over stdio:
    ///   1. spawn `<codex> app-server` with piped stdin/stdout;
    ///   2. send `initialize`, advertising ONLY AiPanel Tools as the tool set;
    ///   3. `thread/start`, then `turn/start` with the user intent;
    ///   4. stream events; tool calls are dispatched to `tools` (read-only by
    ///      default, writes require confirmation) and the results fed back.
    /// Not yet wired — returns a clear, actionable error meanwhile.
    fn not_wired<T>(&self) -> AppResult<T> {
        Err(AppError::Provider(
            "Codex app-server 桥接尚未接通（JSON-RPC 工具回路开发中）；当前可用 mock provider 生成只读计划".into(),
        ))
    }
}

impl AgentProvider for CodexAppServerProvider {
    fn name(&self) -> &str {
        "codex-app-server"
    }
    fn chat(&self, _messages: &[ChatMessage]) -> AppResult<String> {
        self.not_wired()
    }
    fn plan(&self, _intent: &str, _server_id: Option<&str>) -> AppResult<Plan> {
        self.not_wired()
    }
    fn summarize(&self, _context: &str) -> AppResult<String> {
        self.not_wired()
    }
    fn stream_events(&self, _intent: &str) -> AppResult<Vec<AgentEvent>> {
        self.not_wired()
    }
    fn test(&self) -> ProviderTestResult {
        match self.version() {
            Ok(v) => ProviderTestResult {
                ok: true,
                message: format!("找到 codex：{v}"),
                detail: Some("app-server 工具回路尚在开发中".into()),
            },
            Err(e) => ProviderTestResult { ok: false, message: e.to_string(), detail: None },
        }
    }
}

/// Test a provider config (with its API key) without persisting it.
pub fn test_provider(config: &ProviderConfig, api_key: Option<String>) -> ProviderTestResult {
    build_provider(config, api_key).test()
}

/// Resolve the configured provider for planning: the policy's default if set,
/// else the first enabled provider. Returns None to signal "fall back to mock".
pub fn plan_with_provider(
    config: &ProviderConfig,
    api_key: Option<String>,
    intent: &str,
    server_id: Option<&str>,
) -> AppResult<Plan> {
    build_provider(config, api_key).plan(intent, server_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{new_id, now};

    fn cfg(kind: ProviderKind, base_url: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            id: new_id(),
            name: "p".into(),
            kind,
            base_url: base_url.map(|s| s.to_string()),
            model: None,
            codex_path: None,
            credential_ref: None,
            enabled: true,
            created_at: now(),
            updated_at: now(),
        }
    }

    #[test]
    fn mock_plans_and_tests_ok() {
        let p = MockAgentProvider;
        assert!(p.test().ok);
        let plan = p.plan("网站打不开", Some("s")).unwrap();
        assert!(!plan.steps.is_empty());
        assert!(p.stream_events("x").unwrap().iter().any(|e| matches!(e, AgentEvent::Done)));
    }

    #[test]
    fn openai_test_rejects_missing_base_url() {
        // No base_url → fails before any network call.
        assert!(!test_provider(&cfg(ProviderKind::OpenAiCompatible, None), None).ok);
    }

    #[test]
    fn openai_plan_requires_base_url() {
        let p = OpenAiCompatibleProvider {
            config: cfg(ProviderKind::OpenAiCompatible, None),
            api_key: None,
        };
        assert_eq!(p.plan("检查磁盘", None).unwrap_err().code(), "provider");
    }

    #[test]
    fn unfence_strips_code_fences() {
        assert_eq!(unfence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(unfence("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn codex_chat_is_not_wired_yet() {
        let p = CodexAppServerProvider { codex_path: "codex".into() };
        assert_eq!(p.chat(&[]).unwrap_err().code(), "provider");
    }

    #[test]
    fn build_provider_routes_by_kind() {
        assert_eq!(build_provider(&cfg(ProviderKind::Custom, None), None).name(), "mock");
        assert_eq!(build_provider(&cfg(ProviderKind::OpenAiCompatible, None), None).name(), "openai-compatible");
        assert_eq!(build_provider(&cfg(ProviderKind::CodexAppServer, None), None).name(), "codex-app-server");
    }
}
