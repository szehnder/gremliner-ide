//! Re-exported design tokens used by the editor (ADR-0035).

pub use ui_theme::Tokens;

use gpui_kit::{App, Hsla, Pixels, Rems};

pub const BORDER: Pixels = Tokens::BORDER;

pub fn space(n: f32) -> Rems {
    Tokens::space(n)
}

pub fn size(default_px: f32) -> Rems {
    Tokens::size(default_px)
}

pub fn text_sm() -> Rems {
    Tokens::text_sm()
}

pub fn statement_highlight(cx: &App) -> Hsla {
    Tokens::statement_highlight(cx)
}
