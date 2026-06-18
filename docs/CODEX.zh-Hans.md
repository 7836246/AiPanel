# Codex app-server 集成(打包 + 接线)

AiPanel 把 **Codex 作为 Agent 运行时**(对话/多轮/流式/模型),自己只守服务器运维的安全
边界(风险审查 / SSH / 审计 / 确认)。Codex 桌面 app 的底层就是 `codex` CLI;我们打包它的
**`codex-app-server`** 二进制(只用 app-server,比完整 CLI 小很多),通过 newline-delimited JSON/stdio 驱动。

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
- client 请求不能带 `jsonrpc: "2.0"` 字段；真实 0.141.0 sidecar 的 `ClientRequest` 会忽略带该字段的请求。
- 握手:`initialize` → `initialized` 通知 → `thread/start {sandbox:"read-only", approvalPolicy:"on-request"}` → `result.thread.id`;`turn/start` 才需鉴权。
- `turn/start` 文本输入必须包含 `text_elements: []`；事件流会同时出现 v2 `agentMessage` 与 raw Responses `message.content[].output_text` 形态。
- **隔离**:始终用 AiPanel 私有 `CODEX_HOME` 启动,**绝不读用户 `~/.codex`**(否则会加载用户个人 MCP、污染且不安全)。已验证:设了 `CODEX_HOME` 后用户 MCP 一个都不启动。
- 安全:codex 原生本地 shell/文件/提权的审批请求(`*Approval`)**一律拒绝**;服务器只能经 AiPanel 工具触达,写操作仍由 AiPanel UI 确认。
- 鉴权:API Key 模式——把用户配置的 base+key 通过 `-c model_providers`(`env_key` 经环境变量传密钥)喂给 codex。
- **线协议**:codex 0.141 的 `model_providers.wire_api` **只支持 `responses`**(OpenAI `/v1/responses`),不支持 `chat`。因此 codex provider 仅适配 **OpenAI 官方 / Responses 兼容**端点;只实现 `/chat/completions` 的第三方端点请继续用 AiPanel 既有的 OpenAI 兼容 provider(回退链会自动处理)。

## 打包(Tauri sidecar)

`tauri.conf.json` 的 `bundle.externalBin: ["binaries/codex-app-server"]` 会把 `binaries/codex-app-server-<triple>` 打进安装包。**构建前置**:先跑 `scripts/fetch-codex.sh` 取得二进制(否则 `pnpm tauri:dev` / `tauri build` 会报 externalBin 缺文件)。`pnpm build` / `cargo test` 不受影响。

## 当前状态

- ✅ **规划路径**:`CodexAppServerProvider.plan/chat/summarize` 经真实 turn 工作(无需服务器工具),Codex 为 `create_plan` 首选、OpenAI 兼容回退。
- ✅ **工具驱动的自动诊断**:`aipanel mcp-server`(`src/mcp/mod.rs`)以 stdio MCP 暴露**只读** server-ops 工具,复用 `tools::dispatch`(跨进程共享 SQLite/Keychain,经 `AIPANEL_DATA_DIR`);`launch()` 把它注入 codex 的 `mcp_servers`(`default_tools_approval_mode=auto` 自动批准只读工具),`run_agent_turn` 优先 Codex、失败回退 OpenAI function-calling 回路。写/执行类工具绝不暴露、即便被调也拒绝。
- 已验证:codex 0.141 真实 sidecar initialize / thread-start / turn-start 协议形状、`model_providers` 配置接受、`CODEX_HOME` 隔离、`aipanel mcp-server` 真实子进程(initialize、tools/list 仅只读、tools/call 分发、写工具拒绝)。
- ⏳ **待有效供应商 key 验证**:配一个 Responses 兼容 base + key + model 后真跑一轮完整模型 turn，即可端到端确认 codex 经 MCP 调工具诊断并返回最终总结。

> 工具桥仅在 codex 用 **Responses** 端点时生效(0.141 限制);chat-only 第三方端点继续用 AiPanel 既有 OpenAI 回路。

## 代码位置

- `apps/desktop/src-tauri/src/agent/codex.rs` —— transport + turn/工具事件回路(`drive_turn`/`classify`,对齐真实协议;`*Approval` 拒批、`item/tool/call` → `tools::dispatch` 回灌)。
- 权威协议可随时由 `codex-app-server`(完整 CLI 的 `codex app-server generate-json-schema --out <dir>`)导出核对。
