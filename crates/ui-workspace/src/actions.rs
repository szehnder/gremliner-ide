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
        ClearGraph, ExpandSelection, FindInGraph, FitGraph, HideSelection, PinSelection, SelectAll,
        SelectNeighbours, UnhideAll, UnpinSelection,
    };
}

/// Editor commands, declared by `ui-editor` and re-exported for the palette and keymap.
pub mod editor {
    pub use ui_editor::actions::{FormatQuery, RunQuery, ToggleLineComment};
}
