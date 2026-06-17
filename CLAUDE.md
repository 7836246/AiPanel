# CLAUDE.md

本文件为 Claude Code（claude.ai/code）在本仓库中工作时提供指引。

## 当前状态

AiPanel 已是一个**可运行的 Desktop MVP**：桌面端能启动，左侧服务器列表来自持久化数据，可添加服务器（凭据进 Keychain）、做 SSH 连通性测试、执行只读体检、把自然语言转成结构化计划并经风险审查后执行、查看本地审计、配置模型供应商。下面记录的架构和技术选型都是**已经定下的决策**——直接遵循，不要重新推导。

Rust 后端按模块实现在 `apps/desktop/src-tauri/src/`（不拆独立 crate），边界见下方「后端结构」。所有涉及 SSH / 凭据 / 远程命令的改动必须符合 `docs/SECURITY_MODEL.zh-Hans.md`。

## 代码结构

pnpm workspace 单仓库（`pnpm-workspace.yaml`）：

- `apps/desktop` —— Tauri v2 桌面端。前端 React 19 + Vite 7 + TS（`src/`），Rust 后端在 `src-tauri/`。当前 `src/App.tsx` 是用组件库搭的演示界面。
- `packages/ui`（`@aipanel/ui`）—— 设计系统。Tailwind v4 + 设计 token（`src/styles/tokens.css` 里的 `@theme`），用 `cva` 做变体、`cn`（clsx + tailwind-merge）合并类名。primitives（Button/Badge/Card/Input/Textarea/Spinner/CodeBlock/Dialog）+ 领域组件（RiskBadge/ServerCard/CommandPlan/AuditEntry）。详见 `packages/ui/README.md`。

样式集成方式：组件库只用 Tailwind 工具类（不写零散 CSS），桌面端通过 `@import "@aipanel/ui/tokens.css"` 共享 token，并用 `@source` 指向 `packages/ui/src` 让 Tailwind 扫到组件类。组件库 `pnpm build` 出的 `dist/`（编译后的组件 + `styles.css`）也是以后 `/design-sync` 导入时消费的产物。

## 后端结构（`apps/desktop/src-tauri/src/`）

模块化实现，`lib.rs` 只组装 `AppState`（`store` / `credentials` / `plan_engine`）并注册命令——不放业务逻辑。

- `core/` —— `types.rs`（全部 serde `camelCase`，跨前后端）、`error.rs`（`AppError` 带稳定 code）、`sanitize.rs`（输出脱敏：IP/私钥/token/连接串）。
- `store/` —— SQLite（rusqlite bundled）：服务器、供应商、模型策略、审计的持久化 + 迁移（`user_version`）。**只存非敏感数据 + 凭据引用**。
- `credentials/` —— `CredentialStore` trait + 系统 Keychain 实现 + 内存 mock 兜底。**密钥只在这里**，绝不进 SQLite/日志/审计。
- `risk/` —— Risk Reviewer：把命令分级 Low/Medium/High/Blocked；只读模式把非检查命令升级为 Blocked。
- `ssh/` —— 用系统 OpenSSH 执行；超时、脱敏、临时密钥文件 0600；`run_readonly` 受风险审查门控（仅 Low）。
- `doctor/` —— 只读体检（10 条探测命令）生成 `DoctorReport`。
- `audit/` —— 从体检/计划执行构建审计记录（持久化在 store）。
- `plan/` —— `PlanEngine` trait + `MockPlanEngine`（关键词路由，仅产出只读诊断）。
- `agent/` —— `AgentProvider` trait + Mock / OpenAI 兼容 / Codex app-server 桥接（入口 + 健康检查；JSON-RPC 工具回路待接通）。
- `tools/` —— AiPanel Tools：Agent 唯一能触达服务器的入口（`server.list`/`server.info`/`server.doctor.readonly`/`ssh.run_readonly`/`task.plan`/`task.review`/`task.execute_confirmed`/`audit.write`），每个工具带权限与审计策略；写操作需用户确认，Agent 不能自行授权。
- `commands/` —— Tauri 命令薄层（前端 ↔ Core）：`list/create/update/delete/get_server`、`set_server_secret`、`check_ssh_connection`、`run_readonly_command`、`server_doctor_plan`、`run_server_doctor`、`review_plan`、`create_plan`、`execute_confirmed_plan`、`list/get_audit_records`、`list/save/delete/test_provider`、`get/save_model_selection_policy`、`list_tools`、`credential_backend`、`app_version`。

前端通过 `apps/desktop/src/lib/api.ts` 调用这些命令（不在 Tauri 环境时回退到 mock，便于 `pnpm dev` 在浏览器里渲染）。主界面 `src/screens/CodexConsole.tsx`（+ `SettingsPanel`/`AddServerDialog`）。

## 常用命令

在仓库根目录运行（pnpm 10）：

- `pnpm install` —— 安装依赖。
- `pnpm build:ui` —— 构建组件库（tsup 出 JS/类型，tailwind CLI 出 CSS）。**改完组件库后、跑桌面端前先跑这个。**
- `pnpm dev` —— 启动桌面端前端（Vite，仅浏览器，不带 Tauri 壳）。
- `pnpm tauri:dev` —— 启动完整 Tauri 桌面应用（需要 Rust 工具链）。
- `pnpm build` —— 先构建组件库，再构建桌面端前端。
- `pnpm typecheck` —— 全仓库 TS 类型检查。
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
