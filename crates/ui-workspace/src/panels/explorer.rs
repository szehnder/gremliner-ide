//! The Graph Explorer (graph-explorer): a tab that builds a path search from schema labels and typed
//! filters, shows the Gremlin it generates, and runs it into the results dock's graph view.

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    BasePanel, DockPlacement, Panel, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::GValue;
use gremlin_syntax::explorer::{
    Direction, ExplorerPath, Filter, Generated, Op, Step, literal, property_keys, property_type,
};
use schema::Schema;
use uuid::Uuid;

use super::{RunRequest, WorkspacePanel, panel_basics};
use crate::connections::ConnectionManager;
use crate::run::Runner;
use crate::schema::SchemaRegistry;
use crate::theme::ButtonTip as _;
use crate::theme::Tokens;

pub struct ExplorerPanel {
    /// Also the runner's tab id, so the results dock graph and its follow-ups belong to the explorer.
    id: Uuid,
    connection: Option<Uuid>,
    path: ExplorerPath,
    /// One value input per filter, parallel to `path.steps[i].filters()`.
    values: Vec<Vec<Entity<InputState>>>,
    limit: Entity<InputState>,
    /// Clear the graph before each search (else results accumulate).
    pub clear_on_search: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(ExplorerPanel);

impl WorkspacePanel for ExplorerPanel {
    const NAME: &'static str = "gremliner.explorer";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Center;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        Self::new(None, None, window, cx)
    }
}

