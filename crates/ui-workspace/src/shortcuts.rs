//! The keyboard shortcuts sheet: every command that has a key binding, grouped by area.
//!
//! Rows come from the keymap itself (any context), not from what the focused element can reach, so
//! editor and graph bindings show while the sheet's dialog has focus.

use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::palette::CommandRegistry;
use crate::theme::Tokens;

/// One row of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    pub category: &'static str,
    pub title: &'static str,
    /// The action's name, e.g. `workspace::SaveQuery`.
    pub action: &'static str,
    /// Display form of the binding shown in hints, e.g. `Ctrl+S`.
    pub keys: String,
    keystrokes: Vec<Keystroke>,
}

/// Bound commands grouped by category (categories in registration order, commands within them too).
pub fn rows(cx: &App) -> Vec<(&'static str, Vec<ShortcutRow>)> {
    let mut groups: Vec<(&'static str, Vec<ShortcutRow>)> = Vec::new();
    for meta in CommandRegistry::commands(cx) {
        // The last binding added is the one shown in hints (see `keymap`).
        let Some(keystrokes) = keystrokes_for(meta.action.as_ref(), cx) else {
            continue;
        };
        let row = ShortcutRow {
            category: meta.category,
            title: meta.title,
            action: meta.action.name(),
            keys: keystrokes
                .iter()
                .map(Kbd::format)
                .collect::<Vec<_>>()
                .join(" "),
            keystrokes,
        };
        match groups.iter_mut().find(|(c, _)| *c == meta.category) {
            Some((_, list)) => {
                if !list.iter().any(|r| r.action == row.action) {
                    list.push(row);
                }
            }
            None => groups.push((meta.category, vec![row])),
        }
    }
    groups
}

/// The keys shown for `action` in hints: its last binding in any context.
pub fn keystrokes_for(action: &dyn Action, cx: &App) -> Option<Vec<Keystroke>> {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let binding = keymap.bindings_for_action(action).next_back()?;
    Some(
        binding
            .keystrokes()
            .iter()
            .map(|k| k.as_keystroke().clone())
            .collect(),
    )
}

/// Element id of an action's row in the sheet.
pub fn row_id(action: &str) -> SharedString {
    SharedString::from(format!("shortcut-{}", action.replace("::", "-")))
}

pub fn open(window: &mut Window, cx: &mut App) {
    let groups = rows(cx);
    window.open_dialog(cx, move |dialog, _, cx| {
        let theme = cx.theme();
        let sections = groups.iter().map(|(category, rows)| {
            v_flex()
                .pt(Tokens::space(3.0))
                .child(
                    div()
                        .pb(Tokens::space(1.0))
                        .text_size(Tokens::text_xs())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.muted_foreground)
                        .child(category.to_uppercase()),
                )
                .children(rows.iter().map(|row| {
                    h_flex()
                        .id(row_id(row.action))
                        .test_support()
                        .justify_between()
                        .py(Tokens::space(1.5))
                        .border_b(Tokens::BORDER)
                        .border_color(theme.border)
                        .child(row.title)
                        .child(
                            h_flex()
                                .gap(Tokens::space(1.0))
                                .children(row.keystrokes.iter().cloned().map(Kbd::new)),
                        )
                }))
        });
        dialog
            .title("Keyboard shortcuts")
            .w(Tokens::SHORTCUTS_WIDTH)
            .margin_top(Tokens::DIALOG_TOP)
            .child(
                v_flex()
                    .id("shortcuts")
                    .test_support()
                    .max_h(Tokens::shortcuts_height())
                    .overflow_y_scroll()
                    .children(sections),
            )
    });
}
