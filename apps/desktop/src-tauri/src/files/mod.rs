//! 文件管理（SFTP over SSH）—— **用户直接操作**的文件浏览/读取/写入。
//!
//! 安全边界：本模块的能力**绝不暴露给 AI / Agent**。Agent 只能通过 AiPanel
//! Tools 触达服务器（见 `tools/`），文件管理是面向人的功能，由前端命令
//! （`commands/files.rs`）直接驱动。
//!
//! 实现方式：复用 SSH 执行器（`ssh::run_command` / `ssh::run_command_with_input`），
//! 不引入独立的 SFTP 协议栈——通过远端的标准工具（`find`/`ls`/`head`/`cat`）完成。
//! 输出离开 SSH 执行器前都已脱敏（IP/密钥/token 等会被改写——这对文件内容是可接受的取舍）。

use crate::core::error::{AppError, AppResult};
use crate::core::types::{DirListing, FileContent, FileEntry, FileKind, ServerProfile};
use crate::ssh::{run_command, run_command_with_input, run_scp, DEFAULT_TIMEOUT};

/// 读取文件的上限：~256KB。多取 1 字节（262145 = 262144 + 1），
/// 若实际读到 > 262144 字节即说明文件被截断。
const READ_LIMIT: usize = 262_144;
const READ_PROBE: usize = READ_LIMIT + 1;

/// 把字符串安全地包进 shell 单引号，防止路径里的特殊字符破坏命令。
/// 单引号内除了单引号本身没有任何转义，因此把每个 `'` 替换为 `'\''`
/// （闭合、转义一个字面单引号、再重新开启）。
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn validate_remote_path(path: &str) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::Validation("remote path is required".into()));
    }
    if path.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "remote path must not contain control characters".into(),
        ));
    }
    Ok(())
}

fn validate_remote_file_write_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    let trimmed = path.trim();
    if matches!(trimmed, "/" | "." | ".." | "~") {
        return Err(AppError::Validation(
            "remote file path must target a file, not a directory root".into(),
        ));
    }
    if trimmed.ends_with('/') {
        return Err(AppError::Validation(
            "remote file path must not end with /".into(),
        ));
    }
    Ok(())
}

fn validate_local_upload_file(path: &str) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::Validation("local upload path is required".into()));
    }
    if path.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "local upload path must not contain control characters".into(),
        ));
    }
    let meta = std::fs::metadata(path)
        .map_err(|e| AppError::Validation(format!("local upload file is not accessible: {e}")))?;
    if !meta.is_file() {
        return Err(AppError::Validation(
            "local upload path must be a regular file".into(),
        ));
    }
    Ok(())
}

fn validate_local_download_target(path: &str) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::Validation("local download path is required".into()));
    }
    if path.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "local download path must not contain control characters".into(),
        ));
    }
    let target = std::path::Path::new(path);
    if target.is_dir() {
        return Err(AppError::Validation(
            "local download path must be a file, not a directory".into(),
        ));
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(AppError::Validation(
                "local download parent directory does not exist".into(),
            ));
        }
    }
    Ok(())
}

fn list_find_command(path: &str) -> AppResult<String> {
    validate_remote_path(path)?;
    let q = shell_quote(path);
    Ok(format!(
        "find -- {q} -maxdepth 1 -mindepth 1 -printf '%y\\t%s\\t%TY-%Tm-%TdT%TH:%TM\\t%f\\n'"
    ))
}

/// 把 `find -printf '%y'` 的类型字符映射为 [`FileKind`]：
/// `d`=目录，`l`=符号链接，其余（普通文件/设备/管道等）一律视为 File。
fn kind_from_type_char(c: &str) -> FileKind {
    match c {
        "d" => FileKind::Dir,
        "l" => FileKind::Link,
        _ => FileKind::File,
    }
}

/// 排序：目录在前，再按名称（不区分大小写）排列。
fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        let a_dir = a.kind == FileKind::Dir;
        let b_dir = b.kind == FileKind::Dir;
        b_dir
            .cmp(&a_dir) // dir(true) 排前
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// 解析 GNU find 的清晰分隔输出。每行形如：
/// `<type>\t<size>\t<mtime>\t<name>`，由 `-printf '%y\t%s\t%TY-%Tm-%TdT%TH:%TM\t%f\n'` 产生。
/// 名称里可能含制表符的情况极少；用 splitn(4) 保证只切前三个分隔，name 取剩余全部。
fn parse_find_output(out: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    for line in out.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        if parts.len() != 4 {
            continue; // 形状不符的行（脱敏改写等）直接跳过
        }
        let kind = kind_from_type_char(parts[0]);
        let size = parts[1].trim().parse::<u64>().unwrap_or(0);
        let mtime = parts[2].trim().to_string();
        let name = parts[3].to_string();
        entries.push(FileEntry { name, kind, size, mtime });
    }
    entries
}

