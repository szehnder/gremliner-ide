//! The LLM profile dialog: name, endpoint, key, model (listed from the provider or typed), generation
//! settings, capability overrides, and a streaming "Test".

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use llm::{KeyChange, LlmError, Profile, TestReport};

use super::LlmProfiles;
use crate::connections::form::field;
use crate::io::spawn_io;
use crate::theme::Tokens;

/// Open the dialog for `profile` (a preset's new profile, or an existing one to edit).
pub fn open(
    profile: Profile,
    editing: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ProfileForm> {
    let title = if editing {
        "Edit AI profile"
    } else {
        "New AI profile"
    };
    let form = cx.new(|cx| ProfileForm::new(profile, editing, window, cx));
    let view = form.clone();
    let focus = form.read(cx).name.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(title)
            .w(Tokens::CONNECTION_DIALOG_WIDTH)
            .margin_top(Tokens::DIALOG_TOP)
            .child(view.clone())
    });
    window.defer(cx, move |window, cx| {
        focus.update(cx, |i, cx| i.focus(window, cx))
    });
    form
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum TestUi {
    #[default]
    Idle,
    Running(String),
    Done(String, String),
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Models {
    #[default]
    NotFetched,
    Fetching,
    Listed(Vec<String>),
    Failed(String),
}

pub struct ProfileForm {
    base: Profile,
    editing: bool,
    pub name: Entity<InputState>,
    pub base_url: Entity<InputState>,
    pub key: Entity<InputState>,
    pub model: Entity<InputState>,
    pub temperature: Entity<InputState>,
    pub max_tokens: Entity<InputState>,
    pub timeout: Entity<InputState>,
    pub overrides: llm::CapabilityOverrides,
    pub models: Models,
    pub test: TestUi,
    pub error: Option<String>,
    pub saving: bool,
}

fn input(
    window: &mut Window,
    cx: &mut Context<'_, ProfileForm>,
    placeholder: &str,
    value: &str,
) -> Entity<InputState> {
    let (placeholder, value) = (placeholder.to_owned(), value.to_owned());
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value)
    })
}

