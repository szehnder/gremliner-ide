//! The shared stylesheet library: built-in and user stylesheets, per-connection defaults and the
//! session's automatic label colours. Graph views observe it, so edits apply live everywhere. The host
//! loads it at startup and installs a persistence hook for changes.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::*;
use graph_store::style::{AutoPalette, Globals, Shape, Size, Stylesheet, VertexStyle};

pub const DEFAULT_ID: &str = "default";
pub const PRESENTATION_ID: &str = "presentation";

/// A change for the host to persist.
#[derive(Clone, Debug, PartialEq)]
pub enum LibraryChange {
    Saved(Stylesheet),
    Deleted(String),
    /// The whole connection → stylesheet id map after a change.
    ConnectionDefaults(HashMap<String, String>),
}

type Persist = Rc<dyn Fn(&LibraryChange, &mut App)>;

pub struct StyleLibrary {
    sheets: Vec<Stylesheet>,
    connection_defaults: HashMap<String, String>,
    auto: AutoPalette,
    persist: Option<Persist>,
}

struct StyleLibraryGlobal(Entity<StyleLibrary>);

impl Global for StyleLibraryGlobal {}

/// The built-in stylesheets: automatic colours, and a larger, label-heavy look for presenting.
pub fn builtins() -> Vec<Stylesheet> {
    let default = Stylesheet::new(DEFAULT_ID, "Default");
    let mut presentation = Stylesheet::new(PRESENTATION_ID, "Presentation");
    presentation.globals = Globals {
        label_density: 0.9,
        min_label_px: 7.0,
        max_caption_len: 40,
        show_edge_captions: true,
    };
    // `*` applies to every label without its own rule: larger nodes.
    presentation.vertex.insert(
        "*".into(),
        VertexStyle {
            shape: Shape::Circle,
            size: Size::Fixed { px: 20.0 },
            ..VertexStyle::default()
        },
    );
    vec![default, presentation]
}

impl StyleLibrary {
    /// The library, created with the built-ins on first use.
    pub fn global(cx: &mut App) -> Entity<StyleLibrary> {
        if let Some(g) = cx.try_global::<StyleLibraryGlobal>() {
            return g.0.clone();
        }
        let entity = cx.new(|_| StyleLibrary {
            sheets: builtins(),
            connection_defaults: HashMap::new(),
            auto: AutoPalette::default(),
            persist: None,
        });
        cx.set_global(StyleLibraryGlobal(entity.clone()));
        entity
    }

    /// The library if it exists (read-only contexts).
    pub fn global_ref(cx: &App) -> Option<Entity<StyleLibrary>> {
        cx.try_global::<StyleLibraryGlobal>().map(|g| g.0.clone())
    }

    pub fn set_persistence(&mut self, persist: impl Fn(&LibraryChange, &mut App) + 'static) {
        self.persist = Some(Rc::new(persist));
    }

    /// Replace user stylesheets and defaults with what the host loaded (built-ins stay unless a saved
    /// sheet has the same id).
    pub fn load(
        &mut self,
        saved: Vec<Stylesheet>,
        defaults: HashMap<String, String>,
        cx: &mut Context<'_, Self>,
    ) {
        let mut sheets = builtins();
        for s in saved {
            match sheets.iter_mut().find(|b| b.id == s.id) {
                Some(slot) => *slot = s,
                None => sheets.push(s),
            }
        }
        self.sheets = sheets;
        self.connection_defaults = defaults;
        cx.notify();
    }

    pub fn sheets(&self) -> &[Stylesheet] {
        &self.sheets
    }

    pub fn get(&self, id: &str) -> Option<&Stylesheet> {
        self.sheets.iter().find(|s| s.id == id)
    }

    /// `id`, or the Default stylesheet when it no longer exists.
    pub fn get_or_default(&self, id: &str) -> Stylesheet {
        self.get(id)
            .or_else(|| self.get(DEFAULT_ID))
            .cloned()
            .unwrap_or_else(|| Stylesheet::new(DEFAULT_ID, "Default"))
    }

