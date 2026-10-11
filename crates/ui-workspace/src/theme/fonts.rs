//! Bundled fonts (ADR-0023): Inter for UI text, JetBrains Mono for code. Both are SIL OFL 1.1; the
//! licences sit beside the files in `assets/fonts/`.

use std::borrow::Cow;

use gpui_kit::App;

pub const UI_FONT_FAMILY: &str = "Inter";
pub const MONO_FONT_FAMILY: &str = "JetBrains Mono";

macro_rules! font {
    ($path:literal) => {
        include_bytes!(concat!("../../../../assets/fonts/", $path)).as_slice()
    };
}

/// Every bundled font file.
pub const FONT_FILES: &[&[u8]] = &[
    font!("inter/Inter-Regular.ttf"),
    font!("inter/Inter-Medium.ttf"),
    font!("inter/Inter-SemiBold.ttf"),
    font!("inter/Inter-Bold.ttf"),
    font!("jetbrains-mono/JetBrainsMono-Regular.ttf"),
    font!("jetbrains-mono/JetBrainsMono-Bold.ttf"),
    font!("jetbrains-mono/JetBrainsMono-Italic.ttf"),
];

fn font_data() -> Vec<Cow<'static, [u8]>> {
    FONT_FILES
        .iter()
        .map(|bytes| Cow::Borrowed(*bytes))
        .collect()
}

/// Register the bundled fonts with the app's text system.
pub fn register(cx: &App) -> anyhow::Result<()> {
    cx.text_system().add_fonts(font_data())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// Uses the real platform text system (not the test platform's no-op one) so this proves the
    /// bytes are valid fonts with the expected family names, independent of installed system fonts.
    #[test]
    fn scenario_theming_fonts_available_offline() {
        let text_system = gpui_kit::platform::current_platform(true).text_system();
        text_system.add_fonts(font_data()).unwrap();
        let names = text_system.all_font_names();
        for family in [UI_FONT_FAMILY, MONO_FONT_FAMILY] {
            assert!(
                names.iter().any(|name| name == family),
                "{family} missing from the text system after registering bundled fonts"
            );
            let id = text_system.font_id(&gpui_kit::font(family)).unwrap();
            assert!(text_system.font_metrics(id).units_per_em > 0);
        }
    }
}
