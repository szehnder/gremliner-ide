//! LLM profiles in the app: a global entity over `llm::ProfileStore` (shared with the CLI), the AI
//! settings page and the profile dialog. Store and network calls run on the I/O runtime.

pub mod form;
pub mod page;

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::*;
use llm::store::StoreError;
use llm::{KeyChange, LlmClient, LlmError, Profile, ProfileStore};
use storage::Storage;
use storage::secrets::SecretStore;
use uuid::Uuid;

use crate::io::spawn_io;

pub struct LlmProfiles {
    store: ProfileStore,
    profiles: Vec<Profile>,
    default: Option<Uuid>,
    overrides: HashMap<Uuid, Uuid>,
    share: HashMap<Uuid, bool>,
}

struct ProfilesGlobal(Entity<LlmProfiles>);

impl Global for ProfilesGlobal {}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, String> {
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

impl LlmProfiles {
    /// Install the global (reads the profile list synchronously; it is a small SQLite read).
    pub fn init(storage: Storage, secrets: Arc<dyn SecretStore>, cx: &mut App) {
        let store = ProfileStore::new(storage, secrets);
        let profiles = store.list().unwrap_or_else(|err| {
            log::warn!("could not load LLM profiles: {err}");
            Vec::new()
        });
        let default = store.default_id().ok().flatten();
        let overrides = store.connection_overrides().unwrap_or_default();
        let share = store.share_choices().unwrap_or_default();
        let entity = cx.new(|_| Self {
            store,
            profiles,
            default,
            overrides,
            share,
        });
        cx.set_global(ProfilesGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<ProfilesGlobal>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<ProfilesGlobal>().map(|g| g.0.clone())
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn get(&self, id: Uuid) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn default_id(&self) -> Option<Uuid> {
        self.default
    }

    pub fn connection_override(&self, connection: Uuid) -> Option<Uuid> {
        self.overrides.get(&connection).copied()
    }

    /// The profile used for assistant requests on `connection` (override, default, or the only one).
    pub fn resolve(&self, connection: Option<Uuid>) -> Option<&Profile> {
        if let Some(p) = connection
            .and_then(|c| self.overrides.get(&c))
            .and_then(|id| self.get(*id))
        {
            return Some(p);
        }
        if let Some(p) = self.default.and_then(|id| self.get(id)) {
            return Some(p);
        }
        (self.profiles.len() == 1).then(|| &self.profiles[0])
    }

    /// The connection's "share sample data" choice (`None`: the default for the profile).
    pub fn share_choice(&self, connection: Uuid) -> Option<bool> {
        self.share.get(&connection).copied()
    }

    /// Whether sample values may be sent for `connection` (ADR-0021).
    pub fn share_samples(&self, connection: Uuid) -> bool {
        self.share_choice(connection).unwrap_or_else(|| {
            self.resolve(Some(connection))
                .is_some_and(Profile::is_local)
        })
    }

    pub fn store(&self) -> &ProfileStore {
        &self.store
    }

    /// Save (insert or update) with a key change; the first profile becomes the default.
    pub fn save(
        &mut self,
        profile: Profile,
        key: KeyChange,
        cx: &mut Context<'_, Self>,
    ) -> Task<Result<Profile, String>> {
        let store = self.store.clone();
        let first = self.profiles.is_empty();
        let work = spawn_io(
            cx,
            blocking(move || {
                let saved = store.save(profile, key)?;
                if first {
                    store.set_default(Some(saved.id))?;
                }
                Ok(saved)
            }),
        );
        cx.spawn(async move |this, cx| {
            let saved = work.await.map_err(|e| e.to_string())??;
            let _ = this.update(cx, |this, cx| {
                match this.profiles.iter_mut().find(|p| p.id == saved.id) {
                    Some(p) => *p = saved.clone(),
                    None => this.profiles.push(saved.clone()),
                }
                if first {
                    this.default = Some(saved.id);
                }
                cx.notify();
            });
            Ok(saved)
        })
    }

    pub fn delete(&mut self, id: Uuid, cx: &mut Context<'_, Self>) -> Task<Result<(), String>> {
        let Some(profile) = self.get(id).cloned() else {
            return Task::ready(Ok(()));
        };
        let store = self.store.clone();
        let work = spawn_io(cx, blocking(move || store.delete(&profile)));
        cx.spawn(async move |this, cx| {
            work.await.map_err(|e| e.to_string())??;
            let _ = this.update(cx, |this, cx| {
                this.profiles.retain(|p| p.id != id);
                if this.default == Some(id) {
                    this.default = None;
                }
                this.overrides.retain(|_, p| *p != id);
                cx.notify();
            });
            Ok(())
        })
    }

    pub fn set_default(&mut self, id: Option<Uuid>, cx: &mut Context<'_, Self>) {
        self.default = id;
        cx.notify();
        let store = self.store.clone();
        let work = spawn_io(cx, blocking(move || store.set_default(id)));
        cx.background_spawn(async move {
            if let Ok(Err(e)) = work.await {
                log::warn!("could not save the default LLM profile: {e}");
            }
        })
        .detach();
    }

    pub fn set_connection_profile(
        &mut self,
        connection: Uuid,
        profile: Option<Uuid>,
        cx: &mut Context<'_, Self>,
    ) {
        match profile {
            Some(p) => self.overrides.insert(connection, p),
            None => self.overrides.remove(&connection),
        };
        cx.notify();
        let store = self.store.clone();
        let work = spawn_io(
            cx,
            blocking(move || store.set_connection_profile(connection, profile)),
        );
        cx.background_spawn(async move {
            if let Ok(Err(e)) = work.await {
                log::warn!("could not save the connection's LLM profile: {e}");
            }
        })
        .detach();
    }

    pub fn set_share_samples(
        &mut self,
        connection: Uuid,
        share: Option<bool>,
        cx: &mut Context<'_, Self>,
    ) {
        match share {
            Some(v) => self.share.insert(connection, v),
            None => self.share.remove(&connection),
        };
        cx.notify();
        let store = self.store.clone();
        let work = spawn_io(
            cx,
            blocking(move || store.set_share_samples(connection, share)),
        );
        cx.background_spawn(async move {
            if let Ok(Err(e)) = work.await {
                log::warn!("could not save the sample-data choice: {e}");
            }
        })
        .detach();
    }

    /// A client for `profile`, reading its key on the I/O runtime. `key` overrides the stored key (an
    /// unsaved edit in the profile dialog).
    pub fn client(
        &self,
        profile: Profile,
        key: Option<String>,
        cx: &App,
    ) -> Task<Result<LlmClient, LlmError>> {
        let store = self.store.clone();
        let work = spawn_io(cx, async move {
            let key = match key {
                Some(k) => Some(k),
                None => tokio::task::spawn_blocking({
                    let profile = profile.clone();
                    move || store.key(&profile).ok().flatten()
                })
                .await
                .ok()
                .flatten(),
            };
            LlmClient::new(profile, key)
        });
        cx.background_spawn(
            async move { work.await.map_err(|e| LlmError::Network(e.to_string()))? },
        )
    }
}
