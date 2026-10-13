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
    ColorTag, ConnectionConfig, Environment, GremlinServerConfig, LocalStackConfig, NeptuneConfig,
    PasswordChange, ProviderConfig, ProviderError, ProviderKind, ServerInfo,
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
        let submit = view.clone();
        dialog
            .title(title)
            .w(Tokens::CONNECTION_DIALOG_WIDTH)
            .margin_top(Tokens::DIALOG_TOP)
            // Enter anywhere in the form saves it; the dialog closes only once the save succeeds.
            .on_ok(move |_, window, cx| {
                submit.update(cx, |form, cx| form.save(window, cx));
                false
            })
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

/// Finding LocalStack clusters.
#[derive(Debug, Clone, Default)]
pub enum DiscoveryState {
    #[default]
    Idle,
    Running,
    Done,
    Failed(ProviderError),
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
    /// Neptune: connect through this local tunnel (`host:port`).
    pub tunnel: Entity<InputState>,
    /// Neptune: AWS profile; `None` is the default credential chain.
    pub aws_profile: Option<String>,
    /// Profiles found in the user's AWS files.
    pub aws_profiles: Vec<providers::neptune::AwsProfile>,
    /// The region last filled in from the host name (replaced as the host changes).
    inferred_region: Option<String>,
    /// LocalStack: the edge URL used for discovery.
    pub localstack_url: Entity<InputState>,
    /// LocalStack: the picked cluster (`None`: host and port entered by hand).
    pub ls_cluster: Option<String>,
    ls_engine: Option<String>,
    pub ls_clusters: Vec<providers::neptune::localstack::Cluster>,
    pub ls_discovery: DiscoveryState,
    ls_access_key: String,
    ls_secret_key: String,
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
        let localstack = match &config.provider {
            ProviderConfig::NeptuneLocalStack(l) => l.clone(),
            _ => LocalStackConfig::default(),
        };
        let (server, neptune) = match &config.provider {
            ProviderConfig::GremlinServer(s) => (s.clone(), NeptuneConfig::default()),
            ProviderConfig::NeptuneLocalStack(l) => (
                GremlinServerConfig {
                    host: l.host.clone(),
                    port: l.port,
                    tls: l.tls,
                    ..Default::default()
                },
                NeptuneConfig {
                    region: l.region.clone(),
                    iam_auth: l.iam_auth,
                    ..Default::default()
                },
            ),
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
        let tunnel = input(
            window,
            cx,
            "Optional, e.g. localhost:8182",
            neptune.tunnel.as_deref().unwrap_or(""),
        );
        let localstack_url = input(
            window,
            cx,
            providers::config::LOCALSTACK_URL,
            &localstack.localstack_url,
        );
        let profiles = ConnectionManager::global(cx).read(cx).aws_profiles(cx);
        cx.spawn(async move |this, cx| {
            let profiles = profiles.await;
            let _ = this.update(cx, |this, cx| {
                this.aws_profiles = profiles;
                cx.notify();
            });
        })
        .detach();

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
            tunnel,
            aws_profile: neptune.profile.clone(),
            aws_profiles: Vec::new(),
            inferred_region: None,
            localstack_url,
            ls_cluster: localstack.cluster_id.clone(),
            ls_engine: localstack.engine_version.clone(),
            ls_clusters: Vec::new(),
            ls_discovery: DiscoveryState::Idle,
            ls_access_key: localstack.access_key.clone(),
            ls_secret_key: localstack.secret_key.clone(),
            tls: server.tls,
            accept_invalid_certs: server.accept_invalid_certs || neptune.accept_invalid_certs,
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
        self.infer_region(&value, window, cx);
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

    /// Switch provider. LocalStack starts from its usual emulated endpoint when the fields hold defaults.
    pub fn select_kind(
        &mut self,
        kind: ProviderKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if kind == ProviderKind::NeptuneLocalStack && self.kind != kind {
            let host = Self::text(&self.host, cx);
            if host.is_empty() || host == "localhost" {
                let d = LocalStackConfig::default();
                self.last_host = d.host.clone();
                self.host
                    .update(cx, |s, cx| s.set_value(d.host.clone(), window, cx));
                self.port
                    .update(cx, |s, cx| s.set_value(d.port.to_string(), window, cx));
                self.tls = false;
            }
            if Self::text(&self.region, cx).is_empty() {
                self.region
                    .update(cx, |s, cx| s.set_value("us-east-1", window, cx));
            }
        }
        self.kind = kind;
        cx.notify();
    }

    fn localstack_config(&self, cx: &App) -> LocalStackConfig {
        LocalStackConfig {
            localstack_url: match Self::text(&self.localstack_url, cx) {
                u if u.is_empty() => providers::config::LOCALSTACK_URL.into(),
                u => u,
            },
            region: match Self::text(&self.region, cx) {
                r if r.is_empty() => "us-east-1".into(),
                r => r,
            },
            cluster_id: self.ls_cluster.clone(),
            host: Self::text(&self.host, cx),
            port: Self::text(&self.port, cx).parse().unwrap_or(4510),
            engine_version: self.ls_engine.clone(),
            iam_auth: self.iam_auth,
            access_key: self.ls_access_key.clone(),
            secret_key: self.ls_secret_key.clone(),
            tls: self.tls,
        }
    }

    /// List LocalStack's Neptune clusters for the picker.
    pub fn discover_clusters(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let config = self.localstack_config(cx);
        self.ls_discovery = DiscoveryState::Running;
        cx.notify();
        let task = crate::io::spawn_io(cx, async move {
            providers::neptune::localstack::discover(&config).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(clusters)) => {
                        this.ls_clusters = clusters;
                        this.ls_discovery = DiscoveryState::Done;
                    }
                    Ok(Err(err)) => {
                        this.ls_clusters.clear();
                        this.ls_discovery = DiscoveryState::Failed(err.into());
                    }
                    Err(_) => this.ls_discovery = DiscoveryState::Idle,
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Pick a discovered cluster (fills host and port) or `None` for manual entry.
    pub fn pick_cluster(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(cluster) = id
            .as_ref()
            .and_then(|id| self.ls_clusters.iter().find(|c| &c.id == id))
            .cloned()
        {
            self.last_host = cluster.endpoint.clone();
            self.host.update(cx, |s, cx| {
                s.set_value(cluster.endpoint.clone(), window, cx)
            });
            self.port.update(cx, |s, cx| {
                s.set_value(cluster.port.to_string(), window, cx)
            });
            self.ls_engine = Some(cluster.engine_version.clone()).filter(|v| !v.is_empty());
            self.iam_auth = cluster.iam_auth;
        }
        self.ls_cluster = id;
        cx.notify();
    }

    /// Fill the region from a Neptune host name, unless the user typed a region of their own.
    fn infer_region(&mut self, host: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(region) = endpoint::neptune_region(host) else {
            return;
        };
        let current = Self::text(&self.region, cx);
        let ours = current.is_empty() || self.inferred_region.as_deref() == Some(current.as_str());
        if ours && current != region {
            self.region
                .update(cx, |s, cx| s.set_value(region.clone(), window, cx));
        }
        if ours {
            self.inferred_region = Some(region);
        }
        if self.kind == ProviderKind::GremlinServer {
            self.kind = ProviderKind::NeptuneAws;
        }
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
            set(&self.region, region.clone(), window, cx);
            self.inferred_region = Some(region);
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
                profile: self.aws_profile.clone(),
                tunnel: Some(Self::text(&self.tunnel, cx)).filter(|t| !t.is_empty()),
                accept_invalid_certs: self.accept_invalid_certs,
                tls: true,
            }),
            ProviderKind::NeptuneLocalStack => {
                ProviderConfig::NeptuneLocalStack(LocalStackConfig {
                    host,
                    port,
                    ..self.localstack_config(cx)
                })
            }
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
        // Enter on the focused Save button reaches both the button and the dialog's confirm.
        if self.saving {
            return;
        }
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

impl ConnectionForm {
    fn localstack_section(&self, endpoint_row: Div, cx: &mut Context<'_, Self>) -> Div {
        let theme = cx.theme().clone();
        let chip = |id: String,
                    label: String,
                    value: Option<String>,
                    selected: bool,
                    cx: &mut Context<'_, Self>| {
            Button::new(SharedString::from(id))
                .label(label)
                .small()
                .when(selected, |b| b.primary())
                .when(!selected, |b| b.outline())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pick_cluster(value.clone(), window, cx)
                }))
        };
        let mut chips = Vec::new();
        for c in self.ls_clusters.clone() {
            let selected = self.ls_cluster.as_deref() == Some(c.id.as_str());
            chips.push(chip(
                format!("ls-cluster-{}", c.id),
                format!(
                    "{} · {} · {}:{}",
                    c.id, c.engine_version, c.endpoint, c.port
                ),
                Some(c.id),
                selected,
                cx,
            ));
        }
        // A saved cluster stays selected before (or without) discovery.
        if let Some(saved) = self
            .ls_cluster
            .clone()
            .filter(|s| !self.ls_clusters.iter().any(|c| &c.id == s))
        {
            chips.push(chip(
                format!("ls-cluster-{saved}"),
                saved.clone(),
                Some(saved),
                true,
                cx,
            ));
        }
        chips.push(chip(
            "ls-manual".into(),
            "Enter host and port".into(),
            None,
            self.ls_cluster.is_none(),
            cx,
        ));
        let status = match &self.ls_discovery {
            DiscoveryState::Idle => None,
            DiscoveryState::Running => Some(
                h_flex()
                    .gap(Tokens::space(2.0))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child("Asking LocalStack for Neptune clusters…"),
            ),
            DiscoveryState::Done => Some(
                h_flex()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child(format!("{} cluster(s) found.", self.ls_clusters.len())),
            ),
            DiscoveryState::Failed(err) => Some(
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
        };
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
        v_flex()
            .gap(Tokens::space(3.0))
            .child(
                h_flex()
                    .gap(Tokens::space(3.0))
                    .items_end()
                    .child(
                        field("LocalStack URL", Input::new(&self.localstack_url), None, cx)
                            .flex_1(),
                    )
                    .child(
                        field("Region", Input::new(&self.region), None, cx).w(Tokens::size(140.0)),
                    )
                    .child(
                        Button::new("ls-discover")
                            .label("Find clusters")
                            .outline()
                            .loading(matches!(self.ls_discovery, DiscoveryState::Running))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.discover_clusters(window, cx)
                            })),
                    ),
            )
            .children(status)
            .child(field(
                "Cluster",
                h_flex()
                    .id("ls-clusters")
                    .test_support()
                    .flex_wrap()
                    .gap(Tokens::space(1.0))
                    .children(chips),
                None,
                cx,
            ))
            .when(self.ls_cluster.is_none(), |this| this.child(endpoint_row))
            .child(
                h_flex()
                    .gap(Tokens::space(6.0))
                    .child(switch(
                        "conn-iam",
                        "IAM (signs with the test credentials)",
                        self.iam_auth,
                        |t, v| t.iam_auth = v,
                        cx,
                    ))
                    .child(switch(
                        "conn-tls",
                        "Use TLS (wss://)",
                        self.tls,
                        |t, v| t.tls = v,
                        cx,
                    )),
            )
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

        let provider = h_flex().gap(Tokens::space(1.0)).children(
            [
                (ProviderKind::GremlinServer, "gremlin", "Gremlin Server"),
                (ProviderKind::NeptuneAws, "neptune", "Amazon Neptune"),
                (
                    ProviderKind::NeptuneLocalStack,
                    "localstack",
                    "Neptune on LocalStack",
                ),
            ]
            .into_iter()
            .map(|(kind, id, label)| {
                Button::new(SharedString::from(format!("conn-provider-{id}")))
                    .label(label)
                    .small()
                    .when(kind == self.kind, |b| b.primary())
                    .when(kind != self.kind, |b| b.outline())
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_kind(kind, window, cx)),
                    )
            }),
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

        let provider_section = if self.kind == ProviderKind::NeptuneLocalStack {
            self.localstack_section(endpoint_row, cx)
        } else if gremlin {
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
            let profile_chip =
                |id: String, label: String, value: Option<String>, cx: &mut Context<'_, Self>| {
                    let selected = self.aws_profile == value;
                    Button::new(SharedString::from(id))
                        .label(label)
                        .small()
                        .when(selected, |b| b.primary())
                        .when(!selected, |b| b.outline())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.aws_profile = value.clone();
                            cx.notify();
                        }))
                };
            let mut chips = vec![profile_chip(
                "aws-profile-default-chain".into(),
                "Default credentials".into(),
                None,
                cx,
            )];
            for p in self.aws_profiles.clone() {
                let label = if p.sso {
                    format!("{} · SSO", p.name)
                } else {
                    p.name.clone()
                };
                chips.push(profile_chip(
                    format!("aws-profile-{}", p.name),
                    label,
                    Some(p.name),
                    cx,
                ));
            }
            // A saved profile that is no longer in the AWS files stays selectable.
            if let Some(saved) = self
                .aws_profile
                .clone()
                .filter(|s| !self.aws_profiles.iter().any(|p| &p.name == s))
            {
                chips.push(profile_chip(
                    format!("aws-profile-{saved}"),
                    format!("{saved} (not found)"),
                    Some(saved),
                    cx,
                ));
            }
            let profiles = v_flex()
                .gap(Tokens::space(1.0))
                .child(
                    h_flex()
                        .id("aws-profiles")
                        .test_support()
                        .flex_wrap()
                        .gap(Tokens::space(1.0))
                        .children(chips),
                )
                .child(
                    div()
                        .text_size(Tokens::text_xs())
                        .text_color(theme.muted_foreground)
                        .child("From ~/.aws/config and ~/.aws/credentials. Default credentials use AWS_PROFILE, environment variables or the instance role."),
                );
            v_flex()
                .gap(Tokens::space(3.0))
                .child(endpoint_row)
                .child(
                    h_flex()
                        .gap(Tokens::space(3.0))
                        .items_start()
                        .child(
                            field("Region", Input::new(&self.region), None, cx)
                                .w(Tokens::size(160.0)),
                        )
                        .child(
                            field("Tunnel address", Input::new(&self.tunnel), None, cx).flex_1(),
                        ),
                )
                .child(switch(
                    "conn-iam",
                    "IAM database authentication (SigV4)",
                    self.iam_auth,
                    |t, v| t.iam_auth = v,
                    cx,
                ))
                .when(self.iam_auth, |this| {
                    this.child(field("AWS profile", profiles, None, cx))
                })
                .child(switch(
                    "conn-insecure",
                    "Accept invalid certificates (tunnels whose certificate does not match)",
                    self.accept_invalid_certs,
                    |t, v| t.accept_invalid_certs = v,
                    cx,
                ))
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
                let engine = info
                    .engine_version
                    .as_ref()
                    .map(|v| format!(" · Neptune {v}"))
                    .unwrap_or_default();
                let version = info
                    .tinkerpop_version
                    .as_ref()
                    .map(|v| format!("{engine} · TinkerPop {v}"))
                    .unwrap_or(engine);
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
