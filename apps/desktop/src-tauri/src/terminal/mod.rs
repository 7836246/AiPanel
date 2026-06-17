//! 交互式 SSH 终端。
//!
//! 这是**用户自己操作的真实终端**——它在本地用 portable-pty 开一个伪终端（PTY），
//! 在其中启动系统 OpenSSH 的交互式会话，把按键写进 PTY、把远端输出回调出来。
//!
//! 与 `ssh` 模块的只读/计划执行链路不同，这条链路**不暴露给 Agent**：Codex 永远
//! 拿不到这个会话，也无法通过它触达服务器。它纯粹是给人用的交互式终端。
//!
//! 安全特性：
//! - 三种认证方式与 `ssh` 模块完全一致：agent（BatchMode）、key（-i 临时 0600 文件）、
//!   password（sshpass -e，密码经 SSHPASS 环境变量传入，绝不进 argv）；
//! - 临时私钥文件权限 0600，会话句柄持有它，drop 时删除；
//! - 因为这是交互式逐键会话，**不在此处做整缓冲区脱敏/审计**——脱敏针对的是
//!   发送给 AI 或落库的内容，而交互终端的输出直接回显给操作者本人（见下方 TODO）。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::core::error::{AppError, AppResult};
use crate::core::types::{AuthKind, ServerProfile};

/// 连接 / keepalive 相关选项的取值，与 `ssh` 模块保持一致。
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// 一个活跃的交互式终端会话。所有字段都在注册表的 `Mutex` 内，因此即使
/// 某些 portable-pty 类型本身不是 `Send`，被 `Mutex` 包裹后也可安全跨线程持有。
struct Session {
    /// 向 PTY master 写入（即把用户的按键送进远端）。
    writer: Box<dyn Write + Send>,
    /// 子进程句柄（本地 ssh / sshpass 进程），用于关闭时 kill。
    /// `spawn_command` 返回的就是 `Box<dyn Child + Send + Sync>`，原样持有。
    child: Box<dyn Child + Send + Sync>,
    /// PTY master，用于 resize。
    master: Box<dyn MasterPty + Send>,
    /// 可选的临时私钥文件：仅用于在会话存活期间保活，drop 时自动删除。
    _keyfile: Option<KeyFile>,
}

/// 全局会话注册表：session_id -> Session。
static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();

/// 取回（必要时初始化）全局会话注册表。
fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 临时私钥文件，在 drop 时删除。
///
/// 这里复刻 `ssh::KeyFile` 的思路而非复用它（那是私有项）：写入 0600 权限的临时
/// 文件，并在 drop 时删除。会话句柄持有它，保证密钥文件在整个会话期间存活。
struct KeyFile {
    path: std::path::PathBuf,
}

impl KeyFile {
    fn write(secret: &str) -> AppResult<Self> {
        let path =
            std::env::temp_dir().join(format!("aipanel-term-key-{}", crate::core::types::new_id()));
        std::fs::write(&path, secret)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(KeyFile { path })
    }
}

impl Drop for KeyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 共用的 ssh 安全加固选项（与 `ssh` 模块一致）。
fn common_opts() -> Vec<String> {
    vec![
        "-o".into(),
        format!("ConnectTimeout={CONNECT_TIMEOUT_SECS}"),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ServerAliveInterval=5".into(),
    ]
}

/// 在 PATH 中定位可执行文件（用于探测可选的 `sshpass`）。
fn which(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|p| p.join(bin))
        .find(|p| p.is_file())
}

/// 按服务器的认证方式构造一条**交互式** ssh 的 `CommandBuilder`（无远程命令——
/// 这是交互式登录 shell）。返回 builder 以及可能需要保活的临时密钥文件。
///
/// 与 `ssh::build_invocation` 等价的认证处理：
/// - agent：`ssh` + BatchMode=yes；
/// - key：`ssh` + IdentitiesOnly=yes + `-i 临时0600文件`；
/// - password：`sshpass -e ssh ...` 且把密码塞进 SSHPASS 环境变量（绝不进 argv）。
///
/// 注意：交互式会话**不加** `-tt`——PTY 已经是真实终端，ssh 检测到 stdin 是 tty 会
/// 自动分配远端 pty。
fn build_command(
    server: &ServerProfile,
    secret: Option<&str>,
) -> AppResult<(CommandBuilder, Option<KeyFile>)> {
    let mut keyfile: Option<KeyFile> = None;

    let (program, args, env): (&str, Vec<String>, Option<(String, String)>) = match server.auth_kind
    {
        AuthKind::Agent => {
            let mut a = vec!["-o".into(), "BatchMode=yes".into()];
            a.extend(common_opts());
            a.push("-p".into());
            a.push(server.port.to_string());
            a.push(format!("{}@{}", server.username, server.host));
            ("ssh", a, None)
        }
        AuthKind::Key => {
            let secret = secret.ok_or_else(|| AppError::Credential("no SSH key stored".into()))?;
            let kf = KeyFile::write(secret)?;
            let mut a = vec![
                "-o".into(),
                "BatchMode=yes".into(),
                "-o".into(),
                "IdentitiesOnly=yes".into(),
                "-i".into(),
                kf.path.display().to_string(),
            ];
            a.extend(common_opts());
            a.push("-p".into());
            a.push(server.port.to_string());
            a.push(format!("{}@{}", server.username, server.host));
            keyfile = Some(kf);
            ("ssh", a, None)
        }
        AuthKind::Password => {
            let secret =
                secret.ok_or_else(|| AppError::Credential("no SSH password stored".into()))?;
            if which("sshpass").is_none() {
                return Err(AppError::Ssh(
                    "password auth needs `sshpass` installed; prefer key or agent auth".into(),
                ));
            }
            // sshpass -e 从 SSHPASS 环境变量读取密码（而非 argv，这样密码不会在 `ps` 中可见）。
            let mut a = vec![
                "-e".into(),
                "ssh".into(),
                "-o".into(),
                "PreferredAuthentications=password".into(),
                "-o".into(),
                "PubkeyAuthentication=no".into(),
            ];
            a.extend(common_opts());
            a.push("-p".into());
            a.push(server.port.to_string());
            a.push(format!("{}@{}", server.username, server.host));
            (
                "sshpass",
                a,
                Some(("SSHPASS".to_string(), secret.to_string())),
            )
        }
    };

    let mut cmd = CommandBuilder::new(program);
    cmd.args(&args);
    // 交互式终端类型，保证远端 shell 的颜色 / 光标控制正常工作。
    cmd.env("TERM", "xterm-256color");
    if let Some((k, v)) = env {
        cmd.env(k, v);
    }

    Ok((cmd, keyfile))
}

