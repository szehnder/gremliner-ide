//! The "AI" settings page: LLM profiles (add from a preset, edit, delete, default) and, per connection,
//! which profile the assistant uses and whether sample data may be shared with it (ADR-0021).

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::*;
use llm::PRESETS;
use uuid::Uuid;

use super::{LlmProfiles, form};
use crate::connections::ConnectionManager;
use crate::theme::ButtonTip as _;
use crate::theme::Tokens;

/// Lowercase id-safe text for element ids.
pub fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Open the profile dialog for a new profile from preset `id`.
pub fn add_from_preset(
    id: &str,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<form::ProfileForm>> {
    let preset = llm::preset(id)?;
    let existing = LlmProfiles::global(cx).read(cx).profiles().len();
    let name = if existing == 0 {
        preset.name.to_owned()
    } else {
        format!("{} {}", preset.name, existing + 1)
    };
    Some(form::open(preset.profile(name), false, window, cx))
}

pub fn edit(id: Uuid, window: &mut Window, cx: &mut App) -> Option<Entity<form::ProfileForm>> {
    let profile = LlmProfiles::global(cx).read(cx).get(id).cloned()?;
    Some(form::open(profile, true, window, cx))
}

fn confirm_delete(id: Uuid, name: String, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, move |alert, _, cx| {
        alert
            .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
            .title(format!("Delete \u{201c}{name}\u{201d}?"))
            .description("Its API key is removed from the system keychain too.")
            .ok_text("Delete")
            .ok_variant(ButtonVariant::Danger)
            .show_cancel(true)
            .on_ok(move |_, _, cx| {
                LlmProfiles::global(cx)
                    .update(cx, |p, cx| p.delete(id, cx))
                    .detach();
                true
            })
    });
}

pub struct AiSettingsView {
    _subscriptions: Vec<Subscription>,
}

