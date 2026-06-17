//! Tauri command handlers — the thin boundary between the frontend and Core.
//!
//! Commands validate, delegate to Core modules, and return serde types. They
//! never embed business logic and never log or return secrets.

use tauri::State;

use crate::core::error::{AppError, AppResult};
use crate::core::types::{
    AuditRecord, CommandExecution, CredentialRef, DoctorReport, ModelSelectionPolicy, Plan,
    ProviderConfig, ProviderInput, ProviderKind, ProviderTestResult, RiskReview, ServerInput,
    ServerProfile, ServerStatus, TaskStatus,
};
use crate::AppState;

/// Resolve a server and its SSH secret (if its auth method stores one).
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
    // Remove the secret first so deleting a server never orphans a credential.
    if let Ok(profile) = state.store.get_server(&id) {
        if let Some(reference) = &profile.credential_ref {
            let _ = state.credentials.delete_secret(reference);
        }
    }
    state.store.delete_server(&id)
}

/// Store an SSH secret (password or private key) for a server. The secret goes
/// straight to the credential store and is never logged, persisted to SQLite, or
/// written to the audit log.
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

/// Which credential backend is active ("keychain" or "mock"), so the UI can warn
/// when secrets are only in memory.
#[tauri::command]
pub fn credential_backend(state: State<'_, AppState>) -> String {
    state.credentials.backend().to_string()
}

/// Review a plan's risk. `readOnlyMode` escalates any non-inspection step to
/// Blocked. Pure function — no side effects, no state needed.
#[tauri::command]
pub fn review_plan(plan: Plan, read_only_mode: bool) -> RiskReview {
    crate::risk::review_plan(&plan, read_only_mode)
}

/// Test SSH connectivity + auth, caching the result as the server's status.
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

/// Run a single read-only command (gated by the Risk Reviewer). Developer/diagnostic
/// entry point; the user-facing flow goes through the Server Doctor and plans.
#[tauri::command]
pub async fn run_readonly_command(
    state: State<'_, AppState>,
    id: String,
    command: String,
) -> AppResult<CommandExecution> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    crate::ssh::run_readonly(&server, secret.as_deref(), &command, crate::ssh::DEFAULT_TIMEOUT).await
}

/// The read-only plan the doctor would run, for previewing before execution.
#[tauri::command]
pub fn server_doctor_plan(state: State<'_, AppState>, id: String) -> AppResult<Plan> {
    state.store.get_server(&id)?; // ensure it exists
    Ok(crate::doctor::doctor_plan(&id))
}

/// Run the read-only server doctor, caching status + quick facts on the server.
#[tauri::command]
pub async fn run_server_doctor(state: State<'_, AppState>, id: String) -> AppResult<DoctorReport> {
    let (server, secret) = load_server_and_secret(&state, &id)?;
    let plan = crate::doctor::doctor_plan(&id);
    let review = crate::risk::review_plan(&plan, true); // doctor runs in read-only mode
    let report = crate::doctor::run_doctor(&server, secret.as_deref()).await?;

    let succeeded = report.executions.iter().any(|e| e.exit_code == 0);
    let status = if succeeded { ServerStatus::Online } else { ServerStatus::Offline };
    let facts = crate::doctor::facts_from_report(&report);
    state.store.set_server_status(&id, status, Some(&facts))?;

    // Every execution is audited locally.
    let record = crate::audit::record_for_doctor(&id, plan, review, &report);
    state.store.insert_audit_record(&record)?;

    Ok(report)
}

/// Most recent audit records (newest first).
#[tauri::command]
pub fn list_audit_records(state: State<'_, AppState>, limit: Option<u32>) -> AppResult<Vec<AuditRecord>> {
    state.store.list_audit_records(limit.unwrap_or(100))
}

/// One audit record by id, for replaying a task's detail.
#[tauri::command]
pub fn get_audit_record(state: State<'_, AppState>, id: String) -> AppResult<AuditRecord> {
    state.store.get_audit_record(&id)
}

