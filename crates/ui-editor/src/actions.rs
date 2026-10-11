//! Editor actions. Bindings live in `ui_workspace::keymap` under the `QueryEditor > Input` context so they
//! take precedence over the text input's own (e.g. Ctrl+Enter inserting a newline).

use gpui_kit::actions;

actions!(editor, [RunQuery, FormatQuery, ToggleLineComment]);
