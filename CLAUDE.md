# CLAUDE.md

本文件为 Claude Code（claude.ai/code）在本仓库中工作时提供指引。

## 当前状态

AiPanel 已是一个**可运行的 Desktop MVP，真实 AI 已接通，主界面无任何 mock**：桌面端能启动，左侧服务器列表来自持久化数据，可添加/编辑/删除服务器（凭据进 Keychain）、做 SSH 连通性测试、执行**流式**只读体检（终端逐行刷新）、用自然语言生成结构化计划（配置 OpenAI 兼容供应商后是**真实 LLM 规划**，否则按 provider 回退链直至本地 Mock）、执行前可**逐步编辑计划**（改命令/摘要、增删、上移下移，编辑后由风险闸门**重判**、Blocked/空计划禁止执行）、经风险审查弹**确认/二次确认**对话框、**流式执行**计划（按服务端重判等级路由每步），让 AI 用只读工具**自动诊断**并总结。每次运行（计划/诊断/体检）都作为 **TaskRecord 持久化**，左侧按服务器列出运行历史、可点开恢复或删除；首屏在无服务器/无供应商时给真实引导。还提供**多服务器概览**（卡片+结构化指标+收藏置顶+前台健康轮询与告警角标）、所选服务器的**实时监控**（顶栏按钮下拉的「图层」面板:系统信息 facts + CPU/内存/磁盘/负载环形仪表 + 容器/服务/端口/进程计数 + 流量曲线,每 3s 走 SSH 只读采集、服务器零常驻 agent,鼠标悬停看详情）、所选服务器的**交互式 SSH 终端**(xterm.js + 本地 PTY)与**文件管理**(SFTP:列目录/查看/编辑/上传下载;终端与文件均为用户操作、不暴露给 AI)、可见的**连接/重连**流程、**Docker 应用部署工作流**（检测/安装、Compose 部署、Caddy/Nginx 反代 + HTTPS、部署后健康检查,模板 Uptime Kuma/n8n/WordPress/PostgreSQL/Redis;均生成结构化 Plan 走风险审查 + 确认 + 执行）,以及查看本地审计、配置模型供应商（只需 base URL + API Key,模型自动探测 `/v1/models`、首页可选)。Codex app-server 走 JSON-RPC/stdio 桥接：transport + initialize 之上**turn / 工具调用回路已接通**（`thread/start`→`turn/start`→事件流→工具分发回灌,以模拟 JSON-RPC 事件流单测覆盖,待对真实 `codex` 二进制端到端验证），Codex 为首选 Agent Runtime、OpenAI 兼容为回退,失败按 provider 链回退。还支持**应用内在线更新**(Tauri updater 插件 + GitHub Releases `latest.json` + minisign 签名;`vX.Y.Z` tag 管理版本,`scripts/bump-version.sh` 同步各处版本,`release.yml` 多平台构建签名发布;设置「在线更新」可检查/下载/安装/重启 + 启动静默检查;公钥入库、私钥只在本地与 GitHub Secret;详见 `docs/RELEASE.zh-Hans.md`)。下面记录的架构和技术选型都是**已经定下的决策**——直接遵循，不要重新推导。

Rust 后端按模块实现在 `apps/desktop/src-tauri/src/`（不拆独立 crate），边界见下方「后端结构」。所有涉及 SSH / 凭据 / 远程命令的改动必须符合 `docs/SECURITY_MODEL.zh-Hans.md`。

## 代码结构

pnpm workspace 单仓库（`pnpm-workspace.yaml`）：

- `apps/desktop` —— Tauri v2 桌面端。前端 React 19 + Vite 7 + TS（`src/`），Rust 后端在 `src-tauri/`。当前 `src/App.tsx` 是用组件库搭的演示界面。
- `packages/ui`（`@aipanel/ui`）—— 设计系统。Tailwind v4 + 设计 token（`src/styles/tokens.css` 里的 `@theme`），用 `cva` 做变体、`cn`（clsx + tailwind-merge）合并类名。primitives（Button/IconButton/Badge/Card/Input/Textarea/Switch/Select/Spinner/CodeBlock/Terminal/Dialog/Toast）+ 领域组件（RiskBadge/ServerCard/CommandPlan/AuditEntry）。详见 `packages/ui/README.md`。

