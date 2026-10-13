//! The schema graph of a connection, as a tab in the editor area: vertex labels as nodes and edge labels
//! as directed links between the labels they connect. It reuses the graph view offline (no follow-up
//! queries, no editing) and redraws when the connection's schema changes.

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    BasePanel, DockPlacement, Panel, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use schema::Schema;
use ui_results::GraphView;
use uuid::Uuid;

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::connections::ConnectionManager;
use crate::schema::SchemaRegistry;
use crate::theme::ButtonTip as _;
use crate::theme::Tokens;

pub struct SchemaGraphPanel {
    focus_handle: FocusHandle,
    connection: Option<Uuid>,
    graph: Entity<GraphView>,
    /// The elements last drawn, so snapshots that change nothing visible do not re-layout.
    drawn: Vec<gremlin_model::GValue>,
    run: u64,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(SchemaGraphPanel);

impl WorkspacePanel for SchemaGraphPanel {
    const NAME: &'static str = "gremliner.schema-graph";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Center;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        Self::new(None, window, cx)
    }
}

impl SchemaGraphPanel {
    pub fn new(connection: Option<Uuid>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let graph = cx.new(|cx| {
                let mut g = GraphView::new(window, cx);
                g.set_offline(true, cx);
                g.set_edges_between(false, cx);
                if let Some(id) = connection {
                    g.set_connection(&id.to_string(), cx);
                }
                g
            });
            let mut subscriptions = vec![cx.observe(&graph, |_, _, cx| cx.notify())];
            if let Some(registry) = SchemaRegistry::try_global(cx) {
                subscriptions.push(cx.observe(&registry, |this: &mut Self, _, cx| this.sync(cx)));
            }
            if let Some(manager) = ConnectionManager::try_global(cx) {
                subscriptions.push(cx.observe(&manager, |_, _, cx| cx.notify()));
            }
            let mut this = Self {
                focus_handle: cx.focus_handle(),
                connection,
                graph,
                drawn: Vec::new(),
                run: 0,
                _subscriptions: subscriptions,
            };
            this.sync(cx);
            this
        })
    }

    pub fn connection(&self) -> Option<Uuid> {
        self.connection
    }

    pub fn graph(&self) -> &Entity<GraphView> {
        &self.graph
    }

    fn schema(&self, cx: &App) -> Option<Arc<Schema>> {
        let id = self.connection?;
        SchemaRegistry::try_global(cx)?.read(cx).schema(id)
    }

    fn sync(&mut self, cx: &mut Context<'_, Self>) {
        let Some(schema) = self.schema(cx) else {
            return;
        };
        let elements = schema::graph::elements(&schema);
        if elements == self.drawn {
            return;
        }
        self.run += 1;
        let run = self.run;
        self.graph
            .update(cx, |g, cx| g.ingest(run, &elements, true, cx));
        self.drawn = elements;
        cx.notify();
    }

    fn connection_name(&self, cx: &App) -> Option<String> {
        let id = self.connection?;
        let manager = ConnectionManager::try_global(cx)?;
        manager.read(cx).get(id).map(|c| c.name.clone())
    }
}

impl BasePanel for SchemaGraphPanel {
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

impl Panel for SchemaGraphPanel {
    fn title(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        match self.connection_name(cx) {
            Some(name) => format!("Schema \u{b7} {name}"),
            None => "Schema".to_owned(),
        }
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

/// Restore schema graph tabs from a saved layout.
pub fn register(cx: &mut App) {
    register_panel(cx, SchemaGraphPanel::NAME, |ctx, window, cx| {
        let connection = match ctx.info() {
            PanelInfo::Panel(value) => value
                .get("connection")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok()),
            _ => None,
        };
        panel_handle(SchemaGraphPanel::new(connection, window, cx))
    });
}

impl Render for SchemaGraphPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let entry = self
            .connection
            .and_then(|id| SchemaRegistry::try_global(cx).map(|r| (id, r)))
            .and_then(|(id, r)| r.read(cx).entry(id).cloned());
        let base = v_flex()
            .id("schema-graph-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.background);
        let Some(schema) = entry.as_ref().and_then(|e| e.schema.clone()) else {
            let refreshing = entry.as_ref().is_some_and(|e| e.refreshing);
            let (title, body) = if refreshing {
                (
                    "Discovering the schema\u{2026}",
                    "Labels appear here as they are found.",
                )
            } else {
                (
                    "No schema yet",
                    "Connect to this connection to discover its labels and how they connect.",
                )
            };
            return base.child(empty_state(IconName::Network, title, body, cx));
        };
        let refreshing = entry.as_ref().is_some_and(|e| e.refreshing);
        let id = self.connection;
        let summary = format!(
            "{} vertex labels \u{b7} {} edge labels \u{b7} {} connections",
            schema.vertex_labels.len(),
            schema.edge_labels.len(),
            schema.triples.len()
        );
        base.child(
            h_flex()
                .id("schema-graph-header")
                .gap(Tokens::space(2.0))
                .px(Tokens::space(3.0))
                .py(Tokens::space(1.5))
                .border_b(Tokens::BORDER)
                .border_color(theme.border)
                .text_size(Tokens::text_sm())
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .id("schema-graph-summary")
                        .test_support()
                        .child(summary),
                )
                .when(refreshing, |row| row.child("Discovering\u{2026}"))
                .when(schema.partial.any(), |row| {
                    row.child(
                        div()
                            .text_color(theme.warning)
                            .child("Partial: a sample limit or timeout was reached"),
                    )
                })
                .child(div().flex_1())
                .child(
                    Button::new("schema-graph-refresh")
                        .icon(IconName::RefreshCw)
                        .ghost()
                        .xsmall()
                        .tip("Refresh schema")
                        .disabled(refreshing)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            if let (Some(id), Some(registry)) = (id, SchemaRegistry::try_global(cx))
                            {
                                registry.update(cx, |r, cx| r.refresh(id, cx));
                            }
                        })),
                ),
        )
        .child(div().flex_1().min_h_0().child(self.graph.clone()))
    }
}
