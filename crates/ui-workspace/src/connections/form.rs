//! The connection dialog: create or edit a connection, test it, save it.
//!
//! Pasting a URL into the host field fills in the other fields (`providers::endpoint`). Saving validates
//! inline (name, host, port) and never closes the dialog on an error.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use providers::endpoint;
use providers::{
    ColorTag, ConnectionConfig, Environment, GremlinServerConfig, NeptuneConfig, PasswordChange,
    ProviderConfig, ProviderError, ProviderKind, ServerInfo,
};
use uuid::Uuid;

use super::manager::{ConnectionManager, SaveError};
use super::style::{color_tag_color, environment_badge};
use crate::theme::Tokens;

/// Open the dialog for a new connection (`None`) or to edit `existing`.
pub fn open(
    existing: Option<ConnectionConfig>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ConnectionForm> {
    let title = if existing.is_some() {
        "Edit connection"
    } else {
        "New connection"
    };
    let form = cx.new(|cx| ConnectionForm::new(existing, window, cx));
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
        focus.update(cx, |input, cx| input.focus(window, cx))
    });
    form
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldErrors {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<String>,
}

impl FieldErrors {
    fn is_empty(&self) -> bool {
        self.name.is_none() && self.host.is_none() && self.port.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub enum TestState {
    #[default]
    Idle,
    Running,
    Passed(ServerInfo),
    Failed(ProviderError),
}

pub struct ConnectionForm {
    id: Uuid,
    editing: bool,
    pub kind: ProviderKind,
    pub name: Entity<InputState>,
    pub host: Entity<InputState>,
    pub port: Entity<InputState>,
    pub path: Entity<InputState>,
    pub username: Entity<InputState>,
    pub password: Entity<InputState>,
    pub alias: Entity<InputState>,
    pub region: Entity<InputState>,
    pub tls: bool,
    pub accept_invalid_certs: bool,
    pub iam_auth: bool,
    pub read_only: bool,
    pub environment: Environment,
    pub color: ColorTag,
    timeout_ms: u64,
    has_stored_password: bool,
    password_edited: bool,
    pub errors: FieldErrors,
    pub test: TestState,
    pub saving: bool,
    last_host: String,
    _subscriptions: Vec<Subscription>,
}

fn input(
    window: &mut Window,
    cx: &mut Context<'_, ConnectionForm>,
    placeholder: &str,
    value: &str,
) -> Entity<InputState> {
    let placeholder = placeholder.to_owned();
    let value = value.to_owned();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value)
    })
}

