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
        ResetGeneralSettings,
        Quit,
    ]
);

/// Editor commands that only apply when an editor tab is open. The editor change implements them; the
/// shell declares them so their palette visibility (`when`) can be tested now.
pub mod editor {
    use gpui_kit::actions;

    actions!(editor, [FormatQuery]);
}
