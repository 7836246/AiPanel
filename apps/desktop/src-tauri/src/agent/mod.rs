//! Agent Provider 抽象层，以及 Codex app-server 桥接。
//!
//! Codex（或任意供应商）负责对话、理解、规划、模型与上下文；AiPanel 负责
//! 服务器、SSH、权限、执行、安全与审计。不可妥协的边界（见 CLAUDE.md、
//! docs/SECURITY_MODEL.zh-Hans.md）：
//!
//! - 供应商永远不持有 SSH 凭据，永远不跑裸 shell；
//! - 它只能通过 AiPanel Tools（见 `tools`）触达服务器能力——这些工具经过
//!   审核、默认只读、且全部审计。
//!
//! 本模块定义 provider trait 及三个实现：`MockAgentProvider`（离线、始终可用）、
//! `OpenAiCompatibleProvider`（配置 + 连通性骨架）、`CodexAppServerProvider`
//! （目标运行时，以 JSON-RPC/stdio 子进程方式启动——这里只做入口 + 健康检查；
//! 完整的工具调用回路与 tools 层一起接通）。

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod agent_loop;
pub mod codex;

use crate::core::error::{AppError, AppResult};
use crate::core::types::{Plan, ProviderConfig, ProviderKind, ProviderTestResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 流式的 agent 事件（当前是子集，随桥接成熟逐步扩展）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    Token { text: String },
    ToolCall { name: String, args: serde_json::Value },
    Done,
    Error { message: String },
}

/// 每个 agent 运行时都必须提供的能力。MVP 阶段保持同步接口；Codex 桥接
/// 在内部处理自己的异步子进程工作。
pub trait AgentProvider: Send + Sync {
    fn name(&self) -> &str;
    fn chat(&self, messages: &[ChatMessage]) -> AppResult<String>;
    fn plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan>;
    fn summarize(&self, context: &str) -> AppResult<String>;
    fn stream_events(&self, intent: &str) -> AppResult<Vec<AgentEvent>>;
    /// 校验配置 / 连通性，不真正开启完整会话。
    fn test(&self) -> ProviderTestResult;
}

/// 根据存储的配置 + 调用方从凭据库取出的密钥（API Key）构建一个 provider。
/// 密钥只在本次调用期间存活于 provider 实例中。
pub fn build_provider(config: &ProviderConfig, api_key: Option<String>) -> Box<dyn AgentProvider> {
    match config.kind {
        ProviderKind::CodexAppServer => Box::new(CodexAppServerProvider {
            // 始终解析打包的 codex-app-server;base/key/model 复用该供应商配置(走 responses 线协议)。
            program: codex::resolve_codex_bin(config.codex_path.as_deref()),
            base_url: config.base_url.clone(),
            api_key,
            model: config.model.clone(),
        }),
        ProviderKind::OpenAiCompatible => {
            Box::new(OpenAiCompatibleProvider { config: config.clone(), api_key })
        }
        ProviderKind::Custom => Box::new(MockAgentProvider),
    }
}

// ---------------------------------------------------------------------------
// Mock —— 离线、确定性、始终可用。
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
// OpenAI 兼容 —— 配置 + 连通性骨架（尚未发起真实补全请求）。
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

    /// POST /chat/completions 并返回 assistant 消息正文。
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