impl ConnectionForm {
    pub fn new(
        existing: Option<ConnectionConfig>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let editing = existing.is_some();
        let config = existing.unwrap_or_else(|| {
            ConnectionConfig::new(
                "",
                ProviderConfig::GremlinServer(GremlinServerConfig::default()),
            )
        });
        let (server, neptune) = match &config.provider {
            ProviderConfig::GremlinServer(s) => (s.clone(), NeptuneConfig::default()),
            ProviderConfig::NeptuneAws(n) => (
                GremlinServerConfig {
                    host: n.host.clone(),
                    port: n.port,
                    ..Default::default()
                },
                n.clone(),
            ),
        };
        let name = input(window, cx, "e.g. Local air-routes", &config.name);
        let host = input(window, cx, "localhost, or paste a URL", &server.host);
        let port = input(window, cx, "8182", &server.port.to_string());
        let path = input(window, cx, "/gremlin", &server.path);
        let username = input(
            window,
            cx,
            "Optional",
            server.username.as_deref().unwrap_or(""),
        );
        let password_hint = if server.has_password {
            "Saved in the system keychain"
        } else {
            "Optional"
        };
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(password_hint)
        });
        let alias = input(window, cx, "g", &server.alias);
        let region = input(window, cx, "us-east-1", &neptune.region);

        let subscriptions = vec![
            cx.subscribe_in(
                &host,
                window,
                |this, state, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = state.read(cx).value().to_string();
                        this.on_host_changed(value, window, cx);
                    }
                },
            ),
            cx.subscribe(&password, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.password_edited = true;
                    cx.notify();
                }
            }),
            cx.subscribe(&name, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && this.errors.name.take().is_some() {
                    cx.notify();
                }
            }),
        ];

        Self {
            id: config.id,
            editing,
            kind: config.provider.kind(),
            last_host: server.host.clone(),
            name,
            host,
            port,
            path,
            username,
            password,
            alias,
            region,
            tls: server.tls,
            accept_invalid_certs: server.accept_invalid_certs,
            iam_auth: neptune.iam_auth,
            read_only: config.read_only,
            environment: config.environment,
            color: config.color,
            timeout_ms: config.timeout_ms,
            has_stored_password: server.has_password,
            password_edited: false,
            errors: FieldErrors::default(),
            test: TestState::Idle,
            saving: false,
            _subscriptions: subscriptions,
        }
    }

    /// A multi-character insertion into the host field is treated as a paste and parsed.
    fn on_host_changed(&mut self, value: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        let previous = std::mem::replace(&mut self.last_host, value.clone());
        self.errors.host = None;
        let pasted = value.len() >= previous.len() + 2;
        if !pasted {
            cx.notify();
            return;
        }
        let Some(parsed) = endpoint::parse(&value) else {
            cx.notify();
            return;
        };
        let structured = parsed.tls.is_some() || parsed.port.is_some() || parsed.is_neptune();
        if !structured {
            cx.notify();
            return;
        }
        self.apply_endpoint(parsed, window, cx);
    }

    fn apply_endpoint(
        &mut self,
        parsed: endpoint::ParsedEndpoint,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let set = |state: &Entity<InputState>,
                   value: String,
                   window: &mut Window,
                   cx: &mut Context<'_, Self>| {
            state.update(cx, |s, cx| s.set_value(value, window, cx));
        };
        self.last_host = parsed.host.clone();
        set(&self.host, parsed.host.clone(), window, cx);
        if let Some(port) = parsed.port {
            set(&self.port, port.to_string(), window, cx);
            self.errors.port = None;
        }
        if let Some(path) = parsed.path.clone() {
            set(&self.path, path, window, cx);
        }
        if let Some(tls) = parsed.tls {
            self.tls = tls;
        }
        if let Some(region) = parsed.neptune_region {
            self.kind = ProviderKind::NeptuneAws;
            set(&self.region, region, window, cx);
        } else {
            self.kind = ProviderKind::GremlinServer;
        }
        cx.notify();
    }

    fn text(state: &Entity<InputState>, cx: &App) -> String {
        state.read(cx).value().trim().to_owned()
    }

    fn password_change(&self, cx: &App) -> PasswordChange {
        let typed = Self::text(&self.password, cx);
        if self.password_edited || !self.has_stored_password {
            if typed.is_empty() {
                PasswordChange::Clear
            } else {
                PasswordChange::Set(typed)
            }
        } else {
            PasswordChange::Keep
        }
    }

    /// Build the config from the fields, or the field errors.
    pub fn to_config(&self, cx: &App) -> Result<(ConnectionConfig, PasswordChange), FieldErrors> {
        self.build(cx, true)
    }

    /// `check_name` is false for testing, which needs no (unique) name.
    fn build(
        &self,
        cx: &App,
        check_name: bool,
    ) -> Result<(ConnectionConfig, PasswordChange), FieldErrors> {
        let mut errors = FieldErrors::default();
        let name = Self::text(&self.name, cx);
        if check_name
            && let Err(err) = ConnectionManager::global(cx)
                .read(cx)
                .validate_name(&name, Some(self.id))
        {
            errors.name = Some(err);
        }
        let host = Self::text(&self.host, cx);
        if host.is_empty() {
            errors.host = Some("Enter a host name or address.".into());
        }
        let port = match Self::text(&self.port, cx).parse::<u16>() {
            Ok(p) if p > 0 => p,
            _ => {
                errors.port = Some("Enter a port from 1 to 65535.".into());
                0
            }
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        let username = Self::text(&self.username, cx);
        let password = self.password_change(cx);
        let provider = match self.kind {
            ProviderKind::GremlinServer => ProviderConfig::GremlinServer(GremlinServerConfig {
                host,
                port,
                path: match Self::text(&self.path, cx) {
                    p if p.is_empty() => "/gremlin".into(),
                    p => p,
                },
                tls: self.tls,
                accept_invalid_certs: self.accept_invalid_certs,
                username: (!username.is_empty()).then_some(username),
                has_password: self.has_stored_password,
                alias: match Self::text(&self.alias, cx) {
                    a if a.is_empty() => "g".into(),
                    a => a,
                },
            }),
            ProviderKind::NeptuneAws => ProviderConfig::NeptuneAws(NeptuneConfig {
                host,
                port,
                region: Self::text(&self.region, cx),
                iam_auth: self.iam_auth,
            }),
        };
        let config = ConnectionConfig {
            id: self.id,
            name,
            color: self.color,
            environment: self.environment,
            read_only: self.read_only,
            timeout_ms: self.timeout_ms,
            provider,
        };
        Ok((config, password))
    }

    pub fn run_test(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let (config, password) = match self.build(cx, false) {
            Ok(v) => v,
            Err(errors) => {
                self.errors = errors;
                cx.notify();
                return;
            }
        };
        self.test = TestState::Running;
        cx.notify();
        let manager = ConnectionManager::global(cx);
        let task = manager.update(cx, |m, cx| m.test(config, password, cx));
        cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                this.test = match outcome {
                    Ok(report) => match report.result {
                        Ok(info) => TestState::Passed(info),
                        Err(err) => TestState::Failed(err),
                    },
                    Err(err) => TestState::Failed(ProviderError::unsupported(err)),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let (config, password) = match self.to_config(cx) {
            Ok(v) => v,
            Err(errors) => {
                self.errors = errors;
                cx.notify();
                return;
            }
        };
        self.saving = true;
        cx.notify();
        let manager = ConnectionManager::global(cx);
        let task = manager.update(cx, |m, cx| m.save(config, password, cx));
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(saved) => {
                        window.close_dialog(cx);
                        if !saved.password_persisted {
                            window.push_notification(
                                Notification::warning(
                                    "No system keychain was found, so the password is kept only until you quit.",
                                )
                                .title("Password not saved"),
                                cx,
                            );
                        }
                    }
                    Err(SaveError::Name(msg)) => this.errors.name = Some(msg),
                    Err(SaveError::Other(msg)) => {
                        window.push_notification(Notification::error(msg).title("Could not save"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }
}

pub(crate) fn field(
    label: &'static str,
    control: impl IntoElement,
    error: Option<&String>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    v_flex()
        .gap(Tokens::space(1.0))
        .child(
            div()
                .text_size(Tokens::text_sm())
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(label),
        )
        .child(control)
        .children(error.map(|e| {
            div()
                .text_size(Tokens::text_sm())
                .text_color(theme.danger)
                .child(e.clone())
        }))
}

fn segmented<T: Copy + PartialEq + 'static>(
    prefix: &'static str,
    options: &[(T, &'static str, &'static str)],
    selected: T,
    cx: &mut Context<'_, ConnectionForm>,
    on_select: fn(&mut ConnectionForm, T),
) -> Div {
    h_flex()
        .gap(Tokens::space(1.0))
        .children(options.iter().map(|&(value, id, label)| {
            Button::new(SharedString::from(format!("{prefix}-{id}")))
                .label(label)
                .small()
                .when(value == selected, |b| b.primary())
                .when(value != selected, |b| b.outline())
                .on_click(cx.listener(move |this, _, _, cx| {
                    on_select(this, value);
                    cx.notify();
                }))
        }))
}

impl Render for ConnectionForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let gremlin = self.kind == ProviderKind::GremlinServer;

        let provider = segmented(
            "conn-provider",
            &[
                (ProviderKind::GremlinServer, "gremlin", "Gremlin Server"),
                (ProviderKind::NeptuneAws, "neptune", "Amazon Neptune"),
            ],
            self.kind,
            cx,
            |this, kind| this.kind = kind,
        );
        let environment = segmented(
            "conn-env",
            &[
                (Environment::Local, "local", "Local"),
                (Environment::Dev, "dev", "Dev"),
                (Environment::Staging, "staging", "Staging"),
                (Environment::Production, "production", "Production"),
            ],
            self.environment,
            cx,
            |this, env| this.environment = env,
        );
        let colors = h_flex()
            .gap(Tokens::space(1.5))
            .children(ColorTag::ALL.iter().map(|&tag| {
                let selected = tag == self.color;
                let swatch = color_tag_color(tag, cx).unwrap_or(theme.muted);
                div()
                    .id(SharedString::from(format!("conn-color-{}", tag.id())))
                    .size(Tokens::size(18.0))
                    .rounded_full()
                    .bg(swatch)
                    .border_2()
                    .border_color(if selected {
                        theme.ring
                    } else {
                        theme.background
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.color = tag;
                        cx.notify();
                    }))
            }));

        let switch = |id: &'static str,
                      label: &'static str,
                      on: bool,
                      set: fn(&mut ConnectionForm, bool),
                      cx: &mut Context<'_, Self>| {
            Switch::new(id)
                .label(label)
                .checked(on)
                .small()
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    set(this, *checked);
                    cx.notify();
                }))
        };

        let endpoint_row = h_flex()
            .gap(Tokens::space(3.0))
            .items_start()
            .child(
                field(
                    "Host",
                    Input::new(&self.host),
                    self.errors.host.as_ref(),
                    cx,
                )
                .flex_1(),
            )
            .child(
                field(
                    "Port",
                    Input::new(&self.port),
                    self.errors.port.as_ref(),
                    cx,
                )
                .w(Tokens::size(96.0)),
            );

        let provider_section = if gremlin {
            v_flex()
                .gap(Tokens::space(3.0))
                .child(endpoint_row)
                .child(
                    h_flex()
                        .gap(Tokens::space(3.0))
                        .child(field("Path", Input::new(&self.path), None, cx).flex_1())
                        .child(
                            field("Traversal source", Input::new(&self.alias), None, cx).flex_1(),
                        ),
                )
                .child(
                    h_flex()
                        .gap(Tokens::space(3.0))
                        .child(field("Username", Input::new(&self.username), None, cx).flex_1())
                        .child(field("Password", Input::new(&self.password), None, cx).flex_1()),
                )
                .child(
                    h_flex()
                        .gap(Tokens::space(6.0))
                        .child(switch(
                            "conn-tls",
                            "Use TLS (wss://)",
                            self.tls,
                            |t, v| t.tls = v,
                            cx,
                        ))
                        .child(switch(
                            "conn-insecure",
                            "Accept invalid certificates",
                            self.accept_invalid_certs,
                            |t, v| t.accept_invalid_certs = v,
                            cx,
                        )),
                )
        } else {
            v_flex()
                .gap(Tokens::space(3.0))
                .child(endpoint_row)
                .child(field("Region", Input::new(&self.region), None, cx))
                .child(switch("conn-iam", "IAM database authentication", self.iam_auth, |t, v| t.iam_auth = v, cx))
                .child(
                    div()
                        .text_size(Tokens::text_sm())
                        .text_color(theme.muted_foreground)
                        .child("Neptune settings can be saved now; connecting to Neptune arrives in a later version."),
                )
        };

        let test_result = match &self.test {
            TestState::Idle => None,
            TestState::Running => Some(
                h_flex()
                    .id("conn-test-result")
                    .test_support()
                    .gap(Tokens::space(2.0))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child("Testing connection…"),
            ),
            TestState::Passed(info) => {
                let version = info
                    .tinkerpop_version
                    .as_ref()
                    .map(|v| format!(" · TinkerPop {v}"))
                    .unwrap_or_default();
                Some(
                    h_flex()
                        .id("conn-test-result")
                        .test_support()
                        .gap(Tokens::space(2.0))
                        .text_size(Tokens::text_sm())
                        .text_color(theme.success)
                        .child(format!(
                            "Connected in {} ms{version}",
                            info.latency.as_millis()
                        )),
                )
            }
            TestState::Failed(err) => Some(
                h_flex()
                    .id("conn-test-result")
                    .test_support()
                    .items_start()
                    .child(
                        v_flex()
                            .gap(Tokens::space(0.5))
                            .text_size(Tokens::text_sm())
                            .child(div().text_color(theme.danger).child(err.reason.clone()))
                            .children((!err.suggestion.is_empty()).then(|| {
                                div()
                                    .text_color(theme.muted_foreground)
                                    .child(err.suggestion.clone())
                            })),
                    ),
            ),
        };

        let badge_preview = environment_badge(self.environment, cx);

        v_flex()
            .id("connection-form")
            .test_support()
            .gap(Tokens::space(4.0))
            .child(field(
                "Name",
                Input::new(&self.name),
                self.errors.name.as_ref(),
                cx,
            ))
            .child(field("Provider", provider, None, cx))
            .child(provider_section)
            .child(
                h_flex()
                    .gap(Tokens::space(6.0))
                    .items_start()
                    .child(field(
                        "Environment",
                        h_flex()
                            .gap(Tokens::space(2.0))
                            .child(environment)
                            .children(badge_preview),
                        None,
                        cx,
                    ))
                    .child(field("Colour", colors, None, cx)),
            )
            .child(switch(
                "conn-read-only",
                "Read-only: block queries that change the graph",
                self.read_only,
                |t, v| t.read_only = v,
                cx,
            ))
            .children(test_result)
            .child(
                h_flex()
                    .pt(Tokens::space(2.0))
                    .justify_between()
                    .child(
                        Button::new("conn-test")
                            .label("Test connection")
                            .outline()
                            .disabled(matches!(self.test, TestState::Running))
                            .on_click(cx.listener(|this, _, window, cx| this.run_test(window, cx))),
                    )
                    .child(
                        h_flex()
                            .gap(Tokens::space(2.0))
                            .child(
                                Button::new("conn-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(
                                Button::new("conn-save")
                                    .label(if self.editing {
                                        "Save"
                                    } else {
                                        "Save connection"
                                    })
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
