//! The shared empty-state block: an icon, a title, a sentence, optional actions.

use gpui_kit::component::{ActiveTheme as _, Icon, v_flex};
use gpui_kit::*;

use crate::theme::Tokens;

pub fn empty_state(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(Tokens::space(2.0))
        .p(Tokens::space(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(Tokens::size(40.0))
                .rounded(Tokens::RADIUS_LG)
                .bg(theme.muted)
                .child(
                    icon.into()
                        .size(Tokens::size(20.0))
                        .text_color(theme.muted_foreground),
                ),
        )
        .child(
            div()
                .mt(Tokens::space(1.0))
                .text_size(Tokens::text_md())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(title.into()),
        )
        .child(
            div()
                .max_w(Tokens::size(260.0))
                .text_center()
                .text_size(Tokens::text_sm())
                .text_color(theme.muted_foreground)
                .child(body.into()),
        )
}