/// 解析 `ls -la` 的回退输出。逐行取最后一列作为名称、第 5 列作为大小、
/// 首字符判断类型（`d`/`l`/其它）。跳过 `total` 行与 `.`/`..`。
fn parse_ls_output(out: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    for line in out.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 典型 `ls -la` 至少有 9 列：权限 链接数 属主 属组 大小 月 日 时间 名称…
        if cols.len() < 9 {
            continue;
        }
        let perms = cols[0];
        let kind = match perms.chars().next() {
            Some('d') => FileKind::Dir,
            Some('l') => FileKind::Link,
            _ => FileKind::File,
        };
        let size = cols[4].parse::<u64>().unwrap_or(0);
        // 名称是第 9 列起的剩余部分；符号链接形如 `name -> target`，取 `->` 前的部分。
        let name_part = cols[8..].join(" ");
        let name = name_part
            .split(" -> ")
            .next()
            .unwrap_or(&name_part)
            .to_string();
        if name == "." || name == ".." || name.is_empty() {
            continue;
        }
        // ls 的时间列没有统一的机读格式，这里留空 mtime（前端按缺省展示）。
        entries.push(FileEntry { name, kind, size, mtime: String::new() });
    }
    entries
}

/// 列举远程目录。优先用 GNU find 的机读输出；失败（非 GNU find、权限等）回退到 `ls -la`。
/// `path` 支持 `~` / `.` 等由远端 shell 自行展开（命令在 shell 下执行）。
pub async fn list(
    server: &ServerProfile,
    secret: Option<&str>,
    path: &str,
) -> AppResult<DirListing> {
    // 首选：GNU find，maxdepth 1 + mindepth 1 只列出目录下的直接子项（不含目录自身）。
    let find_cmd = list_find_command(path)?;
    let find_exec = run_command(server, secret, &find_cmd, DEFAULT_TIMEOUT).await?;

    let mut entries = if find_exec.exit_code == 0 {
        parse_find_output(&find_exec.stdout)
    } else {
        // 回退：ls -la（非 GNU find、BusyBox 等环境）。
        let q = shell_quote(path);
        let ls_cmd = format!("ls -la -- {q}");
        let ls_exec = run_command(server, secret, &ls_cmd, DEFAULT_TIMEOUT).await?;
        if ls_exec.exit_code != 0 {
            // 两种方式都失败：把 find 的 stderr 作为错误信息（已脱敏）返回。
            let msg = if !find_exec.stderr.trim().is_empty() {
                find_exec.stderr
            } else if !ls_exec.stderr.trim().is_empty() {
                ls_exec.stderr
            } else {
                format!("无法列举目录: {path}")
            };
            return Err(AppError::Ssh(msg.trim().to_string()));
        }
        parse_ls_output(&ls_exec.stdout)
    };

    sort_entries(&mut entries);
    Ok(DirListing { path: path.to_string(), entries })
}

/// 读取文件内容，最多 ~256KB。用 `head -c` 取前 262145 字节，
/// 若实际取到 > 262144 字节即判定为被截断（content 为前缀）。
pub async fn read(
    server: &ServerProfile,
    secret: Option<&str>,
    path: &str,
) -> AppResult<FileContent> {
    validate_remote_path(path)?;
    let q = shell_quote(path);
    let cmd = format!("head -c {READ_PROBE} -- {q}");
    let exec = run_command(server, secret, &cmd, DEFAULT_TIMEOUT).await?;
    if exec.exit_code != 0 {
        let msg = if exec.stderr.trim().is_empty() {
            format!("无法读取文件: {path}")
        } else {
            exec.stderr.trim().to_string()
        };
        return Err(AppError::Ssh(msg));
    }

    // 按字节长度判断是否截断；判定后只保留前 READ_LIMIT 字节（在字符边界处安全裁剪）。
    let bytes = exec.stdout.as_bytes();
    let truncated = bytes.len() > READ_LIMIT;
    let content = if truncated {
        // 在不超过 READ_LIMIT 的最近字符边界处截断，避免切碎多字节字符。
        let mut end = READ_LIMIT;
        while end > 0 && !exec.stdout.is_char_boundary(end) {
            end -= 1;
        }
        exec.stdout[..end].to_string()
    } else {
        exec.stdout
    };

    Ok(FileContent { path: path.to_string(), content, truncated })
}

