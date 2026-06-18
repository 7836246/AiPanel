//! Docker 应用部署工作流的**计划生成引擎**。
//!
//! 与 [`crate::doctor`] 一样，本模块只负责把一次「Docker 操作」表达成一份结构化
//! [`Plan`]（goal + steps），交给现有的 Risk Reviewer + 用户确认 + 执行链路。
//! 本模块**绝不**自己执行命令、绝不触碰 SSH、绝不读写凭据——它只产出可审查的计划。
//!
//! 安全约束（与 docs/SECURITY_MODEL.zh-Hans.md 一致）：
//! - 每个 [`PlanStep`] 的风险等级由 [`crate::risk::classify_command`] 判定，**不**手填，
//!   保证与统一的安全闸门完全一致；`read_only` 仅在 Low 时为 true。
//! - 命令里**绝不**硬编码真实密钥：数据库密码等敏感值一律用占位符（`CHANGE_ME_*`）
//!   或 `openssl rand -base64 24` 现场生成写入 `.env`，并在 summary 中提示用户务必替换/保管。
//! - 写文件统一用 `mkdir -p` + `cat > <path> <<'EOF' ... EOF` heredoc（幂等、可重复执行）。
//! - 部署用 `cd <dir> && docker compose up -d`：注意 Risk Reviewer 解析 docker 子命令时，
//!   `-f <file>` 这种带值短选项会把子命令误吞成路径而把 `up -d` 判为 Low；改用 `cd` 进目录
//!   后让 compose 自动发现 docker-compose.yml，子命令是 `up`，可被正确判为 Medium。
//! - 部署前先做应用端口占用预检，以 `ss` 只读检查暴露，避免执行到
//!   `docker compose up -d` 后才发现端口冲突。反代的 80/443 可能已由 Caddy/Nginx
//!   正常监听，不能简单按“端口占用”阻断。
//! - 部署前预检 compose 里声明的 `container_name` 是否已存在；容器名是 Docker 全局唯一
//!   资源，冲突时必须在写文件/启动前停住。
//!
//! 这些计划面向「在服务器上准备/部署 Docker 应用」。compose / .env / 反代配置都落在
//! `/opt/aipanel/<slug>/` 之下，便于审计与回滚。

use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, AppResult};
use crate::core::types::{new_id, now, Plan, PlanStep, RiskLevel};

/// 所有由本模块生成的文件统一落在此根目录下，按应用 slug 分子目录。
const BASE_DIR: &str = "/opt/aipanel";

/// 构造单个 [`PlanStep`]：summary（中文，这步做什么）+ command（可直接在服务器执行）。
///
/// 风险等级**始终**由 [`crate::risk::classify_command`] 判定（不手填），`read_only`
/// 仅在 Low 时为 true，`tool` 恒为 None（这些是裸命令，交由现有执行链路按等级路由）。
fn step(summary: impl Into<String>, command: impl Into<String>) -> PlanStep {
    let command = command.into();
    let risk = crate::risk::classify_command(&command).level;
    PlanStep {
        summary: summary.into(),
        command,
        risk,
        read_only: risk == RiskLevel::Low,
        tool: None,
    }
}

/// 用 goal + steps 组装一份 [`Plan`]（与 [`crate::doctor::doctor_plan`] 风格一致）。
fn make_plan(server_id: &str, goal: impl Into<String>, steps: Vec<PlanStep>) -> Plan {
    Plan {
        id: new_id(),
        server_id: Some(server_id.to_string()),
        goal: goal.into(),
        steps,
        created_at: now(),
    }
}

/// 生成只读端口占用检查步骤。端口空闲时成功；若有监听则打印占用行并以非 0 退出，
/// 让执行链路在写 compose / 启动容器 / reload 代理前停止。
fn port_check_step(port: u16, summary: impl Into<String>) -> PlanStep {
    step(
        summary,
        format!(
            "used=$(ss -ltnH 'sport = :{port}'); if [ -n \"$used\" ]; then echo \"port {port} is already in use:\"; echo \"$used\"; exit 1; fi"
        ),
    )
}

/// 生成只读容器名冲突检查步骤。容器名存在时以非 0 退出，避免 `docker compose up -d`
/// 到一半才因全局 container_name 冲突失败。
fn container_name_check_step(name: &str) -> PlanStep {
    step(
        format!("预检 Docker 容器名 {name} 是否已存在（不存在才可继续部署）"),
        format!(
            "existing=$(docker ps -a --filter name=^{name}$ --format '{{{{.Names}}}}') || {{ echo '无法查询 docker(未安装/daemon 未运行/无权限),无法预检容器名'; exit 1; }}; if [ -n \"$existing\" ]; then echo \"container name {name} already exists:\"; echo \"$existing\"; exit 1; fi"
        ),
    )
}

// ---------------------------------------------------------------------------
// 1. 只读：检测 Docker 环境
// ---------------------------------------------------------------------------

/// 只读检查 Docker 运行环境：版本、compose 插件、daemon 是否在运行、当前用户是否在
/// docker 组。全部应被 Risk Reviewer 判为 Low / 只读。
pub fn detect_docker_plan(server_id: &str) -> Plan {
    let steps = vec![
        step("检查 Docker 是否已安装及版本", "docker --version"),
        step("检查 Docker Compose 插件版本", "docker compose version"),
        step("检查 Docker daemon 是否在运行", "systemctl is-active docker"),
        step("检查当前用户所属的用户组（是否在 docker 组）", "id -nG"),
    ];
    make_plan(
        server_id,
        "只读检测 Docker 环境：版本、Compose 插件、daemon 状态与用户组",
        steps,
    )
}

// ---------------------------------------------------------------------------
// 2. 安装 Docker（写操作）
// ---------------------------------------------------------------------------

/// 安装 Docker：官方便捷脚本 + 开机自启 + 将当前用户加入 docker 组。
///
/// 这些步骤会被 Risk Reviewer 判为较高风险 / 写操作（管脚本进 shell、enable 服务、
/// usermod），符合预期——执行前会经用户确认。
pub fn install_docker_plan(server_id: &str) -> Plan {
    let steps = vec![
        // 官方便捷脚本：`curl ... | sh` 会被判为 High（remote-script）。
        step(
            "使用 Docker 官方便捷脚本安装 Docker（curl 管入 sh，安装后请核对来源）",
            "curl -fsSL https://get.docker.com | sh",
        ),
        // enable --now：启动并设为开机自启（systemctl 状态变更，Medium）。
        step(
            "启用并立即启动 Docker 服务（开机自启）",
            "systemctl enable --now docker",
        ),
        // usermod -aG：把当前用户加入 docker 组（账户变更，High）；需重新登录生效。
        step(
            "把当前用户加入 docker 组（需重新登录后生效，之后免 sudo 运行 docker）",
            "usermod -aG docker $USER",
        ),
    ];
    make_plan(
        server_id,
        "安装 Docker：官方脚本安装、开机自启、将当前用户加入 docker 组",
        steps,
    )
}

