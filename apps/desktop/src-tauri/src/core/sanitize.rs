//! 在敏感数据离开执行器之前对其做尽力而为的脱敏——即在输出被展示、写入审计
//! 日志或（后续）发送给 AI 之前。由 docs/SECURITY_MODEL.zh-Hans.md 强制要求。
//! 这是尽力而为，并非保证：它能降低意外泄露，但不能让任意输出变得绝对安全。

use std::sync::OnceLock;

use regex::Regex;

/// 预编译的脱敏正则集合。
struct Patterns {
    private_key: Regex,
    ipv4: Regex,
    bearer: Regex,
    aws_key: Regex,
    kv_secret: Regex,
    conn_creds: Regex,
}

/// 惰性初始化并返回全局唯一的正则集合。
fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        private_key: Regex::new(
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
        )
        .unwrap(),
        ipv4: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
        bearer: Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._\-]+").unwrap(),
        aws_key: Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
        // 匹配密钥类键名的 key=value / key: value 形式
        kv_secret: Regex::new(
            r"(?i)\b(password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key)\b\s*[=:]\s*\S+",
        )
        .unwrap(),
        // scheme://user:pass@host  → 隐藏其中的 user:pass
        conn_creds: Regex::new(r"([a-zA-Z][a-zA-Z0-9+.\-]*://)[^\s:@/]+:[^\s:@/]+@").unwrap(),
    })
}

/// 从一段文本中脱敏掉密钥及类 PII 的敏感串。
pub fn sanitize(text: &str) -> String {
    let p = patterns();
    // 顺序很重要：先整体去除私钥块。
    let s = p.private_key.replace_all(text, "[redacted-private-key]");
    let s = p.conn_creds.replace_all(&s, "$1[redacted]@");
    let s = p.kv_secret.replace_all(&s, "$1=[redacted]");
    let s = p.bearer.replace_all(&s, "Bearer [redacted]");
    let s = p.aws_key.replace_all(&s, "[redacted-aws-key]");
    let s = p.ipv4.replace_all(&s, "[redacted-ip]");
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // 脱敏 IPv4 地址
    fn redacts_ipv4() {
        assert_eq!(sanitize("connect to 10.0.0.4 ok"), "connect to [redacted-ip] ok");
    }

    #[test]
    // 整体脱敏私钥块
    fn redacts_private_key_block() {
        let s = sanitize("-----BEGIN OPENSSH PRIVATE KEY-----\nabc\ndef\n-----END OPENSSH PRIVATE KEY-----");
        assert_eq!(s, "[redacted-private-key]");
    }

    #[test]
    // 脱敏 key=value 形式的密钥
    fn redacts_kv_secrets() {
        assert!(sanitize("password=hunter2").contains("[redacted]"));
        assert!(!sanitize("password=hunter2").contains("hunter2"));
        assert!(sanitize("API_KEY: sk-abc123").contains("[redacted]"));
    }

    #[test]
    // 脱敏 Bearer Token 与 AWS Access Key
    fn redacts_bearer_and_aws() {
        assert!(sanitize("Authorization: Bearer abc.def.ghi").contains("Bearer [redacted]"));
        assert!(sanitize("AKIAIOSFODNN7EXAMPLE here").contains("[redacted-aws-key]"));
    }

    #[test]
    // 脱敏连接串中的用户名:密码
    fn redacts_connection_string_creds() {
        let s = sanitize("postgres://admin:s3cr3t@db.internal/app");
        assert!(s.contains("postgres://[redacted]@"));
        assert!(!s.contains("s3cr3t"));
    }

    #[test]
    // 普通文本保持原样不动
    fn leaves_plain_text_alone() {
        assert_eq!(sanitize("Active: active (running)"), "Active: active (running)");
    }
}
