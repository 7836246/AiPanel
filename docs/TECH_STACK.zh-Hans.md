# AiPanel 技术选型

AiPanel 的技术路线是：**Codex 负责 Agent 对话能力，AiPanel 负责服务器运维安全边界**。

这不是 Codex CLI 套壳。AiPanel 会把 Codex app-server 作为 Agent Runtime，承接多轮对话、上下文、模型选择和流式事件；服务器、SSH、风险审查、执行和审计由 AiPanel 自己掌控。

## 最终选型

| 模块 | 技术 |
| --- | --- |
| 桌面端 | Tauri v2 + React + TypeScript |
| Agent Runtime | Codex app-server |
| Agent 通信 | JSON-RPC / stdio |
| 模型配置 | AiPanel Provider Manager |
| 工具系统 | AiPanel MCP / JSON-RPC Tools |
| SSH 执行 | AiPanel 自己实现 |
| 风险审查 | AiPanel 自己实现 |
| 审计记录 | AiPanel 自己实现 |
| 本地存储 | SQLite |
| 密钥存储 | 系统 Keychain |

## 为什么用 Codex app-server

Codex app-server 适合做富客户端 Agent Runtime：

- 支持 JSON-RPC；
- 支持 stdio；
- 支持多轮 Thread；
- 支持流式 Agent 事件；
- 支持模型和上下文管理；
- 适合被桌面客户端集成。

AiPanel 不优先使用 `codex exec` 作为主链路，因为它更适合脚本和 CI 的非交互任务，不适合作为桌面端长期对话运行时。

## 为什么 AiPanel 自己做 SSH 和风险审查

AiPanel 的核心卖点是安全运维。

因此这些能力必须由 AiPanel 自己掌控：

- 服务器连接管理；
- SSH 凭据管理；
- 命令风险识别；
- 用户确认；
- 高风险二次确认；
- 执行超时和中断；
- 输出脱敏；
- 审计记录；
- 本地配置和密钥存储。

Codex 可以理解问题、生成计划、解释日志，但不能绕过 AiPanel 直接持有 SSH 私钥或执行无限制 shell。

## 模型供应商配置

AiPanel Provider Manager 负责模型供应商配置。

第一阶段支持：

- Codex app-server；
- OpenAI-compatible API；
- 自定义 Base URL；
- 自定义 API Key；
- 自定义模型名；
- 自动选择模型。

后续可扩展：

- OpenRouter；
- OneAPI；
- Ollama；
- LM Studio；
- 国产兼容接口；
- 私有模型网关。

## 自动模型选择

自动模式可以按任务选择模型：

- 普通问答：低成本模型；
- 日志分析：长上下文模型；
- 命令计划：强推理模型；
- 高风险操作：强推理模型 + 严格结构化输出；
- 总结报告：普通模型。

用户也可以手动指定供应商和模型。

## 工具调用边界

Codex 通过 AiPanel Tools 使用服务器能力。

示例：

```text
server.list
server.info
server.doctor.readonly
ssh.run_readonly
task.plan
task.review
task.execute_confirmed
audit.write
```

工具必须遵守：

- 默认只读；
- 不暴露原始 SSH 密钥；
- 不提供无限制 shell；
- 写操作必须确认；
- 高风险操作必须二次确认；
- 所有执行写入审计记录。

## 目标架构

```text
AiPanel Desktop
  Tauri v2 + React + TypeScript
        |
        | JSON-RPC / stdio
        v
Codex App Server
  Agent Runtime
        |
        | tool calls
        v
AiPanel Tools
  MCP / JSON-RPC Tools
        |
        v
AiPanel Core
  Risk Reviewer / SSH Executor / Audit Log / SQLite / Keychain
        |
        v
Remote Server
```

