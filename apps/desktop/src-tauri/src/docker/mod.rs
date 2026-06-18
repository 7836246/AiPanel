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
//!
//! 这些计划面向「在服务器上准备/部署 Docker 应用」。compose / .env / 反代配置都落在
//! `/opt/aipanel/<slug>/` 之下，便于审计与回滚。

use serde::{Deserialize, Serialize};

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
      - "5432:5432"
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
      - "6379:6379"
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
    /// 注意：密码用 `$(openssl rand -base64 24)` 在服务器端生成并写入 .env，
    /// **不**在计划文本里硬编码任何真实密钥；非敏感字段（库名/用户名）用可读默认值。
    fn env_file_command(&self) -> Option<String> {
        let path = self.env_path();
        match self {
            AppTemplate::WordPress => Some(format!(
                "mkdir -p {dir} && cat > {path} <<EOF\n\
                 MYSQL_ROOT_PASSWORD=$(openssl rand -base64 24)\n\
                 MYSQL_DATABASE=wordpress\n\
                 MYSQL_USER=wordpress\n\
                 MYSQL_PASSWORD=$(openssl rand -base64 24)\n\
                 EOF",
                dir = self.dir(),
                path = path,
            )),
            AppTemplate::Postgres => Some(format!(
                "mkdir -p {dir} && cat > {path} <<EOF\n\
                 POSTGRES_PASSWORD=$(openssl rand -base64 24)\n\
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
    pub fn parse(s: &str) -> ReverseProxy {
        match s.trim().to_lowercase().as_str() {
            "caddy" => ReverseProxy::Caddy,
            "nginx" => ReverseProxy::Nginx,
            _ => ReverseProxy::None,
        }
    }
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
pub fn deploy_plan(server_id: &str, app: AppTemplate, opts: &DeployOptions) -> Plan {
    let dir = app.dir();
    let compose_path = app.compose_path();
    let port = app.port();

    let mut steps: Vec<PlanStep> = Vec::new();

    // 1) 准备应用目录。
    steps.push(step(
        format!("创建应用目录 {dir}"),
        format!("mkdir -p {dir}"),
    ));

    // 2) 写 docker-compose.yml（heredoc，幂等覆盖）。
    steps.push(step(
        format!("写入 {} 的 docker-compose.yml", app.display_name()),
        format!(
            "mkdir -p {dir} && cat > {compose_path} <<'EOF'\n{body}EOF",
            dir = dir,
            compose_path = compose_path,
            body = app.compose_yaml(),
        ),
    ));

    // 3) 若需要敏感值则写 .env（随机密码现场生成）。
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

    // 4) 启动应用。
    steps.push(step(
        format!("启动 {}（docker compose up -d）", app.display_name()),
        // 用 `cd <dir> && docker compose up -d`（而非 `-f <path>`）：Risk Reviewer 解析
        // docker 子命令时，`-f <file>` 这种带值短选项会把子命令吞成路径而误判为 Low；
        // `cd` 进目录后让 compose 自动发现 docker-compose.yml，子命令是 `up`，能被正确
        // 判为 Medium（状态变更）。
        format!("cd {dir} && docker compose up -d"),
    ));

    // 5) 反向代理（仅 HTTP 服务才有意义）。
    if app.is_http_service() {
        match opts.reverse_proxy {
            ReverseProxy::None => {}
            ReverseProxy::Caddy => append_caddy_steps(&mut steps, opts.domain.as_deref(), port),
            ReverseProxy::Nginx => append_nginx_steps(&mut steps, opts.domain.as_deref(), port),
        }
    }

    // 6) 部署后健康检查：compose ps + （HTTP 应用）只读 curl。
    steps.push(step(
        format!("检查 {} 容器运行状态", app.display_name()),
        format!("docker compose -f {compose_path} ps"),
    ));
    if app.is_http_service() {
        steps.push(step(
            format!("健康检查：访问本地端口 {port}（只读）"),
            format!("curl -fsS http://localhost:{port}/ || true"),
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

    make_plan(server_id, goal, steps)
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
            plans.push(deploy_plan("s", *app, &DeployOptions::default()));
            plans.push(deploy_plan(
                "s",
                *app,
                &DeployOptions {
                    domain: Some("example.com".into()),
                    reverse_proxy: ReverseProxy::Caddy,
                },
            ));
            plans.push(deploy_plan(
                "s",
                *app,
                &DeployOptions {
                    domain: Some("example.com".into()),
                    reverse_proxy: ReverseProxy::Nginx,
                },
            ));
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
            let p = deploy_plan("s1", *app, &DeployOptions::default());
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
    // WordPress / Postgres 需写 .env，且密码用 openssl rand 现场生成（无硬编码密钥）。
    fn secretful_apps_write_env_with_generated_passwords() {
        for app in [AppTemplate::WordPress, AppTemplate::Postgres] {
            let p = deploy_plan("s1", app, &DeployOptions::default());
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
        }
        // 不需要敏感值的应用不应写 .env。
        for app in [AppTemplate::UptimeKuma, AppTemplate::N8n, AppTemplate::Redis] {
            let p = deploy_plan("s1", app, &DeployOptions::default());
            assert!(!any_cmd_contains(&p, "openssl rand"), "{:?} 不应生成密码", app);
        }
    }

    #[test]
    // 计划文本中绝不出现任何看起来像真实密钥的硬编码值（除占位/随机生成方式）。
    fn no_hardcoded_real_secret_values() {
        for app in AppTemplate::ALL {
            let p = deploy_plan("s1", *app, &DeployOptions::default());
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
        let p = deploy_plan("s1", AppTemplate::UptimeKuma, &opts);
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
        let p = deploy_plan("s1", AppTemplate::N8n, &opts);
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
        let p = deploy_plan("s1", AppTemplate::WordPress, &opts);
        assert!(any_cmd_contains(&p, "proxy_pass"), "应含 nginx 反代配置");
        assert!(!any_cmd_contains(&p, "certbot"), "无 domain 不应申请证书");
    }

    #[test]
    // ReverseProxy::None：不含任何反代相关步骤。
    fn reverse_proxy_none_has_no_proxy_steps() {
        let p = deploy_plan("s1", AppTemplate::UptimeKuma, &DeployOptions::default());
        assert!(!any_cmd_contains(&p, "reverse_proxy"));
        assert!(!any_cmd_contains(&p, "proxy_pass"));
        assert!(!any_cmd_contains(&p, "Caddyfile"));
        assert!(!any_cmd_contains(&p, "certbot"));
    }

    #[test]
    // 数据库类应用（非 HTTP）不应有 curl 健康检查或反代步骤。
    fn database_apps_have_no_http_healthcheck_or_proxy() {
        for app in [AppTemplate::Postgres, AppTemplate::Redis] {
            // 即便请求了反代，数据库也不会加反代步骤（非 HTTP）。
            let opts = DeployOptions {
                domain: Some("db.example.com".into()),
                reverse_proxy: ReverseProxy::Caddy,
            };
            let p = deploy_plan("s1", app, &opts);
            assert!(!any_cmd_contains(&p, "curl"), "{:?} 不应有 curl 健康检查", app);
            assert!(!any_cmd_contains(&p, "reverse_proxy"), "{:?} 不应加反代", app);
        }
    }

    #[test]
    // HTTP 应用应有只读 curl 健康检查，且 curl 判为 Low。
    fn http_apps_have_readonly_curl_healthcheck() {
        for app in [AppTemplate::UptimeKuma, AppTemplate::N8n, AppTemplate::WordPress] {
            let p = deploy_plan("s1", app, &DeployOptions::default());
            let curl = p.steps.iter().find(|s| s.command.contains("curl"));
            assert!(curl.is_some(), "{:?} 应有 curl 健康检查", app);
            let curl = curl.unwrap();
            assert_eq!(curl.risk, RiskLevel::Low, "{:?} 的 curl 应为 Low", app);
            assert!(curl.read_only);
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
    // ReverseProxy::parse 与默认值。
    fn reverse_proxy_parse_and_default() {
        assert_eq!(ReverseProxy::parse("caddy"), ReverseProxy::Caddy);
        assert_eq!(ReverseProxy::parse("Nginx"), ReverseProxy::Nginx);
        assert_eq!(ReverseProxy::parse("none"), ReverseProxy::None);
        assert_eq!(ReverseProxy::parse("garbage"), ReverseProxy::None);
        assert_eq!(ReverseProxy::default(), ReverseProxy::None);
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
        assert_eq!(lvl("curl -fsS http://localhost:3001/ || true"), RiskLevel::Low);
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
        let p = deploy_plan("s1", AppTemplate::WordPress, &opts);
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
