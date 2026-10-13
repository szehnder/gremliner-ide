//! First-run onboarding: a three-step tour card and the sample-graph guide.
//!
//! The tour is a card over the bottom-right of the window, not a dialog, so the app stays usable while
//! it shows. Finishing or skipping it is remembered in `ui_state`, so it appears only on first launch.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use providers::config::{ConnectionConfig, GremlinServerConfig, ProviderConfig};
use providers::store::PasswordChange;
use storage::{Storage, Table};

use crate::actions::*;
use crate::connections::ConnectionManager;
use crate::connections::manager::SaveError;
use crate::theme::Tokens;

/// `ui_state` key: `"done"` or `"skipped"` once the tour was dismissed.
pub const TOUR_KEY: &str = "onboarding.tour";
/// Name of the connection the sample guide adds.
pub const SAMPLE_CONNECTION: &str = "Sample graph";
/// The Docker command the sample guide suggests (TinkerPop's image with the "modern" graph as `g`).
pub const SAMPLE_DOCKER: &str =
    "docker run --rm -p 8182:8182 tinkerpop/gremlin-server:3.7.3 conf/gremlin-server-modern.yaml";

struct Step {
    title: &'static str,
    body: fn(&App) -> String,
}

const STEPS: [Step; 3] = [
    Step {
        title: "Connect to a graph",
        body: |_| {
            "Add a Gremlin Server or Amazon Neptune connection in the sidebar. No server yet? Try the sample graph.".into()
        },
    },
    Step {
        title: "Write and run Gremlin",
        body: |cx| {
            format!(
                "Open a query tab with {}, then run the query at the cursor with {}. Results show below as a graph, table, JSON or console output.",
                keys(&NewQueryTab, cx),
                keys(&editor::RunQuery, cx),
            )
        },
    },
    Step {
        title: "Everything is a keystroke away",
        body: |cx| {
            format!(
                "{} finds any command, {} in a query tab asks the assistant in plain English, and {} lists every shortcut.",
                keys(&OpenCommandPalette, cx),
                keys(&ui_assistant::OpenPromptBar, cx),
                keys(&ShowKeyboardShortcuts, cx),
            )
        },
    },
];

