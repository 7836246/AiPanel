//! SQLite persistence.
//!
//! Stores **non-sensitive** config and the audit index only. Secrets (SSH
//! passwords/keys, API keys) live in the credential store and are referenced
//! here by [`CredentialRef`] — never written in plaintext (see
//! docs/SECURITY_MODEL.zh-Hans.md).

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::core::error::{AppError, AppResult};
use crate::core::types::*;

pub struct Store {
    conn: Mutex<Connection>,
}

const SCHEMA_VERSION: i64 = 1;

impl Store {
    /// Open (and migrate) a database at `path`.
    pub fn open(path: &std::path::Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let store = Store { conn: Mutex::new(conn) };
        store.migrate()?;
        Ok(store)
    }

    /// In-memory store for tests.
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Store { conn: Mutex::new(conn) };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS server_profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    username TEXT NOT NULL,
                    auth_kind TEXT NOT NULL,
                    credential_ref TEXT,
                    status TEXT NOT NULL,
                    facts TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS provider_configs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    base_url TEXT,
                    model TEXT,
                    codex_path TEXT,
                    credential_ref TEXT,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS model_selection_policy (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    data TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY,
                    server_id TEXT,
                    intent TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS audit_records (
                    id TEXT PRIMARY KEY,
                    server_id TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    data TEXT NOT NULL
                );
                "#,
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    // ----- servers -------------------------------------------------------

    pub fn list_servers(&self) -> AppResult<Vec<ServerProfile>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_kind, credential_ref, status, facts, \
             created_at, updated_at FROM server_profiles ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], row_to_server)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_server(&self, id: &str) -> AppResult<ServerProfile> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, host, port, username, auth_kind, credential_ref, status, facts, \
             created_at, updated_at FROM server_profiles WHERE id = ?1",
            params![id],
            row_to_server,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("server {id}")))
    }

    pub fn create_server(&self, input: ServerInput) -> AppResult<ServerProfile> {
        validate_server_input(&input)?;
        let id = new_id();
        let credential_ref = match input.auth_kind {
            AuthKind::Password | AuthKind::Key => Some(CredentialRef::for_server(&id)),
            AuthKind::Agent => None,
        };
        let ts = now();
        let profile = ServerProfile {
            id,
            name: input.name,
            host: input.host,
            port: input.port,
            username: input.username,
            auth_kind: input.auth_kind,
            credential_ref,
            status: ServerStatus::Unknown,
            facts: Default::default(),
            created_at: ts,
            updated_at: ts,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO server_profiles (id, name, host, port, username, auth_kind, \
             credential_ref, status, facts, created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                profile.id,
                profile.name,
                profile.host,
                profile.port,
                profile.username,
                auth_kind_str(profile.auth_kind),
                profile.credential_ref.as_ref().map(|c| c.0.clone()),
                status_str(profile.status),
                serde_json::to_string(&profile.facts)?,
                profile.created_at.to_rfc3339(),
                profile.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(profile)
    }

    pub fn update_server(&self, id: &str, input: ServerInput) -> AppResult<ServerProfile> {
        validate_server_input(&input)?;
        let mut profile = self.get_server(id)?;
        profile.name = input.name;
        profile.host = input.host;
        profile.port = input.port;
        profile.username = input.username;
        profile.auth_kind = input.auth_kind;
        // Keep an existing secret pointer, or mint one if the new auth needs it.
        profile.credential_ref = match input.auth_kind {
            AuthKind::Password | AuthKind::Key => {
                Some(profile.credential_ref.unwrap_or_else(|| CredentialRef::for_server(id)))
            }
            AuthKind::Agent => None,
        };
        profile.updated_at = now();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE server_profiles SET name=?2, host=?3, port=?4, username=?5, auth_kind=?6, \
             credential_ref=?7, updated_at=?8 WHERE id=?1",
            params![
                profile.id,
                profile.name,
                profile.host,
                profile.port,
                profile.username,
                auth_kind_str(profile.auth_kind),
                profile.credential_ref.as_ref().map(|c| c.0.clone()),
                profile.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(profile)
    }

    pub fn delete_server(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM server_profiles WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("server {id}")));
        }
        Ok(())
    }

    // ----- audit ---------------------------------------------------------

    pub fn insert_audit_record(&self, rec: &AuditRecord) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO audit_records (id, server_id, created_at, updated_at, data) \
             VALUES (?1,?2,?3,?4,?5)",
            params![
                rec.id,
                rec.server_id,
                rec.created_at.to_rfc3339(),
                rec.updated_at.to_rfc3339(),
                serde_json::to_string(rec)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_audit_records(&self, limit: u32) -> AppResult<Vec<AuditRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT data FROM audit_records ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for data in rows {
            out.push(serde_json::from_str(&data)?);
        }
        Ok(out)
    }

    pub fn get_audit_record(&self, id: &str) -> AppResult<AuditRecord> {
        let conn = self.conn.lock().unwrap();
        let data: Option<String> = conn
            .query_row("SELECT data FROM audit_records WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        match data {
            Some(d) => Ok(serde_json::from_str(&d)?),
            None => Err(AppError::NotFound(format!("audit record {id}"))),
        }
    }

    /// Update the cached status + facts after a doctor/connectivity run.
    pub fn set_server_status(
        &self,
        id: &str,
        status: ServerStatus,
        facts: Option<&std::collections::BTreeMap<String, String>>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        match facts {
            Some(f) => conn.execute(
                "UPDATE server_profiles SET status=?2, facts=?3, updated_at=?4 WHERE id=?1",
                params![id, status_str(status), serde_json::to_string(f)?, now().to_rfc3339()],
            )?,
            None => conn.execute(
                "UPDATE server_profiles SET status=?2, updated_at=?3 WHERE id=?1",
                params![id, status_str(status), now().to_rfc3339()],
            )?,
        };
        Ok(())
    }
}

