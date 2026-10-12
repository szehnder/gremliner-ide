//! Saved LLM profiles (shared by the app and CLI): JSON rows in `llm_profiles`, API keys in the OS
//! credential store (ADR-0022), plus the default profile, per-connection overrides and the per-connection
//! "share sample data" choice (ADR-0021). Blocking: call from a blocking pool or the CLI.

use std::collections::HashMap;
use std::sync::Arc;

use storage::secrets::SecretStore;
use storage::{LlmProfileRow, Storage, StorageError, Table};
use uuid::Uuid;

use crate::types::Profile;

/// `settings` keys.
pub const DEFAULT_KEY: &str = "llm.default_profile";
pub const CONNECTION_PROFILES_KEY: &str = "llm.connection_profiles";
pub const SHARE_SAMPLES_KEY: &str = "llm.share_samples";

/// What to do with a profile's API key on save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyChange {
    Keep,
    Set(String),
    Clear,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("a profile named \"{0}\" already exists")]
    NameTaken(String),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("credential store: {0}")]
    Secrets(String),
}

#[derive(Clone, Debug)]
pub struct ProfileStore {
    storage: Storage,
    secrets: Arc<dyn SecretStore>,
}

impl ProfileStore {
    pub fn new(storage: Storage, secrets: Arc<dyn SecretStore>) -> Self {
        Self { storage, secrets }
    }

    pub fn secrets_persist(&self) -> bool {
        self.secrets.is_persistent()
    }

    /// Every profile; unreadable rows are skipped with a warning.
    pub fn list(&self) -> Result<Vec<Profile>, StoreError> {
        Ok(self
            .storage
            .list_llm_profiles_blocking()?
            .into_iter()
            .filter_map(|row| match serde_json::from_str::<Profile>(&row.json) {
                Ok(p) => Some(p),
                Err(err) => {
                    log::warn!("skipping unreadable LLM profile {:?}: {err}", row.name);
                    None
                }
            })
            .collect())
    }

    pub fn get(&self, id: Uuid) -> Result<Option<Profile>, StoreError> {
        Ok(self.list()?.into_iter().find(|p| p.id == id))
    }

