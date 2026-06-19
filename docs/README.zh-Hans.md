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

------------------------------

## 什么是 AiPanel？

AiPanel 是一个本地运行的 AI 服务器运维客户端。

传统服务器面板通常需要安装并常驻在 VPS 上，这会占用服务器资源，也会带来新的 Web 管理入口和攻击面。AiPanel 的思路不同：客户端运行在本地电脑，通过标准 SSH 连接服务器，用 AI 将自然语言需求拆解成可审计的操作计划，在用户确认后远程执行，并整理执行结果。

- **服务器零常驻**：不要求在 VPS 上安装并长期运行面板程序，减少资源占用；
- **SSH 优先**：使用标准 SSH 连接服务器，不默认新增公网 Web 管理入口；
- **AI 先规划再执行**：先生成步骤、命令和风险说明，确认后再执行；
- **只读诊断模式**：安全检查 CPU、内存、磁盘、端口、服务、Docker、Nginx、日志和防火墙；
- **环境安装与应用部署**：规划支持 Docker、Docker Compose、反向代理、HTTPS 和常见开源应用部署；
- **本地审计记录**：任务计划、命令、输出和总结保存在本地，方便回看和复现。

## 为什么做 AiPanel？

1Panel、宝塔等面板已经很好地解决了服务器图形化管理问题，但它们的共同特点是：管理程序通常运行在服务器上。

对于 1C1G、2C2G 等轻量 VPS 来说，常驻面板会带来额外成本：

- 占用 CPU、内存和磁盘资源；
- 需要维护面板自身的运行环境；
- 需要暴露或保护额外的管理入口；
- 面板本身也会成为新的安全边界；
- 功能依赖插件和服务端生态。

AiPanel 更适合这样的场景：

- 用户有多台轻量服务器，不想每台都安装面板；
- 用户更习惯用自然语言描述目标，而不是寻找插件和菜单；
- 用户希望先诊断、再确认、后执行；
- 用户希望保留命令级审计记录；
- 用户希望服务器默认保持干净，只在需要时执行任务。

## 快速开始

AiPanel 当前处于**桌面端 MVP** 阶段——基于 Tauri v2 + React，在本地运行。

```sh
pnpm install          # 安装依赖
pnpm build:ui         # 构建 @aipanel/ui 设计系统
pnpm tauri:dev        # 启动桌面应用（需 Rust 工具链）
# 或 pnpm dev 仅启动前端（浏览器；后端调用回退到 mock）
```

桌面端当前已覆盖：

- 添加 / 编辑 / 删除服务器连接配置（凭据进系统 Keychain）；
- SSH 连通性测试；
- 流式只读服务器体检并输出结构化报告；
- 自然语言转可审查计划（配置 OpenAI 兼容供应商后为真实 LLM 规划，否则本地 Mock）、风险分级、确认 / 高风险二次确认后执行；
- 让 AI 用只读工具自动诊断并总结；
- 本地审计记录；
- 模型供应商配置（含 Codex app-server / OpenAI 兼容）。

> 想要真实 AI 规划/诊断：在「设置」里添加一个 OpenAI 兼容供应商（Base URL + API Key + 模型），其余只读体检/SSH 能力无需模型即可使用。

## 质量门禁

日常开发和 CI 运行不需要密钥的门禁：

```sh
pnpm ci:check
```

该命令会执行工作区类型检查、检查匹配平台的 Codex app-server sidecar、运行 Rust
测试、执行真实 sidecar initialize 集成测试；随后运行 Rust Clippy（warnings
视为失败）并构建前端包。若本机还没有 sidecar，先运行 `scripts/fetch-codex.sh`。

发布 macOS 包前运行完整发布门禁：

```sh
pnpm release:check
```

该命令会检查打包的 Codex app-server sidecar，并通过真实集成测试启动它验证
initialize 协议；随后执行工作区类型检查、运行 Rust 测试、运行 Rust Clippy（warnings
视为失败）、构建前端、构建 Tauri 安装包，确认 app 主程序和 sidecar 都匹配目标架构，
并验证 macOS `.app` 和 `.dmg` 均使用 Developer ID Application 证书签名且已 stapled
有效公证票据。开发证书签名或未公证的包会被判定为不合格。

