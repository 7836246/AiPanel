//! 应用级错误类型，供所有 Core 模块与 Tauri 命令共享。
//!
//! `AppError` 实现了 `serde::Serialize`，因此可直接作为 `Result<T, AppError>`
//! 从 `#[tauri::command]` 函数返回；前端收到的是 `{ code, message }`。错误消息
//! 绝不能包含密钥（见 docs/SECURITY_MODEL.zh-Hans.md）——调用方需在构造前先脱敏。

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

    /// 计划中的某一步被风险审查器拒绝（Blocked 等级，或未经确认的
    /// 写操作/高风险步骤）。
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
    /// 稳定的、供前端据以分支处理的机器可读错误码。
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
    // 序列化结果应同时包含 code 与 message 两个字段
    fn serializes_with_code_and_message() {
        let err = AppError::NotFound("server x".into());
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["code"], "not_found");
        assert!(v["message"].as_str().unwrap().contains("server x"));
    }

    #[test]
    // 错误码保持稳定不变
    fn codes_are_stable() {
        assert_eq!(AppError::Blocked("rm -rf /".into()).code(), "blocked");
        assert_eq!(AppError::Ssh("timeout".into()).code(), "ssh");
    }
}