// ---------------------------------------------------------------------------
// 3. 应用模板
// ---------------------------------------------------------------------------

/// 可一键部署的应用模板。线格式为 camelCase，与前端字符串路由对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppTemplate {
    /// Uptime Kuma —— 自托管的可用性监控面板。
    UptimeKuma,
    /// n8n —— 工作流自动化。
    N8n,
    /// WordPress + MySQL —— 内容管理系统。
    WordPress,
    /// PostgreSQL —— 关系型数据库。
    Postgres,
    /// Redis —— 内存键值存储。
    Redis,
}

impl AppTemplate {
    /// 全部模板，便于遍历 / 测试。
    pub const ALL: &'static [AppTemplate] = &[
        AppTemplate::UptimeKuma,
        AppTemplate::N8n,
        AppTemplate::WordPress,
        AppTemplate::Postgres,
        AppTemplate::Redis,
    ];

    /// 目录名 / compose 服务名用的 slug（稳定、URL/路径安全）。
    pub fn slug(&self) -> &'static str {
        match self {
            AppTemplate::UptimeKuma => "uptime-kuma",
            AppTemplate::N8n => "n8n",
            AppTemplate::WordPress => "wordpress",
            AppTemplate::Postgres => "postgres",
            AppTemplate::Redis => "redis",
        }
    }

    /// 该应用对外暴露 / 健康检查所用的主端口。
    pub fn port(&self) -> u16 {
        match self {
            AppTemplate::UptimeKuma => 3001,
            AppTemplate::N8n => 5678,
            AppTemplate::WordPress => 8080,
            AppTemplate::Postgres => 5432,
            AppTemplate::Redis => 6379,
        }
    }

    /// compose 中声明的固定容器名。Docker 容器名是全局唯一资源，部署前要预检冲突。
    fn container_names(&self) -> &'static [&'static str] {
        match self {
            AppTemplate::UptimeKuma => &["uptime-kuma"],
            AppTemplate::N8n => &["n8n"],
            AppTemplate::WordPress => &["wordpress", "wordpress-db"],
            AppTemplate::Postgres => &["postgres"],
            AppTemplate::Redis => &["redis"],
        }
    }

    /// 友好展示名（用于 plan 的 goal/summary）。
    pub fn display_name(&self) -> &'static str {
        match self {
            AppTemplate::UptimeKuma => "Uptime Kuma",
            AppTemplate::N8n => "n8n",
            AppTemplate::WordPress => "WordPress",
            AppTemplate::Postgres => "PostgreSQL",
            AppTemplate::Redis => "Redis",
        }
    }

    /// 是否为「HTTP 服务」——只有 HTTP 服务才适合做 `curl` 健康检查与反向代理。
    /// 数据库（Postgres/Redis）不是 HTTP，跳过 curl 健康检查与反代。
    fn is_http_service(&self) -> bool {
        matches!(
            self,
            AppTemplate::UptimeKuma | AppTemplate::N8n | AppTemplate::WordPress
        )
    }

    /// 从字符串解析模板（命令层用字符串路由时调用）。接受 camelCase（线格式）、
    /// slug、以及常见小写别名，便于前端 / CLI 灵活传入。
    pub fn parse(s: &str) -> Option<AppTemplate> {
        let k = s.trim().to_lowercase().replace(['_', '-', ' '], "");
        match k.as_str() {
            "uptimekuma" | "uptime" | "kuma" => Some(AppTemplate::UptimeKuma),
            "n8n" => Some(AppTemplate::N8n),
            "wordpress" | "wp" => Some(AppTemplate::WordPress),
            "postgres" | "postgresql" | "pg" => Some(AppTemplate::Postgres),
            "redis" => Some(AppTemplate::Redis),
            _ => None,
        }
    }

    /// 该应用的 docker-compose.yml 文本（heredoc body）。
    fn compose_yaml(&self) -> String {
        match self {
            AppTemplate::UptimeKuma => r#"services:
  uptime-kuma:
    image: louislam/uptime-kuma:1
    container_name: uptime-kuma
    restart: unless-stopped
    ports:
      - "3001:3001"
    volumes:
      - uptime-kuma:/app/data

volumes:
  uptime-kuma:
"#
            .to_string(),
            AppTemplate::N8n => r#"services:
  n8n:
    image: n8nio/n8n
    container_name: n8n
    restart: unless-stopped
    ports:
      - "5678:5678"
    environment:
      - N8N_HOST=localhost
      - N8N_PORT=5678
    volumes:
      - n8n_data:/home/node/.n8n

volumes:
  n8n_data:
"#
            .to_string(),
            // WordPress：密码等敏感值通过 ${...} 从 .env 注入，不在 compose 里硬编码。
            AppTemplate::WordPress => r#"services:
  wordpress:
    image: wordpress:latest
    container_name: wordpress
    restart: unless-stopped
    depends_on:
      - db
    ports:
      - "8080:80"
    environment:
      - WORDPRESS_DB_HOST=db
      - WORDPRESS_DB_NAME=${MYSQL_DATABASE}
      - WORDPRESS_DB_USER=${MYSQL_USER}
      - WORDPRESS_DB_PASSWORD=${MYSQL_PASSWORD}
    volumes:
      - wp_data:/var/www/html
  db:
    image: mysql:8
    container_name: wordpress-db
    restart: unless-stopped
    environment:
      - MYSQL_ROOT_PASSWORD=${MYSQL_ROOT_PASSWORD}
      - MYSQL_DATABASE=${MYSQL_DATABASE}
      - MYSQL_USER=${MYSQL_USER}
      - MYSQL_PASSWORD=${MYSQL_PASSWORD}
    volumes:
      - db_data:/var/lib/mysql

volumes:
  wp_data:
  db_data:
"#
            .to_string(),
            AppTemplate::Postgres => r#"services:
  postgres:
    image: postgres:16
    container_name: postgres
    restart: unless-stopped
    ports:
      - "127.0.0.1:5432:5432"
    environment:
      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
"#
            .to_string(),
            AppTemplate::Redis => r#"services:
  redis:
    image: redis:7
    container_name: redis
    restart: unless-stopped
    command: redis-server --appendonly yes
    ports:
      - "127.0.0.1:6379:6379"
    volumes:
      - redis_data:/data

volumes:
  redis_data:
"#
            .to_string(),
        }
    }

    /// 该应用是否需要 `.env`（含敏感值）。WordPress / Postgres 需要密码。
    fn needs_env(&self) -> bool {
        matches!(self, AppTemplate::WordPress | AppTemplate::Postgres)
    }

    /// 生成写 `.env` 的命令（用 `openssl rand` 现场生成随机密码 + 非敏感占位）。
    /// 返回 None 表示该应用不需要 .env。summary 会提示用户妥善保管/可自行替换。
    ///
    /// 注意：密码先生成到 shell 变量，并显式检查非空；若远端缺 `openssl` 或随机生成失败，
    /// 命令会在写 `.env` 前退出，避免静默写出空密码。计划文本里不硬编码真实密钥。
    fn env_file_command(&self) -> Option<String> {
        let path = self.env_path();
        match self {
            AppTemplate::WordPress => Some(format!(
                "umask 077; mkdir -p {dir} && mysql_root_password=$(openssl rand -base64 24) || exit 1; mysql_password=$(openssl rand -base64 24) || exit 1; [ -n \"$mysql_root_password\" ] && [ -n \"$mysql_password\" ] || exit 1; cat > {path} <<EOF\n\
                 MYSQL_ROOT_PASSWORD=$mysql_root_password\n\
                 MYSQL_DATABASE=wordpress\n\
                 MYSQL_USER=wordpress\n\
                 MYSQL_PASSWORD=$mysql_password\n\
                 EOF",
                dir = self.dir(),
                path = path,
            )),
            AppTemplate::Postgres => Some(format!(
                "umask 077; mkdir -p {dir} && postgres_password=$(openssl rand -base64 24) || exit 1; [ -n \"$postgres_password\" ] || exit 1; cat > {path} <<EOF\n\
                 POSTGRES_PASSWORD=$postgres_password\n\
                 EOF",
                dir = self.dir(),
                path = path,
            )),
            _ => None,
        }
    }

    /// 应用目录：`/opt/aipanel/<slug>`。
    fn dir(&self) -> String {
        format!("{BASE_DIR}/{}", self.slug())
    }

    /// compose 文件路径。
    fn compose_path(&self) -> String {
        format!("{}/docker-compose.yml", self.dir())
    }

    /// .env 文件路径。
    fn env_path(&self) -> String {
        format!("{}/.env", self.dir())
    }
}

