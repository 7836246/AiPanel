//! 核心领域类型，在 Tauri 命令与前端之间共享。
//!
//! 这里的所有类型都可被 `serde` 序列化，字段名采用 `camelCase`，以便 React
//! 应用（apps/desktop/src）直接消费。这些结构体中绝不出现密钥——凭据仅以
//! [`CredentialRef`] 引用方式表示。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 生成一个新的唯一 id 字符串（uuid v4）。
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// 返回当前 UTC 时间。
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

// ---------------------------------------------------------------------------
// Risk
// ---------------------------------------------------------------------------

/// 操作风险等级，顺序为 Low < Medium < High < Blocked。与
/// docs/SECURITY_MODEL.zh-Hans.md 保持一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Blocked,
}

impl RiskLevel {
    /// 该等级的步骤执行前是否需要用户明确确认。
    pub fn requires_confirmation(self) -> bool {
        self >= RiskLevel::Medium
    }
    /// 是否需要二次明确确认。
    pub fn requires_double_confirmation(self) -> bool {
        self == RiskLevel::High
    }
    /// 是否默认被禁止。
    pub fn is_blocked(self) -> bool {
        self == RiskLevel::Blocked
    }
}

// ---------------------------------------------------------------------------
// Servers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Online,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    /// SSH 密码（密钥存放在凭据存储中）。
    Password,
    /// SSH 私钥（密钥存放在凭据存储中）。
    Key,
    /// 本地 ssh-agent——AiPanel 不存放任何密钥。
    Agent,
}

/// 指向凭据存储中某个密钥的不透明引用。绝不是密钥本身。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialRef(pub String);

impl CredentialRef {
    /// 为某台服务器构造凭据引用。
    pub fn for_server(server_id: &str) -> Self {
        CredentialRef(format!("server:{server_id}"))
    }
    /// 为某个模型供应商构造凭据引用。
    pub fn for_provider(provider_id: &str) -> Self {
        CredentialRef(format!("provider:{provider_id}"))
    }
}

/// 一条已保存的服务器连接。不含任何密钥材料。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    /// 主机名或 IP。界面上脱敏展示；发送给 AI 前会被脱敏。
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_kind: AuthKind,
    /// 当 `auth_kind` 需要存放密钥（密码/密钥）时才设置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CredentialRef>,
    pub status: ServerStatus,
    /// 上次体检（doctor）得到的缓存快速信息（操作系统、CPU 等）。
    #[serde(default)]
    pub facts: std::collections::BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 创建/更新服务器的输入（不含 id/时间戳；密钥单独传递，因此绝不会落入此
/// 结构体或存储中）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    pub name: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    pub auth_kind: AuthKind,
}

/// SSH 默认端口。
fn default_ssh_port() -> u16 {
    22
}

// ---------------------------------------------------------------------------
// Providers / model selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Codex app-server（既定的 Agent 运行时）。
    CodexAppServer,
    /// 任意兼容 OpenAI 的 HTTP API。
    OpenAiCompatible,
    /// 用户自定义/其他。
    Custom,
}

/// 一个已配置的模型供应商。API Key 以引用方式存放，绝不内联。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// codex 可执行文件的路径，供 `CodexAppServer` 使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_path: Option<String>,
    /// 指向凭据存储中 API Key 的引用（若有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CredentialRef>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 应用为某项任务挑选模型/供应商的策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelectionPolicy {
    /// 为 true 时按任务类别选择；否则始终使用 `default_provider_id`。
    pub auto: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider_id: Option<String>,
}

impl Default for ModelSelectionPolicy {
    fn default() -> Self {
        ModelSelectionPolicy { auto: true, default_provider_id: None }
    }
}

/// 创建/更新供应商的输入。API Key 单独传给命令（直接进入凭据存储）——
/// 绝不放在此结构体中。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    /// 更新已有供应商时存在。
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub codex_path: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 默认启用。
fn default_true() -> bool {
    true
}

/// 测试某个供应商配置/可达性的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Plans / tasks
// ---------------------------------------------------------------------------

/// 计划中的单个步骤。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub summary: String,
    /// 该步骤要执行的确切命令（或工具调用）。
    pub command: String,
    pub risk: RiskLevel,
    pub read_only: bool,
    /// 可选的 AiPanel 工具名，用以替代裸命令。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// 一份结构化计划，由 Agent 计划转换而来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// 复述后的任务目标。
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub created_at: DateTime<Utc>,
}

/// 任务的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Planning,
    AwaitingConfirmation,
    Running,
    Completed,
    Failed,
    Blocked,
}

/// 一项任务的核心状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub intent: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 一条 [`TaskRecord`] 记录的运行类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// 经过审查并（可能）执行过的自然语言计划。
    Plan,
    /// 一轮自主的只读诊断。
    Diagnose,
    /// 一次只读的服务器体检（doctor）。
    Doctor,
}

