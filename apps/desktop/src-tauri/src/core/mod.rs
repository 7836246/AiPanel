//! Core domain layer: shared types and the application error.
//!
//! The Core layer owns the security boundary (see CLAUDE.md): SSH execution,
//! risk review, and audit are AiPanel's own — never delegated to the agent.

pub mod error;
pub mod sanitize;
pub mod types;

pub use error::{AppError, AppResult};
