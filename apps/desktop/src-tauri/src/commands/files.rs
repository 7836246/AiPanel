//! 文件管理（SFTP over SSH）的 Tauri 命令薄层。
//!
//! 这是**面向用户**的文件浏览/读取/写入入口——与 AiPanel Tools 无关，能力**绝不
//! 暴露给 AI / Agent**。每个命令各自内联取出服务器及其 SSH 密钥（与
//! `commands::mod::load_server_and_secret` 同风格），再委托给 `crate::files`。

use tauri::State;

use crate::core::error::AppResult;
use crate::core::types::{DirListing, FileContent};
use crate::AppState;

/// 列举某台服务器上一个目录的内容。
#[tauri::command]
pub async fn fs_list(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> AppResult<DirListing> {
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };
    crate::files::list(&server, secret.as_deref(), &path).await
}

/// 读取某台服务器上一个文件的内容（最多 ~256KB，超出标记 truncated）。
#[tauri::command]
pub async fn fs_read(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> AppResult<FileContent> {
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };
    crate::files::read(&server, secret.as_deref(), &path).await
}

/// 把内容写入某台服务器上的一个文件（内容经 stdin 落盘，绝不进 argv）。
#[tauri::command]
pub async fn fs_write(
    state: State<'_, AppState>,
    id: String,
    path: String,
    content: String,
) -> AppResult<()> {
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };
    crate::files::write(&server, secret.as_deref(), &path, &content).await
}

/// 把本地文件**上传**到某台服务器的远程目录（scp over SSH）。
/// 面向用户的操作，绝不暴露给 AI / Agent。
#[tauri::command]
pub async fn fs_upload(
    state: State<'_, AppState>,
    id: String,
    local_path: String,
    remote_dir: String,
) -> AppResult<()> {
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };
    crate::files::upload(&server, secret.as_deref(), &local_path, &remote_dir).await
}

/// 把某台服务器上的远程文件**下载**到本地路径（scp over SSH）。
/// 面向用户的操作，绝不暴露给 AI / Agent。
#[tauri::command]
pub async fn fs_download(
    state: State<'_, AppState>,
    id: String,
    remote_path: String,
    local_path: String,
) -> AppResult<()> {
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };
    crate::files::download(&server, secret.as_deref(), &remote_path, &local_path).await
}