    pub fn auto_palette_mut(&mut self) -> &mut AutoPalette {
        &mut self.auto
    }

    fn changed(&self, change: LibraryChange, cx: &mut Context<'_, Self>) {
        if let Some(p) = self.persist.clone() {
            p(&change, cx);
        }
        cx.notify();
    }

    /// Insert or replace a stylesheet.
    pub fn save(&mut self, sheet: Stylesheet, cx: &mut Context<'_, Self>) {
        match self.sheets.iter_mut().find(|s| s.id == sheet.id) {
            Some(slot) if *slot == sheet => return,
            Some(slot) => *slot = sheet.clone(),
            None => self.sheets.push(sheet.clone()),
        }
        self.changed(LibraryChange::Saved(sheet), cx);
    }

    /// A new empty stylesheet; returns its id.
    pub fn create(&mut self, name: &str, cx: &mut Context<'_, Self>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.save(Stylesheet::new(id.clone(), self.unique_name(name)), cx);
        id
    }

    /// A copy of `id` named "… copy"; returns the new id.
    pub fn duplicate(&mut self, id: &str, cx: &mut Context<'_, Self>) -> Option<String> {
        let mut copy = self.get(id)?.clone();
        copy.id = uuid::Uuid::new_v4().to_string();
        copy.name = self.unique_name(&format!("{} copy", copy.name));
        let new_id = copy.id.clone();
        self.save(copy, cx);
        Some(new_id)
    }

    pub fn rename(&mut self, id: &str, name: &str, cx: &mut Context<'_, Self>) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(mut s) = self.get(id).cloned() {
            s.name = name.to_owned();
            self.save(s, cx);
        }
    }

    /// Deletes a user stylesheet (the Default stylesheet stays).
    pub fn delete(&mut self, id: &str, cx: &mut Context<'_, Self>) {
        if id == DEFAULT_ID || self.get(id).is_none() {
            return;
        }
        self.sheets.retain(|s| s.id != id);
        let before = self.connection_defaults.len();
        self.connection_defaults.retain(|_, v| v != id);
        if self.connection_defaults.len() != before {
            self.changed(
                LibraryChange::ConnectionDefaults(self.connection_defaults.clone()),
                cx,
            );
        }
        self.changed(LibraryChange::Deleted(id.to_owned()), cx);
    }

    /// Import a stylesheet document: keeps its id unless one with that id exists, then it gets a new
    /// one. Returns the id.
    pub fn import(&mut self, mut sheet: Stylesheet, cx: &mut Context<'_, Self>) -> String {
        if self.get(&sheet.id).is_some() || sheet.id.is_empty() {
            sheet.id = uuid::Uuid::new_v4().to_string();
        }
        sheet.name = self.unique_name(&sheet.name);
        let id = sheet.id.clone();
        self.save(sheet, cx);
        id
    }

    pub fn default_for_connection(&self, connection: &str) -> Option<&str> {
        self.connection_defaults.get(connection).map(String::as_str)
    }

    pub fn set_connection_default(
        &mut self,
        connection: &str,
        stylesheet: Option<&str>,
        cx: &mut Context<'_, Self>,
    ) {
        match stylesheet {
            Some(id) => self
                .connection_defaults
                .insert(connection.to_owned(), id.to_owned()),
            None => self.connection_defaults.remove(connection),
        };
        self.changed(
            LibraryChange::ConnectionDefaults(self.connection_defaults.clone()),
            cx,
        );
    }

    fn unique_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() {
            "Untitled"
        } else {
            base.trim()
        };
        if !self.sheets.iter().any(|s| s.name == base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|n| !self.sheets.iter().any(|s| &s.name == n))
            .unwrap_or_else(|| base.to_owned())
    }
}