/// 写入文件：用 `cat > 路径` 把内容经 stdin 落盘（内容绝不进 argv）。
pub async fn write(
    server: &ServerProfile,
    secret: Option<&str>,
    path: &str,
    content: &str,
) -> AppResult<()> {
    validate_remote_file_write_path(path)?;
    let q = shell_quote(path);
    let cmd = format!("cat > {q}");
    let exec = run_command_with_input(server, secret, &cmd, content, DEFAULT_TIMEOUT).await?;
    if exec.exit_code != 0 {
        let msg = if exec.stderr.trim().is_empty() {
            format!("无法写入文件: {path}")
        } else {
            exec.stderr.trim().to_string()
        };
        return Err(AppError::Ssh(msg));
    }
    Ok(())
}

/// 把本地文件**上传**到远程目录（scp over SSH）—— **用户直接操作**，绝不暴露给 AI。
///
/// 目的串形如 `user@host:'<remote_dir>'`：远端路径放进 scp 的 `host:` 之后会被
/// **远端 shell** 再次解析，因此对 `remote_dir` 用单引号包裹（`shell_quote`）防止
/// 空格/特殊字符破坏路径。`local_path` 作为本地 argv 参数直接传给 scp（不经本地
/// shell），无需 shell 转义。
pub async fn upload(
    server: &ServerProfile,
    secret: Option<&str>,
    local_path: &str,
    remote_dir: &str,
) -> AppResult<()> {
    validate_remote_path(remote_dir)?;
    validate_local_upload_file(local_path)?;
    // 远端路径经远端 shell 解析，单引号包裹做最小转义。
    let dest = format!(
        "{}@{}:{}",
        server.username,
        server.host,
        shell_quote(remote_dir)
    );
    run_scp(
        server,
        secret,
        vec![local_path.to_string(), dest],
        DEFAULT_TIMEOUT,
    )
    .await
}

