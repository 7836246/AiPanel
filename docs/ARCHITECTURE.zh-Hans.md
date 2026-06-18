# AiPanel 架构设计

AiPanel 是本地优先的 AI 运维客户端。新的架构原则是：**Codex 负责 Agent 对话能力，AiPanel 负责服务器运维安全边界**。

这不是 Codex CLI 套壳。AiPanel 会把 Codex app-server 作为 Agent Runtime，用来承接多轮对话、上下文管理、模型调用、流式事件和工具调用；服务器、SSH、风险审查、执行和审计由 AiPanel 自己掌控。

## 核心组件

```text
AiPanel Desktop
  Tauri v2 + React + TypeScript
        |
        | newline-delimited JSON / stdio
        v
Codex App Server
  Agent Runtime
        |
        | tool calls
        v
AiPanel Tools
  server.list / server.doctor / ssh.run_readonly / task.execute_confirmed
        |
        v
AiPanel Core
  Risk Reviewer / SSH Executor / Audit Log / SQLite / Keychain
        |
        v
Remote Server
```

## 技术选型

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

## AiPanel Desktop

桌面端负责：

- 展示服务器列表；
- 接收用户自然语言输入；
- 展示 Codex Agent 的计划和结果；
- 展示命令输出和执行状态；
- 提供模型供应商配置；
- 提供权限模式选择；
- 提供用户确认和二次确认交互。

AiPanel 的默认设计是不在服务器上安装常驻面板程序。

## Codex Agent Runtime

AiPanel 不从零重造对话 Agent。底层对话、上下文、多轮推理、模型调用和流式事件优先交给 Codex app-server。

Codex Agent Runtime 负责：

- 多轮对话；
- 任务理解；
- 模型选择；
- 上下文管理；
- 计划生成；
- 日志解释；
- 结果总结。

Codex 不直接持有 SSH 凭据，也不直接裸跑 SSH 命令。Codex 只能通过 AiPanel 暴露的安全工具访问服务器能力。

## AiPanel Provider Manager

Provider Manager 负责模型供应商配置和自动选择。

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

自动选择策略可以按任务类型区分：

- 普通问答：低成本模型；
- 日志分析：长上下文模型；
- 命令计划：强推理模型；
- 高风险任务：强推理模型 + 严格结构化输出；
- 总结报告：普通模型。

## AiPanel Tools

Codex 通过工具调用使用 AiPanel 能力。工具通过 MCP 暴露给 Codex，并在 AiPanel 内部统一路由到 Core Tools。

示例工具：

- `server.list`：列出本地保存的服务器；
- `server.info`：读取服务器基础信息；
- `server.doctor.readonly`：执行只读体检；
- `ssh.run_readonly`：执行只读 SSH 命令；
- `task.plan`：生成结构化任务计划；
- `task.review`：审查计划风险；
- `task.execute_confirmed`：执行用户确认后的任务；
- `audit.write`：写入审计记录。

工具边界：

- 不暴露原始 SSH 私钥；
- 不提供无限制 shell；
- 写操作必须经过 AiPanel 风险审查和用户确认；
- 高风险操作必须二次确认。

## Plan Engine

Plan Engine 负责把 Codex 输出转成 AiPanel 可审查、可执行的结构化计划。

计划应包含：

- 任务目标；
- 执行步骤；
- 每一步的命令或工具调用；
- 是否只读；
- 风险等级；
- 预期输出；
- 失败处理建议。

Codex 生成的自然语言计划不能直接执行，必须转换成结构化 Plan 并经过 Risk Reviewer。

## Risk Reviewer

Risk Reviewer 负责识别风险。

风险维度包括：

- 是否修改系统状态；
- 是否删除文件；
- 是否重启服务；
- 是否修改防火墙；
- 是否修改数据库；
- 是否写入配置文件；
- 是否可能泄露敏感信息。

高风险任务必须二次确认。

## SSH Executor

SSH Executor 负责通过 SSH 执行用户确认后的命令。

执行器需要支持：

- 命令超时；
- 输出流式回传；
- 中断执行；
- 退出码采集；
- 标准输出和错误输出分离；
- 敏感信息脱敏；
- 不把密钥写入日志。

## Audit Log

审计记录保存在本地。

建议记录：

- 用户输入；
- Codex Agent 计划；
- AiPanel 风险审查结果；
- 用户确认时间；
- 实际工具调用或执行命令；
- 退出码；
- 脱敏后的输出；
- 最终总结。

不应记录：

- SSH 私钥；
- 明文密码；
- API Key；
- 未脱敏的敏感日志。

## 数据边界

AiPanel 应默认遵循：

- 凭据保存在本地 Keychain；
- SQLite 只保存非敏感配置和审计索引；
- 服务器状态按需采集；
- AI 请求尽量不包含密钥和敏感日志；
- 用户明确授权后才执行写操作；
- 不默认上传完整服务器日志。

## 实现映射（Desktop MVP）

各组件已在 `apps/desktop/src-tauri/src/` 下模块化实现（不拆独立 crate）：

- AiPanel Tools → `tools/` + `mcp/`：`server.list` / `server.info` / `server.doctor.readonly` / `ssh.run_readonly` / `task.plan` / `task.review` / `task.execute_confirmed` / `audit.write`，每个工具带权限（ReadOnly/Write）与审计策略；只读默认可用，写操作需用户确认，Agent 不能自行授权（`task.execute_confirmed` 没有用户确认会被拒绝）。内部统一通过 `dispatch(name, args)` 路由，Codex 侧通过 stdio MCP server 只暴露只读工具。
- Plan Engine → `plan/`；Risk Reviewer → `risk/`；SSH Executor → `ssh/`；Server Doctor → `doctor/`；Audit Log → `audit/` + `store/`；Provider Manager / Agent Runtime → `agent/`；本地存储 → `store/`（SQLite）；密钥 → `credentials/`（Keychain）。
- 前端经 Tauri 命令（`commands/`）调用，详见 CLAUDE.md「后端结构」。