样式集成方式：组件库只用 Tailwind 工具类（不写零散 CSS），桌面端通过 `@import "@aipanel/ui/tokens.css"` 共享 token，并用 `@source` 指向 `packages/ui/src` 让 Tailwind 扫到组件类。组件库 `pnpm build` 出的 `dist/`（编译后的组件 + `styles.css`）也是以后 `/design-sync` 导入时消费的产物。

## 后端结构（`apps/desktop/src-tauri/src/`）

模块化实现，`lib.rs` 只组装 `AppState`（`store` / `credentials` / `plan_engine`）并注册命令——不放业务逻辑。

- `core/` —— `types.rs`（全部 serde `camelCase`，跨前后端）、`error.rs`（`AppError` 带稳定 code）、`sanitize.rs`（输出脱敏：IP/私钥/token/连接串）。
- `store/` —— SQLite（rusqlite bundled）：服务器（含 `favorite` 收藏，v3 迁移加列、收藏置顶）、供应商、模型策略、**任务/运行历史（TaskRecord，v2 迁移加 `data` 列）**、审计的持久化 + 迁移（`user_version`，当前 3）。**只存非敏感数据 + 凭据引用**。
- `credentials/` —— `CredentialStore` trait + 系统 Keychain 实现 + 内存 mock 兜底。**密钥只在这里**，绝不进 SQLite/日志/审计。
- `risk/` —— Risk Reviewer：把命令分级 Low/Medium/High/Blocked；只读模式把非检查命令升级为 Blocked。
- `ssh/` —— 用系统 OpenSSH 执行；超时、脱敏、临时密钥文件 0600；`run_readonly` 受风险审查门控（仅 Low）。`build_invocation`/`spawn_child` 为阻塞版 `run_command`/`run_readonly` 与流式版 `run_readonly_streamed`/`run_command_streamed` 共用；流式版有 `*_cancellable` 变体 + 模块级取消注册表（`register`/`cancel`/`unregister`），并加 `-tt` 使远端命令随客户端断开收 SIGHUP（真正中断）。
- `doctor/` —— 只读体检（多条探测命令）生成 `DoctorReport`，并解析出**结构化指标**（内存 used/total、根分区 %、负载、服务/容器/端口数）与友好 facts；`run_doctor_streamed` + `DoctorStreamEvent` 提供流式版本（与阻塞版共用 `build_report`/`record_probe`）。
- `metrics/` —— **实时监控采集（SSH 只读、服务器零 agent）**：`collect` 用一条复合只读命令一次性取 CPU(前后两帧 /proc/stat 增量)/负载/内存/磁盘/网络累计/uptime/容器·服务·端口·进程计数,纯函数 `parse_metrics` 解析为 `ServerMetrics`,任何字段缺失安全降级 0 不 panic;输出缺分段标记时报错而非静默全 0。网络/磁盘速率由前端跨样本求差。
- `terminal/` —— **交互式 SSH 终端**（用户操作,不暴露给 AI）：portable-pty 在本地 PTY 内 spawn 交互式 `ssh`（复用三种认证),会话注册表 + `open`/`write`/`resize`/`close`,读线程把输出回调出来并在 EOF/通道断开时自我回收会话。
- `files/` —— **文件管理（SFTP over SSH,用户操作,不暴露给 AI）**：`list`（`find -printf`,回退 `ls`)/`read`（`head -c` 256K 截断)/`write`（`cat>` 经 stdin)/`upload`/`download`（`ssh::run_scp`）。
- `audit/` —— 从体检/计划执行构建审计记录（持久化在 store）。
- `plan/` —— `PlanEngine` trait + `MockPlanEngine`（关键词路由，仅产出只读诊断，离线兜底）。
- `agent/` —— `AgentProvider` trait + 实现：`OpenAiCompatibleProvider`（真实 `/chat/completions`：chat/plan/summarize，plan 用结构化 JSON 输出且**风险由 AiPanel 重新判定、不信模型**;另有 `list_models` 探测 `/v1/models`）、`MockAgentProvider`、`CodexAppServerProvider`（chat/plan/summarize/stream_events 走真实 turn）。`agent/codex.rs` 是 Codex app-server 的 JSON-RPC/stdio transport（spawn + initialize，只暴露 AiPanel Tools）**+ turn/工具回路**：`CodexClient::run_turn`（`thread/start`→`turn/start`→事件流）与传输无关的 `drive_turn`/`classify_event`（tool_call→`on_tool` 分发→`tool/result` 回灌;以模拟事件流单测覆盖,写工具授权仍由工具层把关）。`agent/agent_loop.rs` 是**只读自动诊断回路**（OpenAI function-calling，只给只读工具，写操作永不暴露给自动回路）。
- `mcp/` —— **AiPanel MCP 服务器**(`aipanel mcp-server` 子命令,见 `lib.rs` 入口分流):stdio MCP 协议(initialize/tools/list/tools/call),只暴露**只读** server-ops 工具、复用 `tools::dispatch`(跨进程经 `AIPANEL_DATA_DIR` 共享 SQLite/Keychain);写/执行类绝不暴露。由 codex 按注入的 `mcp_servers` 配置拉起,使 codex 经 MCP 做带工具的只读诊断。
- `docker/` —— **Docker 部署计划引擎**（只产出结构化 Plan,不执行）：`detect_docker_plan`/`install_docker_plan`/`deploy_plan` + `AppTemplate`（Uptime Kuma/n8n/WordPress/Postgres/Redis）+ `ReverseProxy`（None/Caddy/Nginx）;写 compose/.env（密码 `openssl rand` 现场生成,无硬编码）、`docker compose up -d`、反代 + HTTPS、健康检查,每步风险由 `classify_command` 判定,走现有审查 + 确认 + 执行链路。
- `tools/` —— AiPanel Tools：Agent 唯一能触达服务器的入口（`server.list`/`server.info`/`server.doctor.readonly`/`ssh.run_readonly`/`task.plan`/`task.review`/`task.execute_confirmed`/`audit.write`），每个工具带权限与审计策略；写操作需用户确认，Agent 不能自行授权。
- `commands/` —— Tauri 命令薄层（前端 ↔ Core）：`list/create/update/delete/get_server`、`set_server_secret`、`set_server_favorite`、`refresh_all_servers`（并发连通刷新）、`check_ssh_connection`、`run_readonly_command`、`server_doctor_plan`、`run_server_doctor`、`commands/stream.rs` 里的 `run_server_doctor_stream` 与 `run_confirmed_plan_stream`（经 `tauri::ipc::Channel` 流式,后者服务端再审查并强制确认级别；均带 `run_id`）、`cancel_run`（中断流式任务）、`review_plan`、`create_plan`（按 provider 回退链:默认→其它已启用→Mock）、`execute_confirmed_plan`、`run_agent_turn`（只读自动诊断）、`commands/tasks.rs` 里的 `list/get/save/delete_task`（运行历史）、`commands/search.rs` 里的 `search_audit_records`/`search_tasks`/`export_audit_json`、`commands/terminal.rs` 里的 `terminal_open`/`write`/`resize`/`close`(交互式终端)、`commands/files.rs` 里的 `fs_list`/`fs_read`/`fs_write`/`fs_upload`/`fs_download`(文件管理)、`commands/docker.rs` 里的 `docker_detect_plan`/`docker_install_plan`/`docker_deploy_plan`(Docker 部署计划)、`list/get_audit_records`、`list/save/delete/test_provider`、`list_models`/`set_provider_model`(模型探测/激活)、`get/save_model_selection_policy`、`list_tools`、`credential_backend`、`app_version`。