/// 若模型把 JSON 包在 ```json … ``` 代码围栏里，去掉围栏。
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
        let messages = vec![
            ChatMessage { role: "system".into(), content: PLAN_SYSTEM.into() },
            ChatMessage { role: "user".into(), content: intent.to_string() },
        ];
        let content = self.complete(&messages, true)?;
        plan_from_llm_json(&content, intent, server_id)
    }

    fn summarize(&self, context: &str) -> AppResult<String> {
        let messages = vec![
            ChatMessage { role: "system".into(), content: "用简体中文简要总结以下运维执行结果,指出关键发现与下一步建议。".into() },
            ChatMessage { role: "user".into(), content: context.to_string() },
        ];
        self.complete(&messages, false)
    }

    fn stream_events(&self, intent: &str) -> AppResult<Vec<AgentEvent>> {
        // 非流式回退：一次性请求结果包装成事件序列。
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
// Codex app-server —— 目标运行时。以 JSON-RPC/stdio 子进程方式启动。这里只是
// 入口 + 健康检查；完整的工具调用回路与 AiPanel Tools 层一起接通。
// ---------------------------------------------------------------------------

pub struct CodexAppServerProvider {
    /// 解析好的 codex-app-server 二进制路径(见 codex::resolve_codex_bin)。
    pub program: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
}

impl CodexAppServerProvider {
    /// 用当前配置构造一次启动参数(纯对话/规划,不挂 MCP 工具)。
    fn launch_cfg(&self) -> codex::CodexLaunch {
        codex::CodexLaunch {
            program: self.program.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            mcp: None,
        }
    }

    /// 跑一个**无状态**的 Codex turn 并返回 agent 的文本回答。
    ///
    /// 用于 chat/plan/summarize 这类不需要触达服务器的调用:启动 app-server、
    /// `initialize` 声明 AiPanel Tools、`turn/start` 后消费事件流。本上下文不接
    /// `tools::dispatch`(没有 `AppState`),因此一旦 agent 试图调用工具,就把错误
    /// 回灌给它——服务器能力只在带状态的自动诊断回路里开放(接入 run_agent_turn)。
    fn run_text_turn(&self, user_msg: &str) -> AppResult<String> {
        let mut client = codex::CodexClient::launch(&self.launch_cfg())?;
        client.initialize()?;
        client.run_turn(
            user_msg,
            |name, _args| Err(AppError::Provider(format!("此上下文未启用工具调用：{name}"))),
            Duration::from_secs(120),
        )
    }
}

/// 给规划用的系统提示(Codex 与 OpenAI 路径共用):只读优先、只输出结构化 JSON。
const PLAN_SYSTEM: &str = "你是 Linux 服务器运维规划助手。把用户任务转成一个结构化的诊断/操作计划。\
默认只读优先：除非用户明确要求修改,只生成检查类命令,绝不包含 rm/格式化/改防火墙/改 SSH 配置等危险命令。\
只输出 JSON,不要解释,格式:{\"goal\":\"一句话目标\",\"steps\":[{\"summary\":\"这步做什么\",\"command\":\"可直接在服务器执行的命令\"}]}。";

/// 把一份 LLM 产出的计划 JSON 解析为 [`Plan`],并由 AiPanel **重新判定**每步风险
/// (绝不信任模型自评)。Codex 与 OpenAI 路径共用。
fn plan_from_llm_json(content: &str, intent: &str, server_id: Option<&str>) -> AppResult<Plan> {
    let parsed: LlmPlan = serde_json::from_str(unfence(content))
        .map_err(|e| AppError::Provider(format!("计划 JSON 解析失败: {e}")))?;
    if parsed.steps.is_empty() {
        return Err(AppError::Provider("模型未返回任何步骤".into()));
    }
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

impl AgentProvider for CodexAppServerProvider {
    fn name(&self) -> &str {
        "codex-app-server"
    }
    fn chat(&self, messages: &[ChatMessage]) -> AppResult<String> {
        let prompt = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        self.run_text_turn(&prompt)
    }
    fn plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan> {
        let content = self.run_text_turn(&format!("{PLAN_SYSTEM}\n\n用户任务：{intent}"))?;
        plan_from_llm_json(&content, intent, server_id)
    }
    fn summarize(&self, context: &str) -> AppResult<String> {
        self.run_text_turn(&format!(
            "用简体中文简要总结以下运维执行结果,指出关键发现与下一步建议:\n\n{context}"
        ))
    }
    fn stream_events(&self, intent: &str) -> AppResult<Vec<AgentEvent>> {
        let reply = self.run_text_turn(intent)?;
        Ok(vec![AgentEvent::Token { text: reply }, AgentEvent::Done])
    }
    fn test(&self) -> ProviderTestResult {
        // 真正启动打包的 codex-app-server 并完成 initialize 握手(隔离 CODEX_HOME)。
        match codex::CodexClient::launch(&self.launch_cfg()).and_then(|mut c| c.initialize()) {
            Ok(_) => ProviderTestResult {
                ok: true,
                message: "codex-app-server 可用并完成 initialize".into(),
                detail: Some(format!("二进制:{}", self.program)),
            },
            Err(e) => ProviderTestResult {
                ok: false,
                message: format!("codex-app-server initialize 失败:{e}"),
                detail: Some("先运行 scripts/fetch-codex.sh 取得二进制".into()),
            },
        }
    }
}

/// 测试一份 provider 配置（带其 API Key），但不持久化。
pub fn test_provider(config: &ProviderConfig, api_key: Option<String>) -> ProviderTestResult {
    build_provider(config, api_key).test()
}

/// 探测供应商可用的模型列表（阻塞式 HTTP）。
///
/// 对 OpenAI 兼容供应商：GET `{base_url}/models`（base_url 处理与 chat 一致——
/// chat 用 `{base}/chat/completions`，这里就是 `{base}/models`），带
/// `Authorization: Bearer {key}`；解析形如 `{"data":[{"id":"..."}]}` 的响应，
/// 收集所有 `id`，去重 + 排序后返回。请求 / 解析失败返回清晰的 `AppError::Provider`。
/// 非 OpenAI 兼容（codex / custom）不支持模型探测，返回 Provider 错误。
pub fn list_models(config: &ProviderConfig, api_key: Option<String>) -> AppResult<Vec<String>> {
    if !matches!(config.kind, ProviderKind::OpenAiCompatible) {
        return Err(AppError::Provider("该供应商类型不支持模型探测".into()));
    }
    // 复用 OpenAI 兼容 provider 的 base_url / client 处理，确保与 chat 一致。
    let provider = OpenAiCompatibleProvider { config: config.clone(), api_key };
    let base = provider.base()?;
    let url = format!("{base}/models");
    let mut req = OpenAiCompatibleProvider::client()?.get(&url);
    if let Some(key) = &provider.api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().map_err(|e| AppError::Provider(format!("请求失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| AppError::Provider(e.to_string()))?;
    if !status.is_success() {
        return Err(AppError::Provider(format!(
            "HTTP {status}: {}",
            crate::core::sanitize::sanitize(&text)
        )));
    }
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| AppError::Provider(format!("响应解析失败: {e}")))?;
    let data = v["data"]
        .as_array()
        .ok_or_else(|| AppError::Provider("响应缺少 data 数组".into()))?;
    let mut ids: Vec<String> = data
        .iter()
        .filter_map(|item| item["id"].as_str().map(|s| s.to_string()))
        .collect();
    // 去重 + 排序，便于前端稳定展示。
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// 用 Codex 跑一次**带工具的只读自动诊断**:codex 经注入的 AiPanel MCP 工具面调用
/// 只读 server-ops 工具(在独立的 `mcp-server` 进程里执行,复用同一份 Core),收集事实后
/// 给出诊断总结。返回最终文本(工具活动由 mcp-server 进程审计)。
///
/// 这是**阻塞**调用(codex 客户端是阻塞 stdio),应由调用方放进 `spawn_blocking`。
/// 不需要 `&AppState`——工具不在本进程执行,而是走 codex↔MCP 进程。
pub fn run_codex_agent(cfg: codex::CodexLaunch, intent: &str, server_id: Option<&str>) -> AppResult<String> {
    let system = format!(
        "你是 AiPanel 的 Linux 运维助手。你只能调用提供的【只读】工具(server.list/info、\
server.doctor.readonly、ssh.run_readonly、task.plan/review)来收集事实(系统信息、端口、服务、\
日志等),然后用简体中文给出诊断结论和建议。绝不假设、不要编造命令输出。需要修改服务器时只在\
结论里描述建议操作,由用户在确认流程里执行——你自己不能执行任何写操作。当前目标 serverId={}。",
        server_id.unwrap_or("(未指定)")
    );
    let mut client = codex::CodexClient::launch(&cfg)?;
    client.initialize()?;
    // 工具经 MCP(独立进程)执行,不会回到本 app-server 客户端;故 on_tool 不应被触发。
    client.run_turn(
        &format!("{system}\n\n{intent}"),
        |name, _args| Err(AppError::Provider(format!("工具应经 MCP 调用,而非 app-server 客户端:{name}"))),
        Duration::from_secs(180),
    )
}

/// 打包的 codex-app-server 二进制是否就绪(决定 OpenAI 供应商能否走 codex 引擎)。
pub fn codex_binary_available() -> bool {
    let p = codex::resolve_codex_bin(None);
    std::path::Path::new(&p).is_file()
}

/// 用「打包 codex」给某个 **OpenAI 兼容** provider 生成计划(codex 作底层引擎,
/// base/key/model 复用该 provider)。阻塞调用,放进 spawn_blocking。
pub fn codex_plan(
    provider: &ProviderConfig,
    api_key: Option<String>,
    intent: &str,
    server_id: Option<&str>,
) -> AppResult<Plan> {
    CodexAppServerProvider {
        program: codex::resolve_codex_bin(provider.codex_path.as_deref()),
        base_url: provider.base_url.clone(),
        api_key,
        model: provider.model.clone(),
    }
    .plan(intent, server_id)
}

/// 用配置好的 provider 生成计划：构建对应 provider 并调用其 plan。
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
        // 缺 base_url → 在任何网络调用之前就失败。
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
    fn codex_chat_errors_without_binary() {
        // 无 codex 二进制的测试环境:run_turn 在启动子进程阶段失败,返回 provider 错误。
        // (turn / 工具回路本身由 codex.rs 的 drive_turn 单测对模拟事件流覆盖。)
        let p = CodexAppServerProvider {
            program: "definitely-not-a-real-codex-binary".into(),
            base_url: None,
            api_key: None,
            model: None,
        };
        assert_eq!(p.chat(&[ChatMessage { role: "user".into(), content: "hi".into() }]).unwrap_err().code(), "provider");
    }

    #[test]
    fn build_provider_routes_by_kind() {
        assert_eq!(build_provider(&cfg(ProviderKind::Custom, None), None).name(), "mock");
        assert_eq!(build_provider(&cfg(ProviderKind::OpenAiCompatible, None), None).name(), "openai-compatible");
        assert_eq!(build_provider(&cfg(ProviderKind::CodexAppServer, None), None).name(), "codex-app-server");
    }
}
