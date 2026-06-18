//! Docker 应用部署工作流的命令薄层(前端 ↔ Core)。
//!
//! 这些命令只**生成结构化 Plan**——不执行任何东西。生成的计划仍走现有
//! `review_plan` → 用户确认 → `run_confirmed_plan_stream` 链路:部署里的写步骤
//! (写 compose、`docker compose up -d`、改反代/证书)会被 Risk Reviewer 判为
//! Medium/High,正常触发确认/二次确认(见 docs/SECURITY_MODEL.zh-Hans.md)。

use tauri::State;

use crate::core::error::{AppError, AppResult};
use crate::core::types::Plan;
use crate::docker::{self, AppTemplate, DeployOptions, ReverseProxy};
use crate::AppState;

/// 只读探测:目标服务器是否装了 Docker / Compose / 用户是否在 docker 组。
#[tauri::command]
pub fn docker_detect_plan(state: State<'_, AppState>, server_id: String) -> AppResult<Plan> {
    state.store.get_server(&server_id)?; // 确认服务器存在
    Ok(docker::detect_docker_plan(&server_id))
}

/// 安装 Docker 的计划(官方便捷脚本 + 启用服务 + 加入 docker 组;写操作需确认)。
#[tauri::command]
pub fn docker_install_plan(state: State<'_, AppState>, server_id: String) -> AppResult<Plan> {
    state.store.get_server(&server_id)?;
    Ok(docker::install_docker_plan(&server_id))
}

/// 部署某应用模板的计划(写 compose/.env、`docker compose up -d`、可选反代+HTTPS、部署后健康检查)。
#[tauri::command]
pub fn docker_deploy_plan(
    state: State<'_, AppState>,
    server_id: String,
    app: String,
    domain: Option<String>,
    reverse_proxy: String,
) -> AppResult<Plan> {
    state.store.get_server(&server_id)?;
    let app = AppTemplate::parse(&app)
        .ok_or_else(|| AppError::Validation(format!("未知应用模板: {app}")))?;
    let opts = DeployOptions {
        domain: docker::normalize_domain(domain)?,
        reverse_proxy: ReverseProxy::parse(&reverse_proxy)?,
    };
    docker::deploy_plan(&server_id, app, &opts)
}
