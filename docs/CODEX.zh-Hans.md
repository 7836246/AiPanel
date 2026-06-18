# Codex app-server 集成(打包 + 接线)

AiPanel 把 **Codex 作为 Agent 运行时**(对话/多轮/流式/模型),自己只守服务器运维的安全
边界(风险审查 / SSH / 审计 / 确认)。Codex 桌面 app 的底层就是 `codex` CLI;我们打包它的
**`codex-app-server`** 二进制(只用 app-server,比完整 CLI 小很多),通过 JSON-RPC/stdio 驱动。

## 取二进制(不进仓库)

二进制**不提交**(见 `.gitignore`),由脚本按平台取到 Tauri sidecar 目录
`apps/desktop/src-tauri/binaries/`:

```bash
scripts/fetch-codex.sh                      # 当前平台
scripts/fetch-codex.sh x86_64-apple-darwin  # 交叉打包时指定 target triple
```

来源:GitHub `openai/codex` 的 `rust-v0.141.0` release,资产 `codex-app-server-<triple>`。
dev 首次构建前、发布 CI 打包前各跑一次。

## 关键事实(已对真实二进制验证,0.141.0)

- `codex-app-server` **直接就是 server**(默认 `stdio://`,无需 `app-server` 子命令);支持 `-c key=value` 覆盖配置。
- 握手:`initialize` → `initialized` 通知 → `thread/start {sandbox:"read-only", approvalPolicy:"on-request"}` → `result.thread.id`;`turn/start` 才需鉴权。
- **隔离**:始终用 AiPanel 私有 `CODEX_HOME` 启动,**绝不读用户 `~/.codex`**(否则会加载用户个人 MCP、污染且不安全)。已验证:设了 `CODEX_HOME` 后用户 MCP 一个都不启动。
- 安全:codex 原生本地 shell/文件/提权的审批请求(`*Approval`)**一律拒绝**;服务器只能经 AiPanel 工具触达,写操作仍由 AiPanel UI 确认。
- 鉴权:API Key 模式——把用户配置的 base+key 通过 `-c model_providers`(`env_key` 经环境变量传密钥)喂给 codex。
- **线协议**:codex 0.141 的 `model_providers.wire_api` **只支持 `responses`**(OpenAI `/v1/responses`),不支持 `chat`。因此 codex provider 仅适配 **OpenAI 官方 / Responses 兼容**端点;只实现 `/chat/completions` 的第三方端点请继续用 AiPanel 既有的 OpenAI 兼容 provider(回退链会自动处理)。

## 打包(Tauri sidecar)

`tauri.conf.json` 的 `bundle.externalBin: ["binaries/codex-app-server"]` 会把 `binaries/codex-app-server-<triple>` 打进安装包。**构建前置**:先跑 `scripts/fetch-codex.sh` 取得二进制(否则 `pnpm tauri:dev` / `tauri build` 会报 externalBin 缺文件)。`pnpm build` / `cargo test` 不受影响。

## 当前状态与下一步

- ✅ **规划路径已可用**:`CodexAppServerProvider.plan/chat/summarize` 经真实 turn 工作(无需服务器工具),Codex 为 `create_plan` 首选、OpenAI 兼容回退。配好 Responses 端点 + key 即可端到端。
- ⬜ **工具驱动的自动诊断(下一阶段)**:让 codex 调用 AiPanel 的 server-ops 工具,需通过 **MCP** 暴露——`codex app-server` 导出的协议里没有可用的「客户端动态工具」注册位,而 MCP 是 codex 官方支持的工具面(`mcp_servers` 配置 + `mcpServer/tool/call`)。计划:`aipanel mcp-server` 子命令复用 `tools::dispatch`(跨进程共享 SQLite/Keychain),经 `-c mcp_servers` 注入。在此之前,自动诊断仍走 OpenAI function-calling 回路(它已有只读工具)。

## 代码位置

- `apps/desktop/src-tauri/src/agent/codex.rs` —— transport + turn/工具事件回路(`drive_turn`/`classify`,对齐真实协议;`*Approval` 拒批、`item/tool/call` → `tools::dispatch` 回灌)。
- 权威协议可随时由 `codex-app-server`(完整 CLI 的 `codex app-server generate-json-schema --out <dir>`)导出核对。
