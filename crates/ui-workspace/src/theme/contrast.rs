//! WCAG 2.1 contrast check over every text role / surface pair the shell uses, in both bundled themes.
//! Colours are resolved through gpui-kit's own theme loader, so the test sees exactly what is painted.

#![allow(clippy::unwrap_used)]

use std::rc::Rc;

use gpui_kit::Hsla;
use gpui_kit::component::{Theme, ThemeColor, ThemeSet};

use super::THEME_SET_JSON;

/// Normal text needs 4.5:1; large text and UI glyphs (focus rings, icons) need 3:1.
const AA_TEXT: f32 = 4.5;
const AA_UI: f32 = 3.0;

fn relative_luminance(color: Hsla) -> f32 {
    let rgb = color.to_rgb();
    let channel = |c: f32| {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
}

/// Composite `fg` over an opaque `bg` (alpha blending in sRGB space, as the renderer does).
fn over(fg: Hsla, bg: Hsla) -> Hsla {
    let (f, b) = (fg.to_rgb(), bg.to_rgb());
    let a = f.a;
    Hsla::from(gpui_kit::Rgba {
        r: f.r * a + b.r * (1.0 - a),
        g: f.g * a + b.g * (1.0 - a),
        b: f.b * a + b.b * (1.0 - a),
        a: 1.0,
    })
}

pub fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

type Role = fn(&ThemeColor) -> Hsla;

/// (name, text colour, surface colour, minimum ratio).
fn pairs() -> Vec<(&'static str, Role, Role, f32)> {
    let mut pairs: Vec<(&'static str, Role, Role, f32)> = Vec::new();
    // Body and secondary text on every surface they appear on.
    let surfaces: [(&str, Role); 10] = [
        ("background", |c| c.background),
        ("sidebar", |c| c.sidebar),
        ("popover", |c| c.popover),
        ("tab_bar", |c| c.tab_bar),
        ("title_bar", |c| c.title_bar),
        ("status_bar", |c| c.status_bar),
        ("secondary", |c| c.secondary),
        ("muted", |c| c.muted),
        ("list_even", |c| c.list_even),
        ("group_box", |c| c.group_box),
    ];
    for (name, surface) in surfaces {
        pairs.push((name, |c| c.foreground, surface, AA_TEXT));
        pairs.push((name, |c| c.muted_foreground, surface, AA_TEXT));
    }
    pairs.extend([
        (
            "primary",
            (|c| c.primary_foreground) as Role,
            (|c| c.primary) as Role,
            AA_TEXT,
        ),
        (
            "primary_hover",
            |c| c.primary_foreground,
            |c| c.primary_hover,
            AA_TEXT,
        ),
        (
            "secondary",
            |c| c.secondary_foreground,
            |c| c.secondary,
            AA_TEXT,
        ),
        ("accent", |c| c.accent_foreground, |c| c.accent, AA_TEXT),
        ("danger", |c| c.danger_foreground, |c| c.danger, AA_TEXT),
        ("warning", |c| c.warning_foreground, |c| c.warning, AA_TEXT),
        ("success", |c| c.success_foreground, |c| c.success, AA_TEXT),
        ("info", |c| c.info_foreground, |c| c.info, AA_TEXT),
        (
            "sidebar_fg",
            |c| c.sidebar_foreground,
            |c| c.sidebar,
            AA_TEXT,
        ),
        (
            "sidebar_accent",
            |c| c.sidebar_accent_foreground,
            |c| c.sidebar_accent,
            AA_TEXT,
        ),
        (
            "sidebar_primary",
            |c| c.sidebar_primary_foreground,
            |c| c.sidebar_primary,
            AA_TEXT,
        ),
        (
            "popover_fg",
            |c| c.popover_foreground,
            |c| c.popover,
            AA_TEXT,
        ),
        (
            "tab_active",
            |c| c.tab_active_foreground,
            |c| c.tab_active,
            AA_TEXT,
        ),
        ("tab", |c| c.tab_foreground, |c| c.tab_bar, AA_TEXT),
        (
            "table_head",
            |c| c.table_head_foreground,
            |c| c.table_head,
            AA_TEXT,
        ),
        ("link", |c| c.link, |c| c.background, AA_TEXT),
        ("link_hover", |c| c.link_hover, |c| c.background, AA_TEXT),
        // Focus indicators are UI glyphs.
        ("ring", |c| c.ring, |c| c.background, AA_UI),
        ("ring_sidebar", |c| c.ring, |c| c.sidebar, AA_UI),
    ]);
    pairs
}

fn bundled_themes() -> Vec<(String, ThemeColor)> {
    let set: ThemeSet = serde_json::from_str(THEME_SET_JSON).unwrap();
    set.themes
        .into_iter()
        .map(|config| {
            let name = config.name.to_string();
            let mut theme = Theme::default();
            theme.apply_config(&Rc::new(config));
            (name, theme.colors)
        })
        .collect()
}

#[test]
fn scenario_theming_contrast_check() {
    let themes = bundled_themes();
    assert_eq!(themes.len(), 2, "expected a light and a dark theme");
    let mut failures = Vec::new();
    for (theme, colors) in &themes {
        for (name, text, surface, min) in pairs() {
            let bg = surface(colors);
            let ratio = contrast_ratio(over(text(colors), bg), bg);
            if ratio < min {
                failures.push(format!("{theme}: {name} is {ratio:.2}:1, needs {min}:1"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "contrast failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn contrast_formula_matches_wcag_reference() {
    let black = Hsla::black();
    let white = Hsla::white();
    assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
    assert!((contrast_ratio(white, white) - 1.0).abs() < 0.001);
}