fn keys(action: &dyn Action, cx: &App) -> String {
    crate::shortcuts::keystrokes_for(action, cx)
        .map(|k| {
            k.iter()
                .map(component::kbd::Kbd::format)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// True when the tour should show: it was never finished or skipped.
pub fn should_show(storage: Option<&Storage>) -> bool {
    let Some(storage) = storage else {
        return true;
    };
    match storage.get_json_blocking::<String>(Table::UiState, TOUR_KEY) {
        Ok(state) => state.is_none(),
        Err(err) => {
            log::warn!("ignoring unreadable tour state: {err}");
            true
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourEvent {
    Dismissed,
}

pub struct Tour {
    step: usize,
    storage: Option<Storage>,
}

impl EventEmitter<TourEvent> for Tour {}

impl Tour {
    pub fn new(storage: Option<Storage>) -> Self {
        Self { step: 0, storage }
    }

    pub fn step(&self) -> usize {
        self.step
    }

    pub fn next(&mut self, cx: &mut Context<'_, Self>) {
        if self.step + 1 < STEPS.len() {
            self.step += 1;
            cx.notify();
        } else {
            self.dismiss("done", cx);
        }
    }

    pub fn skip(&mut self, cx: &mut Context<'_, Self>) {
        self.dismiss("skipped", cx);
    }

    fn dismiss(&mut self, state: &str, cx: &mut Context<'_, Self>) {
        if let Some(storage) = &self.storage {
            let write = storage.set_json(Table::UiState, TOUR_KEY, &state.to_owned());
            cx.background_spawn(async move {
                if let Err(err) = write.await {
                    log::warn!("failed to save the tour state: {err}");
                }
            })
            .detach();
        }
        cx.emit(TourEvent::Dismissed);
    }
}

impl Render for Tour {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let step = &STEPS[self.step];
        let last = self.step + 1 == STEPS.len();
        v_flex()
            .id("tour")
            .test_support()
            .w(Tokens::tour_width())
            .p(Tokens::space(4.0))
            .gap(Tokens::space(2.0))
            .rounded(Tokens::RADIUS_LG)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            // Clicks on the card must not reach the panels beneath it.
            .occlude()
            .child(
                div()
                    .text_size(Tokens::text_xs())
                    .text_color(theme.muted_foreground)
                    .child(format!("Step {} of {}", self.step + 1, STEPS.len())),
            )
            .child(
                div()
                    .id("tour-title")
                    .test_support()
                    .text_size(Tokens::text_lg())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(step.title),
            )
            .child(
                div()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child((step.body)(cx)),
            )
            .child(
                h_flex()
                    .pt(Tokens::space(1.0))
                    .gap(Tokens::space(1.0))
                    .child(
                        Button::new("tour-skip")
                            .label("Skip tour")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.skip(cx))),
                    )
                    .child(div().flex_1())
                    .when(self.step == 0, |row| {
                        row.child(
                            Button::new("tour-sample")
                                .label("Try a sample graph")
                                .outline()
                                .small()
                                .on_click(|_, window, cx| open_sample_guide(window, cx)),
                        )
                    })
                    .child(
                        Button::new("tour-next")
                            .label(if last { "Done" } else { "Next" })
                            .primary()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                    ),
            )
    }
}

/// How to start the sample graph locally, and a button that adds and opens its connection.
pub fn open_sample_guide(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |dialog, _, cx| {
        let theme = cx.theme();
        dialog
            .title("Try a sample graph")
            .w(Tokens::CONNECTION_DIALOG_WIDTH)
            .margin_top(Tokens::DIALOG_TOP)
            .child(
                v_flex()
                    .id("sample-guide")
                    .test_support()
                    .gap(Tokens::space(3.0))
                    .text_size(Tokens::text_sm())
                    .child(
                        "The sample is Apache TinkerPop's Gremlin Server with the small \"modern\" graph \
                         (people, software, knows and created). Start it with Docker:",
                    )
                    .child(
                        div()
                            .id("sample-command")
                            .test_support()
                            .p(Tokens::space(2.0))
                            .rounded(Tokens::RADIUS_MD)
                            .bg(theme.muted)
                            .font_family(theme.mono_font_family.clone())
                            .text_size(Tokens::text_xs())
                            .child(SAMPLE_DOCKER),
                    )
                    .child(
                        div().text_color(theme.muted_foreground).child(
                            "Working in the Gremliner repository? `cargo xtask testenv up && cargo xtask \
                             testenv seed` adds the air-routes, crew and synthetic graphs too.",
                        ),
                    )
                    .child(
                        h_flex()
                            .gap(Tokens::space(2.0))
                            .child(
                                Button::new("sample-copy")
                                    .label("Copy command")
                                    .icon(IconName::Copy)
                                    .outline()
                                    .small()
                                    .on_click(|_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            SAMPLE_DOCKER.into(),
                                        ));
                                    }),
                            )
                            .child(
                                Button::new("sample-connect")
                                    .label("Add and connect")
                                    .icon(IconName::Plus)
                                    .primary()
                                    .small()
                                    .on_click(|_, window, cx| {
                                        window.close_dialog(cx);
                                        add_sample_connection(window, cx);
                                    }),
                            ),
                    ),
            )
    });
}

/// Add the sample connection (`localhost:8182`) unless it exists, then connect to it.
pub fn add_sample_connection(window: &mut Window, cx: &mut App) {
    let Some(manager) = ConnectionManager::try_global(cx) else {
        return;
    };
    if let Some(id) = manager
        .read(cx)
        .find_by_name(SAMPLE_CONNECTION)
        .map(|c| c.id)
    {
        manager.update(cx, |m, cx| m.connect(id, cx)).detach();
        return;
    }
    let config = ConnectionConfig::new(
        SAMPLE_CONNECTION,
        ProviderConfig::GremlinServer(GremlinServerConfig::default()),
    );
    let save = manager.update(cx, |m, cx| m.save(config, PasswordChange::Keep, cx));
    window
        .spawn(cx, async move |cx| match save.await {
            Ok(saved) => {
                let _ = cx.update(|_, cx| {
                    manager
                        .update(cx, |m, cx| m.connect(saved.config.id, cx))
                        .detach();
                });
            }
            Err(SaveError::Name(msg) | SaveError::Other(msg)) => {
                let _ = cx.update(|window, cx| {
                    window.push_notification(
                        Notification::error(msg).title("Could not add the sample"),
                        cx,
                    );
                });
            }
        })
        .detach();
}
