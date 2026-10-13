//! The Gremlin query editor: gpui-kit's code editor with tree-sitter-gremlin highlighting, diagnostics from
//! `gremlin-syntax`, hover docs, format, comment toggling and run requests.

pub mod actions;
pub mod comment;
pub mod completion;
pub mod editor;
pub mod hover;
pub mod language;
mod tokens;

pub use editor::{KEY_CONTEXT, QueryEditor, QueryEditorEvent, RunMode};
pub use language::{LANGUAGE_NAME, highlight_styles, register};
