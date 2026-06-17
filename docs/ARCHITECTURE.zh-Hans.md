# AiPanel 架构设计

AiPanel 是本地优先的 AI 运维客户端。核心架构围绕自然语言输入、AI 任务规划、风险审查、SSH 执行和本地审计展开。

## 核心组件

```text
Local Desktop / CLI
        |
        v
User Intent
        |
        v
AI Planner
        |
        v
Risk Reviewer
        |
        v
SSH Executor
        |
        v
Result Collector
        |
        v
Audit Log + Summary
```

## Local Client

本地客户端负责：

- 管理服务器连接配置；
- 管理本地密钥和凭据；
- 接收用户自然语言输入；
- 展示 AI 任务计划；
- 展示命令输出和执行结果；
- 保存本地审计记录。

AiPanel 的默认设计是不在服务器上安装常驻面板程序。

## AI Planner

AI Planner 负责把用户输入转成结构化任务计划。

计划应包含：

- 任务目标；
- 执行步骤；
- 每一步的命令；
- 是否只读；
- 风险等级；
- 预期输出；
- 失败处理建议。

AI Planner 不直接执行命令，所有命令必须经过 Risk Reviewer。

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

## Result Collector

Result Collector 负责把命令输出转成可理解结果。

它应提供：

- 原始输出；
- 结构化状态；
- 错误归因；
- 下一步建议；
- 是否需要继续执行。

## Audit Log

审计记录保存在本地。

建议记录：

- 用户输入；
- AI 计划；
- 风险审查结果；
- 用户确认时间；
- 执行命令；
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

- 凭据保存在本地；
- 服务器状态按需采集；
- AI 请求尽量不包含密钥和敏感日志；
- 用户明确授权后才执行写操作；
- 不默认上传完整服务器日志。

