//! Assistant UI: the prompt bar with its verified result card (nl-to-gremlin). The host (the editor tab)
//! runs the pipeline and handles Insert/Run.

pub mod prompt_bar;

pub use prompt_bar::{ClosePromptBar, Disclosure, OpenPromptBar, PromptBar, PromptBarEvent};
