//! Application settings: an in-memory global that applies changes immediately and persists them to the
//! `settings` table in the background. Each settings page is one JSON row (`general`, ...), so a page
//! resets as a unit and a page added by a later change does not disturb older rows.

use gpui_kit::*;
use serde::{Deserialize, Serialize};
use storage::{Storage, Table};

use crate::theme::{self, DEFAULT_EDITOR_FONT_SIZE, DEFAULT_UI_FONT_SIZE, ThemeChoice};

/// `settings` row for the General page.
pub const GENERAL_KEY: &str = "general";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub theme: ThemeChoice,
    pub ui_font_size: f32,
    pub editor_font_size: f32,
    /// Maximum result items collected per query.
    pub result_limit: usize,
    /// Query history entries kept (oldest removed first).
    pub history_limit: usize,
}

/// Default and bounds for [`GeneralSettings::result_limit`].
pub const DEFAULT_RESULT_LIMIT: usize = 10_000;
pub const MAX_RESULT_LIMIT: usize = 1_000_000;

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            ui_font_size: DEFAULT_UI_FONT_SIZE,
            editor_font_size: DEFAULT_EDITOR_FONT_SIZE,
            result_limit: DEFAULT_RESULT_LIMIT,
            history_limit: storage::DEFAULT_HISTORY_LIMIT,
        }
    }
}

impl GeneralSettings {
    fn normalized(mut self) -> Self {
        self.ui_font_size = theme::clamp_font_size(self.ui_font_size);
        self.editor_font_size = theme::clamp_font_size(self.editor_font_size);
        self.result_limit = self.result_limit.clamp(1, MAX_RESULT_LIMIT);
        self.history_limit = self.history_limit.clamp(100, 1_000_000);
        self
    }

    /// Read the persisted page, falling back to defaults (with a warning) when missing or unreadable.
    /// Blocking: startup only.
    pub fn load(storage: Option<&Storage>) -> Self {
        let Some(storage) = storage else {
            return Self::default();
        };
        match storage.get_json_blocking::<Self>(Table::Settings, GENERAL_KEY) {
            Ok(Some(settings)) => settings.normalized(),
            Ok(None) => Self::default(),
            Err(err) => {
                log::warn!("ignoring unreadable general settings: {err}");
                Self::default()
            }
        }
    }
}

/// The settings global.
pub struct AppSettings {
    general: GeneralSettings,
    storage: Option<Storage>,
    /// Last OS appearance reported by the main window, for `ThemeChoice::System`.
    appearance: WindowAppearance,
}

impl Global for AppSettings {}

impl AppSettings {
    /// Install the settings global and apply them. `storage` is `None` in tests that don't persist.
    pub fn init(general: GeneralSettings, storage: Option<Storage>, cx: &mut App) {
        let appearance = cx.window_appearance();
        cx.set_global(AppSettings {
            general: general.normalized(),
            storage,
            appearance,
        });
        Self::apply(cx);
    }

    pub fn general(cx: &App) -> &GeneralSettings {
        &cx.global::<AppSettings>().general
    }

    pub fn storage(cx: &App) -> Option<&Storage> {
        cx.try_global::<AppSettings>()?.storage.as_ref()
    }

    /// Change general settings: applied now, persisted in the background.
    pub fn update_general(cx: &mut App, edit: impl FnOnce(&mut GeneralSettings)) {
        let this = cx.global_mut::<AppSettings>();
        let before = this.general.clone();
        edit(&mut this.general);
        this.general = this.general.clone().normalized();
        if this.general == before {
            return;
        }
        let general = this.general.clone();
        if let Some(storage) = this.storage.clone() {
            let write = storage.set_json(Table::Settings, GENERAL_KEY, &general);
            cx.background_spawn(async move {
                if let Err(err) = write.await {
                    log::warn!("failed to save settings: {err}");
                }
            })
            .detach();
        }
        Self::apply(cx);
    }

    /// Restore the General page to its defaults.
    pub fn reset_general(cx: &mut App) {
        Self::update_general(cx, |general| *general = GeneralSettings::default());
    }

    /// The OS appearance changed; re-resolve `System`.
    pub fn set_appearance(appearance: WindowAppearance, cx: &mut App) {
        let this = cx.global_mut::<AppSettings>();
        if this.appearance != appearance {
            this.appearance = appearance;
            Self::apply(cx);
        }
    }

    fn apply(cx: &mut App) {
        let this = cx.global::<AppSettings>();
        let general = this.general.clone();
        let appearance = this.appearance;
        theme::apply(
            general.theme,
            appearance,
            general.ui_font_size,
            general.editor_font_size,
            cx,
        );
    }
}
