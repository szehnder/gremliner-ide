//! Workspace shell: dock layout, command palette, settings, theming and the I/O bridge.
//!
//! `gremliner` calls [`init`] once, then opens a window whose root view is [`Workspace`]. Feature crates
//! add panels with [`panels::register`] and commands with [`palette::CommandRegistry::register`].

pub mod actions;
pub mod assets;
pub mod connections;
pub mod io;
pub mod keymap;
pub mod palette;
pub mod panels;
pub mod perf_hud;
pub mod persist;
pub mod settings;
pub mod status_bar;
pub mod workspace;

use gpui_kit::App;
use storage::{Storage, Table};

pub use ui_theme as theme;
pub use workspace::{Workspace, WorkspaceOptions};

use crate::actions::*;
use crate::palette::{CommandMeta, CommandRegistry, RECENT_KEY, RecentCommands, When};
use crate::settings::{AppSettings, GeneralSettings};

/// Everything the shell needs at startup.
pub struct ShellConfig {
    /// `None` keeps everything in memory (tests, `--no-persist`).
    pub storage: Option<Storage>,
    /// Overrides the saved theme for this run (`--theme`), without persisting it.
    pub theme_override: Option<theme::ThemeChoice>,
    /// Where passwords go; `None` uses the OS credential store, probed on first use (ADR-0034).
    pub secrets: Option<std::sync::Arc<dyn storage::secrets::SecretStore>>,
}

/// Initialise the shell. Call after `gpui_kit::init` and `io::install`.
pub fn init(config: ShellConfig, cx: &mut App) {
    theme::init(cx);
    ui_editor::register();
    panels::EditorTabs::init(cx);
    keymap::bind_default_keys(cx);
    panels::register_builtin(cx);
    register_commands(cx);

    let mut general = GeneralSettings::load(config.storage.as_ref());
    if let Some(choice) = config.theme_override {
        general.theme = choice;
    }
    let recent = config
        .storage
        .as_ref()
        .and_then(|s| match s.get_json_blocking(Table::UiState, RECENT_KEY) {
            Ok(recent) => recent,
            Err(err) => {
                log::warn!("ignoring unreadable recent commands: {err}");
                None
            }
        })
        .map(RecentCommands)
        .unwrap_or_default();
    cx.set_global(recent);
    let connection_db = match &config.storage {
        Some(storage) => Some(storage.clone()),
        None => Storage::open_in_memory()
            .inspect_err(|err| log::error!("in-memory database unavailable: {err}"))
            .ok(),
    };
    if let Some(db) = connection_db {
        connections::ConnectionManager::init(db, config.secrets, cx);
    }
    AppSettings::init(general, config.storage, cx);
}

fn register_commands(cx: &mut App) {
    for meta in [
        CommandMeta::new("View", "Toggle Sidebar", ToggleSidebar),
        CommandMeta::new("View", "Toggle Results Dock", ToggleResultsDock)
            .keywords(&["panel", "bottom"]),
        CommandMeta::new("View", "Toggle Right Panel", ToggleRightPanel).keywords(&["inspector"]),
        CommandMeta::new("View", "Reset Layout", ResetLayout),
        CommandMeta::new("View", "Zoom In", ZoomIn).keywords(&["font", "bigger"]),
        CommandMeta::new("View", "Zoom Out", ZoomOut).keywords(&["font", "smaller"]),
        CommandMeta::new("View", "Reset Zoom", ZoomReset),
        CommandMeta::new("Preferences", "Open Settings", OpenSettings)
            .keywords(&["preferences", "theme"]),
        CommandMeta::new(
            "Preferences",
            "Reset General Settings",
            ResetGeneralSettings,
        ),
        CommandMeta::new("Help", "Keyboard Shortcuts", ShowKeyboardShortcuts)
            .keywords(&["keymap", "bindings"]),
        CommandMeta::new("Connection", "Add Connection", AddConnection)
            .keywords(&["new", "connect"]),
        CommandMeta::new("Editor", "New Query Tab", NewQueryTab).keywords(&["editor", "open"]),
        CommandMeta::new("Editor", "Run Query", editor::RunQuery)
            .when(When::EditorOpen)
            .keywords(&["execute"]),
        CommandMeta::new("Editor", "Format Query", editor::FormatQuery).when(When::EditorOpen),
        CommandMeta::new("Editor", "Toggle Line Comment", editor::ToggleLineComment)
            .when(When::EditorOpen),
        CommandMeta::new("Editor", "Split Right", SplitRight).when(When::EditorOpen),
        CommandMeta::new("Editor", "Save Query", SaveQuery).when(When::EditorOpen),
        CommandMeta::new("Application", "Quit", Quit).keywords(&["exit"]),
    ] {
        CommandRegistry::register(cx, meta);
    }
}
