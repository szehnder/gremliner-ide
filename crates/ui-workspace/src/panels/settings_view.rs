//! The settings view, opened as a tab in the editor area. Pages come from gpui-kit's `Settings`; values
//! apply on change through [`AppSettings`] (no save button).

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel};
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _};
use gpui_kit::*;

use super::{WorkspacePanel, panel_basics};
use crate::settings::{AppSettings, GeneralSettings};
use crate::theme::{MAX_FONT_SIZE, MIN_FONT_SIZE, ThemeChoice};

pub struct SettingsPanel {
    focus_handle: FocusHandle,
}

panel_basics!(SettingsPanel);

impl WorkspacePanel for SettingsPanel {
    const NAME: &'static str = "gremliner.settings";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Center;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl BasePanel for SettingsPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }
}

impl Panel for SettingsPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Settings"
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

/// Ask before resetting the General page, then reset it.
pub fn confirm_reset_general(window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, |alert, _, cx| {
        alert
            .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning))
            .title("Reset general settings?")
            .description("Theme and font sizes return to their default values.")
            .ok_text("Reset")
            .ok_variant(ButtonVariant::Danger)
            .show_cancel(true)
            .on_ok(|_, _, cx| {
                AppSettings::reset_general(cx);
                true
            })
    });
}

fn font_size_field(
    get: fn(&GeneralSettings) -> f32,
    set: fn(&mut GeneralSettings, f32),
    default: f32,
) -> SettingField<f64> {
    SettingField::number_input(
        NumberFieldOptions {
            min: f64::from(MIN_FONT_SIZE),
            max: f64::from(MAX_FONT_SIZE),
            step: 1.0,
        },
        move |cx| f64::from(get(AppSettings::general(cx))),
        move |value, cx| {
            #[allow(clippy::cast_possible_truncation)]
            let value = value as f32;
            AppSettings::update_general(cx, |g| set(g, value));
        },
    )
    .default_value(f64::from(default))
}

fn general_page() -> SettingPage {
    let defaults = GeneralSettings::default();
    let theme_options = ThemeChoice::ALL
        .iter()
        .map(|c| (c.id().into(), c.label().into()))
        .collect();
    SettingPage::new("General")
        .icon(IconName::Settings)
        .resettable(false)
        .title_suffix(|_, _| {
            Button::new("settings-reset-general")
                .label("Reset to defaults")
                .ghost()
                .small()
                .on_click(|_, window, cx| confirm_reset_general(window, cx))
        })
        .group(
            SettingGroup::new()
                .title("Appearance")
                .item(
                    SettingItem::new(
                        "Theme",
                        SettingField::dropdown(
                            theme_options,
                            |cx| AppSettings::general(cx).theme.id().into(),
                            |value, cx| {
                                let choice = ThemeChoice::from_id(&value).unwrap_or_default();
                                AppSettings::update_general(cx, |g| g.theme = choice);
                            },
                        )
                        .default_value(SharedString::from(defaults.theme.id())),
                    )
                    .description(
                        "System follows your operating system's light or dark appearance.",
                    ),
                )
                .item(
                    SettingItem::new(
                        "UI font size",
                        font_size_field(
                            |g| g.ui_font_size,
                            |g, v| g.ui_font_size = v,
                            defaults.ui_font_size,
                        ),
                    )
                    .description("Also changed with Ctrl+= and Ctrl+-."),
                )
                .item(
                    SettingItem::new(
                        "Result limit",
                        SettingField::number_input(
                            NumberFieldOptions {
                                min: 1.0,
                                #[allow(clippy::cast_precision_loss)]
                                max: crate::settings::MAX_RESULT_LIMIT as f64,
                                step: 1000.0,
                            },
                            #[allow(clippy::cast_precision_loss)]
                            |cx| AppSettings::general(cx).result_limit as f64,
                            |value, cx| {
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                let value = value.max(1.0) as usize;
                                AppSettings::update_general(cx, |g| g.result_limit = value);
                            },
                        )
                        .default_value(
                            #[allow(clippy::cast_precision_loss)]
                            {
                                crate::settings::DEFAULT_RESULT_LIMIT as f64
                            },
                        ),
                    )
                    .description("Queries stop collecting results after this many items."),
                )
                .item(SettingItem::new(
                    "Editor font size",
                    font_size_field(
                        |g| g.editor_font_size,
                        |g, v| g.editor_font_size = v,
                        defaults.editor_font_size,
                    ),
                )),
        )
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .id("settings-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .child(Settings::new("settings").page(general_page()))
    }
}
