//! Best-effort redaction of sensitive data before it leaves the executor —
//! i.e. before output is shown, stored in the audit log, or (later) sent to the
//! AI. Mandated by docs/SECURITY_MODEL.zh-Hans.md. Best-effort, not a guarantee:
//! it reduces accidental leakage; it does not make arbitrary output safe.

use std::sync::OnceLock;

use regex::Regex;

struct Patterns {
    private_key: Regex,
    ipv4: Regex,
    bearer: Regex,
    aws_key: Regex,
    kv_secret: Regex,
    conn_creds: Regex,
}

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
        // key=value / key: value for secret-ish keys
        kv_secret: Regex::new(
            r"(?i)\b(password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key)\b\s*[=:]\s*\S+",
        )
        .unwrap(),
        // scheme://user:pass@host  → hide the user:pass
        conn_creds: Regex::new(r"([a-zA-Z][a-zA-Z0-9+.\-]*://)[^\s:@/]+:[^\s:@/]+@").unwrap(),
    })
}

/// Redact secrets and PII-ish tokens from a block of text.
pub fn sanitize(text: &str) -> String {
    let p = patterns();
    // Order matters: strip whole key blocks first.
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
    fn redacts_ipv4() {
        assert_eq!(sanitize("connect to 10.0.0.4 ok"), "connect to [redacted-ip] ok");
    }

    #[test]
    fn redacts_private_key_block() {
        let s = sanitize("-----BEGIN OPENSSH PRIVATE KEY-----\nabc\ndef\n-----END OPENSSH PRIVATE KEY-----");
        assert_eq!(s, "[redacted-private-key]");
    }

    #[test]
    fn redacts_kv_secrets() {
        assert!(sanitize("password=hunter2").contains("[redacted]"));
        assert!(!sanitize("password=hunter2").contains("hunter2"));
        assert!(sanitize("API_KEY: sk-abc123").contains("[redacted]"));
    }

    #[test]
    fn redacts_bearer_and_aws() {
        assert!(sanitize("Authorization: Bearer abc.def.ghi").contains("Bearer [redacted]"));
        assert!(sanitize("AKIAIOSFODNN7EXAMPLE here").contains("[redacted-aws-key]"));
    }

    #[test]
    fn redacts_connection_string_creds() {
        let s = sanitize("postgres://admin:s3cr3t@db.internal/app");
        assert!(s.contains("postgres://[redacted]@"));
        assert!(!s.contains("s3cr3t"));
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(sanitize("Active: active (running)"), "Active: active (running)");
    }
}
