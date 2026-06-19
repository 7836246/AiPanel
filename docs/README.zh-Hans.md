<p align="center"><a href="#"><img src="../assets/logo-white-bg.png" alt="AiPanel" width="420" /></a></p>
<p align="center"><b>Local AI Server Operations Client</b></p>
<p align="center"><b>本地运行、通过 SSH 管理服务器的 AI 运维客户端</b></p>

<p align="center">
  <a href="#"><img src="https://img.shields.io/badge/项目状态-桌面%20MVP-0F766E" alt="项目状态"></a>
  <a href="#"><img src="https://img.shields.io/badge/平台-macOS%20%7C%20Windows%20%7C%20Linux-0284C7" alt="平台"></a>
  <a href="#"><img src="https://img.shields.io/badge/连接方式-SSH%20优先-16A34A" alt="SSH 优先"></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/许可证-AGPL--3.0-64748B" alt="许可证：AGPL-3.0"></a>
</p>

<p align="center">
  <a href="/README.md"><img alt="English" src="https://img.shields.io/badge/English-d9d9d9"></a>
  <a href="/docs/README.zh-Hans.md"><img alt="中文(简体)" src="https://img.shields.io/badge/中文(简体)-d9d9d9"></a>
</p>

![AiPanel 预览图](../assets/preview.png)
<p align="center"><sub>本地优先的桌面客户端——添加服务器、跑只读体检、把需求转成可审查计划，再经 SSH 执行。</sub></p>

------------------------------

## 什么是 AiPanel？

AiPanel 是一个**本地优先的 Linux 服务器 AI 运维客户端**。它运行在你自己的电脑上，通过 SSH 管理远程服务器——刻意做到**服务器上没有常驻面板进程**，也不新增公网管理入口。自然语言需求会被转成可审查的计划，确认后经 SSH 执行，全程在本地审计。

核心原则:**Codex 负责 Agent / 对话运行时,AiPanel 负责服务器运维的安全边界。** AI 只产出计划——它永远不持有 SSH 凭据,也永远不跑裸 shell。

## 亮点

- 🔌 **服务器零常驻** —— 通过标准 SSH 管理,VPS 上不装、不留任何常驻进程。
- 🧠 **AI 出计划、你来批准** —— 自然语言 → 结构化计划(目标、步骤、命令、风险等级)→ 审查 → 执行。
- 🛡️ **默认只读** —— 安全诊断 CPU / 内存 / 磁盘 / 端口 / 服务 / Docker / Nginx / 日志 / 防火墙;写操作需明确确认,高风险需二次确认。
- 📊 **实时监控与概览** —— 环形仪表 + 流量曲线,每 3s 走只读 SSH 采集;多服务器概览带健康角标。
- 🖥️ **交互式工作区** —— xterm.js SSH 终端 + SFTP 文件管理,均为用户操作、绝不暴露给 AI。
- 🐳 **部署工作流** —— 安装 Docker、部署 Compose 应用、Caddy/Nginx 反代 + HTTPS、部署后健康检查。
- 🔑 **凭据只在本地** —— SSH 密钥、密码、API Key 只存系统 Keychain;输出在送达 AI 或写入审计前先脱敏。
- 📝 **本地审计** —— 意图、计划、风险判定、确认、命令、退出码、脱敏输出、总结,全部留在本机。

## 为什么做 AiPanel？

1Panel、宝塔等面板已经很好地解决了服务器图形化管理问题,但它们的共同特点是:管理程序通常运行在服务器上。对于 1C1G、2C2G 等轻量 VPS,常驻面板会带来额外成本——占资源、要维护面板自身、要暴露/保护额外入口,面板本身也成为新的安全边界。

AiPanel 更适合:有多台轻量服务器、不想每台装面板;习惯用自然语言描述目标而非翻插件菜单;希望先诊断、再确认、后执行;希望保留命令级审计;希望服务器默认保持干净,只在需要时执行任务。

## 工作原理

```
AiPanel Desktop   (Tauri v2 + React + TypeScript)
      │ JSON-RPC / stdio
Codex App Server  (Agent 运行时:多轮对话、上下文、模型选择、流式)
      │ 工具调用
AiPanel Tools     (server.list · server.doctor.readonly · ssh.run_readonly ·
                   task.plan / review / execute_confirmed · audit.write)
      │
AiPanel Core      (风险审查 · SSH 执行 · 审计 · SQLite · Keychain)
      │
远程服务器
```

SSH 执行、风险审查、脱敏、审计都由 AiPanel 自己实现——**绝不**交给 AI。Codex 只能经 AiPanel 审核过的工具触达服务器,不暴露任何无限制 shell。