// ---------------------------------------------------------------------------
// 4. 部署选项 / 反向代理
// ---------------------------------------------------------------------------

/// 反向代理方式。默认 `None`（不加反代步骤）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ReverseProxy {
    /// 不配置反向代理。
    #[default]
    None,
    /// Caddy —— 配置后自动签发并续期 HTTPS 证书（需有 domain）。
    Caddy,
    /// Nginx —— 反代到本地端口；HTTPS 需用 certbot 单独申请。
    Nginx,
}

impl ReverseProxy {
    /// 从字符串解析反代方式（命令层字符串路由）。未知值回退为 None。
    pub fn parse(s: &str) -> AppResult<ReverseProxy> {
        match s.trim().to_lowercase().as_str() {
            "" | "none" => Ok(ReverseProxy::None),
            "caddy" => Ok(ReverseProxy::Caddy),
            "nginx" => Ok(ReverseProxy::Nginx),
            _ => Err(AppError::Validation(
                "未知反向代理方式：仅支持 none、caddy、nginx".into(),
            )),
        }
    }
}

/// 校验并规范化部署域名。
///
/// 空值/纯空白按无域名处理；非空值必须是普通 FQDN，避免把未验证输入拼进
/// Caddyfile、Nginx 配置路径、server_name 或 certbot 命令。
pub fn normalize_domain(input: Option<String>) -> AppResult<Option<String>> {
    let Some(raw) = input else {
        return Ok(None);
    };
    let domain = raw.trim().to_ascii_lowercase();
    if domain.is_empty() {
        return Ok(None);
    }

    let invalid = || {
        AppError::Validation(
            "域名格式无效：请填写普通 FQDN，例如 app.example.com；不支持端口、路径、通配符、下划线或特殊字符"
                .into(),
        )
    };

    if domain.len() > 253
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || domain.contains("..")
    {
        return Err(invalid());
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.iter().any(|label| {
        label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }) {
        return Err(invalid());
    }

    Ok(Some(domain))
}

/// 部署选项：可选域名 + 反向代理方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeployOptions {
    /// 对外域名（用于反代 server 块与 HTTPS 证书）。无域名时跳过 HTTPS。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// 反向代理方式。
    #[serde(default)]
    pub reverse_proxy: ReverseProxy,
}

// ---------------------------------------------------------------------------
// 5. 部署计划
// ---------------------------------------------------------------------------

