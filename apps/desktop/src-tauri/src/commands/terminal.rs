//! 交互式 SSH 终端的 Tauri 命令薄层。
//!
//! 这些命令把前端的「打开终端 / 写入按键 / 调整大小 / 关闭」请求委托给 `terminal`
//! 模块。终端是**用户亲自操作的真实终端**，不暴露给 Agent。
//!
//! 前端 invoke 契约（已固定）：
//! - `terminal_open({ id, cols, rows, onOutput: Channel<string> }) -> string`（返回 session_id）
//! - `terminal_write({ sessionId, data })`
//! - `terminal_resize({ sessionId, cols, rows })`
//! - `terminal_close({ sessionId })`

use tauri::ipc::Channel;
use tauri::State;

use crate::core::error::AppResult;
use crate::AppState;

/// 打开一个面向某台服务器的交互式 SSH 终端，返回 session_id。
/// 远端输出通过 `on_output` 这个 [`Channel`] 持续推送给前端。
///
/// 取服务器 + 密钥的方式与 `commands::load_server_and_secret` 一致（这里内联，
/// 避免依赖父模块的私有项）：从 store 取 ServerProfile，再按 credential_ref 取密钥。
///
/// TODO（审计）：交互式终端是逐键交互的会话，输出本质上是合并的 tty 流，按键也无法
/// 对应到可审查的「计划/命令」。因此这里**刻意不写审计记录**——审计针对的是计划执行
/// 与只读体检这类结构化、可脱敏的运行。若未来需要会话级审计，应记录「会话已开启/关闭」
/// 这类元数据而非逐键内容，且仍须经脱敏。
#[tauri::command]
pub async fn terminal_open(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    on_output: Channel<String>,
) -> AppResult<String> {
    // 取服务器及其 SSH 密钥（若该认证方式存有密钥）。
    let server = state.store.get_server(&id)?;
    let secret = match &server.credential_ref {
        Some(reference) => state.credentials.get_secret(reference)?,
        None => None,
    };

    // 把每一段远端输出通过 Channel 转发给前端。send 返回 Ok 表示前端仍在监听;
    // 返回 false(Channel 已断开,如 webview 刷新)会让读线程结束并回收会话。
    crate::terminal::open(
        &server,
        secret.as_deref(),
        cols,
        rows,
        Box::new(move |chunk| on_output.send(chunk).is_ok()),
    )
}

/// 把用户输入的按键写入指定会话（送往远端）。找不到会话时静默忽略。
#[tauri::command]
pub fn terminal_write(session_id: String, data: String) {
    let _ = crate::terminal::write(&session_id, &data);
}

/// 调整指定会话的终端窗口大小。找不到会话时静默忽略。
#[tauri::command]
pub fn terminal_resize(session_id: String, cols: u16, rows: u16) {
    let _ = crate::terminal::resize(&session_id, cols, rows);
}

/// 关闭指定会话（杀子进程 + 清理资源）。找不到会话时静默忽略（幂等）。
#[tauri::command]
pub fn terminal_close(session_id: String) {
    let _ = crate::terminal::close(&session_id);
}
