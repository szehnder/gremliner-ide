//! Shared visuals for connections: colour tags, environment badges, status dots.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use providers::{ColorTag, Environment};

use super::manager::ConnStatus;
use crate::theme::Tokens;

/// The theme colour for a colour tag (`None` for no tag).
pub fn color_tag_color(tag: ColorTag, cx: &App) -> Option<Hsla> {
    let t = cx.theme();
    Some(match tag {
        ColorTag::None => return None,
        ColorTag::Green => t.green,
        ColorTag::Blue => t.blue,
        ColorTag::Purple => t.magenta,
        ColorTag::Yellow => t.yellow_light,
        ColorTag::Orange => t.yellow,
        ColorTag::Red => t.red,
    })
}

/// A pill naming the environment. Local connections get none; production uses the danger colour.
pub fn environment_badge(env: Environment, cx: &App) -> Option<impl IntoElement + use<>> {
    let t = cx.theme();
    let (bg, fg) = match env {
        Environment::Local => return None,
        Environment::Dev => (t.info.opacity(0.15), t.info),
        Environment::Staging => (t.warning.opacity(0.15), t.warning),
        Environment::Production => (t.danger, t.danger_foreground),
    };
    Some(
        div()
            .id(SharedString::from(format!("env-badge-{}", env.id())))
            .test_support()
            .flex_none()
            .px(Tokens::space(1.5))
            .rounded(Tokens::RADIUS_SM)
            .bg(bg)
            .text_color(fg)
            .text_size(Tokens::text_xs())
            .font_weight(FontWeight::SEMIBOLD)
            .child(env.label().to_uppercase()),
    )
}

pub fn status_color(status: &ConnStatus, cx: &App) -> Hsla {
    let t = cx.theme();
    match status {
        ConnStatus::Disconnected => t.muted_foreground.opacity(0.6),
        ConnStatus::Connecting => t.warning,
        ConnStatus::Connected(_) => t.success,
        ConnStatus::Error(_) => t.danger,
    }
}

pub fn status_dot(status: &ConnStatus, cx: &App) -> Div {
    div()
        .flex_none()
        .size(Tokens::size(8.0))
        .rounded_full()
        .bg(status_color(status, cx))
}