/// 为某个应用模板生成部署计划：建目录 → 写 compose（+ .env）→ `up -d` →（可选反代）
/// → 部署后健康检查。
///
/// - 敏感值（WordPress/Postgres 密码）通过 `.env` 提供，由 `openssl rand` 现场生成，
///   计划文本里不含任何真实密钥；summary 提示用户保管/替换。
/// - 反代：Caddy 自动 HTTPS；Nginx 给出 certbot 步骤（仅当有 domain）；None 不加反代步骤。
/// - 健康检查：`docker compose ... ps`，HTTP 应用再加只读 `curl`（判为 Low）。
pub fn deploy_plan(server_id: &str, app: AppTemplate, opts: &DeployOptions) -> AppResult<Plan> {
    let opts = DeployOptions {
        domain: normalize_domain(opts.domain.clone())?,
        reverse_proxy: opts.reverse_proxy,
    };
    if !app.is_http_service() && opts.reverse_proxy != ReverseProxy::None {
        return Err(AppError::Validation(format!(
            "{} 不是 HTTP 服务，不支持配置 Caddy/Nginx 反向代理",
            app.display_name()
        )));
    }
    let dir = app.dir();
    let compose_path = app.compose_path();
    let port = app.port();

    let mut steps: Vec<PlanStep> = Vec::new();

    // 0) 预检应用宿主端口，避免 up -d 后才发现端口冲突。
    steps.push(port_check_step(
        port,
        format!("预检宿主机端口 {port} 是否已被占用（为空表示可用）"),
    ));

    // 1) 预检固定容器名，避免 Docker 全局名称冲突导致 up -d 失败。
    for name in app.container_names() {
        steps.push(container_name_check_step(name));
    }

    // 2) 准备应用目录。
    steps.push(step(
        format!("创建应用目录 {dir}"),
        format!("mkdir -p {dir}"),
    ));

    // 3) 写 docker-compose.yml（heredoc，幂等覆盖）。
    steps.push(step(
        format!("写入 {} 的 docker-compose.yml", app.display_name()),
        format!(
            "mkdir -p {dir} && cat > {compose_path} <<'EOF'\n{body}EOF",
            dir = dir,
            compose_path = compose_path,
            body = app.compose_yaml(),
        ),
    ));

    // 4) 若需要敏感值则写 .env（随机密码现场生成）。
    if app.needs_env() {
        if let Some(cmd) = app.env_file_command() {
            steps.push(step(
                format!(
                    "写入 {} 的 .env（密码由 openssl 随机生成，请妥善保管，可自行替换为更强口令）",
                    app.display_name()
                ),
                cmd,
            ));
        }
    }

    // 5) 启动应用。
    steps.push(step(
        format!("启动 {}（docker compose up -d）", app.display_name()),
        // 用 `cd <dir> && docker compose up -d`（而非 `-f <path>`）：Risk Reviewer 解析
        // docker 子命令时，`-f <file>` 这种带值短选项会把子命令吞成路径而误判为 Low；
        // `cd` 进目录后让 compose 自动发现 docker-compose.yml，子命令是 `up`，能被正确
        // 判为 Medium（状态变更）。
        format!("cd {dir} && docker compose up -d"),
    ));

    // 6) 反向代理（仅 HTTP 服务才有意义）。
    if app.is_http_service() {
        match opts.reverse_proxy {
            ReverseProxy::None => {}
            ReverseProxy::Caddy => append_caddy_steps(&mut steps, opts.domain.as_deref(), port),
            ReverseProxy::Nginx => append_nginx_steps(&mut steps, opts.domain.as_deref(), port),
        }
    }

    // 7) 部署后健康检查：compose ps + （HTTP 应用）只读 curl。
    steps.push(step(
        format!("检查 {} 容器运行状态", app.display_name()),
        format!("docker compose -f {compose_path} ps"),
    ));
    if app.is_http_service() {
        steps.push(step(
            format!("健康检查：访问本地端口 {port}（只读）"),
            format!("curl -fsS --max-time 10 http://localhost:{port}/"),
        ));
    }

    let goal = match opts.reverse_proxy {
        ReverseProxy::None => format!("用 Docker Compose 部署 {}", app.display_name()),
        rp => {
            let proxy = match rp {
                ReverseProxy::Caddy => "Caddy 反向代理（自动 HTTPS）",
                ReverseProxy::Nginx => "Nginx 反向代理",
                ReverseProxy::None => unreachable!(),
            };
            match (&opts.domain, app.is_http_service()) {
                (Some(d), true) => {
                    format!("用 Docker Compose 部署 {} 并通过 {proxy} 暴露到 {d}", app.display_name())
                }
                _ => format!("用 Docker Compose 部署 {}（配置 {proxy}）", app.display_name()),
            }
        }
    };

    Ok(make_plan(server_id, goal, steps))
}

/// 追加 Caddy 反向代理步骤。Caddy 会为配置的 domain 自动签发并续期 HTTPS 证书。
/// 无 domain 时仅按端口反代（无自动 HTTPS，在 summary 中说明）。
fn append_caddy_steps(steps: &mut Vec<PlanStep>, domain: Option<&str>, port: u16) {
    let caddyfile = "/etc/caddy/Caddyfile";
    match domain {
        Some(domain) => {
            // `domain { reverse_proxy localhost:<port> }` —— Caddy 自动 HTTPS。
            let body = format!("{domain} {{\n    reverse_proxy localhost:{port}\n}}\n");
            steps.push(step(
                format!("写入 Caddyfile，将 {domain} 反代到 localhost:{port}（Caddy 自动签发 HTTPS 证书）"),
                format!("mkdir -p /etc/caddy && cat > {caddyfile} <<'EOF'\n{body}EOF"),
            ));
        }
        None => {
            // 无 domain：用 :80 站点反代，Caddy 不会为裸 IP 自动签发证书。
            let body = format!(":80 {{\n    reverse_proxy localhost:{port}\n}}\n");
            steps.push(step(
                format!("写入 Caddyfile，将 :80 反代到 localhost:{port}（未提供域名，跳过自动 HTTPS）"),
                format!("mkdir -p /etc/caddy && cat > {caddyfile} <<'EOF'\n{body}EOF"),
            ));
        }
    }
    // 重载 Caddy 使配置生效。
    steps.push(step(
        "重载 Caddy 使反向代理配置生效",
        format!("caddy reload --config {caddyfile} || systemctl reload caddy"),
    ));
}

