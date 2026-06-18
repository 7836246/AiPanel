//! Tauri 命令处理器 —— 前端与 Core 之间的薄边界层。
//!
//! 命令负责校验、委托给 Core 模块、并返回 serde 类型。它们绝不内嵌业务逻辑，
//! 也绝不记录或返回密钥。

use tauri::State;

pub mod docker;
pub mod files;
pub mod search;
pub mod stream;
pub mod tasks;
pub mod terminal;

use crate::core::error::{AppError, AppResult};
use crate::core::types::{
    AuditRecord, CommandExecution, CredentialRef, DoctorReport, ModelSelectionPolicy, Plan,
    ProviderConfig, ProviderInput, ProviderKind, ProviderTestResult, RiskReview, ServerInput,
    ServerProfile, ServerStatus, TaskStatus,
};
use crate::AppState;

/// 取出一个服务器及其 SSH 密钥（若其认证方式存有密钥）。
fn load_server_and_secret(
    state: &AppState,
    id: &str,
) -> AppResult<(ServerProfile, Option<String>)> {
    let server = state.store.get_server(id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };
    Ok((server, secret))
}

#[tauri::command]
pub fn list_servers(state: State<'_, AppState>) -> AppResult<Vec<ServerProfile>> {
    state.store.list_servers()
}

#[tauri::command]
pub fn get_server(state: State<'_, AppState>, id: String) -> AppResult<ServerProfile> {
    state.store.get_server(&id)
}

#[tauri::command]
pub fn create_server(state: State<'_, AppState>, input: ServerInput) -> AppResult<ServerProfile> {
    state.store.create_server(input)
}

#[tauri::command]
pub fn update_server(
    state: State<'_, AppState>,
    id: String,
    input: ServerInput,
) -> AppResult<ServerProfile> {
    let previous_ref = state.store.get_server(&id)?.credential_ref;
    let updated = state.store.update_server(&id, input)?;
    if should_delete_replaced_secret(previous_ref.as_ref(), updated.credential_ref.as_ref()) {
        if let Some(reference) = previous_ref {
            let _ = state.credentials.delete_secret(&reference);
        }
    }
    Ok(updated)
}

#[tauri::command]
pub fn delete_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    crate::ssh::cancel_for_server(&id);
    let _ = crate::terminal::close_for_server(&id);
    // 先删密钥，避免删服务器时把凭据遗留成孤儿。
    if let Ok(profile) = state.store.get_server(&id) {
        if let Some(reference) = &profile.credential_ref {
            let _ = state.credentials.delete_secret(reference);
        }
    }
    state.store.delete_server(&id)
}

/// 为某个服务器保存 SSH 密钥（密码或私钥）。密钥直接进凭据库，绝不记录日志、
/// 绝不写入 SQLite、绝不写进审计日志。
#[tauri::command]
pub fn set_server_secret(state: State<'_, AppState>, id: String, secret: String) -> AppResult<()> {
    let profile = state.store.get_server(&id)?;
    let reference = profile.credential_ref.ok_or_else(|| {
        crate::core::error::AppError::Validation(
            "this server's auth method does not use a stored secret".into(),
        )
    })?;
    state.credentials.put_secret(&reference, &secret)
}

/// 当前启用的凭据后端（"keychain" 或 "mock"），便于 UI 在密钥仅存于内存时给出提示。
#[tauri::command]
pub fn credential_backend(state: State<'_, AppState>) -> String {
    state.credentials.backend().to_string()
}

/// 审查一个计划的风险。`readOnlyMode` 会把任何非检查类步骤升级为 Blocked。
/// 纯函数——无副作用，也无需 state。
#[tauri::command]
pub fn review_plan(plan: Plan, read_only_mode: bool) -> RiskReview {
    crate::risk::review_plan(&plan, read_only_mode)
}

/// 测试 SSH 连通性 + 认证，并把结果缓存为该服务器的状态。
#[tauri::command]
pub async fn check_ssh_connection(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<crate::core::types::ConnCheck> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    let check = crate::ssh::check_connection(&server, secret.as_deref())
        .await
        .unwrap_or_else(|e| crate::core::types::ConnCheck {
            ok: false,
            message: e.to_string(),
        });
    let status = if check.ok {
        ServerStatus::Online
    } else {
        ServerStatus::Offline
    };
    state.store.set_server_status(&id, status, None)?;
    Ok(check)
}

/// 执行单条只读命令（受风险审查器把关）。这是开发/诊断入口；面向用户的流程
/// 走 Server Doctor 与计划。
#[tauri::command]
pub async fn run_readonly_command(
    state: State<'_, AppState>,
    id: String,
    command: String,
) -> AppResult<CommandExecution> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    crate::ssh::run_readonly(
        &server,
        secret.as_deref(),
        &command,
        crate::ssh::DEFAULT_TIMEOUT,
    )
    .await
}

/// doctor 将要执行的只读计划，供执行前预览。
#[tauri::command]
pub fn server_doctor_plan(state: State<'_, AppState>, id: String) -> AppResult<Plan> {
    state.store.get_server(&id)?; // 确认存在
    Ok(crate::doctor::doctor_plan(&id))
}

