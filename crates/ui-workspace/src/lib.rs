//! Workspace shell: dock layout, command palette, settings, theming and the I/O bridge.
//!
//! `gremliner` calls [`init`] once, then opens a window whose root view is [`Workspace`]. Feature crates
//! add panels with [`panels::register`] and commands with [`palette::CommandRegistry::register`].

pub mod actions;
pub mod ai;
pub mod assets;
pub mod connections;
pub mod io;
pub mod keymap;
pub mod palette;
pub mod panels;
pub mod perf_hud;
pub mod persist;
pub mod run;
pub mod schema;
pub mod settings;
pub mod status_bar;
pub mod styles;
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
    run::Runner::init(cx);
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
    // One credential store for connection passwords and LLM keys.
    let secrets = config
        .secrets
        .unwrap_or_else(|| std::sync::Arc::new(storage::secrets::LazyStore::default()));
    if let Some(db) = connection_db {
        ai::LlmProfiles::init(db.clone(), secrets.clone(), cx);
        connections::ConnectionManager::init(db, Some(secrets), cx);
    }
    schema::SchemaRegistry::init(cx);
    AppSettings::init(general, config.storage, cx);
    styles::init(cx);
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
        CommandMeta::new(
            "Connection",
            "Open Schema Graph",
            OpenSchemaGraph::default(),
        )
        .keywords(&["labels", "model"]),
        CommandMeta::new("Connection", "Refresh Schema", RefreshSchema::default())
            .keywords(&["discover", "labels"]),
        CommandMeta::new("Editor", "New Query Tab", NewQueryTab).keywords(&["editor", "open"]),
        CommandMeta::new("Editor", "Run Query", editor::RunQuery)
            .when(When::EditorOpen)
            .keywords(&["execute"]),
        CommandMeta::new("Editor", "Profile Query", editor::ProfileQuery)
            .when(When::EditorOpen)
            .keywords(&["performance", "timing", "slow"]),
        CommandMeta::new("Editor", "Explain Query", editor::ExplainQuery)
            .when(When::EditorOpen)
            .keywords(&["plan", "strategies"]),
        CommandMeta::new("Editor", "Format Query", editor::FormatQuery).when(When::EditorOpen),
        CommandMeta::new("Debug", "Debug Query", debug::StartDebug)
            .when(When::EditorOpen)
            .keywords(&["step", "debugger", "trace"]),
        CommandMeta::new("Debug", "Step Over", debug::StepOver).when(When::EditorOpen),
        CommandMeta::new("Debug", "Step Into", debug::StepInto).when(When::EditorOpen),
        CommandMeta::new("Debug", "Step Out", debug::StepOut).when(When::EditorOpen),
        CommandMeta::new("Debug", "Step Back", debug::StepBack).when(When::EditorOpen),
        CommandMeta::new("Debug", "Stop Debugging", debug::StopDebug).when(When::EditorOpen),
        CommandMeta::new(
            "Assistant",
            "Ask in Plain English",
            ui_assistant::OpenPromptBar,
        )
        .when(When::EditorOpen)
        .keywords(&["ai", "natural language", "llm", "prompt"]),
        CommandMeta::new("Editor", "Toggle Line Comment", editor::ToggleLineComment)
            .when(When::EditorOpen),
        CommandMeta::new("Editor", "Split Right", SplitRight).when(When::EditorOpen),
        CommandMeta::new("Editor", "Save Query", SaveQuery).when(When::EditorOpen),
        CommandMeta::new("Graph", "Expand Neighbours", graph::ExpandSelection)
            .when(When::GraphFocused),
        CommandMeta::new("Graph", "Hide Selection", graph::HideSelection).when(When::GraphFocused),
        CommandMeta::new("Graph", "Unhide All", graph::UnhideAll).when(When::GraphFocused),
        CommandMeta::new("Graph", "Select All", graph::SelectAll).when(When::GraphFocused),
        CommandMeta::new("Graph", "Select Neighbours", graph::SelectNeighbours)
            .when(When::GraphFocused),
        CommandMeta::new("Graph", "Pin Selection", graph::PinSelection).when(When::GraphFocused),
        CommandMeta::new("Graph", "Unpin Selection", graph::UnpinSelection)
            .when(When::GraphFocused),
        CommandMeta::new("Graph", "Fit to View", graph::FitGraph).when(When::GraphFocused),
        CommandMeta::new("Graph", "Find in Graph", graph::FindInGraph)
            .when(When::GraphFocused)
            .keywords(&["search"]),
        CommandMeta::new("Graph", "Clear Graph", graph::ClearGraph).when(When::GraphFocused),
        CommandMeta::new("Graph", "Re-run Layout", graph::RerunLayout)
            .when(When::GraphFocused)
            .keywords(&["layout", "arrange"]),
        CommandMeta::new("Graph", "Stop Layout", graph::StopLayout).when(When::GraphFocused),
        CommandMeta::new("Graph", "Undo Edit", graph::UndoEdit).when(When::GraphFocused),
        CommandMeta::new("Graph", "Redo Edit", graph::RedoEdit).when(When::GraphFocused),
        CommandMeta::new("Graph", "Delete from Database", graph::DeleteFromDatabase)
            .when(When::GraphFocused)
            .keywords(&["drop", "remove"]),
        CommandMeta::new("Graph", "New Vertex", graph::NewVertex)
            .when(When::GraphFocused)
            .keywords(&["create", "add"]),
        CommandMeta::new("Application", "Quit", Quit).keywords(&["exit"]),
    ] {
        CommandRegistry::register(cx, meta);
    }
}
