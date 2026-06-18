//! Tauri 命令处理器 —— 前端与 Core 之间的薄边界层。
//!
//! 命令负责校验、委托给 Core 模块、并返回 serde 类型。它们绝不内嵌业务逻辑，
//! 也绝不记录或返回密钥。

use tauri::State;

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
    state.store.update_server(&id, input)
}

#[tauri::command]
pub fn delete_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
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
pub async fn check_ssh_connection(state: State<'_, AppState>, id: String) -> AppResult<bool> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    let ok = matches!(
        crate::ssh::check_connection(&server, secret.as_deref()).await,
        Ok(true)
    );
    let status = if ok { ServerStatus::Online } else { ServerStatus::Offline };
    state.store.set_server_status(&id, status, None)?;
    Ok(ok)
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
    crate::ssh::run_readonly(&server, secret.as_deref(), &command, crate::ssh::DEFAULT_TIMEOUT).await
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
    let status = if succeeded { ServerStatus::Online } else { ServerStatus::Offline };
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

/// 最近的审计记录（最新在前）。
#[tauri::command]
pub fn list_audit_records(state: State<'_, AppState>, limit: Option<u32>) -> AppResult<Vec<AuditRecord>> {
    state.store.list_audit_records(limit.unwrap_or(100))
}

/// 按 id 取单条审计记录，用于回放某次任务的细节。
#[tauri::command]
pub fn get_audit_record(state: State<'_, AppState>, id: String) -> AppResult<AuditRecord> {
    state.store.get_audit_record(&id)
}

/// 按顺序排好的、用于规划的 AI provider 候选：已启用、非 custom，且把策略里的
/// 默认 provider 排在最前。返回空 → 回退到离线 mock 引擎。这也是回退链——
/// 例如所选 Codex provider 的 turn 回路不可用时，会接着尝试任何已配置的
/// OpenAI 兼容 provider。
fn candidate_providers(state: &AppState) -> AppResult<Vec<ProviderConfig>> {
    let mut list: Vec<ProviderConfig> = state
        .store
        .list_providers()?
        .into_iter()
        .filter(|p| p.enabled && !matches!(p.kind, ProviderKind::Custom))
        .collect();
    if let Some(id) = state.store.get_policy()?.default_provider_id {
        list.sort_by_key(|p| usize::from(p.id != id)); // 默认 provider 排最前
    }
    Ok(list)
}

/// 把自然语言意图转成结构化、可审查的计划。依次尝试每个已配置的 AI provider
/// （默认优先）；若全部失败或一个都没配，就回退到离线 mock 引擎，确保 app 始终可用。
#[tauri::command]
pub async fn create_plan(
    state: State<'_, AppState>,
    intent: String,
    server_id: Option<String>,
) -> AppResult<Plan> {
    for provider in candidate_providers(&state)? {
        let key = provider
            .credential_ref
            .as_ref()
            .and_then(|r| state.credentials.get_secret(r).ok().flatten());
        let p = provider.clone();
        let intent2 = intent.clone();
        let sid = server_id.clone();
        // provider 调用是阻塞式 HTTP —— 放到 UI 线程之外跑。任务 panic 不能让
        // 整个命令崩溃：记录日志后继续尝试下一个候选 / mock。
        let joined = tokio::task::spawn_blocking(move || {
            crate::agent::plan_with_provider(&p, key, &intent2, sid.as_deref())
        })
        .await;
        match joined {
            Ok(Ok(plan)) => return Ok(plan),
            Ok(Err(e)) => eprintln!("[plan] provider '{}' failed ({}); trying next / mock", provider.name, e.code()),
            Err(e) => eprintln!("[plan] provider '{}' task panicked ({e}); trying next / mock", provider.name),
        }
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
        return Err(AppError::Blocked("plan requires a second confirmation".into()));
    }

    let mut executions = Vec::new();
    let mut failed = false;
    for (index, step) in plan.steps.iter().enumerate() {
        // 按「服务端重判的等级」而非过时的客户端 step.read_only 路由：用户编辑某步后，
        // step.read_only 可能已过时。Low 等级走只读路径（带 Low 校验门），其余走写路径。
        let res = if review.step_levels[index] == crate::core::types::RiskLevel::Low {
            crate::ssh::run_readonly(&server, secret.as_deref(), &step.command, crate::ssh::DEFAULT_TIMEOUT).await
        } else {
            crate::ssh::run_command(&server, secret.as_deref(), &step.command, crate::ssh::DEFAULT_TIMEOUT).await
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
            Err(_) => {
                failed = true;
                break;
            }
        }
    }

    let status = if failed { TaskStatus::Failed } else { TaskStatus::Completed };
    let intent = plan.goal.clone();
    let record = crate::audit::record_for_plan(Some(&server_id), &intent, plan, review, executions, status);
    state.store.insert_audit_record(&record)?;
    Ok(record)
}

/// 跑一次自主的、只读的诊断回合：模型通过只读的 AiPanel Tools 自行调查并返回
/// 总结。它无法修改服务器——写操作仍需走显式的「确认并执行」流程。
#[tauri::command]
pub async fn run_agent_turn(
    state: State<'_, AppState>,
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
    crate::agent::agent_loop::run_turn(&state, &provider, key, &intent, server_id.as_deref()).await
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
    Ok(tokio::task::spawn_blocking(move || crate::agent::test_provider(&config, key))
        .await
        .unwrap_or_else(|e| ProviderTestResult {
            ok: false,
            message: format!("测试任务失败: {e}"),
            detail: None,
        }))
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
    state.store.set_provider_model(&id, model.as_deref())
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
) -> AppResult<ProviderConfig> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("provider name is required".into()));
    }
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
    let credential_ref = if api_key.is_some() {
        Some(CredentialRef::for_provider(&id))
    } else {
        existing_ref
    };
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
        state.credentials.put_secret(&CredentialRef::for_provider(&config.id), &key)?;
    }
    Ok(config)
}

#[tauri::command]
pub fn delete_provider(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if let Ok(p) = state.store.get_provider(&id) {
        if let Some(reference) = &p.credential_ref {
            let _ = state.credentials.delete_secret(reference);
        }
    }
    state.store.delete_provider(&id)
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
    state.store.set_policy(&policy)
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
    let mut prepared: Vec<(String, ServerProfile, Option<String>)> = Vec::with_capacity(servers.len());
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
            let online = matches!(
                crate::ssh::check_connection(&server, secret.as_deref()).await,
                Ok(true)
            );
            (id, online)
        });
    }

    // 收集结果并各自落库；任务 panic 视为该服务器离线，但 id 已无从得知，故仅记录日志。
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((id, online)) => {
                let status = if online { ServerStatus::Online } else { ServerStatus::Offline };
                // facts 传 None：仅刷新连通状态，保留上次体检缓存的 facts。
                if let Err(e) = state.store.set_server_status(&id, status, None) {
                    eprintln!("[refresh] failed to persist status for server {id}: {}", e.code());
                }
            }
            Err(e) => eprintln!("[refresh] connectivity task panicked: {e}"),
        }
    }

    state.store.list_servers()
}
