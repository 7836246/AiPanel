//! Application error type shared across all Core modules and Tauri commands.
//!
//! `AppError` implements `serde::Serialize` so it can be returned directly from
//! `#[tauri::command]` functions as `Result<T, AppError>`; the frontend receives
//! `{ code, message }`. Error messages must never contain secrets (see
//! docs/SECURITY_MODEL.zh-Hans.md) — callers redact before constructing.

use serde::{Serialize, Serializer};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation: {0}")]
    Validation(String),

    #[error("storage: {0}")]
    Storage(String),

    #[error("credential: {0}")]
    Credential(String),

    /// A planned step was rejected by the Risk Reviewer (Blocked level, or a
    /// write/high-risk step that was not confirmed).
    #[error("blocked by risk policy: {0}")]
    Blocked(String),

    #[error("ssh: {0}")]
    Ssh(String),

    #[error("agent provider: {0}")]
    Provider(String),

    #[error("config: {0}")]
    Config(String),

    #[error("io: {0}")]
    Io(String),
}

impl AppError {
    /// Stable machine-readable code for the frontend to branch on.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "not_found",
            AppError::Validation(_) => "validation",
            AppError::Storage(_) => "storage",
            AppError::Credential(_) => "credential",
            AppError::Blocked(_) => "blocked",
            AppError::Ssh(_) => "ssh",
            AppError::Provider(_) => "provider",
            AppError::Config(_) => "config",
            AppError::Io(_) => "io",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Storage(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Storage(format!("serde: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_code_and_message() {
        let err = AppError::NotFound("server x".into());
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["code"], "not_found");
        assert!(v["message"].as_str().unwrap().contains("server x"));
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(AppError::Blocked("rm -rf /".into()).code(), "blocked");
        assert_eq!(AppError::Ssh("timeout".into()).code(), "ssh");
    }
}