impl ExplorerPanel {
    /// An explorer on `connection`, optionally starting from a fixed vertex `(id, name)`.
    pub fn new(
        connection: Option<Uuid>,
        start: Option<(GValue, String)>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let limit = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value(gremlin_syntax::explorer::DEFAULT_LIMIT.to_string())
            });
            let mut subscriptions =
                vec![
                    cx.subscribe(&limit, |this: &mut Self, input, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            let text = input.read(cx).value().trim().to_owned();
                            this.path.limit = text.parse().unwrap_or(0);
                            cx.notify();
                        }
                    }),
                ];
            if let Some(registry) = SchemaRegistry::try_global(cx) {
                subscriptions.push(cx.observe(&registry, |_, _, cx| cx.notify()));
            }
            let mut path = ExplorerPath::default();
            if let Some(start) = start
                && let Step::Vertex { fixed, .. } = &mut path.steps[0]
            {
                *fixed = Some(start);
            }
            Self {
                id: Uuid::new_v4(),
                connection,
                path,
                values: vec![Vec::new()],
                limit,
                clear_on_search: true,
                focus_handle: cx.focus_handle(),
                _subscriptions: subscriptions,
            }
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn connection(&self) -> Option<Uuid> {
        self.connection
    }

    pub fn path(&self) -> &ExplorerPath {
        &self.path
    }

    fn schema(&self, cx: &App) -> Option<Arc<Schema>> {
        SchemaRegistry::try_global(cx)?
            .read(cx)
            .schema(self.connection?)
    }

    /// The query for the current path, or why there is none.
    pub fn generated(&self, cx: &App) -> Result<Generated, String> {
        self.path.generate(self.schema(cx).as_deref())
    }

    pub fn push_step(&mut self, cx: &mut Context<'_, Self>) {
        self.path.push();
        self.values.push(Vec::new());
        cx.notify();
    }

    pub fn pop_step(&mut self, cx: &mut Context<'_, Self>) {
        if self.path.steps.len() > 1 {
            self.path.pop();
            self.values.pop();
        }
        cx.notify();
    }

    pub fn set_label(&mut self, i: usize, label: Option<String>, cx: &mut Context<'_, Self>) {
        if let Some(step) = self.path.steps.get_mut(i) {
            step.set_label(label);
        }
        cx.notify();
    }

    pub fn set_direction(&mut self, i: usize, d: Direction, cx: &mut Context<'_, Self>) {
        if let Some(Step::Edge { direction, .. }) = self.path.steps.get_mut(i) {
            *direction = d;
        }
        cx.notify();
    }

    pub fn unfix_start(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(Step::Vertex { fixed, .. }) = self.path.steps.get_mut(0) {
            *fixed = None;
        }
        cx.notify();
    }

    pub fn set_randomize(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        self.path.randomize = on;
        cx.notify();
    }

    pub fn set_limit(&mut self, n: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.limit
            .update(cx, |s, cx| s.set_value(n.to_string(), window, cx));
        self.path.limit = n;
        cx.notify();
    }

    /// Add a filter on step `i` for its first known property.
    pub fn add_filter(&mut self, i: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        let schema = self.schema(cx);
        let Some(step) = self.path.steps.get(i) else {
            return;
        };
        let key = property_keys(schema.as_deref(), step)
            .into_iter()
            .next()
            .unwrap_or_default();
        let t = property_type(schema.as_deref(), step, &key);
        let op = Op::for_type(t)[0];
        self.path.steps[i].filters_mut().push(Filter {
            key,
            op,
            value: String::new(),
        });
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(op.placeholder()));
        let j = self.values[i].len();
        self._subscriptions.push(cx.subscribe(
            &input,
            move |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().to_string();
                    if let Some(f) = this
                        .path
                        .steps
                        .get_mut(i)
                        .and_then(|s| s.filters_mut().get_mut(j))
                    {
                        f.value = text;
                    }
                    cx.notify();
                }
            },
        ));
        self.values[i].push(input);
        cx.notify();
    }

    pub fn remove_filter(&mut self, i: usize, j: usize, cx: &mut Context<'_, Self>) {
        if let Some(step) = self.path.steps.get_mut(i)
            && j < step.filters().len()
        {
            step.filters_mut().remove(j);
            self.values[i].remove(j);
        }
        cx.notify();
    }

    /// Choose the filter's property; the operator resets when the type changes.
    pub fn set_filter_key(&mut self, i: usize, j: usize, key: String, cx: &mut Context<'_, Self>) {
        let schema = self.schema(cx);
        let Some(step) = self.path.steps.get(i) else {
            return;
        };
        let ops = Op::for_type(property_type(schema.as_deref(), step, &key));
        if let Some(f) = self.path.steps[i].filters_mut().get_mut(j) {
            f.key = key;
            if !ops.contains(&f.op) {
                f.op = ops[0];
            }
        }
        cx.notify();
    }

    pub fn set_filter_op(&mut self, i: usize, j: usize, op: Op, cx: &mut Context<'_, Self>) {
        if let Some(f) = self
            .path
            .steps
            .get_mut(i)
            .and_then(|s| s.filters_mut().get_mut(j))
        {
            f.op = op;
        }
        cx.notify();
    }

    pub fn set_filter_value(
        &mut self,
        i: usize,
        j: usize,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(input) = self.values.get(i).and_then(|v| v.get(j)).cloned() {
            let text = text.to_owned();
            input.update(cx, |s, cx| s.set_value(text.clone(), window, cx));
        }
        if let Some(f) = self
            .path
            .steps
            .get_mut(i)
            .and_then(|s| s.filters_mut().get_mut(j))
        {
            f.value = text.to_owned();
        }
        cx.notify();
    }

    /// The operators offered for filter `j` of step `i`.
    pub fn operators(&self, i: usize, j: usize, cx: &App) -> &'static [Op] {
        let schema = self.schema(cx);
        let step = &self.path.steps[i];
        let key = step
            .filters()
            .get(j)
            .map(|f| f.key.clone())
            .unwrap_or_default();
        Op::for_type(property_type(schema.as_deref(), step, &key))
    }

    /// Run the search into the results dock.
    pub fn run(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Ok(generated) = self.generated(cx) else {
            return;
        };
        let Some(connection) = self.connection else {
            return;
        };
        let runner = Runner::global(cx);
        if self.clear_on_search
            && let Some(graph) = runner.read(cx).graph_for_tab(self.id).cloned()
        {
            graph.update(cx, |g, cx| g.clear(cx));
        }
        let request = RunRequest {
            tab_id: self.id,
            connection_id: Some(connection),
            text: generated.script,
            range: 0..0,
            mode: ui_editor::RunMode::Run,
            generated: true,
            bindings: generated.bindings,
            settings: Default::default(),
        };
        runner.update(cx, |r, cx| r.execute(request, window, cx));
    }

    /// The query as an editor tab: values written in, and listed in a comment.
    pub fn editor_text(&self, cx: &App) -> Option<String> {
        let generated = self.generated(cx).ok()?;
        let mut text = String::from("// Graph Explorer search");
        if !generated.bindings.is_empty() {
            let params: Vec<String> = generated
                .bindings
                .iter()
                .map(|(k, v)| format!("{k} = {}", literal(v)))
                .collect();
            text.push_str(&format!("; parameters: {}", params.join(", ")));
        }
        text.push('\n');
        text.push_str(&generated.inlined());
        Some(text)
    }

    pub fn open_in_editor(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(text) = self.editor_text(cx) else {
            return;
        };
        let connection = self.connection;
        window.defer(cx, move |window, cx| {
            if let Some(ws) = crate::Workspace::for_window(window, cx) {
                ws.update(cx, |ws, cx| ws.open_query(&text, connection, window, cx));
            }
        });
    }

    fn connection_name(&self, cx: &App) -> Option<String> {
        let manager = ConnectionManager::try_global(cx)?;
        manager
            .read(cx)
            .get(self.connection?)
            .map(|c| c.name.clone())
    }

    fn render_step(
        &self,
        i: usize,
        schema: Option<&Schema>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let step = &self.path.steps[i];
        let labels: Vec<String> = schema
            .map(|s| {
                let list = if step.is_vertex() {
                    &s.vertex_labels
                } else {
                    &s.edge_labels
                };
                list.iter().map(|l| l.name.clone()).collect()
            })
            .unwrap_or_default();
        let this = cx.entity().downgrade();
        let label_button = Button::new(SharedString::from(format!("explorer-label-{i}")))
            .label(step.label().unwrap_or("any label").to_owned())
            .icon(IconName::ChevronDown)
            .outline()
            .xsmall()
            .dropdown_menu(move |mut menu, _, _| {
                let set = |label: Option<String>, this: WeakEntity<ExplorerPanel>| {
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        let _ = this.update(cx, |p, cx| p.set_label(i, label.clone(), cx));
                    }
                };
                menu = menu.item(PopupMenuItem::new("Any label").on_click(set(None, this.clone())));
                for l in &labels {
                    menu = menu.item(
                        PopupMenuItem::new(l.clone()).on_click(set(Some(l.clone()), this.clone())),
                    );
                }
                menu
            });
        let title = if step.is_vertex() { "Vertex" } else { "Edge" };
        let mut card = v_flex()
            .id(SharedString::from(format!("explorer-step-{i}")))
            .test_support()
            .gap(Tokens::space(1.5))
            .p(Tokens::space(2.0))
            .min_w(Tokens::size(200.0))
            .rounded(Tokens::RADIUS_MD)
            .border(Tokens::BORDER)
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                h_flex()
                    .gap(Tokens::space(2.0))
                    .child(
                        div()
                            .text_size(Tokens::text_xs())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(title),
                    )
                    .child(label_button),
            );
        if let Step::Vertex {
            fixed: Some((_, name)),
            ..
        } = step
        {
            card = card.child(
                h_flex()
                    .gap(Tokens::space(1.0))
                    .text_size(Tokens::text_sm())
                    .child(
                        div()
                            .id("explorer-fixed")
                            .test_support()
                            .text_color(theme.foreground)
                            .child(format!("Starts at {name}")),
                    )
                    .child(
                        Button::new("explorer-unfix")
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tip("Start from any vertex")
                            .on_click(cx.listener(|this, _, _, cx| this.unfix_start(cx))),
                    ),
            );
        }
        if let Step::Edge { direction, .. } = step {
            let current = *direction;
            card = card.child(
                h_flex()
                    .gap(Tokens::space(1.0))
                    .children(Direction::ALL.map(|d| {
                        Button::new(SharedString::from(format!("explorer-dir-{i}-{}", d.id())))
                            .label(match d {
                                Direction::Out => "out →",
                                Direction::In => "← in",
                                Direction::Both => "both ↔",
                            })
                            .xsmall()
                            .when(d == current, |b| b.primary())
                            .when(d != current, |b| b.ghost())
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.set_direction(i, d, cx)),
                            )
                    })),
            );
        }
        let keys = property_keys(schema, step);
        for (j, f) in step.filters().iter().enumerate() {
            let this = cx.entity().downgrade();
            let key_menu_keys = keys.clone();
            let key_button = Button::new(SharedString::from(format!("explorer-key-{i}-{j}")))
                .label(if f.key.is_empty() {
                    "property".to_owned()
                } else {
                    f.key.clone()
                })
                .ghost()
                .xsmall()
                .dropdown_menu(move |mut menu, _, _| {
                    for k in &key_menu_keys {
                        let this = this.clone();
                        let k2 = k.clone();
                        menu =
                            menu.item(PopupMenuItem::new(k.clone()).on_click(move |_, _, cx| {
                                let _ =
                                    this.update(cx, |p, cx| p.set_filter_key(i, j, k2.clone(), cx));
                            }));
                    }
                    if key_menu_keys.is_empty() {
                        menu = menu.label("No properties known for this label");
                    }
                    menu
                });
            let ops = self.operators(i, j, cx);
            let this = cx.entity().downgrade();
            let op_button = Button::new(SharedString::from(format!("explorer-op-{i}-{j}")))
                .label(f.op.label())
                .ghost()
                .xsmall()
                .dropdown_menu(move |mut menu, _, _| {
                    for &op in ops {
                        let this = this.clone();
                        menu =
                            menu.item(PopupMenuItem::new(op.label()).on_click(move |_, _, cx| {
                                let _ = this.update(cx, |p, cx| p.set_filter_op(i, j, op, cx));
                            }));
                    }
                    menu
                });
            let input = self.values[i][j].clone();
            card = card.child(
                h_flex()
                    .id(SharedString::from(format!("explorer-filter-{i}-{j}")))
                    .gap(Tokens::space(1.0))
                    .child(key_button)
                    .child(op_button)
                    .child(
                        div()
                            .w(Tokens::size(120.0))
                            .child(Input::new(&input).small()),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "explorer-remove-filter-{i}-{j}"
                        )))
                        .icon(IconName::Close)
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(move |this, _, _, cx| this.remove_filter(i, j, cx))),
                    ),
            );
        }
        card.child(
            Button::new(SharedString::from(format!("explorer-add-filter-{i}")))
                .label("Add filter")
                .icon(IconName::Plus)
                .ghost()
                .xsmall()
                .on_click(cx.listener(move |this, _, window, cx| this.add_filter(i, window, cx))),
        )
        .into_any_element()
    }
}