/// 打开一个交互式 SSH 终端会话。
///
/// 流程：
/// 1) 在本地用 portable-pty 开一个 `cols x rows` 的 PTY；
/// 2) 按认证方式构造交互式 ssh 的 `CommandBuilder`；
/// 3) 在 PTY 的 slave 端 spawn 该命令；拿到 writer（写按键）与 reader（读输出）；
/// 4) 起一个后台线程循环读 reader，把字节按 UTF-8（lossy）转成 String 后调用 `on_data`，
///    直到 EOF / 出错退出；
/// 5) 生成 session_id 并把会话存入注册表，返回 id。
pub fn open(
    server: &ServerProfile,
    secret: Option<&str>,
    cols: u16,
    rows: u16,
    // 返回 false 表示消费端（前端 Channel）已断开,读线程据此结束并回收会话。
    on_data: Box<dyn Fn(String) -> bool + Send + 'static>,
) -> AppResult<String> {
    // 1) 开本地 PTY。
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Ssh(format!("failed to open pty: {e}")))?;

    // 2) 构造交互式 ssh 命令。
    let (cmd, keyfile) = build_command(server, secret)?;

    // 3) 在 slave 端启动 ssh / sshpass，拿到子进程、writer、reader。
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::Ssh(format!("failed to launch interactive ssh: {e}")))?;
    // spawn 之后 slave 已无须保留；显式 drop 释放它的文件描述符。
    drop(pair.slave);

    // spawn 之后若任一步失败,必须 kill 已启动的子进程,否则会留下持有 SSHPASS 密钥
    // 环境的孤儿进程(且无 session_id 可供前端回收)。
    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            let _ = child.kill();
            return Err(AppError::Ssh(format!("failed to take pty writer: {e}")));
        }
    };
    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            let _ = child.kill();
            return Err(AppError::Ssh(format!("failed to clone pty reader: {e}")));
        }
    };

    // 4) 先登记会话(在起读线程之前,避免「立即 EOF 的线程先于 insert 调 close」的竞态)。
    let session_id = crate::core::types::new_id();
    sessions().lock().unwrap().insert(
        session_id.clone(),
        Session {
            writer,
            child,
            master: pair.master,
            _keyfile: keyfile,
        },
    );

    // 5) 后台读线程：把远端输出回调给前端,直到 EOF / 出错 / 前端 Channel 断开;
    // 线程退出即自我回收会话(杀子进程 + 移除注册表),使后端寿命不依赖前端是否调用 close
    // (例如 webview 硬刷新导致 Channel 失效时也能回收)。
    let thread_id = session_id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                // EOF：会话结束，退出线程。
                Ok(0) => break,
                Ok(n) => {
                    // PTY 字节流可能在任意位置切断多字节 UTF-8 序列；from_utf8_lossy
                    // 用替换字符兜底，保证回调拿到的总是合法 String。
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if !on_data(chunk) {
                        break; // 前端 Channel 已断开,无需继续读
                    }
                }
                // 读出错（通常是 master 被关闭）：退出线程。
                Err(_) => break,
            }
        }
        let _ = close(&thread_id);
    });

    Ok(session_id)
}

/// 把用户输入写进会话的 PTY（即送往远端）。找不到会话时报错。
pub fn write(id: &str, data: &str) -> AppResult<()> {
    let mut guard = sessions().lock().unwrap();
    let session = guard
        .get_mut(id)
        .ok_or_else(|| AppError::NotFound(format!("terminal session not found: {id}")))?;
    session
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| AppError::Ssh(format!("failed to write to terminal: {e}")))?;
    session
        .writer
        .flush()
        .map_err(|e| AppError::Ssh(format!("failed to flush terminal: {e}")))?;
    Ok(())
}

/// 调整会话 PTY 的窗口大小（前端容器尺寸变化时调用）。找不到会话时报错。
pub fn resize(id: &str, cols: u16, rows: u16) -> AppResult<()> {
    let guard = sessions().lock().unwrap();
    let session = guard
        .get(id)
        .ok_or_else(|| AppError::NotFound(format!("terminal session not found: {id}")))?;
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Ssh(format!("failed to resize terminal: {e}")))?;
    Ok(())
}

/// 关闭会话：杀掉子进程并从注册表移除。被移除的 `Session` 随之 drop——
/// writer / master 释放、临时密钥文件被删除、读线程因 master 关闭而读到 EOF/出错后退出。
/// 找不到会话时静默忽略（幂等关闭）。
pub fn close(id: &str) -> AppResult<()> {
    let session = sessions().lock().unwrap().remove(id);
    if let Some(mut session) = session {
        // best-effort kill；即便失败也照常 drop 释放资源。
        let _ = session.child.kill();
    }
    Ok(())
}