impl ProfileForm {
    pub fn new(
        profile: Profile,
        editing: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let key_hint = if profile.has_key {
            "Stored in the system keychain; type to replace"
        } else if profile.requires_key() {
            "API key"
        } else {
            "Not needed for local models"
        };
        let key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(key_hint)
        });
        Self {
            name: input(window, cx, "Name", &profile.name),
            base_url: input(window, cx, "Base URL", &profile.base_url),
            key,
            model: input(window, cx, "Model id", &profile.model),
            temperature: input(window, cx, "0.2", &profile.temperature.to_string()),
            max_tokens: input(window, cx, "2048", &profile.max_tokens.to_string()),
            timeout: input(window, cx, "60", &(profile.timeout_ms / 1000).to_string()),
            overrides: profile.overrides,
            base: profile,
            editing,
            models: Models::default(),
            test: TestUi::default(),
            error: None,
            saving: false,
        }
    }

    /// The profile as currently edited.
    pub fn profile(&self, cx: &App) -> Profile {
        let mut p = self.base.clone();
        p.name = self.name.read(cx).value().trim().to_owned();
        p.base_url = self
            .base_url
            .read(cx)
            .value()
            .trim()
            .trim_end_matches('/')
            .to_owned();
        p.model = self.model.read(cx).value().trim().to_owned();
        if let Ok(t) = self.temperature.read(cx).value().trim().parse::<f32>() {
            p.temperature = t.clamp(0.0, 2.0);
        }
        if let Ok(n) = self.max_tokens.read(cx).value().trim().parse::<u32>() {
            p.max_tokens = n.max(16);
        }
        if let Ok(s) = self.timeout.read(cx).value().trim().parse::<u64>() {
            p.timeout_ms = s.max(1) * 1000;
        }
        p.overrides = self.overrides;
        p
    }

    fn typed_key(&self, cx: &App) -> Option<String> {
        Some(self.key.read(cx).value().trim().to_owned()).filter(|k| !k.is_empty())
    }

    pub fn set_model(&mut self, model: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        let model = model.to_owned();
        self.model
            .update(cx, |i, cx| i.set_value(model, window, cx));
        cx.notify();
    }

    pub fn fetch_models(&mut self, cx: &mut Context<'_, Self>) {
        let profile = self.profile(cx);
        let key = self.typed_key(cx);
        self.models = Models::Fetching;
        cx.notify();
        let client = LlmProfiles::global(cx).read(cx).client(profile, key, cx);
        cx.spawn(async move |this, cx| {
            let result = match client.await {
                Ok(client) => {
                    let work =
                        cx.update(|cx| spawn_io(cx, async move { client.list_models().await }));
                    work.await
                        .unwrap_or_else(|e| Err(LlmError::Network(e.to_string())))
                }
                Err(e) => Err(e),
            };
            let _ = this.update(cx, |this, cx| {
                this.models = match result {
                    Ok(list) if list.is_empty() => {
                        Models::Failed("The provider listed no models; type one.".into())
                    }
                    Ok(list) => Models::Listed(list),
                    Err(e) => Models::Failed(storage::secrets::redact(&e.to_string()).into_owned()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Stream a short reply with the settings as edited (nothing is saved).
    pub fn run_test(&mut self, cx: &mut Context<'_, Self>) {
        let profile = self.profile(cx);
        let key = self.typed_key(cx);
        self.test = TestUi::Running(String::new());
        cx.notify();
        let client = LlmProfiles::global(cx).read(cx).client(profile, key, cx);
        let (tx, mut rx) = mpsc::unbounded::<String>();
        cx.spawn(async move |this, cx| {
            let client = match client.await {
                Ok(c) => c,
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.test = TestUi::Failed(e.to_string());
                        cx.notify();
                    });
                    return;
                }
            };
            let work = cx.update(|cx| {
                spawn_io(cx, async move {
                    client
                        .test(move |t| {
                            let _ = tx.unbounded_send(t.to_owned());
                        })
                        .await
                })
            });
            while let Some(text) = rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    if let TestUi::Running(so_far) = &mut this.test {
                        so_far.push_str(&text);
                        cx.notify();
                    }
                });
            }
            let result: Result<TestReport, LlmError> = work
                .await
                .unwrap_or_else(|e| Err(LlmError::Network(e.to_string())));
            let _ = this.update(cx, |this, cx| {
                this.test = match result {
                    Ok(report) => TestUi::Done(report.text.clone(), report.summary()),
                    Err(e) => TestUi::Failed(storage::secrets::redact(&e.to_string()).into_owned()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let profile = self.profile(cx);
        if profile.name.is_empty() {
            self.error = Some("Give the profile a name.".into());
            cx.notify();
            return;
        }
        if profile.model.is_empty() {
            self.error = Some("Choose a model from the list or type its id.".into());
            cx.notify();
            return;
        }
        let key = match self.typed_key(cx) {
            Some(k) => KeyChange::Set(k),
            None => KeyChange::Keep,
        };
        self.saving = true;
        self.error = None;
        cx.notify();
        let task = LlmProfiles::global(cx).update(cx, |p, cx| p.save(profile, key, cx));
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(_) => window.close_dialog(cx),
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ProfileForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let edited = self.profile(cx);
        let caps = edited.capabilities();
        let kind = edited.kind.label();
        let models = match &self.models {
            Models::NotFetched => None,
            Models::Fetching => Some(
                h_flex()
                    .gap(Tokens::space(2.0))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child("Fetching models…")
                    .into_any_element(),
            ),
            Models::Failed(e) => Some(
                div()
                    .id("ai-models-error")
                    .test_support()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.danger)
                    .child(format!("{e} You can still type a model id."))
                    .into_any_element(),
            ),
            Models::Listed(list) => Some(
                h_flex()
                    .id("ai-models")
                    .test_support()
                    .flex_wrap()
                    .gap(Tokens::space(1.0))
                    .max_h(Tokens::size(120.0))
                    .overflow_y_scroll()
                    .children(list.iter().enumerate().map(|(i, m)| {
                        let m = m.clone();
                        let selected = edited.model == m;
                        Button::new(SharedString::from(format!("ai-model-{i}")))
                            .label(m.clone())
                            .xsmall()
                            .when(selected, |b| b.primary())
                            .when(!selected, |b| b.outline())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_model(&m, window, cx)
                            }))
                    }))
                    .into_any_element(),
            ),
        };
        let switch = |id: &'static str,
                      label: &'static str,
                      on: bool,
                      set: fn(&mut llm::CapabilityOverrides, bool),
                      cx: &mut Context<'_, Self>| {
            Switch::new(id)
                .label(label)
                .checked(on)
                .small()
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    set(&mut this.overrides, *checked);
                    cx.notify();
                }))
        };
        let test = match &self.test {
            TestUi::Idle => None,
            TestUi::Running(text) => Some((text.clone(), None, false, true)),
            TestUi::Done(text, summary) => {
                Some((text.clone(), Some(summary.clone()), false, false))
            }
            TestUi::Failed(msg) => Some((msg.clone(), None, true, false)),
        };
        v_flex()
            .id("ai-profile-form")
            .test_support()
            .gap(Tokens::space(4.0))
            .child(field("Name", Input::new(&self.name), None, cx))
            .child(field(
                "Provider",
                div().text_color(theme.muted_foreground).child(kind),
                None,
                cx,
            ))
            .child(field("Base URL", Input::new(&self.base_url), None, cx))
            .child(field("API key", Input::new(&self.key), None, cx))
            .child(
                field(
                    "Model",
                    h_flex()
                        .gap(Tokens::space(2.0))
                        .child(div().flex_1().child(Input::new(&self.model)))
                        .child(
                            Button::new("ai-fetch-models")
                                .label("List models")
                                .outline()
                                .small()
                                .disabled(matches!(self.models, Models::Fetching))
                                .on_click(cx.listener(|this, _, _, cx| this.fetch_models(cx))),
                        ),
                    None,
                    cx,
                )
                .children(models),
            )
            .child(
                h_flex()
                    .gap(Tokens::space(3.0))
                    .child(field("Temperature", Input::new(&self.temperature), None, cx).flex_1())
                    .child(
                        field("Max output tokens", Input::new(&self.max_tokens), None, cx).flex_1(),
                    )
                    .child(field("Timeout (s)", Input::new(&self.timeout), None, cx).flex_1()),
            )
            .child(field(
                "Capabilities",
                h_flex()
                    .gap(Tokens::space(4.0))
                    .child(switch(
                        "ai-cap-streaming",
                        "Streaming",
                        caps.streaming,
                        |o, v| o.streaming = Some(v),
                        cx,
                    ))
                    .child(switch(
                        "ai-cap-tools",
                        "Tool calling",
                        caps.tools,
                        |o, v| o.tools = Some(v),
                        cx,
                    ))
                    .child(switch(
                        "ai-cap-json",
                        "JSON schema output",
                        caps.json_schema,
                        |o, v| o.json_schema = Some(v),
                        cx,
                    )),
                None,
                cx,
            ))
            .children(test.map(|(text, summary, failed, running)| {
                v_flex()
                    .id("ai-test-result")
                    .test_support()
                    .gap(Tokens::space(1.0))
                    .p(Tokens::space(2.0))
                    .rounded(Tokens::RADIUS_MD)
                    .bg(theme.muted)
                    .text_size(Tokens::text_sm())
                    .child(
                        div()
                            .id("ai-test-text")
                            .test_support()
                            .text_color(if failed {
                                theme.danger
                            } else {
                                theme.foreground
                            })
                            .child(if text.is_empty() && running {
                                "Waiting for the first token…".to_owned()
                            } else {
                                text
                            }),
                    )
                    .children(summary.map(|s| {
                        div()
                            .id("ai-test-summary")
                            .test_support()
                            .text_color(theme.muted_foreground)
                            .child(s)
                    }))
            }))
            .children(self.error.clone().map(|e| {
                div()
                    .id("ai-form-error")
                    .test_support()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.danger)
                    .child(e)
            }))
            .child(
                h_flex()
                    .pt(Tokens::space(2.0))
                    .justify_between()
                    .child(
                        Button::new("ai-test")
                            .label("Test")
                            .outline()
                            .disabled(matches!(self.test, TestUi::Running(_)))
                            .on_click(cx.listener(|this, _, _, cx| this.run_test(cx))),
                    )
                    .child(
                        h_flex()
                            .gap(Tokens::space(2.0))
                            .child(
                                Button::new("ai-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(
                                Button::new("ai-save")
                                    .label(if self.editing { "Save" } else { "Save profile" })
                                    .primary()
                                    .loading(self.saving)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.save(window, cx)),
                                    ),
                            ),
                    ),
            )
    }
}
