//! Credential store: the single boundary for secrets.
//!
//! SSH passwords/keys and provider API keys live ONLY here (system Keychain in
//! production). The rest of the app holds a [`CredentialRef`] — never the secret
//! (see docs/SECURITY_MODEL.zh-Hans.md). Nothing in this module logs secret
//! values, and secrets must never be written to SQLite or audit records.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::error::{AppError, AppResult};
use crate::core::types::CredentialRef;

/// Service name under which AiPanel namespaces its Keychain entries.
const SERVICE: &str = "com.aipanel.app";

pub trait CredentialStore: Send + Sync {
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> AppResult<()>;
    fn get_secret(&self, reference: &CredentialRef) -> AppResult<Option<String>>;
    fn delete_secret(&self, reference: &CredentialRef) -> AppResult<()>;
    /// Human-readable backend name, for diagnostics/UI ("keychain" vs "mock").
    fn backend(&self) -> &'static str;
}

/// System Keychain backend (macOS Keychain / Windows Credential Manager).
pub struct KeyringCredentialStore;

impl KeyringCredentialStore {
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

/// In-memory store for development (no Keychain available) and tests.
///
/// **Insecure — never use in production.** Secrets vanish on restart and live
/// only in process memory. The UI and docs flag when this backend is active.
#[derive(Default)]
pub struct LocalMockCredentialStore {
    map: Mutex<HashMap<String, String>>,
}

impl CredentialStore for LocalMockCredentialStore {
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> AppResult<()> {
        self.map.lock().unwrap().insert(reference.0.clone(), secret.to_string());
        Ok(())
    }

    fn get_secret(&self, reference: &CredentialRef) -> AppResult<Option<String>> {
        Ok(self.map.lock().unwrap().get(&reference.0).cloned())
    }

    fn delete_secret(&self, reference: &CredentialRef) -> AppResult<()> {
        self.map.lock().unwrap().remove(&reference.0);
        Ok(())
    }

    fn backend(&self) -> &'static str {
        "mock"
    }
}

/// Pick the best available backend: the system Keychain if a round-trip probe
/// succeeds, otherwise the in-memory mock (development fallback).
pub fn default_credential_store() -> Box<dyn CredentialStore> {
    let keyring = KeyringCredentialStore;
    let probe = CredentialRef("__aipanel_probe__".into());
    let ok = keyring.put_secret(&probe, "probe").is_ok()
        && keyring.get_secret(&probe).map(|v| v.as_deref() == Some("probe")).unwrap_or(false);
    let _ = keyring.delete_secret(&probe);
    if ok {
        Box::new(keyring)
    } else {
        eprintln!("[credentials] system Keychain unavailable — using in-memory mock (dev only)");
        Box::new(LocalMockCredentialStore::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
    fn delete_missing_is_ok() {
        let store = LocalMockCredentialStore::default();
        store.delete_secret(&CredentialRef("missing".into())).unwrap();
    }

    #[test]
    fn backend_name() {
        assert_eq!(LocalMockCredentialStore::default().backend(), "mock");
    }
}
