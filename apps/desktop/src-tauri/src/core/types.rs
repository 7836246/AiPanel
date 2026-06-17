//! Core domain types, shared between Tauri commands and the frontend.
//!
//! Everything here is `serde`-serializable with `camelCase` field names so the
//! React app (apps/desktop/src) consumes them directly. Secrets never appear in
//! these structs — credentials are referenced by [`CredentialRef`] only.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Generate a fresh unique id string (uuid v4).
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}

// ---------------------------------------------------------------------------
// Risk
// ---------------------------------------------------------------------------

/// Operation risk, ordered Low < Medium < High < Blocked. Mirrors
/// docs/SECURITY_MODEL.zh-Hans.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Blocked,
}

impl RiskLevel {
    /// Whether a step at this level needs explicit user confirmation before running.
    pub fn requires_confirmation(self) -> bool {
        self >= RiskLevel::Medium
    }
    /// Whether it needs a second, explicit confirmation.
    pub fn requires_double_confirmation(self) -> bool {
        self == RiskLevel::High
    }
    /// Whether it is forbidden by default.
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
    /// SSH password (secret lives in the credential store).
    Password,
    /// SSH private key (secret lives in the credential store).
    Key,
    /// Local ssh-agent — no secret stored by AiPanel.
    Agent,
}

/// Opaque reference to a secret in the credential store. NEVER the secret itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialRef(pub String);

impl CredentialRef {
    pub fn for_server(server_id: &str) -> Self {
        CredentialRef(format!("server:{server_id}"))
    }
    pub fn for_provider(provider_id: &str) -> Self {
        CredentialRef(format!("provider:{provider_id}"))
    }
}

/// A saved server connection. Contains no secret material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    /// Host or IP. Display-masked in the UI; redacted before going to the AI.
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_kind: AuthKind,
    /// Set when `auth_kind` needs a stored secret (password/key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CredentialRef>,
    pub status: ServerStatus,
    /// Cached quick facts (OS, CPU, …) from the last doctor run.
    #[serde(default)]
    pub facts: std::collections::BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating/updating a server (no id/timestamps; secret passed
/// separately so it never lands in this struct or in storage).
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

fn default_ssh_port() -> u16 {
    22
}

// ---------------------------------------------------------------------------
// Providers / model selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Codex app-server (the intended agent runtime).
    CodexAppServer,
    /// Any OpenAI-compatible HTTP API.
    OpenAiCompatible,
    /// User-defined / other.
    Custom,
}

/// A configured model provider. The API key is referenced, never inlined.
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
    /// Path to the codex binary, for `CodexAppServer`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_path: Option<String>,
    /// Reference to the API key in the credential store, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CredentialRef>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How the app picks a model/provider for a given task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelectionPolicy {
    /// When true, pick per task class; otherwise always use `default_provider_id`.
    pub auto: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider_id: Option<String>,
}

impl Default for ModelSelectionPolicy {
    fn default() -> Self {
        ModelSelectionPolicy { auto: true, default_provider_id: None }
    }
}

/// Input for creating/updating a provider. The API key is passed separately to
/// the command (straight to the credential store) — never in this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    /// Present when updating an existing provider.
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

fn default_true() -> bool {
    true
}

/// Result of testing a provider's configuration / reachability.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub summary: String,
    /// The exact command (or tool invocation) this step runs.
    pub command: String,
    pub risk: RiskLevel,
    pub read_only: bool,
    /// Optional AiPanel tool name instead of a raw command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// The restated task goal.
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub created_at: DateTime<Utc>,
}

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

// ---------------------------------------------------------------------------
// Risk review
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskFinding {
    /// Index of the offending step in the plan.
    pub step_index: usize,
    /// Short category, e.g. "delete", "firewall", "remote-script".
    pub category: String,
    pub level: RiskLevel,
    /// Human-readable explanation (no secrets).
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskReview {
    pub overall: RiskLevel,
    pub requires_confirmation: bool,
    pub requires_double_confirmation: bool,
    pub blocked: bool,
    pub findings: Vec<RiskFinding>,
    /// Per-step effective risk (parallel to the plan's steps).
    pub step_levels: Vec<RiskLevel>,
}

// ---------------------------------------------------------------------------
// Execution / doctor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecution {
    pub command: String,
    pub exit_code: i32,
    /// Sanitized stdout (IPs/tokens/keys redacted before storage).
    pub stdout: String,
    /// Sanitized stderr.
    pub stderr: String,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub warnings: Vec<String>,
    /// The read-only commands that produced this report.
    #[serde(default)]
    pub executions: Vec<CommandExecution>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// The user's original intent.
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
    fn risk_levels_ordered() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Blocked);
    }

    #[test]
    fn confirmation_rules() {
        assert!(!RiskLevel::Low.requires_confirmation());
        assert!(RiskLevel::Medium.requires_confirmation());
        assert!(RiskLevel::High.requires_double_confirmation());
        assert!(!RiskLevel::Medium.requires_double_confirmation());
        assert!(RiskLevel::Blocked.is_blocked());
    }

    #[test]
    fn risk_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&RiskLevel::High).unwrap(), "\"high\"");
    }

    #[test]
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
    fn credential_ref_is_a_reference_not_a_secret() {
        let r = CredentialRef::for_server("abc");
        assert_eq!(r.0, "server:abc");
    }
}