前端通过 `apps/desktop/src/lib/api.ts` 调用这些命令（不在 Tauri 环境时回退到 mock，便于 `pnpm dev` 在浏览器里渲染）。主界面 `src/screens/CodexConsole.tsx`（+ `SettingsPanel`/`AddServerDialog`）。

## 常用命令

在仓库根目录运行（pnpm 10）：

- `pnpm install` —— 安装依赖。
- `pnpm build:ui` —— 构建组件库（tsup 出 JS/类型，tailwind CLI 出 CSS）。**改完组件库后、跑桌面端前先跑这个。**
- `pnpm dev` —— 启动桌面端前端（Vite，仅浏览器，不带 Tauri 壳）。
- `pnpm tauri:dev` —— 启动完整 Tauri 桌面应用（需要 Rust 工具链）。
- `pnpm build` —— 先构建组件库，再构建桌面端前端。
- `pnpm typecheck` —— 全仓库 TS 类型检查。
- `pnpm --filter @aipanel/desktop test` —— 前端 vitest 单测（纯逻辑:格式化、风险展示映射、api mock 路径、settingsKeys）。
- `pnpm ci:check` —— 一站式门禁:typecheck + 前端 vitest + Rust 测试 + Codex sidecar 集成 + Clippy(-D warnings) + 前端构建。**提交前跑这个。**
- Rust 侧检查：`cd apps/desktop/src-tauri && cargo check`。
- Rust 测试：`cd apps/desktop/src-tauri && cargo test`（Core/Store/Risk/SSH/Doctor/Audit/Plan/Agent/Tools 单测）。运行单个：`cargo test risk`。

