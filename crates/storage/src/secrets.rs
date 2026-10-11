//! Secrets: the OS credential store, and redaction of known secrets from logs (ADR-0022, ADR-0034).
//!
//! Every call here may block (D-Bus on Linux, possibly an unlock prompt), so call it from a blocking
//! context: the storage thread, `tokio::task::spawn_blocking`, or a CLI.
//!
//! Each secret read or written is registered with the redactor, and [`RedactingLogger`] replaces
//! registered values in every log line, so a secret that leaks into an error message never reaches a log.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

/// Keychain service name for every Gremliner secret.
pub const SERVICE: &str = "gremliner";
/// Set to `memory` to keep secrets in memory only (tests, CI, machines without a keychain).
pub const STORE_ENV: &str = "GREMLINER_SECRET_STORE";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("credential store unavailable: {0}")]
    Unavailable(String),
    #[error("credential store error: {0}")]
    Store(String),
}

/// Where secrets live. Implementations are blocking.
pub trait SecretStore: Send + Sync + std::fmt::Debug {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError>;
    fn delete(&self, account: &str) -> Result<(), SecretError>;
    /// False when secrets are lost on exit (the in-memory fallback).
    fn is_persistent(&self) -> bool;
}

/// The OS credential store: Secret Service (GNOME Keyring, KWallet) on Linux, Keychain on macOS,
/// Credential Manager on Windows.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, account).map_err(map_keyring)
    }
}

fn map_keyring(err: keyring::Error) -> SecretError {
    match err {
        keyring::Error::PlatformFailure(e) | keyring::Error::NoStorageAccess(e) => {
            SecretError::Unavailable(e.to_string())
        }
        other => SecretError::Store(other.to_string()),
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => {
                register_secret(&secret);
                Ok(Some(secret))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring(e)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        register_secret(secret);
        Self::entry(account)?
            .set_password(secret)
            .map_err(map_keyring)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring(e)),
        }
    }

    fn is_persistent(&self) -> bool {
        true
    }
}

/// Secrets kept for the life of the process only.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        Ok(self.lock().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        register_secret(secret);
        self.lock().insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.lock().remove(account);
        Ok(())
    }

    fn is_persistent(&self) -> bool {
        false
    }
}

impl MemoryStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The OS store when it works, otherwise an in-memory store (with a warning). Blocking: probes the store
/// with a harmless lookup.
pub fn detect() -> Arc<dyn SecretStore> {
    if std::env::var(STORE_ENV).is_ok_and(|v| v == "memory") {
        return Arc::new(MemoryStore::default());
    }
    match KeyringStore.get("probe") {
        Ok(_) => Arc::new(KeyringStore),
        Err(err) => {
            log::warn!("{err}; passwords will be kept for this session only");
            Arc::new(MemoryStore::default())
        }
    }
}

/// Probes the credential store on first use rather than at construction (ADR-0034), so a keychain unlock
/// prompt can only appear when the user does something that needs a secret.
#[derive(Debug, Default)]
pub struct LazyStore(std::sync::OnceLock<Arc<dyn SecretStore>>);

impl LazyStore {
    fn inner(&self) -> &Arc<dyn SecretStore> {
        self.0.get_or_init(detect)
    }

    /// Whether the store has been probed yet (no keychain access has happened if false).
    pub fn is_probed(&self) -> bool {
        self.0.get().is_some()
    }
}

impl SecretStore for LazyStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        self.inner().get(account)
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        self.inner().set(account, secret)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.inner().delete(account)
    }

    fn is_persistent(&self) -> bool {
        self.inner().is_persistent()
    }
}

/// Secrets shorter than this are not redacted (they would mangle ordinary words in logs).
const MIN_REDACT_LEN: usize = 4;
const MASK: &str = "••••••";

static SECRETS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Remember a secret so [`redact`] removes it from text.
pub fn register_secret(secret: &str) {
    if secret.len() < MIN_REDACT_LEN {
        return;
    }
    let mut secrets = SECRETS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !secrets.iter().any(|s| s == secret) {
        secrets.push(secret.to_owned());
        // Longest first, so a secret containing another is masked whole.
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    }
}

/// `text` with every registered secret masked.
pub fn redact(text: &str) -> Cow<'_, str> {
    let secrets = SECRETS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut out = Cow::Borrowed(text);
    for secret in secrets.iter() {
        if out.contains(secret.as_str()) {
            out = Cow::Owned(out.replace(secret.as_str(), MASK));
        }
    }
    out
}

/// A `log::Log` wrapper that masks registered secrets in every record before passing it on.
pub struct RedactingLogger<L> {
    inner: L,
}

impl<L: log::Log> RedactingLogger<L> {
    pub fn new(inner: L) -> Self {
        Self { inner }
    }
}

impl<L: log::Log> log::Log for RedactingLogger<L> {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.inner.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let redacted = redact(&message);
        self.inner.log(
            &log::Record::builder()
                .args(format_args!("{redacted}"))
                .metadata(record.metadata().clone())
                .module_path(record.module_path())
                .file(record.file())
                .line(record.line())
                .build(),
        );
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[derive(Default)]
    struct Capture(Mutex<Vec<String>>);

    impl log::Log for &'static Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    #[test]
    fn registered_secrets_are_redacted_from_logs() {
        let capture: &'static Capture = Box::leak(Box::default());
        let logger = RedactingLogger::new(capture);
        let store = MemoryStore::default();
        store
            .set("conn-1/password", "hunter2-correct-horse")
            .unwrap();
        log::Log::log(
            &logger,
            &log::Record::builder()
                .args(format_args!(
                    "auth failed for stephen with hunter2-correct-horse"
                ))
                .level(log::Level::Warn)
                .build(),
        );
        let lines = capture.0.lock().unwrap();
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].contains("hunter2-correct-horse"), "{}", lines[0]);
        assert!(lines[0].contains(MASK));
    }

    #[test]
    fn short_values_are_not_registered() {
        register_secret("ab");
        assert_eq!(redact("abc ab"), "abc ab");
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::default();
        assert_eq!(store.get("x").unwrap(), None);
        store.set("x", "s3cret-value").unwrap();
        assert_eq!(store.get("x").unwrap().as_deref(), Some("s3cret-value"));
        store.delete("x").unwrap();
        assert_eq!(store.get("x").unwrap(), None);
        assert!(!store.is_persistent());
    }
}
