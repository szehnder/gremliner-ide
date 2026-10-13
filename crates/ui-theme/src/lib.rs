//! Design tokens and theming (ADR-0023), shared by every UI crate (ADR-0035).
//!
//! Colours live in `assets/themes/gremliner.json` (gpui-kit theme format) and reach UI code through
//! `cx.theme()`. Sizes reach UI code through [`Tokens`]. This module and its children are the only UI
//! code allowed to contain raw colours or pixel literals (`cargo xtask tokens`).

pub mod fonts;

#[cfg(test)]
mod contrast;

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;
use serde::{Deserialize, Serialize};

/// The light and dark themes shipped with the app.
pub const THEME_SET_JSON: &str = include_str!("../../../assets/themes/gremliner.json");
pub const LIGHT_THEME: &str = "Gremliner Light";
pub const DARK_THEME: &str = "Gremliner Dark";

/// The theme setting chosen by the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    Light,
    Dark,
    /// Follow the operating system appearance, live.
    #[default]
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn id(self) -> &'static str {
        match self {
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
            ThemeChoice::System => "system",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::System => "System",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    /// The palette to show for this choice given the OS appearance.
    pub fn resolve(self, appearance: WindowAppearance) -> ThemeMode {
        match self {
            ThemeChoice::Light => ThemeMode::Light,
            ThemeChoice::Dark => ThemeMode::Dark,
            ThemeChoice::System => appearance.into(),
        }
    }
}

/// Font sizes in logical pixels. The UI size is also the rem size, so every gpui-kit component and every
/// [`Tokens`] spacing value scales with it.
pub const DEFAULT_UI_FONT_SIZE: f32 = 14.0;
pub const DEFAULT_EDITOR_FONT_SIZE: f32 = 13.0;
pub const MIN_FONT_SIZE: f32 = 10.0;
pub const MAX_FONT_SIZE: f32 = 24.0;
/// One zoom step.
pub const FONT_SIZE_STEP: f32 = 1.0;

/// Register fonts and themes. Call once at startup, after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    if let Err(err) = fonts::register(cx) {
        log::warn!("failed to register bundled fonts: {err:#}");
    }
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEME_SET_JSON) {
        log::error!("bundled theme set does not parse: {err:#}");
        return;
    }
    let registry = ThemeRegistry::global(cx);
    let light = registry.themes().get(LIGHT_THEME).cloned();
    let dark = registry.themes().get(DARK_THEME).cloned();
    if let (Some(light), Some(dark)) = (light, dark) {
        let theme = Theme::global_mut(cx);
        theme.light_theme = light;
        theme.dark_theme = dark;
    }
}

/// Apply theme mode and font sizes. Cheap when nothing changed; refreshes every window otherwise.
pub fn apply(
    choice: ThemeChoice,
    appearance: WindowAppearance,
    ui_font_size: f32,
    editor_font_size: f32,
    cx: &mut App,
) {
    let mode = choice.resolve(appearance);
    let ui = px(clamp_font_size(ui_font_size));
    let mono = px(clamp_font_size(editor_font_size));
    let current = Theme::global(cx);
    let mode_changed = current.mode != mode;
    if !mode_changed && current.font_size == ui && current.mono_font_size == mono {
        return;
    }
    if mode_changed {
        // Loading the mode's theme resets font sizes to the theme file's, so sizes are set afterwards.
        Theme::change(mode, None, cx);
    }
    Theme::update(cx, |theme| {
        theme.font_size = ui;
        theme.mono_font_size = mono;
    });
}

pub fn clamp_font_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE).round()
    } else {
        DEFAULT_UI_FONT_SIZE
    }
}

/// Tooltip plus accessible name for icon-only buttons: a tooltip alone is not announced, and an icon
/// button has no label for gpui-kit to fall back on.
pub trait ButtonTip {
    fn tip(self, text: impl Into<SharedString>) -> Self;
}

impl ButtonTip for gpui_kit::component::button::Button {
    fn tip(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.accessibility_label(text.clone()).tooltip(text)
    }
}

/// Standard motion durations (ADR-0023: 120/200/320 ms, ease-out).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Hover and press feedback, toasts.
    Fast,
    /// Panels and dialogs.
    Normal,
    /// Camera moves (graph focus and fit).
    Slow,
}

impl Motion {
    pub const fn millis(self) -> u64 {
        match self {
            Motion::Fast => 120,
            Motion::Normal => 200,
            Motion::Slow => 320,
        }
    }

    /// The duration to animate for: zero when reduce motion is on, so transitions happen instantly.
    pub fn duration(self, cx: &App) -> std::time::Duration {
        if cx.reduce_motion() {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_millis(self.millis())
        }
    }
}

/// Spacing, radii and component sizes. Spacing is in rems so it scales with the UI font size: at the
/// default 14 px, `space(1)` is exactly 4 px (ADR-0023's 4 px grid).
pub struct Tokens;

