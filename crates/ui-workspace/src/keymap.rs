//! The single table of default key bindings. `secondary` is Cmd on macOS and Ctrl elsewhere.

use gpui_kit::{App, KeyBinding};

use crate::actions::*;

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
        KeyBinding::new("secondary-shift-i", editor::FormatQuery, Some("Editor")),
    ]);
}