/// 执行只读的服务器 doctor，并把状态 + 快速事实缓存到该服务器上。
#[tauri::command]
pub async fn run_server_doctor(state: State<'_, AppState>, id: String) -> AppResult<DoctorReport> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    let plan = crate::doctor::doctor_plan(&id);
    let review = crate::risk::review_plan(&plan, true); // doctor 以只读模式运行
    let report = crate::doctor::run_doctor(&server, secret.as_deref()).await?;

    let succeeded = report.executions.iter().any(|e| e.exit_code == 0);
    let status = if succeeded {
        ServerStatus::Online
    } else {
        ServerStatus::Offline
    };
    let facts = crate::doctor::facts_from_report(&report);
    // 取消或全失败的体检可能产出空/部分 facts；为空时传 None 以保留上次缓存的完整 facts，
    // 仅在 facts 非空时才覆盖。
    let facts_arg = if facts.is_empty() { None } else { Some(&facts) };
    state.store.set_server_status(&id, status, facts_arg)?;

    // 每次执行都在本地审计。
    let record = crate::audit::record_for_doctor(&id, plan, review, &report);
    state.store.insert_audit_record(&record)?;

    Ok(report)
}

/// 采集一份服务器监控指标快照（SSH 只读，服务器零 agent）。前端定时轮询本命令，
/// 并跨两次结果对网络 / 磁盘累计字节求差得到速率（后端只回累计值，不做 sleep 测速）。
#[tauri::command]
pub async fn server_metrics(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<crate::core::types::ServerMetrics> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    crate::metrics::collect(&server, secret.as_deref()).await
}

