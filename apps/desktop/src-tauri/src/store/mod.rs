//! SQLite 持久化。
//!
//! 只存放**非敏感**配置与审计索引。密钥（SSH 密码/密钥、API Key）存放在凭据
//! 存储中，这里仅以 [`CredentialRef`] 引用——绝不明文写入（见
//! docs/SECURITY_MODEL.zh-Hans.md）。

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::core::error::{AppError, AppResult};
use crate::core::types::*;

/// 对 SQLite 连接的封装，所有持久化操作的入口。
pub struct Store {
    conn: Mutex<Connection>,
}

/// 当前数据库 schema 版本号。
const SCHEMA_VERSION: i64 = 3;

impl Store {
    /// 打开（并迁移）位于 `path` 的数据库。
    pub fn open(path: &std::path::Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let store = Store { conn: Mutex::new(conn) };
        store.migrate()?;
        Ok(store)
    }

    /// 供测试使用的内存数据库。
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Store { conn: Mutex::new(conn) };
        store.migrate()?;
        Ok(store)
    }

    /// 按版本号增量执行 schema 迁移，并更新 user_version。
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
        }
        if version < 2 {
            // 面向用户的运行历史：保留现有可查询列，并把完整的 TaskRecord JSON
            // 存进 `data` 列（与 audit_records 一致）。
            conn.execute_batch(
                "ALTER TABLE tasks ADD COLUMN data TEXT NOT NULL DEFAULT '{}';",
            )?;
        }
        if version < 3 {
            // 服务器收藏：新增 favorite 列，默认 0（未收藏）。
            conn.execute_batch(
                "ALTER TABLE server_profiles ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // ----- servers -------------------------------------------------------

    /// 列出所有服务器（按创建时间升序）。
    pub fn list_servers(&self) -> AppResult<Vec<ServerProfile>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_kind, credential_ref, status, facts, \
             created_at, updated_at, favorite FROM server_profiles \
             ORDER BY favorite DESC, created_at ASC",
        )?;
        let rows = stmt
            .query_map([], row_to_server)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 按 id 获取服务器，不存在则返回 NotFound。
    pub fn get_server(&self, id: &str) -> AppResult<ServerProfile> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, host, port, username, auth_kind, credential_ref, status, facts, \
             created_at, updated_at, favorite FROM server_profiles WHERE id = ?1",
            params![id],
            row_to_server,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("server {id}")))
    }

    /// 校验并创建一台服务器；需要密钥的认证方式会分配一个凭据引用。
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
            favorite: false,
            facts: Default::default(),
            created_at: ts,
            updated_at: ts,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO server_profiles (id, name, host, port, username, auth_kind, \
             credential_ref, status, facts, created_at, updated_at, favorite) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
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
                profile.favorite as i64,
            ],
        )?;
        Ok(profile)
    }

    /// 校验并更新一台服务器；按新的认证方式保留或新建凭据引用。
    pub fn update_server(&self, id: &str, input: ServerInput) -> AppResult<ServerProfile> {
        validate_server_input(&input)?;
        let mut profile = self.get_server(id)?;
        profile.name = input.name;
        profile.host = input.host;
        profile.port = input.port;
        profile.username = input.username;
        profile.auth_kind = input.auth_kind;
        // 保留已有的密钥引用；若新的认证方式需要而又没有，则新建一个。
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

    /// 删除一台服务器，不存在则返回 NotFound。
    pub fn delete_server(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM server_profiles WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("server {id}")));
        }
        Ok(())
    }

    // ----- providers / model policy --------------------------------------

    /// 列出所有模型供应商配置（按创建时间升序）。
    pub fn list_providers(&self) -> AppResult<Vec<ProviderConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, base_url, model, codex_path, credential_ref, enabled, \
             created_at, updated_at FROM provider_configs ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], row_to_provider)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 按 id 获取供应商配置，不存在则返回 NotFound。
    pub fn get_provider(&self, id: &str) -> AppResult<ProviderConfig> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, kind, base_url, model, codex_path, credential_ref, enabled, \
             created_at, updated_at FROM provider_configs WHERE id = ?1",
            params![id],
            row_to_provider,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("provider {id}")))
    }

    /// 插入或按 id 替换一条供应商配置。
    pub fn upsert_provider(&self, p: &ProviderConfig) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO provider_configs (id, name, kind, base_url, model, codex_path, \
             credential_ref, enabled, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                p.id,
                p.name,
                provider_kind_str(p.kind),
                p.base_url,
                p.model,
                p.codex_path,
                p.credential_ref.as_ref().map(|c| c.0.clone()),
                p.enabled as i64,
                p.created_at.to_rfc3339(),
                p.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 仅更新某个供应商的激活模型（model 列）与 updated_at，不触碰其它列
    /// （凭据引用、base_url、enabled 等）。`model` 为 None 时把该列置 NULL。
    /// 返回更新后的 ProviderConfig；供应商不存在则返回 NotFound。
    pub fn set_provider_model(&self, id: &str, model: Option<&str>) -> AppResult<ProviderConfig> {
        {
            let conn = self.conn.lock().unwrap();
            let n = conn.execute(
                "UPDATE provider_configs SET model=?2, updated_at=?3 WHERE id=?1",
                params![id, model, now().to_rfc3339()],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!("provider {id}")));
            }
        }
        self.get_provider(id)
    }

    /// 删除一条供应商配置，不存在则返回 NotFound。
    pub fn delete_provider(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM provider_configs WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }

    /// 读取模型选择策略，未设置时返回默认值。
    pub fn get_policy(&self) -> AppResult<ModelSelectionPolicy> {
        let conn = self.conn.lock().unwrap();
        let data: Option<String> = conn
            .query_row("SELECT data FROM model_selection_policy WHERE id = 1", [], |r| r.get(0))
            .optional()?;
        match data {
            Some(d) => Ok(serde_json::from_str(&d)?),
            None => Ok(ModelSelectionPolicy::default()),
        }
    }

    /// 写入（覆盖）模型选择策略。
    pub fn set_policy(&self, p: &ModelSelectionPolicy) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO model_selection_policy (id, data) VALUES (1, ?1)",
            params![serde_json::to_string(p)?],
        )?;
        Ok(())
    }

    // ----- audit ---------------------------------------------------------

    /// 插入或按 id 替换一条审计记录（完整 JSON 存入 `data` 列）。
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

    /// 列出最近的审计记录（按创建时间倒序，最多 `limit` 条）。
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

    /// 在审计记录中做不区分大小写的子串搜索（意图 / 总结 / 命令）。
    ///
    /// 空 query 退化为 [`list_audit_records`](Self::list_audit_records)。完整 JSON
    /// 已脱敏（执行输出不含密钥），因此对其整体做 SQL LIKE 即可命中
    /// intent/summary/命令等字段；再在 Rust 侧按上限截断。
    pub fn search_audit_records(&self, query: &str, limit: u32) -> AppResult<Vec<AuditRecord>> {
        let q = query.trim();
        if q.is_empty() {
            return self.list_audit_records(limit);
        }
        let conn = self.conn.lock().unwrap();
        // SQLite 的 LIKE 对 ASCII 默认不区分大小写；为兼顾非 ASCII，
        // 统一对 data 列与模式做 lower() 处理。
        let pattern = format!("%{}%", escape_like(&q.to_lowercase()));
        let mut stmt = conn.prepare(
            "SELECT data FROM audit_records \
             WHERE lower(data) LIKE ?1 ESCAPE '\\' \
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for data in rows {
            match serde_json::from_str(&data) {
                Ok(r) => out.push(r),
                Err(e) => eprintln!("[store] skipping unreadable audit row: {e}"),
            }
        }
        Ok(out)
    }

    /// 按 id 获取一条审计记录，不存在则返回 NotFound。
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

    // ----- tasks (run history) -------------------------------------------

    /// 插入或按 id 替换一条运行历史记录（完整 JSON 存入 `data` 列）。
    pub fn upsert_task(&self, rec: &TaskRecord) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO tasks (id, server_id, intent, status, created_at, updated_at, data) \
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                rec.id,
                rec.server_id,
                rec.intent,
                task_status_str(rec.status),
                rec.created_at.to_rfc3339(),
                rec.updated_at.to_rfc3339(),
                serde_json::to_string(rec)?,
            ],
        )?;
        Ok(())
    }

    /// 列出运行历史，可按服务器过滤（按创建时间倒序，最多 `limit` 条）。
    pub fn list_tasks(&self, server_id: Option<&str>, limit: u32) -> AppResult<Vec<TaskRecord>> {
        let conn = self.conn.lock().unwrap();
        let rows: Vec<String> = match server_id {
            Some(sid) => {
                let mut stmt = conn.prepare(
                    "SELECT data FROM tasks WHERE server_id = ?1 ORDER BY created_at DESC LIMIT ?2",
                )?;
                let v = stmt
                    .query_map(params![sid, limit], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                v
            }
            None => {
                let mut stmt =
                    conn.prepare("SELECT data FROM tasks ORDER BY created_at DESC LIMIT ?1")?;
                let v = stmt
                    .query_map(params![limit], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                v
            }
        };
        let mut out = Vec::with_capacity(rows.len());
        for data in rows {
            // 跳过无法解析的行，而不是让整个历史查询失败（例如遗留/空行，
            // 或未来的 schema 漂移）。
            match serde_json::from_str(&data) {
                Ok(t) => out.push(t),
                Err(e) => eprintln!("[store] skipping unreadable task row: {e}"),
            }
        }
        Ok(out)
    }

    /// 在运行历史中做不区分大小写的子串搜索（标题 / 意图），可按服务器过滤。
    ///
    /// 空 query 退化为 [`list_tasks`](Self::list_tasks)。完整 JSON 已脱敏，
    /// 对其整体做 SQL LIKE 可命中 title/intent 等字段；再在 Rust 侧按上限截断。
    pub fn search_tasks(
        &self,
        server_id: Option<&str>,
        query: &str,
        limit: u32,
    ) -> AppResult<Vec<TaskRecord>> {
        let q = query.trim();
        if q.is_empty() {
            return self.list_tasks(server_id, limit);
        }
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", escape_like(&q.to_lowercase()));
        let rows: Vec<String> = match server_id {
            Some(sid) => {
                let mut stmt = conn.prepare(
                    "SELECT data FROM tasks \
                     WHERE server_id = ?1 AND lower(data) LIKE ?2 ESCAPE '\\' \
                     ORDER BY created_at DESC LIMIT ?3",
                )?;
                let v = stmt
                    .query_map(params![sid, pattern, limit], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                v
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT data FROM tasks \
                     WHERE lower(data) LIKE ?1 ESCAPE '\\' \
                     ORDER BY created_at DESC LIMIT ?2",
                )?;
                let v = stmt
                    .query_map(params![pattern, limit], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                v
            }
        };
        let mut out = Vec::with_capacity(rows.len());
        for data in rows {
            match serde_json::from_str(&data) {
                Ok(t) => out.push(t),
                Err(e) => eprintln!("[store] skipping unreadable task row: {e}"),
            }
        }
        Ok(out)
    }

    /// 导出全部审计记录为格式化 JSON 字符串（最新在前）。
    ///
    /// 记录中的执行输出已脱敏、不含密钥，可安全交给前端写盘/分享。
    pub fn export_audit_records_json(&self) -> AppResult<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT data FROM audit_records ORDER BY created_at DESC")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut records: Vec<AuditRecord> = Vec::with_capacity(rows.len());
        for data in rows {
            match serde_json::from_str(&data) {
                Ok(r) => records.push(r),
                Err(e) => eprintln!("[store] skipping unreadable audit row on export: {e}"),
            }
        }
        Ok(serde_json::to_string_pretty(&records)?)
    }

    /// 按 id 获取一条运行历史，不存在则返回 NotFound。
    pub fn get_task(&self, id: &str) -> AppResult<TaskRecord> {
        let conn = self.conn.lock().unwrap();
        let data: Option<String> = conn
            .query_row("SELECT data FROM tasks WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        match data {
            Some(d) => Ok(serde_json::from_str(&d)?),
            None => Err(AppError::NotFound(format!("task {id}"))),
        }
    }

    /// 删除一条运行历史，不存在则返回 NotFound。
    pub fn delete_task(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("task {id}")));
        }
        Ok(())
    }

    /// 在一次体检/连通性检测后更新缓存的状态与快速信息（facts）。
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
            // 仅更新状态（连通性检测）时不动 updated_at——它表示「facts 的新鲜度」，
            // 概览页据此显示「更新于」，纯连通刷新不应让该时间戳跳到当前。
            None => conn.execute(
                "UPDATE server_profiles SET status=?2 WHERE id=?1",
                params![id, status_str(status)],
            )?,
        };
        Ok(())
    }

    /// 设置某台服务器的收藏状态，返回更新后的 ServerProfile。
    pub fn set_server_favorite(&self, id: &str, favorite: bool) -> AppResult<ServerProfile> {
        {
            let conn = self.conn.lock().unwrap();
            let n = conn.execute(
                "UPDATE server_profiles SET favorite=?2, updated_at=?3 WHERE id=?1",
                params![id, favorite as i64, now().to_rfc3339()],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!("server {id}")));
            }
        }
        self.get_server(id)
    }
}

