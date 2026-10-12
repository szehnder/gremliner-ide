//! The schema of each connection, as the sidebar, schema graph and (later) autocomplete and the assistant
//! see it. On connect the cached schema shows immediately; discovery then runs on the I/O runtime and
//! streams snapshots back, and the finished schema replaces the cache. Results showing labels the schema
//! does not know trigger a refresh.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::*;
use providers::schema::{discover_schema, load_cached, save_cached};
use schema::{Options, Schema};
use storage::Storage;
use uuid::Uuid;

use crate::connections::{ConnStatus, ConnectionManager};
use crate::io::spawn_io_tracked;

/// Snapshots cross to the UI at most this often while discovery runs (the final one always does).
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100);

/// Unknown labels in results refresh the schema at most this often.
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default)]
pub struct SchemaEntry {
    pub schema: Option<Arc<Schema>>,
    /// Discovery is running for this connection.
    pub refreshing: bool,
    /// The schema shown came from the cache and has not been refreshed yet.
    pub from_cache: bool,
    last_refresh: Option<Instant>,
    generation: u64,
}

pub struct SchemaRegistry {
    storage: Option<Storage>,
    entries: HashMap<Uuid, SchemaEntry>,
    connected: HashSet<Uuid>,
    options: Options,
    _subscription: Option<Subscription>,
}

struct RegistryGlobal(Entity<SchemaRegistry>);

impl Global for RegistryGlobal {}

impl SchemaRegistry {
    /// Install the global registry; call after `ConnectionManager::init`.
    pub fn init(cx: &mut App) {
        let manager = ConnectionManager::try_global(cx);
        let entity = cx.new(|cx| {
            let storage = manager.as_ref().map(|m| m.read(cx).storage().clone());
            let subscription = manager
                .as_ref()
                .map(|m| cx.observe(m, |this: &mut Self, _, cx| this.sync_connections(cx)));
            Self {
                storage,
                entries: HashMap::new(),
                connected: HashSet::new(),
                options: Options::default(),
                _subscription: subscription,
            }
        });
        cx.set_global(RegistryGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<RegistryGlobal>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<RegistryGlobal>().map(|g| g.0.clone())
    }

    pub fn entry(&self, connection: Uuid) -> Option<&SchemaEntry> {
        self.entries.get(&connection)
    }

    pub fn schema(&self, connection: Uuid) -> Option<Arc<Schema>> {
        self.entries.get(&connection)?.schema.clone()
    }

    pub fn set_options(&mut self, options: Options) {
        self.options = options;
    }

    /// Start discovery for newly connected connections.
    fn sync_connections(&mut self, cx: &mut Context<'_, Self>) {
        let manager = ConnectionManager::global(cx);
        let now: HashSet<Uuid> = manager
            .read(cx)
            .connections()
            .iter()
            .filter(|c| matches!(manager.read(cx).status(c.id), ConnStatus::Connected(_)))
            .map(|c| c.id)
            .collect();
        let fresh: Vec<Uuid> = now.difference(&self.connected).copied().collect();
        self.connected = now;
        for id in fresh {
            self.on_connected(id, cx);
        }
    }

    fn on_connected(&mut self, id: Uuid, cx: &mut Context<'_, Self>) {
        let has_schema = self.entries.get(&id).is_some_and(|e| e.schema.is_some());
        let Some(storage) = self.storage.clone().filter(|_| !has_schema) else {
            self.refresh(id, cx);
            return;
        };
        let load = crate::io::spawn_io(cx, async move { load_cached(&storage, id).await });
        cx.spawn(async move |this, cx| {
            let cached = load.await.ok().flatten();
            let _ = this.update(cx, |this, cx| {
                if let Some(schema) = cached {
                    let entry = this.entries.entry(id).or_default();
                    // A refresh that already finished wins over the older cache.
                    if entry.schema.is_none() {
                        entry.schema = Some(Arc::new(schema));
                        entry.from_cache = true;
                        cx.notify();
                    }
                }
                this.refresh(id, cx);
            });
        })
        .detach();
    }

    /// Re-discover the schema in the background. Does nothing while a refresh is running or when the
    /// connection has no session.
    pub fn refresh(&mut self, id: Uuid, cx: &mut Context<'_, Self>) {
        let manager = ConnectionManager::global(cx);
        let Some(session) = manager.read(cx).session(id).cloned() else {
            return;
        };
        let name = manager
            .read(cx)
            .get(id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let entry = self.entries.entry(id).or_default();
        if entry.refreshing {
            return;
        }
        entry.refreshing = true;
        entry.generation += 1;
        entry.last_refresh = Some(Instant::now());
        let generation = entry.generation;
        cx.notify();

        let (tx, mut rx) = mpsc::unbounded::<Schema>();
        let options = self.options;
        let storage = self.storage.clone();
        let work = spawn_io_tracked(format!("Discovering schema of {name}"), cx, async move {
            let last_sent = Mutex::new(None::<Instant>);
            let progress_tx = tx.clone();
            let progress = move |snapshot: &Schema| {
                let mut last = last_sent
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if last.is_none_or(|t| t.elapsed() >= SNAPSHOT_INTERVAL) {
                    *last = Some(Instant::now());
                    let _ = progress_tx.unbounded_send(snapshot.clone());
                }
            };
            let schema = discover_schema(&session, &options, &progress).await;
            if let Some(storage) = &storage
                && let Err(err) = save_cached(storage, id, &schema).await
            {
                log::warn!("could not cache the schema: {err}");
            }
            let _ = tx.unbounded_send(schema);
        });
        cx.spawn(async move |this, cx| {
            while let Some(snapshot) = rx.next().await {
                let done = !snapshot.in_progress;
                let alive = this.update(cx, |this, cx| {
                    let Some(entry) = this.entries.get_mut(&id) else {
                        return;
                    };
                    if entry.generation != generation {
                        return;
                    }
                    // Keep showing the cached schema until discovery has found at least the labels.
                    let worth_showing =
                        done || entry.schema.is_none() || !snapshot.vertex_labels.is_empty();
                    if worth_showing {
                        entry.schema = Some(Arc::new(snapshot));
                        entry.from_cache = false;
                    }
                    if done {
                        entry.refreshing = false;
                    }
                    cx.notify();
                });
                if alive.is_err() || done {
                    break;
                }
            }
            let _ = work.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(entry) = this.entries.get_mut(&id)
                    && entry.generation == generation
                    && entry.refreshing
                {
                    entry.refreshing = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Results on `id` showed these labels; refresh when some are unknown (rate-limited).
    pub fn note_result_labels<'a>(
        &mut self,
        id: Uuid,
        labels: impl IntoIterator<Item = &'a str>,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(entry) = self.entries.get(&id) else {
            return;
        };
        let Some(schema) = &entry.schema else {
            return;
        };
        if entry.refreshing
            || entry
                .last_refresh
                .is_some_and(|t| t.elapsed() < AUTO_REFRESH_INTERVAL)
        {
            return;
        }
        let unknown = schema.unknown_labels(labels);
        if !unknown.is_empty() {
            log::info!("results show unknown labels {unknown:?}; refreshing the schema");
            self.refresh(id, cx);
        }
    }

    /// Drop what is known about a connection (as after a restart); the next connect loads the cache.
    pub fn forget(&mut self, id: Uuid) {
        self.entries.remove(&id);
        self.connected.remove(&id);
    }

    /// Forget the rate limit (tests).
    pub fn reset_auto_refresh(&mut self, id: Uuid) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.last_refresh = None;
        }
    }
}