/// 最近的审计记录（最新在前）。
#[tauri::command]
pub fn list_audit_records(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<AuditRecord>> {
    state
        .store
        .list_audit_records(crate::commands::search::normalize_limit(limit))
}

/// 按 id 取单条审计记录，用于回放某次任务的细节。
#[tauri::command]
pub fn get_audit_record(state: State<'_, AppState>, id: String) -> AppResult<AuditRecord> {
    state.store.get_audit_record(&id)
}

/// 按顺序排好的、用于规划的 AI provider 候选：已启用、非 custom；仅在模型策略为
/// 手动固定时才把默认 provider 排在最前。返回空才允许回退到离线规则引擎；一旦用户
/// 配置了真实 provider，失败就必须显式反馈，不能静默伪装成 AI 规划成功。
fn candidate_providers(state: &AppState) -> AppResult<Vec<ProviderConfig>> {
    let mut list: Vec<ProviderConfig> = state
        .store
        .list_providers()?
        .into_iter()
        .filter(|p| p.enabled && !matches!(p.kind, ProviderKind::Custom))
        .collect();
    // 自动模式保留创建顺序；手动模式才把默认 provider 排最前。每个 OpenAI 兼容
    // provider 内部再「先 codex 引擎、后直连」(见 create_plan)。
    let policy = state.store.get_policy()?;
    if !policy.auto {
        if let Some(id) = policy.default_provider_id {
            list.sort_by_key(|p| usize::from(p.id != id));
        }
    }
    Ok(list)
}

/// 本会话中「打包 codex 引擎」对某 provider 失败过的集合 → 后续跳过 codex、直接走
/// AiPanel 直连,避免每次都白试一遍(尤其端点只支持 chat 而不支持 Responses 时)。
static CODEX_SKIP: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

fn codex_skip() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    CODEX_SKIP.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

fn codex_skip_contains(id: &str) -> bool {
    codex_skip()
        .lock()
        .map(|guard| guard.contains(id))
        .unwrap_or_else(|_| {
            eprintln!("[plan] codex skip cache lock poisoned; bypassing cache for {id}");
            false
        })
}

fn codex_skip_insert(id: &str) {
    if let Ok(mut guard) = codex_skip().lock() {
        guard.insert(id.to_string());
    } else {
        eprintln!("[plan] codex skip cache lock poisoned; failed to cache failure for {id}");
    }
}

fn codex_skip_remove(id: &str) {
    if let Ok(mut guard) = codex_skip().lock() {
        guard.remove(id);
    } else {
        eprintln!("[plan] codex skip cache lock poisoned; failed to clear cache for {id}");
    }
}

/// 该 provider 是否走 codex 引擎:OpenAI 兼容 + 打包二进制就绪 + 本会话未失败过。
/// (设置里只暴露 OpenAI 兼容一种类型;codex 是其底层引擎,而非单独的供应商类型。)
fn codex_engine_usable(p: &ProviderConfig) -> bool {
    matches!(p.kind, ProviderKind::OpenAiCompatible)
        && crate::agent::codex_binary_available()
        && !codex_skip_contains(&p.id)
}

fn mark_codex_failed(id: &str) {
    codex_skip_insert(id);
}

fn should_delete_replaced_secret(
    previous: Option<&CredentialRef>,
    next: Option<&CredentialRef>,
) -> bool {
    previous.is_some() && previous != next
}

fn provider_chain_failed_error(last_error: Option<String>) -> AppError {
    AppError::Provider(format!(
        "已配置的模型供应商均不可用，未使用离线规则兜底。请检查模型供应商配置后重试{}",
        last_error
            .filter(|s| !s.trim().is_empty())
            .map(|s| format!("：{s}"))
            .unwrap_or_default()
    ))
}

/// 把自然语言意图转成结构化、可审查的计划。对每个候选 provider:**优先用打包的 codex
/// 引擎**(端点支持 Responses 时),失败则回退 AiPanel 直连(chat/completions)。只有完全
/// 没配置供应商时，才回退离线规则计划；配置了真实 provider 但失败时必须显式报错。
#[tauri::command]
pub async fn create_plan(
    state: State<'_, AppState>,
    intent: String,
    server_id: Option<String>,
) -> AppResult<Plan> {
    let providers = candidate_providers(&state)?;
    let has_configured_provider = !providers.is_empty();
    let mut last_error: Option<String> = None;

    for provider in providers {
        let key = provider
            .credential_ref
            .as_ref()
            .and_then(|r| state.credentials.get_secret(r).ok().flatten());

        // 1) 优先打包 codex 引擎(OpenAI 兼容 + 二进制就绪 + 本会话未失败过)。
        if codex_engine_usable(&provider) {
            let p = provider.clone();
            let k = key.clone();
            let i2 = intent.clone();
            let sid = server_id.clone();
            match tokio::task::spawn_blocking(move || {
                crate::agent::codex_plan(&p, k, &i2, sid.as_deref())
            })
            .await
            {
                Ok(Ok(plan)) => return Ok(plan),
                Ok(Err(e)) => {
                    last_error.get_or_insert_with(|| e.to_string());
                    mark_codex_failed(&provider.id);
                    eprintln!("[plan] codex 引擎失败({}),回退直连", e.code());
                }
                Err(e) => {
                    last_error.get_or_insert_with(|| format!("codex 引擎线程异常: {e}"));
                    mark_codex_failed(&provider.id);
                    eprintln!("[plan] codex 引擎线程异常({e}),回退直连");
                }
            }
        }

        // 2) AiPanel 直连(/chat/completions),阻塞式 HTTP 放到 UI 线程之外。
        let p = provider.clone();
        let k = key.clone();
        let i2 = intent.clone();
        let sid = server_id.clone();
        match tokio::task::spawn_blocking(move || {
            crate::agent::plan_with_provider(&p, k, &i2, sid.as_deref())
        })
        .await
        {
            Ok(Ok(plan)) => return Ok(plan),
            Ok(Err(e)) => {
                last_error = Some(e.to_string());
                eprintln!(
                    "[plan] provider '{}' 直连失败({}); 继续下一个 provider",
                    provider.name,
                    e.code()
                );
            }
            Err(e) => {
                last_error = Some(format!("provider 线程异常: {e}"));
                eprintln!("[plan] provider '{}' 直连线程异常({e})", provider.name);
            }
        }
    }
    if has_configured_provider {
        return Err(provider_chain_failed_error(last_error));
    }
    state.plan_engine.create_plan(&intent, server_id.as_deref())
}

/// 执行用户已确认的计划。计划**总是**在服务端重新审查（绝不信任客户端）：
/// 拒绝被 Blocked 的步骤，并在任何命令运行前强制要求达到所需的确认级别。
/// 每次执行都会审计。
#[tauri::command]
pub async fn execute_confirmed_plan(
    state: State<'_, AppState>,
    plan: Plan,
    confirmed: bool,
    double_confirmed: bool,
    read_only_mode: bool,
) -> AppResult<AuditRecord> {
    let server_id = plan
        .server_id
        .clone()
        .ok_or_else(|| AppError::Validation("plan has no target server".into()))?;
    let (server, secret) = load_server_and_secret(&state, &server_id)?;

    // 空计划没有任何步骤可执行；提前拒绝，避免产生一条无内容的审计记录。
    if plan.steps.is_empty() {
        return Err(AppError::Validation("plan has no steps".into()));
    }

    let review = crate::risk::review_plan(&plan, read_only_mode);
    if review.blocked {
        return Err(AppError::Blocked("plan contains blocked steps".into()));
    }
    if review.requires_confirmation && !confirmed {
        return Err(AppError::Blocked("plan requires confirmation".into()));
    }
    if review.requires_double_confirmation && !double_confirmed {
        return Err(AppError::Blocked(
            "plan requires a second confirmation".into(),
        ));
    }

    let mut executions = Vec::new();
    let mut failed = false;
    for (index, step) in plan.steps.iter().enumerate() {
        // 按「服务端重判的等级」而非过时的客户端 step.read_only 路由：用户编辑某步后，
        // step.read_only 可能已过时。Low 等级走只读路径（带 Low 校验门），其余走写路径。
        let res = if review.step_levels[index] == crate::core::types::RiskLevel::Low {
            crate::ssh::run_readonly(
                &server,
                secret.as_deref(),
                &step.command,
                crate::ssh::DEFAULT_TIMEOUT,
            )
            .await
        } else {
            crate::ssh::run_command(
                &server,
                secret.as_deref(),
                &step.command,
                crate::ssh::DEFAULT_TIMEOUT,
            )
            .await
        };
        match res {
            Ok(exec) => {
                let bad = exec.exit_code != 0;
                executions.push(exec);
                if bad {
                    failed = true;
                    break;
                }
            }
            Err(e) => {
                executions.push(crate::audit::record_failed_command(&step.command, &e.to_string()));
                failed = true;
                break;
            }
        }
    }

    let status = if failed {
        TaskStatus::Failed
    } else {
        TaskStatus::Completed
    };
    let intent = plan.goal.clone();
    let record =
        crate::audit::record_for_plan(Some(&server_id), &intent, plan, review, executions, status);
    state.store.insert_audit_record(&record)?;
    Ok(record)
}

/// 跑一次自主的、只读的诊断回合：模型通过只读的 AiPanel Tools 自行调查并返回
/// 总结。它无法修改服务器——写操作仍需走显式的「确认并执行」流程。
#[tauri::command]
pub async fn run_agent_turn(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    intent: String,
    server_id: Option<String>,
) -> AppResult<crate::agent::agent_loop::AgentTurnResult> {
    let provider = candidate_providers(&state)?
        .into_iter()
        .find(|p| matches!(p.kind, ProviderKind::OpenAiCompatible))
        .ok_or_else(|| {
            AppError::Provider("自动诊断需要一个已启用的 OpenAI 兼容供应商,请在设置中配置".into())
        })?;
    let key = provider
        .credential_ref
        .as_ref()
        .and_then(|r| state.credentials.get_secret(r).ok().flatten());

    // 优先用打包的 codex 引擎(经注入的 AiPanel MCP 工具面做带工具的只读诊断);
    // 失败则回退 OpenAI function-calling 回路。
    if codex_engine_usable(&provider) {
        if let Some(bridge) = codex_mcp_bridge(&app) {
            let trace_path = std::env::temp_dir().join(format!(
                "aipanel-codex-trace-{}.jsonl",
                crate::core::types::new_id()
            ));
            let cfg = crate::agent::codex::CodexLaunch {
                program: crate::agent::codex::resolve_codex_bin(provider.codex_path.as_deref()),
                base_url: provider.base_url.clone(),
                api_key: key.clone(),
                model: provider.model.clone(),
                mcp: Some(crate::agent::codex::McpBridge {
                    trace_path: Some(trace_path.to_string_lossy().to_string()),
                    ..bridge
                }),
            };
            let intent2 = intent.clone();
            let sid = server_id.clone();
            let trace_path2 = trace_path.clone();
            // codex 客户端是阻塞 stdio,放到阻塞线程池。工具在独立 mcp-server 进程执行,无需 &AppState。
            match tokio::task::spawn_blocking(move || {
                crate::agent::run_codex_agent(cfg, &intent2, sid.as_deref())
            })
            .await
            {
                Ok(Ok(summary)) if !summary.trim().is_empty() => {
                    let tool_calls = read_codex_trace(&trace_path2);
                    let _ = std::fs::remove_file(&trace_path2);
                    return Ok(crate::agent::agent_loop::AgentTurnResult {
                        summary,
                        tool_calls,
                    });
                }
                Ok(Ok(_)) => {
                    let _ = std::fs::remove_file(&trace_path);
                    mark_codex_failed(&provider.id);
                    eprintln!("[agent] codex 诊断空结果,回退 OpenAI");
                }
                Ok(Err(e)) => {
                    let _ = std::fs::remove_file(&trace_path);
                    mark_codex_failed(&provider.id);
                    eprintln!("[agent] codex 诊断失败({}),回退 OpenAI", e.code());
                }
                Err(_) => {
                    let _ = std::fs::remove_file(&trace_path);
                    mark_codex_failed(&provider.id);
                    eprintln!("[agent] codex 诊断线程异常,回退 OpenAI");
                }
            }
        }
    }

    // 回退:OpenAI 兼容的 function-calling 只读诊断回路。
    crate::agent::agent_loop::run_turn(&state, &provider, key, &intent, server_id.as_deref()).await
}

/// 构造把 AiPanel 自身注入 codex 的 MCP 桥(当前可执行文件 + 应用数据目录,后者让
/// mcp-server 子进程复用同一份 SQLite/Keychain)。
fn codex_mcp_bridge(app: &tauri::AppHandle) -> Option<crate::agent::codex::McpBridge> {
    use tauri::Manager;
    let exe = std::env::current_exe().ok()?.to_string_lossy().to_string();
    let data_dir = app
        .path()
        .app_data_dir()
        .ok()?
        .to_string_lossy()
        .to_string();
    Some(crate::agent::codex::McpBridge {
        aipanel_exe: exe,
        data_dir,
        trace_path: None,
    })
}

fn read_codex_trace(path: &std::path::Path) -> Vec<crate::agent::agent_loop::ToolTrace> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<crate::agent::agent_loop::ToolTrace>(line).ok())
        .collect()
}