// --- 行映射 / 枚举（反）序列化 ---------------------------------

/// 把一行 server_profiles 映射为 ServerProfile。
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
        favorite: row.get::<_, i64>(11)? != 0,
        facts,
        created_at: parse_ts(&row.get::<_, String>(9)?),
        updated_at: parse_ts(&row.get::<_, String>(10)?),
    })
}

/// 把一行 provider_configs 映射为 ProviderConfig。
fn row_to_provider(row: &Row) -> rusqlite::Result<ProviderConfig> {
    Ok(ProviderConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: parse_provider_kind(&row.get::<_, String>(2)?),
        base_url: row.get(3)?,
        model: row.get(4)?,
        codex_path: row.get(5)?,
        credential_ref: row.get::<_, Option<String>>(6)?.map(CredentialRef),
        enabled: row.get::<_, i64>(7)? != 0,
        created_at: parse_ts(&row.get::<_, String>(8)?),
        updated_at: parse_ts(&row.get::<_, String>(9)?),
    })
}

/// ProviderKind 与其存储字符串之间的转换。
fn provider_kind_str(k: ProviderKind) -> &'static str {
    match k {
        ProviderKind::CodexAppServer => "codex_app_server",
        ProviderKind::OpenAiCompatible => "openai_compatible",
        ProviderKind::Custom => "custom",
    }
}
fn parse_provider_kind(s: &str) -> ProviderKind {
    match s {
        "codex_app_server" => ProviderKind::CodexAppServer,
        "openai_compatible" => ProviderKind::OpenAiCompatible,
        _ => ProviderKind::Custom,
    }
}

