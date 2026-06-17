# Security Policy

AiPanel 的目标是通过本地客户端和 SSH 帮助用户管理服务器。由于项目会涉及远程命令执行、凭据管理和日志分析，安全问题需要优先处理。

## Supported Versions

AiPanel 当前处于早期规划和 MVP 阶段，尚未发布稳定版本。

| Version | Supported |
| --- | --- |
| Unreleased | Yes |

## Reporting a Vulnerability

请不要在公开 issue 中披露可被直接利用的漏洞细节。

在项目正式建立安全邮箱前，请通过 GitHub 私有安全报告功能提交漏洞。如果仓库暂未开启该功能，请先创建一个不包含利用细节的 issue，说明“需要私下沟通安全问题”，维护者会安排私下渠道。

报告时请包含：

- 影响范围；
- 复现步骤；
- 受影响版本或 commit；
- 是否涉及远程命令执行；
- 是否可能泄露 SSH 密钥、API Key、服务器日志或环境变量；
- 已脱敏的证据。

不要提交：

- 真实 SSH 私钥；
- 可登录服务器的完整凭据；
- 未脱敏的生产日志；
- 第三方用户数据；
- 可直接攻击公网服务的完整利用脚本。

## Security Expectations

AiPanel 的安全设计应遵循：

- 默认只读诊断优先；
- AI 生成计划后由用户确认；
- 高风险命令二次确认；
- 命令执行全量审计；
- 敏感信息本地加密存储；
- 不默认在服务器上安装常驻 agent；
- 不默认开放新的公网管理入口。

更多设计说明请阅读 [安全模型](./docs/SECURITY_MODEL.zh-Hans.md)。