/// 测试一份 agent provider 配置（合法性 / 可达性），但不保存。API Key 来自本次
/// 调用（用户正在表单里输入的 key），若没有则从凭据库取已保存 provider 的密钥。
#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    config: ProviderConfig,
    api_key: Option<String>,
) -> AppResult<ProviderTestResult> {
    let key = api_key.or_else(|| {
        config
            .credential_ref
            .as_ref()
            .and_then(|r| state.credentials.get_secret(r).ok().flatten())
    });
    // 探测是阻塞式 HTTP —— 放到 UI 线程之外。join 失败以「非 ok 结果」呈现
    // （而非 reject 的 promise），这样 UI 能就地展示。
    Ok(
        tokio::task::spawn_blocking(move || crate::agent::test_provider(&config, key))
            .await
            .unwrap_or_else(|e| ProviderTestResult {
                ok: false,
                message: format!("测试任务失败: {e}"),
                detail: None,
            }),
    )
}

/// agent 可调用的 AiPanel Tools 清单（名称、权限、审计策略）。
#[tauri::command]
pub fn list_tools() -> Vec<crate::tools::ToolSpec> {
    crate::tools::registry()
}

/// 探测某个供应商可用的模型列表。API Key 优先取本次调用入参（用户正在表单里
/// 输入的 key），否则从凭据库取已保存 provider 的密钥。仅 OpenAI 兼容供应商支持。
#[tauri::command]
pub async fn list_models(
    state: State<'_, AppState>,
    config: ProviderConfig,
    api_key: Option<String>,
) -> AppResult<Vec<String>> {
    let key = api_key.or_else(|| {
        config
            .credential_ref
            .as_ref()
            .and_then(|r| state.credentials.get_secret(r).ok().flatten())
    });
    // 探测是阻塞式 HTTP —— 放到 UI 线程之外。
    tokio::task::spawn_blocking(move || crate::agent::list_models(&config, key))
        .await
        .map_err(|e| AppError::Provider(format!("探测任务失败: {e}")))?
}