/// 转义 SQL LIKE 模式中的特殊字符（`\`、`%`、`_`），配合 `ESCAPE '\'` 使用，
/// 使用户输入只作为普通子串匹配，而不被当作通配符。
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// 解析 RFC3339 时间戳，失败时回退为当前时间。
fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// AuthKind 与其存储字符串之间的转换。
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
/// 存入 `tasks.status` 列（JSON 中 `status` 的可查询镜像）。
/// 与 `TaskStatus` 的 snake_case serde 表示保持一致。
fn task_status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Pending => "pending",
        TaskStatus::Planning => "planning",
        TaskStatus::AwaitingConfirmation => "awaiting_confirmation",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Blocked => "blocked",
    }
}

/// ServerStatus 与其存储字符串之间的转换。
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

/// 校验创建/更新服务器的输入，缺少必填字段则返回 Validation 错误。
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
    fn tasks_round_trip_and_filter() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.list_tasks(None, 10).unwrap().is_empty());

        let rec = TaskRecord {
            id: new_id(),
            server_id: Some("srv".into()),
            title: "只读体检".into(),
            intent: "看看磁盘".into(),
            kind: TaskKind::Doctor,
            plan: None,
            risk_review: None,
            executions: vec![],
            tool_calls: vec![],
            summary: Some("ok".into()),
            status: TaskStatus::Completed,
            created_at: now(),
            updated_at: now(),
        };
        s.upsert_task(&rec).unwrap();
        // upsert 对同一 id 是幂等的
        s.upsert_task(&rec).unwrap();

        let all = s.list_tasks(None, 10).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "只读体检");

        assert_eq!(s.list_tasks(Some("srv"), 10).unwrap().len(), 1);
        assert!(s.list_tasks(Some("other"), 10).unwrap().is_empty());

        let got = s.get_task(&rec.id).unwrap();
        assert_eq!(got.summary.as_deref(), Some("ok"));
        assert_eq!(s.get_task("missing").unwrap_err().code(), "not_found");

        s.delete_task(&rec.id).unwrap();
        assert!(s.list_tasks(None, 10).unwrap().is_empty());
        assert_eq!(s.delete_task(&rec.id).unwrap_err().code(), "not_found");
    }

    #[test]
    fn providers_and_policy_persist() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.list_providers().unwrap().len(), 0);
        // 未设置时返回默认策略
        assert!(s.get_policy().unwrap().auto);

        let p = ProviderConfig {
            id: new_id(),
            name: "Codex".into(),
            kind: ProviderKind::CodexAppServer,
            base_url: None,
            model: Some("gpt-5-codex".into()),
            codex_path: Some("codex".into()),
            credential_ref: None,
            enabled: true,
            created_at: now(),
            updated_at: now(),
        };
        s.upsert_provider(&p).unwrap();
        assert_eq!(s.list_providers().unwrap().len(), 1);
        assert_eq!(s.get_provider(&p.id).unwrap().kind, ProviderKind::CodexAppServer);

        s.set_policy(&ModelSelectionPolicy { auto: false, default_provider_id: Some(p.id.clone()) }).unwrap();
        let pol = s.get_policy().unwrap();
        assert!(!pol.auto);
        assert_eq!(pol.default_provider_id.as_deref(), Some(p.id.as_str()));

        s.delete_provider(&p.id).unwrap();
        assert_eq!(s.list_providers().unwrap().len(), 0);
    }

    #[test]
    fn favorite_defaults_false_and_sorts_first() {
        let s = Store::open_in_memory().unwrap();
        // 先建 a（默认排在前），再建 b。
        let a = s.create_server(input("a")).unwrap();
        let b = s.create_server(input("b")).unwrap();
        assert!(!a.favorite, "新建服务器默认未收藏");

        // 收藏 b → 返回的 profile 标记为已收藏。
        let updated = s.set_server_favorite(&b.id, true).unwrap();
        assert!(updated.favorite);

        // list_servers 中 b 已收藏并置顶（在 a 之前）。
        let listed = s.list_servers().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, b.id);
        assert!(listed[0].favorite);
        assert!(!listed[1].favorite);

        // 取消收藏后恢复原有顺序（a 在前）。
        s.set_server_favorite(&b.id, false).unwrap();
        let listed = s.list_servers().unwrap();
        assert_eq!(listed[0].id, a.id);

        // 对不存在的服务器返回 NotFound。
        assert_eq!(s.set_server_favorite("missing", true).unwrap_err().code(), "not_found");
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
