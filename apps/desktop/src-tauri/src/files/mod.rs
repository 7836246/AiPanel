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
use crate::ssh::{run_command, run_command_with_input, DEFAULT_TIMEOUT};

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
    let q = shell_quote(path);

    // 首选：GNU find，maxdepth 1 + mindepth 1 只列出目录下的直接子项（不含目录自身）。
    let find_cmd = format!(
        "find {q} -maxdepth 1 -mindepth 1 -printf '%y\\t%s\\t%TY-%Tm-%TdT%TH:%TM\\t%f\\n'"
    );
    let find_exec = run_command(server, secret, &find_cmd, DEFAULT_TIMEOUT).await?;

    let mut entries = if find_exec.exit_code == 0 {
        parse_find_output(&find_exec.stdout)
    } else {
        // 回退：ls -la（非 GNU find、BusyBox 等环境）。
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