/// 追加 Nginx 反向代理步骤。Nginx 自身不签发证书：有 domain 时追加 certbot 步骤申请
/// HTTPS；无 domain 时跳过 HTTPS（在 summary 中说明）。
fn append_nginx_steps(steps: &mut Vec<PlanStep>, domain: Option<&str>, port: u16) {
    // server_name：有 domain 用 domain，否则用 _（默认服务器）。
    let server_name = domain.unwrap_or("_");
    let site_name = domain.unwrap_or("aipanel-app");
    let conf_path = format!("/etc/nginx/conf.d/{site_name}.conf");
    let body = format!(
        "server {{\n    listen 80;\n    server_name {server_name};\n\n    location / {{\n        proxy_pass http://localhost:{port};\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n    }}\n}}\n"
    );
    steps.push(step(
        format!("写入 Nginx 反向代理配置（server_name={server_name}，反代到 localhost:{port}）"),
        format!("mkdir -p /etc/nginx/conf.d && cat > {conf_path} <<'EOF'\n{body}EOF"),
    ));
    // 校验配置后重载 Nginx。
    steps.push(step(
        "校验 Nginx 配置并重载",
        "nginx -t && systemctl reload nginx",
    ));
    // HTTPS：仅当提供了 domain 才用 certbot 申请证书；无 domain 时跳过。
    if let Some(domain) = domain {
        steps.push(step(
            format!("使用 certbot 为 {domain} 申请并安装 HTTPS 证书"),
            format!("certbot --nginx -d {domain}"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试辅助：取某条命令经 Risk Reviewer 判定的等级。
    fn lvl(cmd: &str) -> RiskLevel {
        crate::risk::classify_command(cmd).level
    }

    /// 测试辅助：plan 里是否存在满足谓词的步骤。
    fn any_step(plan: &Plan, pred: impl Fn(&PlanStep) -> bool) -> bool {
        plan.steps.iter().any(pred)
    }

    /// 测试辅助：plan 里任意步骤的命令包含子串。
    fn any_cmd_contains(plan: &Plan, needle: &str) -> bool {
        plan.steps.iter().any(|s| s.command.contains(needle))
    }

    /// 测试辅助：测试用例里的合法输入应能生成计划。
    fn deploy_plan_ok(server_id: &str, app: AppTemplate, opts: &DeployOptions) -> Plan {
        deploy_plan(server_id, app, opts).unwrap()
    }

    #[test]
    // detect 计划全部步骤应为 Low / 只读，且非空。
    fn detect_plan_is_all_read_only_low() {
        let p = detect_docker_plan("s1");
        assert!(!p.steps.is_empty());
        assert!(p.steps.iter().all(|s| s.read_only && s.risk == RiskLevel::Low));
        // 与 Risk Reviewer 判定一致。
        for s in &p.steps {
            assert_eq!(lvl(&s.command), RiskLevel::Low, "应为 Low: {}", s.command);
        }
        assert_eq!(p.server_id.as_deref(), Some("s1"));
    }

    #[test]
    // install 计划至少有一步非只读（安装脚本 / usermod 等写操作）。
    fn install_plan_has_non_read_only_steps() {
        let p = install_docker_plan("s1");
        assert!(!p.steps.is_empty());
        assert!(
            p.steps.iter().any(|s| !s.read_only),
            "安装计划应含非只读步骤"
        );
        // 具体：curl | sh 应为 High（remote-script），usermod 应为 High（account）。
        assert!(any_cmd_contains(&p, "get.docker.com"));
        let script_step = p.steps.iter().find(|s| s.command.contains("get.docker.com")).unwrap();
        assert!(!script_step.read_only);
        assert!(script_step.risk >= RiskLevel::Medium);
        let usermod_step = p.steps.iter().find(|s| s.command.contains("usermod")).unwrap();
        assert!(!usermod_step.read_only);
        assert_eq!(usermod_step.risk, RiskLevel::High);
    }

    #[test]
    // 每步的 risk / read_only 必须与 classify_command 完全一致（不手填）。
    fn every_step_risk_matches_classifier() {
        let mut plans = vec![detect_docker_plan("s"), install_docker_plan("s")];
        for app in AppTemplate::ALL {
            plans.push(deploy_plan_ok("s", *app, &DeployOptions::default()));
            if app.is_http_service() {
                plans.push(deploy_plan_ok(
                    "s",
                    *app,
                    &DeployOptions {
                        domain: Some("example.com".into()),
                        reverse_proxy: ReverseProxy::Caddy,
                    },
                ));
                plans.push(deploy_plan_ok(
                    "s",
                    *app,
                    &DeployOptions {
                        domain: Some("example.com".into()),
                        reverse_proxy: ReverseProxy::Nginx,
                    },
                ));
            }
        }
        for p in &plans {
            for s in &p.steps {
                let expected = lvl(&s.command);
                assert_eq!(s.risk, expected, "risk 应与分类一致: {}", s.command);
                assert_eq!(
                    s.read_only,
                    expected == RiskLevel::Low,
                    "read_only 应仅在 Low 时为真: {}",
                    s.command
                );
            }
        }
    }

    #[test]
    // 每个模板的部署计划非空，compose heredoc 含预期镜像名，且至少有一步 up -d。
    fn deploy_plan_includes_expected_image_and_up() {
        let cases: &[(AppTemplate, &str)] = &[
            (AppTemplate::UptimeKuma, "louislam/uptime-kuma"),
            (AppTemplate::N8n, "n8nio/n8n"),
            (AppTemplate::WordPress, "wordpress"),
            (AppTemplate::Postgres, "postgres"),
            (AppTemplate::Redis, "redis"),
        ];
        for (app, image) in cases {
            let p = deploy_plan_ok("s1", *app, &DeployOptions::default());
            assert!(!p.steps.is_empty(), "{:?} 计划不应为空", app);
            assert!(
                any_cmd_contains(&p, image),
                "{:?} 的 compose 应含镜像 {}",
                app,
                image
            );
            // 至少一步是 docker compose ... up -d。
            assert!(
                any_step(&p, |s| s.command.contains("docker compose")
                    && s.command.contains("up -d")),
                "{:?} 应含 up -d 步骤",
                app
            );
            // 至少一步是 compose ps 健康检查。
            assert!(
                any_cmd_contains(&p, "docker compose -f") && any_cmd_contains(&p, " ps"),
                "{:?} 应含 compose ps 健康检查",
                app
            );
        }
    }

    #[test]
    // 每个部署计划都应先只读预检应用宿主端口，避免 up -d 后才发现端口冲突。
    fn deploy_plan_checks_application_port_before_starting() {
        for app in AppTemplate::ALL {
            let p = deploy_plan_ok("s1", *app, &DeployOptions::default());
            let needle = format!("sport = :{}", app.port());
            let idx_port = p
                .steps
                .iter()
                .position(|s| s.command.contains(&needle))
                .expect("应包含应用端口预检");
            let idx_up = p
                .steps
                .iter()
                .position(|s| s.command.contains("docker compose") && s.command.contains("up -d"))
                .expect("应包含启动步骤");
            let check = &p.steps[idx_port];
            assert!(idx_port < idx_up, "端口预检必须早于启动步骤: {:?}", app);
            assert!(check.command.contains("exit 1"), "端口冲突时必须阻止后续步骤");
            assert_eq!(check.risk, RiskLevel::Low, "端口预检必须是只读 Low");
            assert!(check.read_only, "端口预检必须标记为只读");
        }
    }

    #[test]
    // 每个固定 container_name 都应在写文件/启动前预检；冲突时停止后续部署。
    fn deploy_plan_checks_container_names_before_writing_or_starting() {
        for app in AppTemplate::ALL {
            let p = deploy_plan_ok("s1", *app, &DeployOptions::default());
            let idx_write = p
                .steps
                .iter()
                .position(|s| s.command.contains("docker-compose.yml"))
                .expect("应包含写 compose 步骤");
            let idx_up = p
                .steps
                .iter()
                .position(|s| s.command.contains("docker compose") && s.command.contains("up -d"))
                .expect("应包含启动步骤");

            for name in app.container_names() {
                let exact_filter = format!("name=^{name}$");
                let idx_check = p
                    .steps
                    .iter()
                    .position(|s| s.command.contains(&exact_filter))
                    .unwrap_or_else(|| panic!("{:?} 应预检容器名 {name}", app));
                let check = &p.steps[idx_check];
                assert!(idx_check < idx_write, "容器名预检必须早于写 compose: {:?}", app);
                assert!(idx_check < idx_up, "容器名预检必须早于启动: {:?}", app);
                assert!(check.command.contains("docker ps -a"));
                assert!(check.command.contains("exit 1"), "容器名冲突时必须阻止后续步骤");
                assert_eq!(check.risk, RiskLevel::Low, "容器名预检必须是只读 Low");
                assert!(check.read_only);
            }
        }
    }

    #[test]
    // HTTP 反代计划不应因 80/443 已由代理服务监听而提前阻断；配置错误交给 reload/certbot 暴露。
    fn reverse_proxy_plans_do_not_block_existing_proxy_ports() {
        for reverse_proxy in [ReverseProxy::Caddy, ReverseProxy::Nginx] {
            let p = deploy_plan_ok(
                "s1",
                AppTemplate::UptimeKuma,
                &DeployOptions {
                    domain: Some("kuma.example.com".into()),
                    reverse_proxy,
                },
            );
            for port in [80, 443] {
                let needle = format!("sport = :{port}");
                assert!(
                    !p
                    .steps
                    .iter()
                    .any(|s| s.command.contains(&needle) && s.command.contains("exit 1")),
                    "反代计划不应把代理监听端口 {port} 当作阻断条件: {:?}",
                    reverse_proxy
                );
            }
        }
    }

    #[test]
    // WordPress / Postgres 需写 .env，且密码用 openssl rand 现场生成（无硬编码密钥）。
    fn secretful_apps_write_env_with_generated_passwords() {
        for app in [AppTemplate::WordPress, AppTemplate::Postgres] {
            let p = deploy_plan_ok("s1", app, &DeployOptions::default());
            // 有写 .env 的步骤。
            assert!(
                any_cmd_contains(&p, "/.env <<") || any_cmd_contains(&p, ".env <<EOF"),
                "{:?} 应写 .env",
                app
            );
            // 密码现场随机生成，而非硬编码。
            assert!(
                any_cmd_contains(&p, "openssl rand -base64"),
                "{:?} 的密码应由 openssl 随机生成",
                app
            );
            assert!(
                any_cmd_contains(&p, "|| exit 1") && any_cmd_contains(&p, "[ -n"),
                "{:?} 生成密码失败或为空时必须阻止写入 .env",
                app
            );
            assert!(
                !any_cmd_contains(&p, "PASSWORD=$(openssl rand"),
                "{:?} 不应在 .env heredoc 里直接展开 openssl，避免失败时写出空密码",
                app
            );
            // 安全回归:写含密码的 .env 前必须 umask 077,避免落地为世界可读(0644)。
            assert!(
                any_cmd_contains(&p, "umask 077"),
                "{:?} 写 .env 前应 umask 077,避免凭据文件世界可读",
                app
            );
        }
        // 不需要敏感值的应用不应写 .env。
        for app in [AppTemplate::UptimeKuma, AppTemplate::N8n, AppTemplate::Redis] {
            let p = deploy_plan_ok("s1", app, &DeployOptions::default());
            assert!(!any_cmd_contains(&p, "openssl rand"), "{:?} 不应生成密码", app);
        }
    }

    #[test]
    // 安全回归:容器名预检在 docker 查询本身失败(未装/daemon 未运行/无权限)时必须报错退出,
    // 而非静默通过(空结果当作「无冲突」)。
    fn container_name_precheck_fails_when_docker_query_fails() {
        for app in [AppTemplate::WordPress, AppTemplate::Postgres, AppTemplate::Redis] {
            let p = deploy_plan_ok("s1", app, &DeployOptions::default());
            for name in app.container_names() {
                let filter = format!("name=^{name}$");
                let check = p
                    .steps
                    .iter()
                    .find(|s| s.command.contains(&filter))
                    .unwrap_or_else(|| panic!("{:?} 应有容器名预检 {name}", app));
                // 命令子串失败应被 `|| { ...; exit 1; }` 捕获(而非忽略错误继续)。
                assert!(
                    check.command.contains("无法查询 docker") && check.command.contains("exit 1"),
                    "预检应在 docker 查询失败时报错退出: {}",
                    check.command
                );
                // 仍是只读 Low(不得因加固引入重定向等被升级)。
                assert_eq!(check.risk, RiskLevel::Low, "容器名预检应为 Low: {}", check.command);
            }
        }
    }

    #[test]
    // 计划文本中绝不出现任何看起来像真实密钥的硬编码值（除占位/随机生成方式）。
    fn no_hardcoded_real_secret_values() {
        for app in AppTemplate::ALL {
            let p = deploy_plan_ok("s1", *app, &DeployOptions::default());
            for s in &p.steps {
                // 出现密码相关字段时，要么是 ${...} 占位，要么是 openssl 生成，
                // 不能直接 `PASSWORD=<明文字面量>`。
                if s.command.contains("MYSQL_PASSWORD=") {
                    assert!(
                        s.command.contains("${MYSQL_PASSWORD}")
                            || s.command.contains("openssl rand"),
                        "MYSQL_PASSWORD 不应硬编码: {}",
                        s.command
                    );
                }
                if s.command.contains("POSTGRES_PASSWORD=") {
                    assert!(
                        s.command.contains("${POSTGRES_PASSWORD}")
                            || s.command.contains("openssl rand"),
                        "POSTGRES_PASSWORD 不应硬编码: {}",
                        s.command
                    );
                }
            }
        }
    }

    #[test]
    // Caddy + domain：含 reverse_proxy 且语义上自动 HTTPS（goal 提到 HTTPS）。
    fn caddy_with_domain_has_reverse_proxy_and_auto_https() {
        let opts = DeployOptions {
            domain: Some("kuma.example.com".into()),
            reverse_proxy: ReverseProxy::Caddy,
        };
        let p = deploy_plan_ok("s1", AppTemplate::UptimeKuma, &opts);
        assert!(any_cmd_contains(&p, "reverse_proxy"), "应含 reverse_proxy");
        assert!(any_cmd_contains(&p, "kuma.example.com"), "应含 domain");
        // Caddy 自动 HTTPS 的语义：summary 或 goal 提到自动签发 HTTPS。
        let mentions_https = p.goal.contains("HTTPS")
            || p.steps.iter().any(|s| s.summary.contains("HTTPS"));
        assert!(mentions_https, "Caddy 带 domain 应体现自动 HTTPS");
        // 不应出现 certbot（Caddy 自动签发，无需 certbot）。
        assert!(!any_cmd_contains(&p, "certbot"));
    }

    #[test]
    // Nginx + domain：含 nginx 反代配置且含 certbot 步骤。
    fn nginx_with_domain_has_certbot() {
        let opts = DeployOptions {
            domain: Some("n8n.example.com".into()),
            reverse_proxy: ReverseProxy::Nginx,
        };
        let p = deploy_plan_ok("s1", AppTemplate::N8n, &opts);
        assert!(any_cmd_contains(&p, "proxy_pass"), "应含 nginx proxy_pass");
        assert!(any_cmd_contains(&p, "n8n.example.com"), "应含 domain");
        assert!(any_cmd_contains(&p, "certbot --nginx -d n8n.example.com"), "应含 certbot 步骤");
    }

    #[test]
    // Nginx 无 domain：不应有 certbot 步骤（跳过 HTTPS）。
    fn nginx_without_domain_skips_certbot() {
        let opts = DeployOptions {
            domain: None,
            reverse_proxy: ReverseProxy::Nginx,
        };
        let p = deploy_plan_ok("s1", AppTemplate::WordPress, &opts);
        assert!(any_cmd_contains(&p, "proxy_pass"), "应含 nginx 反代配置");
        assert!(!any_cmd_contains(&p, "certbot"), "无 domain 不应申请证书");
    }

    #[test]
    // ReverseProxy::None：不含任何反代相关步骤。
    fn reverse_proxy_none_has_no_proxy_steps() {
        let p = deploy_plan_ok("s1", AppTemplate::UptimeKuma, &DeployOptions::default());
        assert!(!any_cmd_contains(&p, "reverse_proxy"));
        assert!(!any_cmd_contains(&p, "proxy_pass"));
        assert!(!any_cmd_contains(&p, "Caddyfile"));
        assert!(!any_cmd_contains(&p, "certbot"));
    }

    #[test]
    // 数据库类应用（非 HTTP）不应有 curl 健康检查，且请求反代时必须明确拒绝。
    fn database_apps_have_no_http_healthcheck_and_reject_proxy() {
        for app in [AppTemplate::Postgres, AppTemplate::Redis] {
            let p = deploy_plan_ok("s1", app, &DeployOptions::default());
            assert!(!any_cmd_contains(&p, "curl"), "{:?} 不应有 curl 健康检查", app);
            assert!(!any_cmd_contains(&p, "reverse_proxy"), "{:?} 默认不应加反代", app);

            let opts = DeployOptions {
                domain: Some("db.example.com".into()),
                reverse_proxy: ReverseProxy::Caddy,
            };
            let err = deploy_plan("s1", app, &opts).unwrap_err();
            assert_eq!(err.code(), "validation", "{:?} 请求反代应被拒绝", app);
        }
    }

    #[test]
    // 数据库类模板默认仅绑定本机回环地址，避免一键部署后把数据库端口直接暴露到公网。
    fn database_apps_bind_ports_to_loopback_only() {
        let postgres = deploy_plan_ok("s1", AppTemplate::Postgres, &DeployOptions::default());
        assert!(any_cmd_contains(&postgres, "\"127.0.0.1:5432:5432\""));
        assert!(!any_cmd_contains(&postgres, "\"5432:5432\""));

        let redis = deploy_plan_ok("s1", AppTemplate::Redis, &DeployOptions::default());
        assert!(any_cmd_contains(&redis, "\"127.0.0.1:6379:6379\""));
        assert!(!any_cmd_contains(&redis, "\"6379:6379\""));
    }

    #[test]
    // HTTP 应用应有只读 curl 健康检查，且 curl 判为 Low。
    fn http_apps_have_readonly_curl_healthcheck() {
        for app in [AppTemplate::UptimeKuma, AppTemplate::N8n, AppTemplate::WordPress] {
            let p = deploy_plan_ok("s1", app, &DeployOptions::default());
            let curl = p.steps.iter().find(|s| s.command.contains("curl"));
            assert!(curl.is_some(), "{:?} 应有 curl 健康检查", app);
            let curl = curl.unwrap();
            assert_eq!(curl.risk, RiskLevel::Low, "{:?} 的 curl 应为 Low", app);
            assert!(curl.read_only);
            assert!(
                !curl.command.contains("|| true"),
                "{:?} 的健康检查失败时必须让计划执行失败",
                app
            );
        }
    }

    #[test]
    // AppTemplate::parse 往返正确（含 slug / camelCase / 别名）。
    fn template_parse_round_trips() {
        for app in AppTemplate::ALL {
            // slug 能解析回自身。
            assert_eq!(AppTemplate::parse(app.slug()), Some(*app), "slug 往返: {:?}", app);
        }
        // camelCase 线格式（serde）也能解析。
        assert_eq!(AppTemplate::parse("uptimeKuma"), Some(AppTemplate::UptimeKuma));
        assert_eq!(AppTemplate::parse("wordPress"), Some(AppTemplate::WordPress));
        // 常见别名。
        assert_eq!(AppTemplate::parse("pg"), Some(AppTemplate::Postgres));
        assert_eq!(AppTemplate::parse("postgresql"), Some(AppTemplate::Postgres));
        assert_eq!(AppTemplate::parse("kuma"), Some(AppTemplate::UptimeKuma));
        assert_eq!(AppTemplate::parse("wp"), Some(AppTemplate::WordPress));
        // 未知值返回 None。
        assert_eq!(AppTemplate::parse("nope"), None);
    }

    #[test]
    // AppTemplate 的 serde 线格式为 camelCase（与前端字符串对齐）。
    fn template_serde_camel_case() {
        assert_eq!(
            serde_json::to_value(AppTemplate::UptimeKuma).unwrap(),
            serde_json::json!("uptimeKuma")
        );
        assert_eq!(
            serde_json::to_value(AppTemplate::WordPress).unwrap(),
            serde_json::json!("wordPress")
        );
    }

    #[test]
    // ReverseProxy::parse 与默认值；未知值必须显式拒绝，避免静默生成与用户选择不一致的计划。
    fn reverse_proxy_parse_and_default() {
        assert_eq!(ReverseProxy::parse("caddy").unwrap(), ReverseProxy::Caddy);
        assert_eq!(ReverseProxy::parse("Nginx").unwrap(), ReverseProxy::Nginx);
        assert_eq!(ReverseProxy::parse("none").unwrap(), ReverseProxy::None);
        assert_eq!(ReverseProxy::parse("").unwrap(), ReverseProxy::None);
        assert_eq!(ReverseProxy::parse("garbage").unwrap_err().code(), "validation");
        assert_eq!(ReverseProxy::default(), ReverseProxy::None);
    }

    #[test]
    // 部署域名进入 shell / Nginx / Caddy / certbot 前必须先规范化。
    fn normalize_domain_accepts_fqdn_and_blank() {
        assert_eq!(normalize_domain(None).unwrap(), None);
        assert_eq!(normalize_domain(Some("   ".into())).unwrap(), None);
        assert_eq!(
            normalize_domain(Some(" App.Example.COM ".into())).unwrap(),
            Some("app.example.com".into())
        );
        assert_eq!(
            normalize_domain(Some("xn--fsqu00a.xn--0zwm56d".into())).unwrap(),
            Some("xn--fsqu00a.xn--0zwm56d".into())
        );
    }

    #[test]
    // 拒绝可破坏 shell、Caddyfile、Nginx server_name/conf path 或 certbot 参数边界的输入。
    fn normalize_domain_rejects_invalid_or_injectable_values() {
        let long_label = format!("{}.example.com", "a".repeat(64));
        let long_domain = format!("{}.com", "a".repeat(250));
        let cases = [
            "localhost",
            "*.example.com",
            ".example.com",
            "example.com.",
            "example..com",
            "-bad.example.com",
            "bad-.example.com",
            "exa_mple.com",
            "bad name.com",
            "bad/name.com",
            r"bad\name.com",
            "example.com:443",
            "example.com; touch /tmp/pwn",
            "example.com && reboot",
            "example.com | sh",
            "example.com $HOME",
            "example.com `id`",
            "example.com 'quoted'",
            "example.com \"quoted\"",
            "example.com\nreverse_proxy localhost:1",
            long_label.as_str(),
            long_domain.as_str(),
        ];

        for case in cases {
            assert!(normalize_domain(Some(case.into())).is_err(), "应拒绝非法域名: {case:?}");
        }
    }

    #[test]
    // 非法域名必须在生成计划前被拒绝，不能进入 Caddy/Nginx/certbot 命令文本。
    fn rejected_domain_values_do_not_reach_deploy_commands() {
        for rejected in ["example.com; touch /tmp/pwn", "example.com\nserver_name _;"] {
            let normalized = normalize_domain(Some(rejected.into()));
            assert!(normalized.is_err());
            let opts = DeployOptions {
                domain: Some(rejected.into()),
                reverse_proxy: ReverseProxy::Nginx,
            };
            assert!(
                deploy_plan("s1", AppTemplate::UptimeKuma, &opts).is_err(),
                "核心部署计划入口也必须拒绝未规范化的危险域名"
            );
        }

        let opts = DeployOptions {
            domain: normalize_domain(Some(" App.Example.COM ".into())).unwrap(),
            reverse_proxy: ReverseProxy::Nginx,
        };
        let p = deploy_plan_ok("s1", AppTemplate::UptimeKuma, &opts);
        assert!(any_cmd_contains(&p, "app.example.com"));
        assert!(!any_cmd_contains(&p, "App.Example.COM"));
    }

    #[test]
    // 抽样断言：分类一致性符合预期（up -d / heredoc 写文件为非 Low；curl/ps/version 为 Low）。
    fn sampled_risk_expectations() {
        // 非 Low：实际部署用 `cd <dir> && docker compose up -d`（见下方说明）。
        assert!(lvl("cd /opt/aipanel/redis && docker compose up -d") >= RiskLevel::Medium);
        assert!(lvl("mkdir -p /opt/aipanel/n8n && cat > /opt/aipanel/n8n/docker-compose.yml <<'EOF'\nx\nEOF") >= RiskLevel::Medium);
        // 分类器已修复(risk/mod.rs 改为扫描子命令 token,不再被 `-f` 带值选项坑到):
        // `docker compose -f <file> up -d` 现在正确判为 Medium。`cd <dir> &&` 形式同样为 Medium。
        assert!(lvl("docker compose -f /opt/aipanel/redis/docker-compose.yml up -d") >= RiskLevel::Medium);
        // Low（只读）：
        assert_eq!(lvl("curl -fsS --max-time 10 http://localhost:3001/"), RiskLevel::Low);
        assert_eq!(lvl("docker compose -f /opt/aipanel/redis/docker-compose.yml ps"), RiskLevel::Low);
        assert_eq!(lvl("docker --version"), RiskLevel::Low);
        assert_eq!(lvl("docker compose version"), RiskLevel::Low);
        assert_eq!(lvl("systemctl is-active docker"), RiskLevel::Low);
        assert_eq!(lvl("id -nG"), RiskLevel::Low);
    }

    #[test]
    // 所有部署/反代步骤写文件都用 mkdir -p + heredoc（幂等、可重复执行）。
    fn write_steps_use_mkdir_and_heredoc() {
        let opts = DeployOptions {
            domain: Some("example.com".into()),
            reverse_proxy: ReverseProxy::Nginx,
        };
        let p = deploy_plan_ok("s1", AppTemplate::WordPress, &opts);
        for s in &p.steps {
            // 凡是带 heredoc 写文件的步骤，都应先 mkdir -p。
            if s.command.contains("<<'EOF'") || s.command.contains("<<EOF") {
                assert!(
                    s.command.contains("mkdir -p"),
                    "写文件步骤应先 mkdir -p: {}",
                    s.command
                );
            }
        }
    }
}
