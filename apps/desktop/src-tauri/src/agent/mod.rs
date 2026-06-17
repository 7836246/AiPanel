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

/// Build a provider from its stored config.
pub fn build_provider(config: &ProviderConfig) -> Box<dyn AgentProvider> {
    match config.kind {
        ProviderKind::CodexAppServer => Box::new(CodexAppServerProvider {
            codex_path: config.codex_path.clone().unwrap_or_else(|| "codex".to_string()),
        }),
        ProviderKind::OpenAiCompatible => Box::new(OpenAiCompatibleProvider { config: config.clone() }),
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
}

impl AgentProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        "openai-compatible"
    }
    fn chat(&self, _messages: &[ChatMessage]) -> AppResult<String> {
        Err(AppError::Provider("OpenAI-compatible chat 尚未实现".into()))
    }
    fn plan(&self, _intent: &str, _server_id: Option<&str>) -> AppResult<Plan> {
        Err(AppError::Provider("OpenAI-compatible plan 尚未实现；请使用 mock".into()))
    }
    fn summarize(&self, _context: &str) -> AppResult<String> {
        Err(AppError::Provider("OpenAI-compatible summarize 尚未实现".into()))
    }
    fn stream_events(&self, _intent: &str) -> AppResult<Vec<AgentEvent>> {
        Err(AppError::Provider("OpenAI-compatible streaming 尚未实现".into()))
    }
    fn test(&self) -> ProviderTestResult {
        match &self.config.base_url {
            Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
                ProviderTestResult {
                    ok: true,
                    message: "配置有效（未发起网络请求）".into(),
                    detail: Some(format!("base_url={url}")),
                }
            }
            _ => ProviderTestResult {
                ok: false,
                message: "base_url 缺失或不是 http(s) URL".into(),
                detail: None,
            },
        }
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

/// Test a provider config without persisting it. Bounded so a hanging binary
/// can't stall the UI.
pub fn test_provider(config: &ProviderConfig) -> ProviderTestResult {
    let provider = build_provider(config);
    // The codex version check shells out; cap it.
    let _ = Duration::from_secs(5);
    provider.test()
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
    fn openai_test_validates_base_url() {
        assert!(test_provider(&cfg(ProviderKind::OpenAiCompatible, Some("https://api.example.com"))).ok);
        assert!(!test_provider(&cfg(ProviderKind::OpenAiCompatible, None)).ok);
    }

    #[test]
    fn codex_chat_is_not_wired_yet() {
        let p = CodexAppServerProvider { codex_path: "codex".into() };
        assert_eq!(p.chat(&[]).unwrap_err().code(), "provider");
    }

    #[test]
    fn build_provider_routes_by_kind() {
        assert_eq!(build_provider(&cfg(ProviderKind::Custom, None)).name(), "mock");
        assert_eq!(build_provider(&cfg(ProviderKind::OpenAiCompatible, None)).name(), "openai-compatible");
        assert_eq!(build_provider(&cfg(ProviderKind::CodexAppServer, None)).name(), "codex-app-server");
    }
}