impl AiSettingsView {
    pub fn new(cx: &mut Context<'_, Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(p) = LlmProfiles::try_global(cx) {
            subscriptions.push(cx.observe(&p, |_, _, cx| cx.notify()));
        }
        if let Some(m) = ConnectionManager::try_global(cx) {
            subscriptions.push(cx.observe(&m, |_, _, cx| cx.notify()));
        }
        Self {
            _subscriptions: subscriptions,
        }
    }

    fn profiles(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let state = LlmProfiles::global(cx);
        let (profiles, default) = {
            let s = state.read(cx);
            (s.profiles().to_vec(), s.default_id())
        };
        let add = Button::new("ai-add")
            .label("Add profile")
            .icon(IconName::Plus)
            .small()
            .dropdown_menu(|mut menu, _, _| {
                for preset in PRESETS {
                    let id = preset.id;
                    menu = menu.item(PopupMenuItem::new(preset.name).on_click(
                        move |_, window, cx| {
                            add_from_preset(id, window, cx);
                        },
                    ));
                }
                menu
            });
        let rows = profiles.into_iter().map(|p| {
            let slug = slug(&p.name);
            let id = p.id;
            let is_default = default == Some(id);
            let name = p.name.clone();
            let model = if p.model.is_empty() {
                "no model".to_owned()
            } else {
                p.model.clone()
            };
            h_flex()
                .id(SharedString::from(format!("ai-profile-{slug}")))
                .test_support()
                .gap(Tokens::space(2.0))
                .py(Tokens::space(1.5))
                .border_b(Tokens::BORDER)
                .border_color(theme.border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .gap(Tokens::space(2.0))
                                .child(div().text_color(theme.foreground).child(p.name.clone()))
                                .children(is_default.then(|| {
                                    div()
                                        .px(Tokens::space(1.5))
                                        .rounded(Tokens::RADIUS_SM)
                                        .bg(theme.accent)
                                        .text_size(Tokens::text_xs())
                                        .text_color(theme.accent_foreground)
                                        .child("Default")
                                }))
                                .children(p.is_local().then(|| {
                                    div()
                                        .text_size(Tokens::text_xs())
                                        .text_color(theme.success)
                                        .child("Local")
                                })),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(Tokens::text_xs())
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "{} \u{b7} {model} \u{b7} {}",
                                    p.kind.label(),
                                    p.base()
                                )),
                        ),
                )
                .children((!is_default).then(|| {
                    Button::new(SharedString::from(format!("ai-default-{slug}")))
                        .label("Make default")
                        .ghost()
                        .xsmall()
                        .on_click(move |_, _, cx| {
                            LlmProfiles::global(cx).update(cx, |s, cx| s.set_default(Some(id), cx));
                        })
                }))
                .child(
                    Button::new(SharedString::from(format!("ai-edit-{slug}")))
                        .icon(IconName::Settings2)
                        .ghost()
                        .xsmall()
                        .tip("Edit")
                        .on_click(move |_, window, cx| {
                            edit(id, window, cx);
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("ai-delete-{slug}")))
                        .icon(IconName::Delete)
                        .ghost()
                        .xsmall()
                        .tip("Delete")
                        .on_click(move |_, window, cx| {
                            confirm_delete(id, name.clone(), window, cx)
                        }),
                )
        });
        let empty = state.read(cx).profiles().is_empty();
        v_flex()
            .id("ai-profiles")
            .test_support()
            .w_full()
            .gap(Tokens::space(1.0))
            .children(rows)
            .children(empty.then(|| {
                div()
                    .py(Tokens::space(2.0))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child("No AI profiles yet. Add one for a cloud provider or a local model (Ollama, LM Studio).")
            }))
            .child(h_flex().pt(Tokens::space(2.0)).child(add))
    }

    fn connections(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let state = LlmProfiles::global(cx);
        let connections = ConnectionManager::try_global(cx)
            .map(|m| m.read(cx).connections().to_vec())
            .unwrap_or_default();
        let profiles: Vec<(Uuid, String)> = state
            .read(cx)
            .profiles()
            .iter()
            .map(|p| (p.id, p.name.clone()))
            .collect();
        let default_name = {
            let s = state.read(cx);
            s.default_id()
                .and_then(|id| s.get(id))
                .map(|p| p.name.clone())
        };
        let rows = connections.into_iter().map(|c| {
            let slug = slug(&c.name);
            let conn = c.id;
            let (chosen, share, choice) = {
                let s = state.read(cx);
                (
                    s.connection_override(conn),
                    s.share_samples(conn),
                    s.share_choice(conn),
                )
            };
            let label = match chosen.and_then(|id| profiles.iter().find(|(p, _)| *p == id)) {
                Some((_, name)) => name.clone(),
                None => match &default_name {
                    Some(d) => format!("Default ({d})"),
                    None => "Default".to_owned(),
                },
            };
            let options = profiles.clone();
            let picker = Button::new(SharedString::from(format!("ai-conn-profile-{slug}")))
                .label(label)
                .outline()
                .xsmall()
                .dropdown_menu(move |mut menu, _, _| {
                    menu = menu.item(
                        PopupMenuItem::new("Default")
                            .checked(chosen.is_none())
                            .on_click(move |_, _, cx| {
                                LlmProfiles::global(cx)
                                    .update(cx, |s, cx| s.set_connection_profile(conn, None, cx));
                            }),
                    );
                    for (id, name) in &options {
                        let id = *id;
                        menu = menu.item(
                            PopupMenuItem::new(name.clone())
                                .checked(chosen == Some(id))
                                .on_click(move |_, _, cx| {
                                    LlmProfiles::global(cx).update(cx, |s, cx| {
                                        s.set_connection_profile(conn, Some(id), cx)
                                    });
                                }),
                        );
                    }
                    menu
                });
            let hint = if choice.is_none() {
                " (default for this model)"
            } else {
                ""
            };
            h_flex()
                .id(SharedString::from(format!("ai-conn-{slug}")))
                .test_support()
                .gap(Tokens::space(3.0))
                .py(Tokens::space(1.5))
                .border_b(Tokens::BORDER)
                .border_color(theme.border)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(theme.foreground)
                        .child(c.name.clone()),
                )
                .child(picker)
                .child(
                    Switch::new(SharedString::from(format!("ai-share-{slug}")))
                        .label(format!("Share sample data{hint}"))
                        .checked(share)
                        .small()
                        .on_click(move |checked: &bool, _, cx| {
                            LlmProfiles::global(cx)
                                .update(cx, |s, cx| s.set_share_samples(conn, Some(*checked), cx));
                        }),
                )
        });
        v_flex()
            .id("ai-connections")
            .test_support()
            .w_full()
            .children(rows)
    }
}

impl Render for AiSettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        v_flex()
            .id("ai-settings")
            .test_support()
            .w_full()
            .gap(Tokens::space(4.0))
            .child(self.profiles(cx))
            .child(
                div()
                    .text_size(Tokens::text_sm())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .child("Connections"),
            )
            .child(
                div()
                    .text_size(Tokens::text_sm())
                    .text_color(cx.theme().muted_foreground)
                    .child("Choose the profile the assistant uses for each connection. Sample data (a few values per property) is shared only when switched on; by default only with local models."),
            )
            .child(self.connections(cx))
    }
}
