//! 核心领域层：共享类型与应用级错误。
//!
//! Core 层掌管安全边界（见 CLAUDE.md）：SSH 执行、风险审查、审计都由 AiPanel
//! 自己实现——绝不交给 Agent。

pub mod error;
pub mod sanitize;
pub mod types;

pub use error::{AppError, AppResult};