/// 设置某个供应商的激活模型（model 为 None 时清空），返回更新后的 ProviderConfig。
/// 只更新 model 列，不触碰凭据等其它配置。
#[tauri::command]
pub fn set_provider_model(
    state: State<'_, AppState>,
    id: String,
    model: Option<String>,
) -> AppResult<ProviderConfig> {
    let provider = state.store.set_provider_model(&id, model.as_deref())?;
    codex_skip_remove(&provider.id);
    Ok(provider)
}

// ----- provider / 模型选择 -----------------------------------------------

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> AppResult<Vec<ProviderConfig>> {
    state.store.list_providers()
}

/// 新建或更新一个 provider。API Key（若有）直接进凭据库；SQLite 里只持久化
/// 一个 CredentialRef。
#[tauri::command]
pub fn save_provider(
    state: State<'_, AppState>,
    input: ProviderInput,
    api_key: Option<String>,
    clear_api_key: bool,
) -> AppResult<ProviderConfig> {
    let input = validate_provider_input(input)?;
    let now = crate::core::types::now();
    let (id, created_at, existing_ref) = match &input.id {
        Some(id) => {
            let existing = state.store.get_provider(id).ok();
            (
                id.clone(),
                existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                existing.and_then(|e| e.credential_ref),
            )
        }
        None => (crate::core::types::new_id(), now, None),
    };
    let api_key = api_key.filter(|key| !key.is_empty());
    let credential_ref = provider_credential_ref_for_save(&id, existing_ref, api_key.is_some(), clear_api_key);
    let config = ProviderConfig {
        id,
        name: input.name,
        kind: input.kind,
        base_url: input.base_url,
        model: input.model,
        codex_path: input.codex_path,
        credential_ref,
        enabled: input.enabled,
        created_at,
        updated_at: now,
    };
    state.store.upsert_provider(&config)?;
    if let Some(key) = api_key {
        state
            .credentials
            .put_secret(&CredentialRef::for_provider(&config.id), &key)?;
    } else if clear_api_key {
        let _ = state
            .credentials
            .delete_secret(&CredentialRef::for_provider(&config.id));
    }
    codex_skip_remove(&config.id);
    Ok(config)
}

fn provider_credential_ref_for_save(
    provider_id: &str,
    existing_ref: Option<CredentialRef>,
    has_new_api_key: bool,
    clear_api_key: bool,
) -> Option<CredentialRef> {
    if has_new_api_key {
        Some(CredentialRef::for_provider(provider_id))
    } else if clear_api_key {
        None
    } else {
        existing_ref
    }
}

