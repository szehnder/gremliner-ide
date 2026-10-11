//! The single table of default key bindings. `secondary` is Cmd on macOS and Ctrl elsewhere.

use gpui_kit::{App, KeyBinding};

use crate::actions::*;

/// The text input inside a query editor.
const EDITOR_INPUT: &str = "QueryEditor > Input";

pub fn bind_default_keys(cx: &mut App) {
    cx.bind_keys([
        // The last binding for an action is the one shown in hints, so the primary one comes last.
        KeyBinding::new("f1", OpenCommandPalette, None),
        KeyBinding::new("secondary-shift-p", OpenCommandPalette, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-/", ShowKeyboardShortcuts, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        KeyBinding::new("secondary-j", ToggleResultsDock, None),
        KeyBinding::new("secondary-alt-b", ToggleRightPanel, None),
        KeyBinding::new("secondary-+", ZoomIn, None),
        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
        KeyBinding::new("secondary-0", ZoomReset, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-t", NewQueryTab, None),
        KeyBinding::new("secondary-s", SaveQuery, None),
        KeyBinding::new("secondary-\\", SplitRight, None),
        // Inside the editor these must beat the text input's own bindings (Ctrl+Enter inserts a newline,
        // Ctrl+/ is the global shortcuts key), so they bind in the more specific context.
        KeyBinding::new("secondary-enter", editor::RunQuery, Some(EDITOR_INPUT)),
        KeyBinding::new("alt-enter", editor::RunQuery, Some(EDITOR_INPUT)),
        KeyBinding::new("shift-alt-f", editor::FormatQuery, Some(EDITOR_INPUT)),
        KeyBinding::new("secondary-/", editor::ToggleLineComment, Some(EDITOR_INPUT)),
    ]);
}