## AiPanel 是什么

一个面向 Linux 服务器的**本地优先的 AI 运维客户端**。它运行在用户自己的机器上，通过 SSH 管理远程服务器——刻意做到**服务器上没有常驻面板进程**，也不新增公网管理入口。它把自然语言请求转换成可审查的计划，经批准后通过 SSH 执行操作，并在本地保留审计记录。

## 架构（已确定）

核心原则：**Codex 负责 Agent / 对话运行时，AiPanel 负责服务器运维的安全边界。** 这明确**不是**对 Codex CLI 的套壳。

```
AiPanel Desktop  (Tauri v2 + React + TypeScript)
      | JSON-RPC / stdio
Codex App Server  (Agent Runtime：多轮对话、上下文、模型选择、流式)
      | tool calls
AiPanel Tools  (MCP / JSON-RPC：server.list、server.doctor.readonly、ssh.run_readonly、task.plan/review/execute_confirmed、audit.write)
      |
AiPanel Core  (Risk Reviewer / SSH Executor / Audit Log / SQLite / Keychain)
      |
Remote Server
```

技术栈：Tauri v2 + React + TypeScript（桌面端）；Codex app-server 作为 Agent Runtime，通过 JSON-RPC/stdio 通信；SQLite 存放非敏感配置和审计索引；系统 Keychain 存放凭据。SSH 执行、风险审查、审计均由 AiPanel 自己实现——**绝不**交给 Codex。

`codex exec` 刻意**不**作为主链路（它适合非交互的 CI 脚本，不适合长时间运行的桌面端对话运行时）。

## 不可妥协的安全边界

这些约束决定每一个功能的设计。拿不准时，默认选择更严格的方案。

- **Codex 永远不持有 SSH 凭据，永远不跑裸 shell。** 它只能通过 AiPanel 审核过的工具触达服务器能力。不得暴露任何无限制的 shell 工具。
- **AI 的输出是计划，不是事实。** Codex 的自然语言计划必须先转换成结构化 Plan（目标、步骤、每步的命令/工具、是否只读、风险等级、预期输出、失败处理），并通过 Risk Reviewer 之后才能执行。
- **默认只读。** 诊断模式只允许检查类命令（`uname`、`df`、`free`、`ss`、`ps`、`systemctl status`、`docker ps`、限量的 `journalctl`、读取配置）。
- **写操作必须用户明确确认；高风险操作必须二次确认。** 风险等级：Low（只读）/ Medium（可恢复的状态变更）/ High（数据丢失、服务中断、安全边界变化）/ Blocked（例如 `rm -rf /`、格式化系统盘、清空生产数据库、关闭 SSH 且无回滚方案）。
- **凭据（SSH 密钥/密码、sudo 密码、API Key、数据库密码）只存在本地 Keychain**——绝不提交、绝不明文记录、绝不发送给 AI，除非用户明确授权且确有必要。界面上默认脱敏展示。
- **发送给 AI 或写入审计记录前先脱敏**：IP、Token、密码、Cookie、Authorization 头、私钥、数据库连接串、云厂商 AK/SK。
- **每次执行都在本地审计**：用户意图、AI 计划、风险判定、确认、实际命令、退出码、脱敏后的输出、总结。绝不记录密钥。

完整的命令审查模式和风险分类见 `docs/SECURITY_MODEL.zh-Hans.md`——它是安全边界的权威来源。

## 文档索引

权威文档为 `docs/` 下的**简体中文**版本（英文 `README.md` 只是摘要）：

- `docs/ARCHITECTURE.zh-Hans.md` —— 组件设计、数据边界
- `docs/TECH_STACK.zh-Hans.md` —— 技术选型及理由、供应商/模型选择
- `docs/SECURITY_MODEL.zh-Hans.md` —— 风险等级、命令审查、脱敏（改动执行/凭据相关代码前必读）
- `docs/ROADMAP.zh-Hans.md` —— 里程碑

## 约定

- **提交信息**：使用 Conventional Commits 风格（`feat:`、`fix:`、`docs:`、`chore:`），保持简短。
- **涉及命令执行、凭据存储或远程变更的 PR** 必须说明采用了什么风险控制机制（见 `CONTRIBUTING.md`）。
- **许可证为 AGPL-3.0**——贡献的代码默认遵循 AGPL-3.0。