#[tauri::command]
pub fn delete_provider(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if let Ok(p) = state.store.get_provider(&id) {
        if let Some(reference) = &p.credential_ref {
            let _ = state.credentials.delete_secret(reference);
        }
    }
    state.store.delete_provider(&id)?;
    codex_skip_remove(&id);
    clear_deleted_default_provider(&state, &id)?;
    Ok(())
}

fn clear_deleted_default_provider(state: &AppState, deleted_provider_id: &str) -> AppResult<()> {
    let policy = state.store.get_policy()?;
    if policy.default_provider_id.as_deref() == Some(deleted_provider_id) {
        state.store.set_policy(&ModelSelectionPolicy {
            default_provider_id: None,
            ..policy
        })?;
    }
    Ok(())
}

fn validate_provider_input(input: ProviderInput) -> AppResult<ProviderInput> {
    let id = input
        .id
        .map(validate_provider_id)
        .transpose()?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("provider name is required".into()));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "provider name must not contain control characters".into(),
        ));
    }

    let base_url = normalize_optional_provider_field(input.base_url, "base URL")?;
    let model = normalize_optional_provider_field(input.model, "model")?;
    let codex_path = normalize_optional_provider_field(input.codex_path, "codex path")?;

    if matches!(input.kind, ProviderKind::OpenAiCompatible) {
        let Some(url) = &base_url else {
            return Err(AppError::Validation(
                "OpenAI compatible provider requires a base URL".into(),
            ));
        };
        validate_http_base_url(url)?;
    }

    Ok(ProviderInput {
        id,
        name,
        kind: input.kind,
        base_url,
        model,
        codex_path,
        enabled: input.enabled,
    })
}

fn validate_provider_id(id: String) -> AppResult<String> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(AppError::Validation("provider id is required".into()));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(
            "provider id may only contain ASCII letters, numbers, '-' and '_'".into(),
        ));
    }
    Ok(id)
}

fn normalize_optional_provider_field(
    value: Option<String>,
    field: &str,
) -> AppResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