/// 面向用户的单次运行历史。侧边栏会列出并可恢复它们。携带完整的
/// 计划/审查/执行记录，因此一次运行可完全从存储中渲染出来。与这里的其他类型
/// 一样已脱敏——不含密钥。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub title: String,
    /// 用户的原始意图。
    pub intent: String,
    pub kind: TaskKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_review: Option<RiskReview>,
    #[serde(default)]
    pub executions: Vec<CommandExecution>,
    /// AI 诊断的工具调用轨迹（仅 diagnose 类任务）。以不透明 JSON 存储，
    /// 避免 core 依赖 agent 模块、并能随 ToolTrace 形状演进而原样往返。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Risk review
// ---------------------------------------------------------------------------

/// 风险审查中针对单个步骤的一条发现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskFinding {
    /// 计划中触发问题的步骤下标。
    pub step_index: usize,
    /// 简短分类，例如 "delete"、"firewall"、"remote-script"。
    pub category: String,
    pub level: RiskLevel,
    /// 人类可读的说明（不含密钥）。
    pub message: String,
}

/// 对整份计划的风险审查结论。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskReview {
    pub overall: RiskLevel,
    pub requires_confirmation: bool,
    pub requires_double_confirmation: bool,
    pub blocked: bool,
    pub findings: Vec<RiskFinding>,
    /// 每个步骤的有效风险（与计划的 steps 一一对应）。
    pub step_levels: Vec<RiskLevel>,
}

// ---------------------------------------------------------------------------
// Execution / doctor
// ---------------------------------------------------------------------------

/// 一条命令的实际执行结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecution {
    pub command: String,
    pub exit_code: i32,
    /// 已脱敏的 stdout（IP/Token/密钥在写入存储前已脱敏）。
    pub stdout: String,
    /// 已脱敏的 stderr。
    pub stderr: String,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
}

/// 只读服务器体检报告。
// 含 f64（cpuPercent）字段，故不派生 Eq（f64 不实现 Eq）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub server_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker: Option<String>,
    // ---- Doctor v2：从原始探测输出中解析出的结构化指标 ----
    // 全部为可选字段并带 #[serde(default)]，因此旧的 JSON（没有这些字段）仍可正常反序列化，
    // 审计读取（audit::record_for_doctor）也不会因新增字段而被破坏。
    /// CPU 使用率百分比（0-100）。当前探测不直接采集，保留为 None（负载见 facts 的「Load」）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    /// 已用内存（MB），由 `free -m` 解析。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem_used_mb: Option<u64>,
    /// 总内存（MB），由 `free -m` 解析。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem_total_mb: Option<u64>,
    /// 根分区（/）使用率百分比，由 `df` 解析。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_used_percent: Option<u64>,
    /// 运行中的服务数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_count: Option<usize>,
    /// 运行中的容器数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_count: Option<usize>,
    /// 监听端口数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_count: Option<usize>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// 生成此报告所执行的只读命令。
    #[serde(default)]
    pub executions: Vec<CommandExecution>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

/// 一条本地审计记录：记录意图、计划、风险判定、确认、执行与总结。绝不记录密钥。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// 用户的原始意图。
    pub intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_review: Option<RiskReview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub executions: Vec<CommandExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // 风险等级按 Low < Medium < High < Blocked 排序
    fn risk_levels_ordered() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Blocked);
    }

    #[test]
    // 确认/二次确认/拦截规则
    fn confirmation_rules() {
        assert!(!RiskLevel::Low.requires_confirmation());
        assert!(RiskLevel::Medium.requires_confirmation());
        assert!(RiskLevel::High.requires_double_confirmation());
        assert!(!RiskLevel::Medium.requires_double_confirmation());
        assert!(RiskLevel::Blocked.is_blocked());
    }

    #[test]
    // 风险等级序列化为小写字符串
    fn risk_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&RiskLevel::High).unwrap(), "\"high\"");
    }

    #[test]
    // PlanStep 序列化字段名采用 camelCase
    fn plan_step_camel_case() {
        let step = PlanStep {
            summary: "check".into(),
            command: "uptime".into(),
            risk: RiskLevel::Low,
            read_only: true,
            tool: None,
        };
        let v = serde_json::to_value(&step).unwrap();
        assert_eq!(v["readOnly"], true);
        assert!(v.get("tool").is_none());
    }

    #[test]
    // CredentialRef 只是引用，不是密钥本身
    fn credential_ref_is_a_reference_not_a_secret() {
        let r = CredentialRef::for_server("abc");
        assert_eq!(r.0, "server:abc");
    }
}
