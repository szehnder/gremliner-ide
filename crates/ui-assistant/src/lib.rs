//! Assistant UI: the prompt bar with its verified result card (nl-to-gremlin). The host (the editor tab)
//! runs the pipeline and handles Insert/Run.

pub mod chat_view;
pub mod prompt_bar;

pub use chat_view::{Available, ChatView, ChatViewEvent, ChipKind};
pub use prompt_bar::{ClosePromptBar, Disclosure, OpenPromptBar, PromptBar, PromptBarEvent};