fn validate_http_base_url(url: &str) -> AppResult<()> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(AppError::Validation(
            "base URL must start with http:// or https://".into(),
        ));
    }
    if url.contains(char::is_whitespace) {
        return Err(AppError::Validation(
            "base URL must not contain whitespace".into(),
        ));
    }
    let after_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || host.starts_with(':') || host.contains('@') {
        return Err(AppError::Validation(
            "base URL must include a host".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn get_model_selection_policy(state: State<'_, AppState>) -> AppResult<ModelSelectionPolicy> {
    state.store.get_policy()
}

#[tauri::command]
pub fn save_model_selection_policy(
    state: State<'_, AppState>,
    policy: ModelSelectionPolicy,
) -> AppResult<()> {
    let policy = validate_model_selection_policy(&state, policy)?;
    state.store.set_policy(&policy)
}

fn validate_model_selection_policy(
    state: &AppState,
    policy: ModelSelectionPolicy,
) -> AppResult<ModelSelectionPolicy> {
    let default_provider_id = policy
        .default_provider_id
        .map(validate_provider_id)
        .transpose()?;
    if !policy.auto {
        let Some(id) = &default_provider_id else {
            return Err(AppError::Validation(
                "manual model selection requires a default provider".into(),
            ));
        };
        let provider = state
            .store
            .get_provider(id)
            .map_err(|_| AppError::Validation("default provider does not exist".into()))?;
        if !provider.enabled || matches!(provider.kind, ProviderKind::Custom) {
            return Err(AppError::Validation(
                "default provider must be enabled and usable for planning".into(),
            ));
        }
    }
    Ok(ModelSelectionPolicy {
        auto: policy.auto,
        default_provider_id,
    })
}

// ----- 收藏 / 批量连通刷新 ------------------------------------------------

/// 设置某台服务器的收藏状态，返回更新后的 ServerProfile。
#[tauri::command]
pub fn set_server_favorite(
    state: State<'_, AppState>,
    id: String,
    favorite: bool,
) -> AppResult<ServerProfile> {
    state.store.set_server_favorite(&id, favorite)
}

/// 对所有服务器并发做 SSH 连通性检查，把各自状态更新为 online/offline，
/// 最后返回刷新后的服务器列表（按收藏 / 创建时间排序）。
/// 单台检查失败（无法连接、密钥读取出错等）一律记为 offline，绝不中断整体。
#[tauri::command]
pub async fn refresh_all_servers(state: State<'_, AppState>) -> AppResult<Vec<ServerProfile>> {
    // 先在持有 store 的同步阶段把每台服务器及其密钥取出，得到可移动进并发任务的 owned 数据，
    // 避免把 &State / 非 Send 的连接句柄带进 spawn。
    let servers = state.store.list_servers()?;
    let mut prepared: Vec<(String, ServerProfile, Option<String>)> =
        Vec::with_capacity(servers.len());
    for server in servers {
        // 密钥读取失败不应让整轮刷新失败：取不到就当作没有密钥，让连通性检查自然判定为离线。
        let secret = match &server.credential_ref {
            Some(reference) => state.credentials.get_secret(reference).ok().flatten(),
            None => None,
        };
        prepared.push((server.id.clone(), server, secret));
    }

    // 并发执行连通性检查（每台一个 tokio 任务）。check_connection 内部已带超时。
    let mut set = tokio::task::JoinSet::new();
    for (id, server, secret) in prepared {
        set.spawn(async move {
            let online = crate::ssh::check_connection(&server, secret.as_deref())
                .await
                .map(|c| c.ok)
                .unwrap_or(false);
            (id, online)
        });
    }

    // 收集结果并各自落库；任务 panic 视为该服务器离线，但 id 已无从得知，故仅记录日志。
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((id, online)) => {
                let status = if online {
                    ServerStatus::Online
                } else {
                    ServerStatus::Offline
                };
                // facts 传 None：仅刷新连通状态，保留上次体检缓存的 facts。
                if let Err(e) = state.store.set_server_status(&id, status, None) {
                    eprintln!(
                        "[refresh] failed to persist status for server {id}: {}",
                        e.code()
                    );
                }
            }
            Err(e) => eprintln!("[refresh] connectivity task panicked: {e}"),
        }
    }

    state.store.list_servers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_chain_failure_is_explicit_provider_error() {
        let err = provider_chain_failed_error(Some("agent provider: unauthorized".into()));
        assert_eq!(err.code(), "provider");
        let text = err.to_string();
        assert!(text.contains("已配置的模型供应商均不可用"));
        assert!(text.contains("未使用离线规则兜底"));
        assert!(text.contains("unauthorized"));
    }

    #[test]
    fn provider_chain_failure_handles_missing_last_error() {
        let err = provider_chain_failed_error(None);
        assert_eq!(err.code(), "provider");
        assert!(err.to_string().contains("请检查模型供应商配置后重试"));
    }

    #[test]
    fn codex_skip_cache_tracks_failed_provider_without_panic() {
        let id = format!("provider-{}", crate::core::types::new_id());
        assert!(!codex_skip_contains(&id));
        mark_codex_failed(&id);
        assert!(codex_skip_contains(&id));
        codex_skip_remove(&id);
        assert!(!codex_skip_contains(&id));
    }

    #[test]
    fn replaced_server_secret_is_deleted_only_when_reference_changes() {
        let old = CredentialRef::for_server("s1");
        let same = CredentialRef::for_server("s1");
        let new = CredentialRef::for_server("s2");

        assert!(!should_delete_replaced_secret(None, None));
        assert!(!should_delete_replaced_secret(None, Some(&new)));
        assert!(!should_delete_replaced_secret(Some(&old), Some(&same)));
        assert!(should_delete_replaced_secret(Some(&old), None));
        assert!(should_delete_replaced_secret(Some(&old), Some(&new)));
    }

    fn test_provider_config(id: &str, name: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            name: name.into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: Some(format!("https://{id}.example.com/v1")),
            model: Some("gpt-4o-mini".into()),
            codex_path: None,
            credential_ref: None,
            enabled: true,
            created_at: crate::core::types::now(),
            updated_at: crate::core::types::now(),
        }
    }

    #[test]
    fn candidate_providers_honors_policy_auto_flag() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let first = test_provider_config("first", "First");
        let second = test_provider_config("second", "Second");
        state.store.upsert_provider(&first).unwrap();
        state.store.upsert_provider(&second).unwrap();

        state
            .store
            .set_policy(&ModelSelectionPolicy {
                auto: true,
                default_provider_id: Some(second.id.clone()),
            })
            .unwrap();
        let auto_order = candidate_providers(&state).unwrap();
        assert_eq!(auto_order.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["first", "second"]);

        state
            .store
            .set_policy(&ModelSelectionPolicy {
                auto: false,
                default_provider_id: Some(second.id.clone()),
            })
            .unwrap();
        let fixed_order = candidate_providers(&state).unwrap();
        assert_eq!(fixed_order.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["second", "first"]);
    }

    #[test]
    fn model_selection_policy_validation_requires_usable_manual_default() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let usable = test_provider_config("usable", "Usable");
        let mut disabled = test_provider_config("disabled", "Disabled");
        disabled.enabled = false;
        let mut custom = test_provider_config("custom", "Custom");
        custom.kind = ProviderKind::Custom;
        state.store.upsert_provider(&usable).unwrap();
        state.store.upsert_provider(&disabled).unwrap();
        state.store.upsert_provider(&custom).unwrap();

        assert!(validate_model_selection_policy(
            &state,
            ModelSelectionPolicy {
                auto: true,
                default_provider_id: None,
            },
        )
        .is_ok());
        assert_eq!(
            validate_model_selection_policy(
                &state,
                ModelSelectionPolicy {
                    auto: false,
                    default_provider_id: Some(" usable ".into()),
                },
            )
            .unwrap()
            .default_provider_id
            .as_deref(),
            Some("usable")
        );

        for default_provider_id in [None, Some("missing".into()), Some("disabled".into()), Some("custom".into()), Some("bad:id".into())] {
            let err = validate_model_selection_policy(
                &state,
                ModelSelectionPolicy {
                    auto: false,
                    default_provider_id,
                },
            )
            .unwrap_err();
            assert_eq!(err.code(), "validation");
        }
    }

    #[test]
    fn deleting_default_provider_clears_model_selection_reference() {
        let state = AppState {
            store: crate::store::Store::open_in_memory().unwrap(),
            credentials: Box::new(crate::credentials::LocalMockCredentialStore::default()),
            plan_engine: Box::new(crate::plan::MockPlanEngine),
        };
        let provider = test_provider_config("default", "Default");
        state.store.upsert_provider(&provider).unwrap();
        state
            .store
            .set_policy(&ModelSelectionPolicy {
                auto: false,
                default_provider_id: Some(provider.id.clone()),
            })
            .unwrap();

        state.store.delete_provider(&provider.id).unwrap();
        clear_deleted_default_provider(&state, &provider.id).unwrap();

        let policy = state.store.get_policy().unwrap();
        assert!(!policy.auto);
        assert_eq!(policy.default_provider_id, None);
    }

    #[test]
    fn provider_credential_ref_save_policy_handles_keep_clear_and_replace() {
        let existing = Some(CredentialRef::for_provider("p1"));
        assert_eq!(
            provider_credential_ref_for_save("p1", existing.clone(), false, false),
            existing,
            "未输入新 key 且未勾选清除时应保留旧凭据引用"
        );
        assert_eq!(
            provider_credential_ref_for_save("p1", Some(CredentialRef("legacy".into())), false, true),
            None,
            "显式清除时必须移除凭据引用"
        );
        assert_eq!(
            provider_credential_ref_for_save("p1", None, true, true),
            Some(CredentialRef::for_provider("p1")),
            "输入新 key 时应写入 provider 自身的稳定凭据引用"
        );
    }

    #[test]
    fn provider_input_validation_normalizes_fields() {
        let input = ProviderInput {
            id: Some("p1".into()),
            name: "  OpenAI  ".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: Some("  https://api.example.com/v1/  ".into()),
            model: Some("  gpt-4o-mini  ".into()),
            codex_path: Some("  /usr/local/bin/codex  ".into()),
            enabled: true,
        };

        let got = validate_provider_input(input).unwrap();
        assert_eq!(got.name, "OpenAI");
        assert_eq!(got.id.as_deref(), Some("p1"));
        assert_eq!(got.base_url.as_deref(), Some("https://api.example.com/v1/"));
        assert_eq!(got.model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(got.codex_path.as_deref(), Some("/usr/local/bin/codex"));
    }

    #[test]
    fn provider_input_validation_rejects_bad_openai_config() {
        let valid = || ProviderInput {
            id: None,
            name: "OpenAI".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: Some("https://api.example.com/v1".into()),
            model: Some("gpt-4o-mini".into()),
            codex_path: None,
            enabled: true,
        };

        let mut missing_base = valid();
        missing_base.base_url = Some("  ".into());
        assert_eq!(
            validate_provider_input(missing_base).unwrap_err().code(),
            "validation"
        );

        let mut bad_scheme = valid();
        bad_scheme.base_url = Some("ftp://api.example.com/v1".into());
        assert_eq!(
            validate_provider_input(bad_scheme).unwrap_err().code(),
            "validation"
        );

        let mut bad_space = valid();
        bad_space.base_url = Some("https://api.example.com /v1".into());
        assert_eq!(
            validate_provider_input(bad_space).unwrap_err().code(),
            "validation"
        );

        let mut bad_host = valid();
        bad_host.base_url = Some("https:///v1".into());
        assert_eq!(
            validate_provider_input(bad_host).unwrap_err().code(),
            "validation"
        );
    }

    #[test]
    fn provider_input_validation_rejects_control_chars() {
        let input = ProviderInput {
            id: None,
            name: "OpenAI\nbad".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: Some("https://api.example.com/v1".into()),
            model: None,
            codex_path: None,
            enabled: true,
        };
        assert_eq!(validate_provider_input(input).unwrap_err().code(), "validation");

        let input = ProviderInput {
            id: None,
            name: "OpenAI".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: Some("https://api.example.com/v1".into()),
            model: Some("gpt\nbad".into()),
            codex_path: None,
            enabled: true,
        };
        assert_eq!(validate_provider_input(input).unwrap_err().code(), "validation");
    }

    #[test]
    fn provider_input_validation_rejects_bad_ids() {
        let valid = || ProviderInput {
            id: Some("provider_1-OK".into()),
            name: "OpenAI".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: Some("https://api.example.com/v1".into()),
            model: None,
            codex_path: None,
            enabled: true,
        };

        assert_eq!(
            validate_provider_input(valid()).unwrap().id.as_deref(),
            Some("provider_1-OK")
        );

        for id in ["", "  ", "provider:openai", "bad/id", "bad\nid"] {
            let mut input = valid();
            input.id = Some(id.into());
            assert_eq!(validate_provider_input(input).unwrap_err().code(), "validation");
        }
    }
}
