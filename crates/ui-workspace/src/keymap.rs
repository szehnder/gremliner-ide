//! The single table of default key bindings. `secondary` is Cmd on macOS and Ctrl elsewhere.

use gpui_kit::{App, KeyBinding};

use crate::actions::*;

/// The text input inside a query editor.
const EDITOR_INPUT: &str = "QueryEditor > Input";
/// The graph canvas (not the graph view's search or filter inputs).
const GRAPH_CANVAS: &str = ui_results::graph_view::CANVAS_CONTEXT;
const GRAPH_VIEW: &str = ui_results::graph_view::VIEW_CONTEXT;

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
        KeyBinding::new("secondary-shift-a", ToggleAssistant, None),
        KeyBinding::new("secondary-+", ZoomIn, None),
        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
        KeyBinding::new("secondary-0", ZoomReset, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-t", NewQueryTab, None),
        KeyBinding::new("secondary-s", SaveQuery, None),
        KeyBinding::new("secondary-shift-h", ShowHistory, None),
        KeyBinding::new("secondary-\\", SplitRight, None),
        // Inside the editor these must beat the text input's own bindings (Ctrl+Enter inserts a newline,
        // Ctrl+/ is the global shortcuts key), so they bind in the more specific context.
        // Esc cancels only while a query runs, so the editor keeps its own Escape otherwise.
        KeyBinding::new("escape", CancelQuery, Some("QueryRunning")),
        KeyBinding::new("escape", CancelQuery, Some("QueryRunning > Input")),
        KeyBinding::new(
            "secondary-k",
            ui_assistant::OpenPromptBar,
            Some(EDITOR_INPUT),
        ),
        KeyBinding::new(
            "secondary-k",
            ui_assistant::OpenPromptBar,
            Some("EditorPanel"),
        ),
        KeyBinding::new(
            "escape",
            ui_assistant::ClosePromptBar,
            Some("PromptBar > Input"),
        ),
        KeyBinding::new("secondary-enter", editor::RunQuery, Some(EDITOR_INPUT)),
        KeyBinding::new("alt-enter", editor::RunQuery, Some(EDITOR_INPUT)),
        KeyBinding::new(
            "secondary-shift-enter",
            editor::ProfileQuery,
            Some(EDITOR_INPUT),
        ),
        KeyBinding::new("shift-alt-f", editor::FormatQuery, Some(EDITOR_INPUT)),
        KeyBinding::new("ctrl-f5", debug::StartDebug, Some(EDITOR_INPUT)),
        KeyBinding::new("ctrl-f5", debug::StartDebug, Some("EditorPanel")),
        KeyBinding::new("shift-f5", debug::StopDebug, Some("EditorPanel")),
        KeyBinding::new("f10", debug::StepOver, Some("EditorPanel")),
        KeyBinding::new("f11", debug::StepInto, Some("EditorPanel")),
        KeyBinding::new("shift-f11", debug::StepOut, Some("EditorPanel")),
        KeyBinding::new("f9", debug::StepBack, Some("EditorPanel")),
        KeyBinding::new("secondary-/", editor::ToggleLineComment, Some(EDITOR_INPUT)),
        KeyBinding::new("backspace", graph::HideSelection, Some(GRAPH_CANVAS)),
        KeyBinding::new("delete", graph::HideSelection, Some(GRAPH_CANVAS)),
        KeyBinding::new("secondary-shift-h", graph::UnhideAll, Some(GRAPH_CANVAS)),
        KeyBinding::new("secondary-a", graph::SelectAll, Some(GRAPH_CANVAS)),
        KeyBinding::new("n", graph::SelectNeighbours, Some(GRAPH_CANVAS)),
        KeyBinding::new("e", graph::ExpandSelection, Some(GRAPH_CANVAS)),
        KeyBinding::new("f", graph::FitGraph, Some(GRAPH_CANVAS)),
        KeyBinding::new("secondary-f", graph::FindInGraph, Some(GRAPH_VIEW)),
        KeyBinding::new(
            "shift-delete",
            graph::DeleteFromDatabase,
            Some(GRAPH_CANVAS),
        ),
        KeyBinding::new("secondary-shift-z", graph::RedoEdit, Some(GRAPH_CANVAS)),
        KeyBinding::new("secondary-z", graph::UndoEdit, Some(GRAPH_CANVAS)),
    ]);
}