/// 把远程文件**下载**到本地路径（scp over SSH）—— **用户直接操作**，绝不暴露给 AI。
///
/// 源串形如 `user@host:'<remote_path>'`：远端路径同样经远端 shell 解析，单引号
/// 包裹做最小转义。`local_path` 作为本地 argv 参数直接传给 scp（不经本地 shell）。
pub async fn download(
    server: &ServerProfile,
    secret: Option<&str>,
    remote_path: &str,
    local_path: &str,
) -> AppResult<()> {
    validate_remote_path(remote_path)?;
    validate_local_download_target(local_path)?;
    let src = format!(
        "{}@{}:{}",
        server.username,
        server.host,
        shell_quote(remote_path)
    );
    run_scp(
        server,
        secret,
        vec![src, local_path.to_string()],
        DEFAULT_TIMEOUT,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_wraps_and_escapes() {
        assert_eq!(shell_quote("/etc/nginx"), "'/etc/nginx'");
        // 含空格
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        // 含单引号：闭合 -> 转义单引号 -> 重开
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn remote_path_validation_rejects_empty_and_control_chars() {
        assert!(validate_remote_path("/etc/nginx").is_ok());
        assert!(validate_remote_path(".").is_ok());
        assert_eq!(validate_remote_path("  ").unwrap_err().code(), "validation");
        assert_eq!(validate_remote_path("/tmp/a\nb").unwrap_err().code(), "validation");
        assert_eq!(validate_remote_path("/tmp/a\0b").unwrap_err().code(), "validation");
    }

    #[test]
    fn file_write_path_validation_rejects_directory_targets() {
        assert!(validate_remote_file_write_path("/tmp/app.conf").is_ok());
        assert!(validate_remote_file_write_path("relative/file.txt").is_ok());

        for path in ["/", ".", "..", "~", "/etc/nginx/"] {
            let err = validate_remote_file_write_path(path).unwrap_err();
            assert_eq!(err.code(), "validation", "{path}");
        }
    }

    #[test]
    fn local_upload_validation_requires_existing_regular_file() {
        let dir = std::env::temp_dir().join(format!(
            "aipanel-files-test-{}",
            crate::core::types::new_id()
        ));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("app.conf");
        std::fs::write(&file, "ok").unwrap();

        assert!(validate_local_upload_file(file.to_str().unwrap()).is_ok());

        let err = validate_local_upload_file(dir.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "validation");
        assert!(err.to_string().contains("regular file"));

        let missing = dir.join("missing.txt");
        let err = validate_local_upload_file(missing.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "validation");

        assert_eq!(validate_local_upload_file("  ").unwrap_err().code(), "validation");
        assert_eq!(
            validate_local_upload_file("/tmp/a\nb").unwrap_err().code(),
            "validation"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_download_validation_requires_file_target_with_existing_parent() {
        let dir = std::env::temp_dir().join(format!(
            "aipanel-files-test-{}",
            crate::core::types::new_id()
        ));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("download.txt");

        assert!(validate_local_download_target(file.to_str().unwrap()).is_ok());

        let err = validate_local_download_target(dir.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "validation");
        assert!(err.to_string().contains("not a directory"));

        let missing_parent = dir.join("missing").join("download.txt");
        let err = validate_local_download_target(missing_parent.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "validation");
        assert!(err.to_string().contains("parent directory"));

        assert_eq!(validate_local_download_target("  ").unwrap_err().code(), "validation");
        assert_eq!(
            validate_local_download_target("/tmp/a\nb").unwrap_err().code(),
            "validation"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_find_command_uses_option_separator_before_path() {
        let cmd = list_find_command("-looks-like-option").unwrap();
        assert!(cmd.starts_with("find -- '-looks-like-option' "), "{cmd}");
    }

    #[test]
    fn parse_find_typical_lines() {
        let out = "d\t4096\t2026-06-18T14:30\tsub\nf\t12\t2026-01-02T03:04\tfile.txt\nl\t7\t2026-01-02T03:04\tlink";
        let entries = parse_find_output(out);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].kind, FileKind::Dir);
        assert_eq!(entries[0].name, "sub");
        assert_eq!(entries[0].size, 4096);
        assert_eq!(entries[0].mtime, "2026-06-18T14:30");
        assert_eq!(entries[1].kind, FileKind::File);
        assert_eq!(entries[2].kind, FileKind::Link);
    }

    #[test]
    fn parse_find_skips_malformed() {
        let out = "garbage line without tabs\nf\t1\t2026-01-01T00:00\tok";
        let entries = parse_find_output(out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "ok");
    }

    #[test]
    fn sort_puts_dirs_first_then_name() {
        let mut entries = vec![
            FileEntry { name: "zebra.txt".into(), kind: FileKind::File, size: 0, mtime: String::new() },
            FileEntry { name: "Apple".into(), kind: FileKind::Dir, size: 0, mtime: String::new() },
            FileEntry { name: "apple.txt".into(), kind: FileKind::File, size: 0, mtime: String::new() },
            FileEntry { name: "beta".into(), kind: FileKind::Dir, size: 0, mtime: String::new() },
        ];
        sort_entries(&mut entries);
        assert_eq!(entries[0].name, "Apple"); // 目录在前
        assert_eq!(entries[1].name, "beta");
        assert_eq!(entries[2].name, "apple.txt");
        assert_eq!(entries[3].name, "zebra.txt");
    }

    #[test]
    fn parse_ls_fallback() {
        let out = "total 8\n\
drwxr-xr-x 2 root root 4096 Jun 18 14:30 sub\n\
-rw-r--r-- 1 root root   12 Jan  2 03:04 file.txt\n\
lrwxrwxrwx 1 root root    7 Jan  2 03:04 link -> target\n\
drwxr-xr-x 2 root root 4096 Jun 18 14:30 .\n\
drwxr-xr-x 3 root root 4096 Jun 18 14:30 ..";
        let entries = parse_ls_output(out);
        assert_eq!(entries.len(), 3); // . 与 .. 被跳过
        assert_eq!(entries[0].kind, FileKind::Dir);
        assert_eq!(entries[0].name, "sub");
        assert_eq!(entries[2].kind, FileKind::Link);
        assert_eq!(entries[2].name, "link"); // 取 -> 之前
    }
}