    pub fn find(&self, name: &str) -> Result<Option<Profile>, StoreError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim())))
    }

    /// Insert or update, applying the key change. Returns the profile as stored.
    pub fn save(&self, mut profile: Profile, key: KeyChange) -> Result<Profile, StoreError> {
        profile.name = profile.name.trim().to_owned();
        profile.base_url = profile.base_url.trim().trim_end_matches('/').to_owned();
        let account = profile.key_account();
        match &key {
            KeyChange::Set(k) if !k.trim().is_empty() => {
                self.secrets
                    .set(&account, k.trim())
                    .map_err(|e| StoreError::Secrets(e.to_string()))?;
                storage::secrets::register_secret(k.trim());
                profile.has_key = true;
            }
            KeyChange::Set(_) | KeyChange::Clear => {
                self.secrets
                    .delete(&account)
                    .map_err(|e| StoreError::Secrets(e.to_string()))?;
                profile.has_key = false;
            }
            KeyChange::Keep => {}
        }
        let row = LlmProfileRow {
            id: profile.id.to_string(),
            name: profile.name.clone(),
            json: serde_json::to_string(&profile).unwrap_or_default(),
        };
        match self.storage.save_llm_profile_blocking(row) {
            Ok(()) => Ok(profile),
            Err(StorageError::NameTaken(n)) => Err(StoreError::NameTaken(n)),
            Err(e) => Err(e.into()),
        }
    }

    /// The stored API key, if any.
    pub fn key(&self, profile: &Profile) -> Result<Option<String>, StoreError> {
        if !profile.has_key {
            return Ok(None);
        }
        let key = self
            .secrets
            .get(&profile.key_account())
            .map_err(|e| StoreError::Secrets(e.to_string()))?;
        if let Some(k) = &key {
            storage::secrets::register_secret(k);
        }
        Ok(key)
    }

    pub fn delete(&self, profile: &Profile) -> Result<(), StoreError> {
        self.secrets
            .delete(&profile.key_account())
            .map_err(|e| StoreError::Secrets(e.to_string()))?;
        self.storage
            .delete_llm_profile_blocking(&profile.id.to_string())?;
        if self.default_id()? == Some(profile.id) {
            self.set_default(None)?;
        }
        let mut map = self.connection_overrides()?;
        let before = map.len();
        map.retain(|_, p| *p != profile.id);
        if map.len() != before {
            self.storage
                .set_json_blocking(Table::Settings, CONNECTION_PROFILES_KEY, &map)?;
        }
        Ok(())
    }

    pub fn default_id(&self) -> Result<Option<Uuid>, StoreError> {
        Ok(self
            .storage
            .get_json_blocking::<Option<Uuid>>(Table::Settings, DEFAULT_KEY)?
            .flatten())
    }

    pub fn set_default(&self, id: Option<Uuid>) -> Result<(), StoreError> {
        Ok(self
            .storage
            .set_json_blocking(Table::Settings, DEFAULT_KEY, &id)?)
    }

    pub fn connection_overrides(&self) -> Result<HashMap<Uuid, Uuid>, StoreError> {
        Ok(self
            .storage
            .get_json_blocking(Table::Settings, CONNECTION_PROFILES_KEY)?
            .unwrap_or_default())
    }

    /// Use `profile` (or the default, with `None`) for assistant requests on `connection`.
    pub fn set_connection_profile(
        &self,
        connection: Uuid,
        profile: Option<Uuid>,
    ) -> Result<(), StoreError> {
        let mut map = self.connection_overrides()?;
        match profile {
            Some(p) => map.insert(connection, p),
            None => map.remove(&connection),
        };
        Ok(self
            .storage
            .set_json_blocking(Table::Settings, CONNECTION_PROFILES_KEY, &map)?)
    }

    /// The profile for assistant requests on `connection`: its override, else the default, else the
    /// only profile there is.
    pub fn resolve(&self, connection: Option<Uuid>) -> Result<Option<Profile>, StoreError> {
        let all = self.list()?;
        let by_id = |id: Uuid| all.iter().find(|p| p.id == id).cloned();
        if let Some(c) = connection
            && let Some(p) = self
                .connection_overrides()?
                .get(&c)
                .and_then(|id| by_id(*id))
        {
            return Ok(Some(p));
        }
        if let Some(p) = self.default_id()?.and_then(by_id) {
            return Ok(Some(p));
        }
        Ok((all.len() == 1).then(|| all[0].clone()))
    }

    /// Every explicit "share sample data" choice, by connection.
    pub fn share_choices(&self) -> Result<HashMap<Uuid, bool>, StoreError> {
        Ok(self
            .storage
            .get_json_blocking(Table::Settings, SHARE_SAMPLES_KEY)?
            .unwrap_or_default())
    }

    /// The connection's explicit "share sample data with the assistant" choice, if made.
    pub fn share_samples_choice(&self, connection: Uuid) -> Result<Option<bool>, StoreError> {
        let map: HashMap<Uuid, bool> = self
            .storage
            .get_json_blocking(Table::Settings, SHARE_SAMPLES_KEY)?
            .unwrap_or_default();
        Ok(map.get(&connection).copied())
    }

    /// Whether sample values may be sent: the explicit choice, else on for local models only (ADR-0021).
    pub fn share_samples(&self, connection: Uuid, profile: &Profile) -> Result<bool, StoreError> {
        Ok(self
            .share_samples_choice(connection)?
            .unwrap_or_else(|| profile.is_local()))
    }

    pub fn set_share_samples(
        &self,
        connection: Uuid,
        share: Option<bool>,
    ) -> Result<(), StoreError> {
        let mut map: HashMap<Uuid, bool> = self
            .storage
            .get_json_blocking(Table::Settings, SHARE_SAMPLES_KEY)?
            .unwrap_or_default();
        match share {
            Some(v) => map.insert(connection, v),
            None => map.remove(&connection),
        };
        Ok(self
            .storage
            .set_json_blocking(Table::Settings, SHARE_SAMPLES_KEY, &map)?)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::presets::preset;
    use storage::secrets::MemoryStore;

    fn store() -> (ProfileStore, Storage, Arc<MemoryStore>) {
        let storage = Storage::open_in_memory().unwrap();
        let secrets = Arc::new(MemoryStore::default());
        (
            ProfileStore::new(storage.clone(), secrets.clone()),
            storage,
            secrets,
        )
    }

    #[test]
    fn scenario_llm_providers_key_stored_securely() {
        let (store, storage, secrets) = store();
        let key = "sk-ant-api03-secret-value-1234567890";
        let saved = store
            .save(
                preset("anthropic").unwrap().profile("Claude"),
                KeyChange::Set(key.into()),
            )
            .unwrap();
        assert!(saved.has_key);
        // Retrievable for requests…
        let loaded = store.find("claude").unwrap().unwrap();
        assert_eq!(store.key(&loaded).unwrap().as_deref(), Some(key));
        assert_eq!(
            secrets.get(&loaded.key_account()).unwrap().as_deref(),
            Some(key)
        );
        // …never in the database…
        for row in storage.list_llm_profiles_blocking().unwrap() {
            assert!(!row.json.contains(key), "{}", row.json);
        }
        for table_key in [DEFAULT_KEY, CONNECTION_PROFILES_KEY] {
            let raw = storage.get_blocking(Table::Settings, table_key).unwrap();
            assert!(!raw.unwrap_or_default().contains(key));
        }
        // …and masked in logs.
        let line = format!("calling with {key}");
        assert!(!storage::secrets::redact(&line).contains(key));
        // Clearing removes it.
        let cleared = store.save(loaded, KeyChange::Clear).unwrap();
        assert!(!cleared.has_key);
        assert_eq!(secrets.get(&cleared.key_account()).unwrap(), None);
    }

    #[test]
    fn scenario_llm_providers_per_connection_override() {
        let (store, _, _) = store();
        let cloud = store
            .save(
                preset("anthropic").unwrap().profile("Claude"),
                KeyChange::Set("k-123456".into()),
            )
            .unwrap();
        let local = store
            .save(preset("ollama").unwrap().profile("Local"), KeyChange::Keep)
            .unwrap();
        store.set_default(Some(cloud.id)).unwrap();
        let prod = Uuid::new_v4();
        let other = Uuid::new_v4();
        store.set_connection_profile(prod, Some(local.id)).unwrap();
        assert_eq!(store.resolve(Some(prod)).unwrap().unwrap().id, local.id);
        assert_eq!(store.resolve(Some(other)).unwrap().unwrap().id, cloud.id);
        assert_eq!(store.resolve(None).unwrap().unwrap().id, cloud.id);
        // Deleting the local profile drops the override.
        store.delete(&local).unwrap();
        assert_eq!(store.resolve(Some(prod)).unwrap().unwrap().id, cloud.id);
        // Privacy default: samples only for local models unless chosen.
        assert!(!store.share_samples(prod, &cloud).unwrap());
        store.set_share_samples(prod, Some(true)).unwrap();
        assert!(store.share_samples(prod, &cloud).unwrap());
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let (store, _, _) = store();
        store
            .save(preset("ollama").unwrap().profile("Local"), KeyChange::Keep)
            .unwrap();
        let err = store
            .save(preset("ollama").unwrap().profile("local "), KeyChange::Keep)
            .unwrap_err();
        assert!(matches!(err, StoreError::NameTaken(_)));
    }
}
