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
        SplitRight,
        SaveQuery,
        ResetGeneralSettings,
        Quit,
    ]
);

/// Editor commands, declared by `ui-editor` and re-exported for the palette and keymap.
pub mod editor {
    pub use ui_editor::actions::{FormatQuery, RunQuery, ToggleLineComment};
}