impl BasePanel for ExplorerPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn dump(&self, _: &App) -> PanelState {
        let mut state = PanelState::new(Self::NAME);
        state.info = PanelInfo::panel(serde_json::json!({
            "connection": self.connection.map(|c| c.to_string()),
        }));
        state
    }
}

impl Panel for ExplorerPanel {
    fn title(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        match self.connection_name(cx) {
            Some(name) => format!("Explore \u{b7} {name}"),
            None => "Explore".to_owned(),
        }
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

/// Restore explorer tabs (empty, on their connection) from a saved layout.
pub fn register(cx: &mut App) {
    register_panel(cx, ExplorerPanel::NAME, |ctx, window, cx| {
        let connection = match ctx.info() {
            PanelInfo::Panel(value) => value
                .get("connection")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok()),
            _ => None,
        };
        panel_handle(ExplorerPanel::new(connection, None, window, cx))
    });
}

impl Render for ExplorerPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let schema = self.schema(cx);
        let mut steps = h_flex()
            .id("explorer-steps")
            .flex_wrap()
            .items_start()
            .gap(Tokens::space(2.0));
        for i in 0..self.path.steps.len() {
            if i > 0 {
                steps = steps.child(
                    div()
                        .pt(Tokens::space(3.0))
                        .text_color(theme.muted_foreground)
                        .child("→"),
                );
            }
            steps = steps.child(self.render_step(i, schema.as_deref(), cx));
        }
        let next_is_edge = self.path.steps.last().is_some_and(Step::is_vertex);
        steps = steps.child(
            v_flex()
                .gap(Tokens::space(1.0))
                .child(
                    Button::new("explorer-add-step")
                        .label(if next_is_edge {
                            "Add edge"
                        } else {
                            "Add vertex"
                        })
                        .icon(IconName::Plus)
                        .outline()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.push_step(cx))),
                )
                .when(self.path.steps.len() > 1, |el| {
                    el.child(
                        Button::new("explorer-remove-step")
                            .label("Remove last")
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| this.pop_step(cx))),
                    )
                }),
        );
        let generated = self.path.generate(schema.as_deref());
        let hint = match &generated {
            Err(e) => Some((e.clone(), theme.danger)),
            Ok(_) => self.path.warning().map(|w| (w.to_owned(), theme.warning)),
        };
        let (preview, params) = match &generated {
            Ok(g) => (
                g.script.clone(),
                g.bindings
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", literal(v)))
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
            Err(_) => (String::new(), String::new()),
        };
        let can_run = generated.is_ok() && self.connection.is_some();
        let options = h_flex()
            .gap(Tokens::space(4.0))
            .items_center()
            .child(
                h_flex()
                    .gap(Tokens::space(1.5))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.foreground)
                    .child("Limit")
                    .child(
                        div()
                            .w(Tokens::size(72.0))
                            .child(Input::new(&self.limit).small()),
                    ),
            )
            .child(
                Switch::new("explorer-random")
                    .label("Random order")
                    .checked(self.path.randomize)
                    .small()
                    .on_click(cx.listener(|this, on: &bool, _, cx| this.set_randomize(*on, cx))),
            )
            .child(
                Switch::new("explorer-clear")
                    .label("Clear graph on each search")
                    .checked(self.clear_on_search)
                    .small()
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.clear_on_search = *on;
                        cx.notify();
                    })),
            );
        v_flex()
            .id("explorer-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_y_scroll()
            .gap(Tokens::space(3.0))
            .p(Tokens::space(3.0))
            .bg(theme.background)
            .child(
                div()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child("Build a path from vertex and edge steps. Labels and properties come from the schema."),
            )
            .child(steps)
            .child(options)
            .children(hint.map(|(text, color)| {
                div()
                    .id("explorer-hint")
                    .test_support()
                    .text_size(Tokens::text_sm())
                    .text_color(color)
                    .child(text)
            }))
            .child(
                v_flex()
                    .gap(Tokens::space(1.0))
                    .p(Tokens::space(2.0))
                    .rounded(Tokens::RADIUS_MD)
                    .bg(theme.secondary)
                    .child(
                        div()
                            .id("explorer-preview")
                            .test_support()
                            .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                            .text_size(Tokens::text_sm())
                            .text_color(theme.foreground)
                            .whitespace_normal()
                            .child(preview),
                    )
                    .when(!params.is_empty(), |el| {
                        el.child(
                            div()
                                .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                                .text_size(Tokens::text_xs())
                                .text_color(theme.muted_foreground)
                                .child(params),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap(Tokens::space(2.0))
                    .child(
                        Button::new("explorer-run")
                            .label("Search")
                            .icon(IconName::Play)
                            .primary()
                            .small()
                            .disabled(!can_run)
                            .on_click(cx.listener(|this, _, window, cx| this.run(window, cx))),
                    )
                    .child(
                        Button::new("explorer-open-editor")
                            .label("Open in editor")
                            .outline()
                            .small()
                            .disabled(generated.is_err())
                            .on_click(cx.listener(|this, _, window, cx| this.open_in_editor(window, cx))),
                    ),
            )
    }
}
