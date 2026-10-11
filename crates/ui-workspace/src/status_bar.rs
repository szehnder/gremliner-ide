//! The status bar: connection state on the left, background activity, zoom and version on the right.

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::*;

use crate::io::Activity;
use crate::settings::AppSettings;
use crate::theme::{DEFAULT_UI_FONT_SIZE, Tokens};

pub struct StatusBar {
    activity: Entity<Activity>,
    _subscriptions: Vec<Subscription>,
}

impl StatusBar {
    pub fn new(cx: &mut Context<'_, Self>) -> Self {
        let activity = Activity::global(cx);
        let subscriptions = vec![
            cx.observe(&activity, |_, _, cx| cx.notify()),
            cx.observe_global::<AppSettings>(|_, cx| cx.notify()),
        ];
        Self {
            activity,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let activity = self.activity.read(cx).summary();
        let ui_size = AppSettings::general(cx).ui_font_size;
        #[allow(clippy::cast_possible_truncation)]
        let zoom = (ui_size / DEFAULT_UI_FONT_SIZE * 100.0).round() as i32;

        let item = || {
            h_flex()
                .gap(Tokens::space(1.5))
                .items_center()
                .text_size(Tokens::text_xs())
                .text_color(theme.muted_foreground)
        };

        let connection = item()
            .id("status-connection")
            .child(
                div()
                    .size(Tokens::size(7.0))
                    .rounded_full()
                    .bg(theme.muted_foreground.opacity(0.6)),
            )
            .child("Not connected");

        let busy = activity.map(|label| {
            item()
                .id("status-activity")
                .test_support()
                .child(Spinner::new().xsmall().color(theme.primary))
                .child(label)
        });

        let zoom_indicator = (zoom != 100).then(|| {
            item()
                .id("status-zoom")
                .test_support()
                .child(format!("{zoom}%"))
        });

        h_flex()
            .id("status-bar")
            .test_support()
            .flex_none()
            .h(Tokens::status_bar_height())
            .px(Tokens::space(3.0))
            .justify_between()
            .bg(theme.status_bar)
            .border_t(Tokens::BORDER)
            .border_color(theme.status_bar_border)
            .child(
                h_flex()
                    .gap(Tokens::space(4.0))
                    .child(connection)
                    .children(busy),
            )
            .child(
                h_flex()
                    .gap(Tokens::space(4.0))
                    .children(zoom_indicator)
                    .child(item().child(concat!("Gremliner ", env!("CARGO_PKG_VERSION")))),
            )
    }
}
