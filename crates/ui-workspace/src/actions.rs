//! Every workspace-level user command is a GPUI action declared here. Feature crates declare their own
//! actions and register [`CommandMeta`](crate::palette::CommandMeta) entries so they show in the palette.

use gpui_kit::actions;

actions!(
    workspace,
    [
        OpenCommandPalette,
        OpenSettings,
        ShowKeyboardShortcuts,
        ToggleSidebar,
        ToggleResultsDock,
        ToggleRightPanel,
        ToggleAssistant,
        ResetLayout,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        AddConnection,
        NewQueryTab,
        CancelQuery,
        SplitRight,
        SaveQuery,
        ResetGeneralSettings,
        Quit,
    ]
);

/// Graph view commands, declared by `ui-results` and re-exported for the palette and keymap.
pub mod graph {
    pub use ui_results::graph_view::{
        ClearGraph, DeleteFromDatabase, ExpandSelection, FindInGraph, FitGraph, HideSelection,
        NewVertex, PinSelection, RedoEdit, RerunLayout, SelectAll, SelectNeighbours, StopLayout,
        UndoEdit, UnhideAll, UnpinSelection,
    };
}

/// Editor commands, declared by `ui-editor` and re-exported for the palette and keymap.
pub mod editor {
    pub use ui_editor::actions::{FormatQuery, RunQuery, ToggleLineComment};
}

/// Open a new tab bound to `connection` with `query`, and run it (the schema browser's "Sample 25").
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = workspace, no_json)]
pub struct OpenSampleQuery {
    pub connection: uuid::Uuid,
    pub query: String,
}

/// Open the schema graph of `connection` (the active connection when `None`).
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = workspace, no_json)]
pub struct OpenSchemaGraph {
    pub connection: Option<uuid::Uuid>,
}

/// Re-discover the schema of the active connection.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = workspace, no_json)]
pub struct RefreshSchema {
    pub connection: Option<uuid::Uuid>,
}