// --- row mapping / enum (de)serialization ---------------------------------

fn row_to_server(row: &Row) -> rusqlite::Result<ServerProfile> {
    let facts_json: String = row.get(8)?;
    let facts = serde_json::from_str(&facts_json).unwrap_or_default();
    Ok(ServerProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: row.get(3)?,
        username: row.get(4)?,
        auth_kind: parse_auth_kind(&row.get::<_, String>(5)?),
        credential_ref: row.get::<_, Option<String>>(6)?.map(CredentialRef),
        status: parse_status(&row.get::<_, String>(7)?),
        facts,
        created_at: parse_ts(&row.get::<_, String>(9)?),
        updated_at: parse_ts(&row.get::<_, String>(10)?),
    })
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn auth_kind_str(k: AuthKind) -> &'static str {
    match k {
        AuthKind::Password => "password",
        AuthKind::Key => "key",
        AuthKind::Agent => "agent",
    }
}
fn parse_auth_kind(s: &str) -> AuthKind {
    match s {
        "password" => AuthKind::Password,
        "key" => AuthKind::Key,
        _ => AuthKind::Agent,
    }
}
fn status_str(s: ServerStatus) -> &'static str {
    match s {
        ServerStatus::Online => "online",
        ServerStatus::Offline => "offline",
        ServerStatus::Unknown => "unknown",
    }
}
fn parse_status(s: &str) -> ServerStatus {
    match s {
        "online" => ServerStatus::Online,
        "offline" => ServerStatus::Offline,
        _ => ServerStatus::Unknown,
    }
}

fn validate_server_input(input: &ServerInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("server name is required".into()));
    }
    if input.host.trim().is_empty() {
        return Err(AppError::Validation("host is required".into()));
    }
    if input.username.trim().is_empty() {
        return Err(AppError::Validation("username is required".into()));
    }
    if input.port == 0 {
        return Err(AppError::Validation("port must be non-zero".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> ServerInput {
        ServerInput {
            name: name.into(),
            host: "10.0.0.4".into(),
            port: 22,
            username: "root".into(),
            auth_kind: AuthKind::Password,
        }
    }

    #[test]
    fn create_list_get_update_delete() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.list_servers().unwrap().len(), 0);

        let created = s.create_server(input("web-prod-1")).unwrap();
        assert_eq!(created.status, ServerStatus::Unknown);
        assert_eq!(created.credential_ref, Some(CredentialRef::for_server(&created.id)));

        assert_eq!(s.list_servers().unwrap().len(), 1);

        let got = s.get_server(&created.id).unwrap();
        assert_eq!(got.name, "web-prod-1");

        let mut up = input("web-prod-1");
        up.name = "renamed".into();
        up.auth_kind = AuthKind::Agent;
        let updated = s.update_server(&created.id, up).unwrap();
        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.credential_ref, None);

        s.delete_server(&created.id).unwrap();
        assert_eq!(s.list_servers().unwrap().len(), 0);
    }

    #[test]
    fn get_missing_is_not_found() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.get_server("nope").unwrap_err().code(), "not_found");
    }

    #[test]
    fn validation_rejects_blank_name() {
        let s = Store::open_in_memory().unwrap();
        let mut i = input("x");
        i.name = "  ".into();
        assert_eq!(s.create_server(i).unwrap_err().code(), "validation");
    }

    #[test]
    fn audit_records_round_trip() {
        let s = Store::open_in_memory().unwrap();
        let rec = AuditRecord {
            id: new_id(),
            server_id: Some("srv".into()),
            intent: "只读体检".into(),
            plan: None,
            risk_review: None,
            confirmed_at: Some(now()),
            executions: vec![],
            summary: Some("ok".into()),
            status: TaskStatus::Completed,
            created_at: now(),
            updated_at: now(),
        };
        s.insert_audit_record(&rec).unwrap();
        let listed = s.list_audit_records(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].intent, "只读体检");
        let got = s.get_audit_record(&rec.id).unwrap();
        assert_eq!(got.summary.as_deref(), Some("ok"));
        assert_eq!(s.get_audit_record("missing").unwrap_err().code(), "not_found");
    }

    #[test]
    fn status_and_facts_persist() {
        let s = Store::open_in_memory().unwrap();
        let c = s.create_server(input("db")).unwrap();
        let mut facts = std::collections::BTreeMap::new();
        facts.insert("OS".to_string(), "Ubuntu 22.04".to_string());
        s.set_server_status(&c.id, ServerStatus::Online, Some(&facts)).unwrap();
        let got = s.get_server(&c.id).unwrap();
        assert_eq!(got.status, ServerStatus::Online);
        assert_eq!(got.facts.get("OS").unwrap(), "Ubuntu 22.04");
    }
}