## 功能规划

### 服务器管理

- SSH 密钥、密码、端口和服务器标签管理；
- 自动识别 Linux 发行版、架构、内核和包管理器；
- 多服务器分组；
- 连接状态和基础信息缓存。

### AI 服务器体检

- CPU、内存、磁盘、负载检查；
- 进程、端口和监听服务检查；
- Docker、Nginx、数据库、Redis 状态检查；
- 防火墙、安全组和公网连通性提示；
- 常见异常自动归因。

### 环境安装

- Docker / Docker Compose；
- Nginx / Caddy；
- Node.js / Python / Go；
- MySQL / PostgreSQL / Redis；
- 常见系统工具和基础安全配置。

### 应用部署

- Docker Compose 应用部署；
- 环境变量管理；
- 反向代理配置；
- HTTPS 证书申请和续期；
- 部署后健康检查。

### 日志诊断

- Docker 容器日志分析；
- Systemd 服务日志分析；
- Nginx / Caddy 访问日志和错误日志分析；
- 根据错误信息自动补充检查命令。

## 安全机制

AiPanel 不应该只是“让 AI 执行命令”。核心安全机制会围绕计划、确认、执行和审计展开：

- 执行前展示操作计划；
- 标记命令风险等级；
- 高风险操作强制二次确认；
- 默认支持只读诊断模式；
- 识别 `rm -rf`、格式化磁盘、清空数据库、覆盖配置等危险操作；
- 支持命令白名单和任务模板；
- 敏感信息仅保存在本地；
- 所有命令和输出保留本地审计记录。

## 技术方向

当前技术选型：

- 桌面端：Tauri v2 + React + TypeScript；
- Agent Runtime：Codex app-server；
- Agent 通信：newline-delimited JSON / stdio；
- 模型配置：AiPanel Provider Manager；
- 工具系统：AiPanel MCP / Core Tools；
- SSH 执行：AiPanel 自己实现；
- 风险审查：AiPanel 自己实现；
- 审计记录：AiPanel 自己实现；
- 本地存储：SQLite；
- 密钥存储：系统 Keychain。

核心原则：

```text
Codex 负责对话、理解、规划、模型和上下文
AiPanel 负责服务器、SSH、权限、执行、安全和审计
```

核心流程：

```text
用户自然语言输入
        |
        v
Codex Agent Runtime
        |
        v
AiPanel Tools
        |
        v
风险识别和用户确认
        |
        v
AiPanel SSH Executor
        |
        v
审计记录和结果总结
```

## 路线图

- [x] 项目定位；
- [x] README 标准化；
- [x] 初始 logo 和预览图；
- [x] gpt-image-2 生成正式品牌图；
- [x] SSH 连接管理；
- [x] 只读服务器体检；
- [x] AI 任务规划；
- [x] 命令风险审查；
- [x] 桌面客户端；
- [x] Docker 应用部署流程；
- [x] 服务器实时监控（系统信息 + CPU/内存/磁盘/负载仪表 + 流量曲线,SSH 只读、服务器零 agent）；
- [x] 应用内在线更新（GitHub Releases + minisign 签名,tag vX.Y.Z 管理版本）；
- [x] 测试与 CI 基线（Rust 单测/集成 + 前端 vitest + Clippy + `pnpm ci:check`）；
- [ ] Developer ID 签名、公证与可分发发布流程实测;
- [ ] 带真实供应商 API Key 的完整模型轮次与真实服务器部署验收。

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

`AiPanel` 能够直接表达“AI + Panel”的方向，中文用户也容易理解。

但 AiPanel 的产品定位不是安装在服务器上的传统面板，而是一个本地 AI 运维客户端。正式发布前建议继续确认 GitHub、npm、Homebrew、Docker Hub 和商标层面的名称占用情况。

## License

Copyright (c) 2026 AiPanel.

本项目基于 [GNU Affero General Public License v3.0](../LICENSE) 开源。
