//! 凭据存储：密钥唯一的边界。
//!
//! SSH 密码/密钥与供应商 API Key 只存在这里（生产环境用系统 Keychain）。应用
//! 其余部分只持有 [`CredentialRef`]——绝不持有密钥本身（见
//! docs/SECURITY_MODEL.zh-Hans.md）。本模块不记录任何密钥值，密钥也绝不能写入
//! SQLite 或审计记录。

use std::collections::HashMap;
use std::env;
use std::sync::Mutex;

use crate::core::error::{AppError, AppResult};
use crate::core::types::CredentialRef;

/// AiPanel 在系统 Keychain 中为其条目所用的服务名（命名空间）。
///
/// 该值不是 macOS bundle identifier。保留历史服务名可避免改包名或应用 ID 时让
/// 已保存的服务器/供应商凭据失联；新的 bundle identifier 见 `tauri.conf.json`。
const SERVICE: &str = "com.aipanel.app";
const BACKEND_ENV: &str = "AIPANEL_CREDENTIAL_BACKEND";

/// 凭据存储的抽象接口：写入/读取/删除密钥，并报告后端名称。
pub trait CredentialStore: Send + Sync {
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> AppResult<()>;
    fn get_secret(&self, reference: &CredentialRef) -> AppResult<Option<String>>;
    fn delete_secret(&self, reference: &CredentialRef) -> AppResult<()>;
    /// 人类可读的后端名，供诊断/界面使用（"keychain" 或 "mock"）。
    fn backend(&self) -> &'static str;
}

/// 系统 Keychain 后端（macOS Keychain / Windows 凭据管理器）。
pub struct KeyringCredentialStore;

impl KeyringCredentialStore {
    /// 为给定引用构造一个 Keychain 条目句柄。
    fn entry(reference: &CredentialRef) -> AppResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, &reference.0)
            .map_err(|e| AppError::Credential(e.to_string()))
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> AppResult<()> {
        Self::entry(reference)?
            .set_password(secret)
            .map_err(|e| AppError::Credential(e.to_string()))
    }

    fn get_secret(&self, reference: &CredentialRef) -> AppResult<Option<String>> {
        match Self::entry(reference)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Credential(e.to_string())),
        }
    }

    fn delete_secret(&self, reference: &CredentialRef) -> AppResult<()> {
        match Self::entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Credential(e.to_string())),
        }
    }

    fn backend(&self) -> &'static str {
        "keychain"
    }
}

/// 供开发（无可用 Keychain 时）与测试使用的内存存储。
///
/// **不安全——绝不可用于生产。** 密钥在重启后消失，且只存在于进程内存中。
/// 当此后端启用时，界面与文档会予以提示。
#[derive(Default)]
pub struct LocalMockCredentialStore {
    map: Mutex<HashMap<String, String>>,
}

impl CredentialStore for LocalMockCredentialStore {
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> AppResult<()> {
        self.map
            .lock()
            .map_err(|_| AppError::Credential("credential mock store lock poisoned".into()))?
            .insert(reference.0.clone(), secret.to_string());
        Ok(())
    }

    fn get_secret(&self, reference: &CredentialRef) -> AppResult<Option<String>> {
        Ok(self
            .map
            .lock()
            .map_err(|_| AppError::Credential("credential mock store lock poisoned".into()))?
            .get(&reference.0)
            .cloned())
    }

    fn delete_secret(&self, reference: &CredentialRef) -> AppResult<()> {
        self.map
            .lock()
            .map_err(|_| AppError::Credential("credential mock store lock poisoned".into()))?
            .remove(&reference.0);
        Ok(())
    }

    fn backend(&self) -> &'static str {
        "mock"
    }
}

/// 选择当前最合适的后端。
///
/// 默认直接使用系统 Keychain，不做启动时写入-读取探测：macOS 对每个新建
/// Keychain 条目单独授权，临时 probe 会导致开发期每次启动都弹授权提示。需要
/// 完全绕开系统 Keychain 时，可显式设置 `AIPANEL_CREDENTIAL_BACKEND=mock`。
pub fn default_credential_store() -> Box<dyn CredentialStore> {
    let requested_backend = env::var(BACKEND_ENV)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase());
    if requested_backend.as_deref() == Some("mock") {
        eprintln!("[credentials] using in-memory mock because {BACKEND_ENV}=mock");
        return Box::new(LocalMockCredentialStore::default());
    }

    Box::new(KeyringCredentialStore)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // mock 后端的写入-读取-删除往返
    fn mock_round_trip() {
        let store = LocalMockCredentialStore::default();
        let r = CredentialRef::for_server("s1");
        assert_eq!(store.get_secret(&r).unwrap(), None);
        store.put_secret(&r, "hunter2").unwrap();
        assert_eq!(store.get_secret(&r).unwrap().as_deref(), Some("hunter2"));
        store.delete_secret(&r).unwrap();
        assert_eq!(store.get_secret(&r).unwrap(), None);
    }

    #[test]
    // mock 后端锁被 poison 时应返回 credential 错误，而不是让调用方 panic。
    fn mock_lock_poison_returns_error() {
        let store = LocalMockCredentialStore::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = store.map.lock().unwrap();
            panic!("poison mock store");
        }));

        let err = store
            .get_secret(&CredentialRef::for_server("s1"))
            .unwrap_err();
        assert_eq!(err.code(), "credential");
        assert!(err.to_string().contains("lock poisoned"));
    }

    #[test]
    // 删除不存在的密钥应当成功（幂等）
    fn delete_missing_is_ok() {
        let store = LocalMockCredentialStore::default();
        store.delete_secret(&CredentialRef("missing".into())).unwrap();
    }

    #[test]
    // mock 后端名应为 "mock"
    fn backend_name() {
        assert_eq!(LocalMockCredentialStore::default().backend(), "mock");
    }

    #[test]
    // Keychain service 是凭据命名空间，不跟随 bundle identifier 重命名。
    fn keychain_service_name_is_stable() {
        assert_eq!(SERVICE, "com.aipanel.app");
    }
}