impl Tokens {
    /// `n` steps of the 4 px grid, scaled with the UI font size.
    pub fn space(n: f32) -> Rems {
        rems(n * 4.0 / DEFAULT_UI_FONT_SIZE)
    }

    /// A size in "default pixels" that scales with zoom like [`Self::space`].
    pub fn size(default_px: f32) -> Rems {
        rems(default_px / DEFAULT_UI_FONT_SIZE)
    }

    /// Hairline borders do not scale.
    pub const BORDER: Pixels = px(1.0);
    pub const RADIUS_SM: Pixels = px(4.0);
    pub const RADIUS_MD: Pixels = px(6.0);
    pub const RADIUS_LG: Pixels = px(10.0);

    /// Height of the status bar.
    pub fn status_bar_height() -> Rems {
        Self::size(26.0)
    }
    /// Height of the title bar on platforms where we draw it.
    pub fn title_bar_height() -> Rems {
        Self::size(34.0)
    }
    /// Width of the command palette dialog (gpui-kit dialogs take pixels).
    pub const PALETTE_WIDTH: Pixels = px(620.0);
    /// Width of the connection dialog.
    pub const CONNECTION_DIALOG_WIDTH: Pixels = px(600.0);
    /// Width of the keyboard shortcuts dialog.
    pub const SHORTCUTS_WIDTH: Pixels = px(560.0);
    /// Distance of dialogs from the top of the window.
    pub const DIALOG_TOP: Pixels = px(96.0);
    /// Max height of the command palette list.
    pub fn palette_list_height() -> Rems {
        Self::size(380.0)
    }
    /// Width of the first-run tour card.
    pub fn tour_width() -> Rems {
        Self::size(360.0)
    }
    /// Max height of the keyboard shortcuts list.
    pub fn shortcuts_height() -> Rems {
        Self::size(520.0)
    }
    /// Max width of the welcome view content column.
    pub fn welcome_width() -> Rems {
        Self::size(560.0)
    }
    /// Size of the welcome view's hero mark.
    pub fn hero_size() -> Rems {
        Self::size(56.0)
    }

    /// Text scale relative to the UI font size.
    pub fn text_xs() -> Rems {
        rems(11.0 / DEFAULT_UI_FONT_SIZE)
    }
    pub fn text_sm() -> Rems {
        rems(12.0 / DEFAULT_UI_FONT_SIZE)
    }
    pub fn text_md() -> Rems {
        rems(1.0)
    }
    pub fn text_lg() -> Rems {
        rems(16.0 / DEFAULT_UI_FONT_SIZE)
    }
    pub fn text_xl() -> Rems {
        rems(20.0 / DEFAULT_UI_FONT_SIZE)
    }
    pub fn text_display() -> Rems {
        rems(28.0 / DEFAULT_UI_FONT_SIZE)
    }

    /// Default widths of the docks in logical pixels, used when no layout was saved.
    pub const SIDEBAR_WIDTH: Pixels = px(260.0);
    pub const SIDEBAR_MIN_WIDTH: Pixels = px(180.0);
    pub const SIDEBAR_MAX_WIDTH: Pixels = px(520.0);
    pub const RIGHT_PANEL_WIDTH: Pixels = px(320.0);
    pub const RESULTS_HEIGHT: Pixels = px(300.0);

    /// Background of the statement that Run will execute.
    pub fn statement_highlight(cx: &App) -> Hsla {
        cx.theme().primary.opacity(0.07)
    }

    /// The debugger's current step.
    pub fn debug_step_highlight(cx: &App) -> Hsla {
        cx.theme().primary.opacity(0.22)
    }

    /// Steps an assistant explanation points at.
    pub fn hint_highlight(cx: &App) -> Hsla {
        cx.theme().warning.opacity(0.25)
    }
}

#[cfg(test)]
mod tests {
    // `gpui_kit::*` (test-support) exports a `test` macro that would shadow the built-in one.
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn choice_resolves_against_appearance() {
        assert_eq!(
            ThemeChoice::System.resolve(WindowAppearance::Dark),
            ThemeMode::Dark
        );
        assert_eq!(
            ThemeChoice::System.resolve(WindowAppearance::Light),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeChoice::Light.resolve(WindowAppearance::Dark),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeChoice::Dark.resolve(WindowAppearance::Light),
            ThemeMode::Dark
        );
        for choice in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::from_id(choice.id()), Some(choice));
        }
    }

    #[test]
    fn font_sizes_are_clamped() {
        assert_eq!(clamp_font_size(4.0), MIN_FONT_SIZE);
        assert_eq!(clamp_font_size(99.0), MAX_FONT_SIZE);
        assert_eq!(clamp_font_size(f32::NAN), DEFAULT_UI_FONT_SIZE);
        assert_eq!(clamp_font_size(14.4), 14.0);
    }
}
