# AiPanel 路线图

AiPanel 的目标不是复刻传统服务器面板，而是做一个本地运行的 AI 运维客户端。路线图会优先验证安全执行闭环，再扩展图形界面和应用部署。

## 当前进展（Desktop MVP）

实现路径改为**桌面端优先**（不做独立 CLI）。已落地的安全执行闭环与桌面客户端：服务器配置持久化（SQLite）、增删改、凭据进系统 Keychain、SSH 连通性测试、**流式**只读体检、命令风险分级（Low/Medium/High/Blocked）、写操作确认 / 高风险二次确认对话框、本地审计、AiPanel Tools 安全工具层、模型供应商管理。

**AI 已接通真实模型**：配置 OpenAI 兼容供应商后（只需 base URL + API Key，模型自动探测 `/v1/models` 并在首页选择），自然语言经真实 LLM 产出结构化计划（风险仍由 AiPanel 重判），执行前可**逐步编辑**计划，并可让 Agent 用只读工具自动诊断并总结；无供应商时回退到本地 Mock。**Codex app-server 的 turn / 工具调用回路已接通**：`CodexAppServerProvider` 的 chat/plan/summarize/stream_events 走真实 turn（`thread/start` → `turn/start` → 事件流 → 工具分发回灌），并作为首选 Agent Runtime、OpenAI 兼容为回退；该回路以模拟事件流单测覆盖（tool call / result / final / 错误 / 超时 / 子进程退出 / 写工具未确认即拒绝），并已用打包的真实 sidecar 验证 initialize / thread-start / turn-start 协议形状。完整模型轮次仍取决于用户配置的供应商 API Key、Base URL 与模型权限。**Docker 应用部署工作流已落地**（检测/安装、Compose 部署、Caddy/Nginx 反代 + HTTPS、部署后健康检查、5 个常用模板，全部生成结构化 Plan 并经风险审查与确认）。下面 M1–M5 多数条目已完成。

## M0：项目定义

- [x] 明确产品定位：本地 AI 运维客户端；
- [x] 明确差异点：服务器零常驻、SSH 优先、AI 先规划再执行；
- [x] 准备 README、中文文档、许可证和品牌素材；
- [x] 准备 GitHub 发布所需社区文件。

## M1：核心链路 MVP（以桌面端实现，非独立 CLI）

目标：验证核心链路。实现路径改为桌面端优先，以下能力均已在桌面客户端落地。

- [x] 本地配置目录（应用数据目录 + SQLite）；
- [x] 服务器连接配置；
- [x] SSH 连通性测试；
- [x] 系统基础信息采集；
- [x] 只读服务器体检（含流式 + 结构化指标）；
- [x] 命令输出采集（脱敏）；
- [x] 本地任务记录（运行历史）。

## M2：AI 任务规划

目标：让 AI 生成可审计计划，而不是直接执行命令。

- [x] 自然语言转结构化任务（真实 LLM / Codex turn，风险由 AiPanel 重判）；
- [x] 任务步骤拆分（执行前可逐步编辑、增删、上移下移）；
- [x] 命令风险分级（Low/Medium/High/Blocked）；
- [x] 用户确认流程（写操作确认、高风险二次确认）；
- [x] 执行结果总结（AI 只读自动诊断 + 总结）；
- [x] 失败原因分析（诊断回路按只读工具调查后给结论）。

## M3：安全执行器

目标：建立可信执行边界。

- [x] 只读模式（默认只读，非检查类命令升级为 Blocked）；
- [x] 危险命令识别（含 docker 子命令精确判定、SQL 破坏性语句等）；
- [x] 二次确认；
- [x] 命令白名单（只读体检 / `run_readonly` 仅放行 Low）；
- [x] 执行超时和中断（取消注册表 + 强制 tty 让远端收 SIGHUP）；
- [x] 审计日志（持久化 + 搜索 + 导出）；
- [x] 敏感信息脱敏（IP/密钥/Token/连接串）。

## M4：桌面客户端

目标：提供适合普通用户的本地客户端体验。

- [x] 服务器列表（+ 多服务器概览、收藏置顶、前台健康轮询告警）；
- [x] 任务对话界面（Codex 式三栏:控制台 / 文件树 / 终端）；
- [x] 执行计划预览（可逐步编辑后再执行）；
- [x] 实时命令输出（流式执行 + 交互式 SSH 终端）；
- [x] 任务历史（按服务器列出、可恢复/删除）；
- [x] 本地密钥和配置管理（Keychain + 供应商/模型设置）；
- [x] 服务器文件管理（SFTP:浏览/查看/编辑/上传下载）。

## M5：应用部署能力

目标：覆盖高频轻量服务器场景。

- [x] Docker 安装（检测 + 官方便捷脚本安装计划）；
- [x] Docker Compose 部署（生成 compose + 现场随机密码写 .env + `up -d`）；
- [x] Nginx / Caddy 反向代理；
- [x] HTTPS 证书配置（Caddy 自动签发 / Nginx + certbot）；
- [x] 应用健康检查（`compose ps` + HTTP 应用只读 curl）；
- [x] 常见应用模板：Uptime Kuma、n8n、WordPress、PostgreSQL、Redis。

> 以上均生成**结构化 Plan**，经现有 Risk Reviewer 与确认流程后执行（写步骤判为 Medium/High，触发确认/二次确认）。

## 暂不做

- 不做云端托管控制台；
- 不默认安装服务器常驻 agent；
- 不默认开放新的公网管理入口；
- 不直接执行未审查的 AI 命令；
- 不把用户服务器凭据上传到第三方服务。