/// The provider to use for planning: the policy default if enabled, else the
/// first enabled provider, else None (→ fall back to the offline mock engine).
fn pick_provider(state: &AppState) -> AppResult<Option<ProviderConfig>> {
    let enabled: Vec<ProviderConfig> =
        state.store.list_providers()?.into_iter().filter(|p| p.enabled).collect();
    if enabled.is_empty() {
        return Ok(None);
    }
    if let Some(id) = state.store.get_policy()?.default_provider_id {
        if let Some(p) = enabled.iter().find(|p| p.id == id) {
            return Ok(Some(p.clone()));
        }
    }
    Ok(enabled.into_iter().next())
}

/// Turn a natural-language intent into a structured, reviewable plan. Uses the
/// configured AI provider when available; falls back to the offline mock engine
/// if none is configured or the provider call fails (so the app always works).
#[tauri::command]
pub fn create_plan(
    state: State<'_, AppState>,
    intent: String,
    server_id: Option<String>,
) -> AppResult<Plan> {
    if let Some(provider) = pick_provider(&state)? {
        if !matches!(provider.kind, ProviderKind::Custom) {
            let key = provider
                .credential_ref
                .as_ref()
                .and_then(|r| state.credentials.get_secret(r).ok().flatten());
            match crate::agent::plan_with_provider(&provider, key, &intent, server_id.as_deref()) {
                Ok(plan) => return Ok(plan),
                Err(e) => eprintln!("[plan] provider '{}' failed ({}); falling back to mock", provider.name, e.code()),
            }
        }
    }
    state.plan_engine.create_plan(&intent, server_id.as_deref())
}

/// Execute a plan the user confirmed. The plan is ALWAYS re-reviewed server-side
/// (never trust the client): blocked steps are rejected, and the required
/// confirmation level is enforced before anything runs. Every run is audited.
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
    for step in &plan.steps {
        let res = if step.read_only {
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

/// Run an autonomous, read-only diagnosis turn: the model investigates via the
/// read-only AiPanel Tools and returns a summary. It cannot change servers —
/// writes still require the explicit confirm-and-execute flow.
#[tauri::command]
pub async fn run_agent_turn(
    state: State<'_, AppState>,
    intent: String,
    server_id: Option<String>,
) -> AppResult<crate::agent::agent_loop::AgentTurnResult> {
    let provider = pick_provider(&state)?
        .ok_or_else(|| AppError::Provider("未配置可用的模型供应商".into()))?;
    if !matches!(provider.kind, ProviderKind::OpenAiCompatible) {
        return Err(AppError::Provider("自动诊断目前仅支持 OpenAI 兼容供应商".into()));
    }
    let key = provider
        .credential_ref
        .as_ref()
        .and_then(|r| state.credentials.get_secret(r).ok().flatten());
    crate::agent::agent_loop::run_turn(&state, &provider, key, &intent, server_id.as_deref()).await
}

/// Test an agent provider config (validity / reachability) without saving it.
/// The API key comes from the call (a key being typed in the form) or, failing
/// that, from the credential store for an already-saved provider.
#[tauri::command]
pub fn test_provider(
    state: State<'_, AppState>,
    config: ProviderConfig,
    api_key: Option<String>,
) -> ProviderTestResult {
    let key = api_key.or_else(|| {
        config
            .credential_ref
            .as_ref()
            .and_then(|r| state.credentials.get_secret(r).ok().flatten())
    });
    crate::agent::test_provider(&config, key)
}

/// The AiPanel Tools surface the agent may call (names, permissions, audit policy).
#[tauri::command]
pub fn list_tools() -> Vec<crate::tools::ToolSpec> {
    crate::tools::registry()
}

// ----- providers / model selection ---------------------------------------

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> AppResult<Vec<ProviderConfig>> {
    state.store.list_providers()
}

/// Create or update a provider. The API key (if any) goes straight to the
/// credential store; only a CredentialRef is persisted in SQLite.
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