## 安全边界(不可妥协)

- AI 永远不持有 SSH 凭据、永远不跑裸 shell——只有 AiPanel 审核过的工具能触达服务器。
- AI 的输出是**计划,不是事实**——必须先过风险审查才能执行。
- **默认只读**;写操作需明确确认,破坏性操作需二次确认;`rm -rf /` 一类命令直接 **Blocked**。
- 凭据(SSH 密钥/密码、sudo、API Key、数据库密码)只存本地 Keychain——绝不提交、记录或发给 AI。
- IP、Token、密钥、连接串在送达 AI 或写入审计前先脱敏。

权威的命令审查与风险分级见[安全模型](./SECURITY_MODEL.zh-Hans.md)。

## 快速开始

AiPanel 当前处于**桌面端 MVP** 阶段——基于 Tauri v2 + React,在本地运行。

```sh
pnpm install          # 安装依赖
pnpm build:ui         # 构建 @aipanel/ui 设计系统
pnpm tauri:dev        # 启动桌面应用（需 Rust 工具链）
# 或 pnpm dev         # 仅启动前端（浏览器；后端调用回退到 mock）
```

凭据默认走系统 Keychain(`tauri:dev` 亦然);仅做 mock 开发可设 `AIPANEL_CREDENTIAL_BACKEND=mock`。

> 想要真实 AI 规划/诊断:在「设置」里添加一个 OpenAI 兼容供应商(Base URL + API Key + 模型),其余只读体检/SSH 能力无需模型即可使用。

## 质量门禁

```sh
pnpm ci:check         # 类型检查 · 前端 vitest · Rust 测试 · Codex sidecar · Clippy(-D warnings) · 构建
```

若本机还没拉取对应平台的 Codex sidecar,先运行 `scripts/fetch-codex.sh`。发布 macOS 包前运行 `pnpm release:check`——完整门禁,会额外构建 Tauri 包并校验其使用 Developer ID Application 证书签名且有有效公证票据。

## 功能

**连接与诊断**
- SSH 连接管理——添加服务器、连通性测试、可见的连接/重连
- 只读服务器体检,带结构化指标 + 流式输出
- 实时监控——环形仪表 + 流量曲线,走只读 SSH、服务器零 agent
- 多服务器概览,支持收藏置顶与前台健康轮询

**AI 运维**
- 自然语言任务规划——经 OpenAI 兼容供应商真实 LLM 规划(结构化输出),离线 Mock 兜底
- 只读自动诊断——Agent 仅用只读工具自行排查
- 风险审查(Low / Medium / High / Blocked),写操作前确认 / 二次确认 + 本地审计
- Codex app-server 运行时——`thread/start` → `turn/start` → 事件流 → 工具分发回灌;首选,OpenAI 回退
- 模型供应商管理——填 Base URL + API Key,经 `/v1/models` 自动探测模型

**操作**
- 交互式 SSH 终端 + SFTP 文件管理——Codex 式三栏工作区;用户操作,不暴露给 AI
- Docker 部署工作流——检测/安装、Compose 部署、Caddy/Nginx 反代 + HTTPS、健康检查;Uptime Kuma / n8n / WordPress / PostgreSQL / Redis 模板,每步均走风险审查
- 应用内在线更新——GitHub Releases 签名分发(Tauri updater + minisign),`vX.Y.Z` tag 管理,检查/下载/安装 + 重启

**基础**
- `@aipanel/ui` 设计系统——Tailwind v4 token + Codex 风格组件
- Rust 单测/集成 + 前端 vitest、Clippy(`-D warnings`)、一站式 `pnpm ci:check`

## 项目文档

- [路线图](./ROADMAP.zh-Hans.md)
- [发布与在线更新](./RELEASE.zh-Hans.md)
- [架构设计](./ARCHITECTURE.zh-Hans.md)
- [技术选型](./TECH_STACK.zh-Hans.md)
- [安全模型](./SECURITY_MODEL.zh-Hans.md)
- [安全政策](../SECURITY.md)
- [贡献指南](../CONTRIBUTING.md)
- [更新日志](../CHANGELOG.md)

## 名称说明

`AiPanel` 直接表达「AI + Panel」的方向,中文用户也容易理解。但它的定位不是装在服务器上的传统面板,而是一个本地 AI 运维客户端。正式发布前建议继续确认 GitHub、npm、Homebrew、Docker Hub 和商标层面的名称占用情况。

## License

Copyright (c) 2026 AiPanel.

本项目基于 [GNU Affero General Public License v3.0](../LICENSE) 开源。
