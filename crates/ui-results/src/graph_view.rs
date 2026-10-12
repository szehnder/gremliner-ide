//! The Graph results view: query results merged into a per-tab graph store and drawn on the graph canvas,
//! with expand-neighbours, hide/pin/select, a legend with label toggles, a property filter, find, and a
//! clear/accumulate toggle.
//!
//! The view never talks to the database itself. Follow-up queries (missing endpoints, edges between
//! vertices, expand) are emitted as [`GraphViewEvent::Fetch`]; the host runs them and hands the items back
//! through [`GraphView::apply_fetch`].

use std::collections::{HashMap, HashSet};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use graph_canvas::theme::CanvasStyle;
use graph_canvas::{CanvasEvent, GraphCanvas, LayoutScope};
use graph_layout::{LayoutKind, LayoutOptions};
use graph_store::visibility::LabelCount;
use graph_store::{
    EdgeIn, ExtId, Filter, FilterOp, GraphDelta, GraphStore, MergeReport, NodeIn, NodeIx,
    PropValue, Props, flags,
};
use gremlin_model::elements::{ElementSet, elements};
use gremlin_model::{GValue, console};

use crate::style_library::StyleLibrary;

mod edit;
pub use edit::{MutationRequest, Proposal, SelectedElement, parse_typed, value_type_names};
use ui_theme::Tokens;

actions!(
    graph,
    [
        HideSelection,
        UnhideAll,
        SelectAll,
        SelectNeighbours,
        PinSelection,
        UnpinSelection,
        ExpandSelection,
        FitGraph,
        FindInGraph,
        ClearGraph,
        RerunLayout,
        StopLayout,
        UndoEdit,
        RedoEdit,
        DeleteFromDatabase,
        NewVertex,
    ]
);

/// Node spacing preset (applies to every layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    Compact,
    Normal,
    Wide,
}

/// How strongly the force layout pulls everything to the centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gravity {
    Weak,
    Normal,
    Strong,
}

/// The tab's layout choice and options (persisted per tab by the host).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutSettings {
    pub kind: LayoutKind,
    pub spacing: Spacing,
    pub gravity: Gravity,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            kind: LayoutKind::ForceAtlas2,
            spacing: Spacing::Normal,
            gravity: Gravity::Normal,
        }
    }
}

impl LayoutSettings {
    pub fn options(&self) -> LayoutOptions {
        let mut o = LayoutOptions::default();
        let k = match self.spacing {
            Spacing::Compact => 0.5,
            Spacing::Normal => 1.0,
            Spacing::Wide => 2.0,
        };
        o.fa2.scaling *= k * k;
        o.hierarchical.layer_gap *= k;
        o.hierarchical.node_gap *= k;
        o.spacing *= k;
        o.fa2.gravity = match self.gravity {
            Gravity::Weak => 0.03,
            Gravity::Normal => 0.1,
            Gravity::Strong => 0.3,
        };
        o
    }

    pub fn to_json(self) -> serde_json::Value {
        let spacing = match self.spacing {
            Spacing::Compact => "compact",
            Spacing::Normal => "normal",
            Spacing::Wide => "wide",
        };
        let gravity = match self.gravity {
            Gravity::Weak => "weak",
            Gravity::Normal => "normal",
            Gravity::Strong => "strong",
        };
        serde_json::json!({ "kind": self.kind.id(), "spacing": spacing, "gravity": gravity })
    }

    /// Parses [`Self::to_json`] output; unknown or missing fields keep their defaults.
    pub fn from_json(v: &serde_json::Value) -> Self {
        let d = Self::default();
        let field = |k: &str| v.get(k).and_then(serde_json::Value::as_str);
        Self {
            kind: field("kind")
                .and_then(LayoutKind::from_id)
                .unwrap_or(d.kind),
            spacing: match field("spacing") {
                Some("compact") => Spacing::Compact,
                Some("wide") => Spacing::Wide,
                _ => d.spacing,
            },
            gravity: match field("gravity") {
                Some("weak") => Gravity::Weak,
                Some("strong") => Gravity::Strong,
                _ => d.gravity,
            },
        }
    }
}

/// Key context of the canvas area (bindings such as Delete apply only there, not in the search box).
pub const CANVAS_CONTEXT: &str = "GraphCanvas";
/// Key context of the whole view.
pub const VIEW_CONTEXT: &str = "GraphView";
/// Neighbours added per expand (the spec's default cap).
pub const DEFAULT_EXPAND_CAP: usize = 100;
/// Above this many vertices, edges between them are fetched only on request.
pub const EDGES_BETWEEN_LIMIT: usize = 2_000;
/// Ids per follow-up query.
const ID_BATCH: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpandDirection {
    Both,
    Out,
    In,
}

/// What a follow-up query is for.
#[derive(Clone, Debug, PartialEq)]
pub enum FetchKind {
    /// Vertices referenced by edges but not returned, or returned without properties.
    Endpoints,
    /// Edges among the displayed vertices.
    EdgesBetween,
    /// One page of a node's neighbours.
    Expand {
        node: ExtId,
        direction: ExpandDirection,
        label: Option<String>,
        offset: usize,
    },
    /// The total for the "N of M shown" text.
    ExpandCount {
        node: ExtId,
        direction: ExpandDirection,
        label: Option<String>,
    },
    /// In and out degree for the inspector.
    Degree(ExtId),
    /// Incident edges of vertices about to be deleted (for the confirmation's counts).
    IncidentCount(u64),
}

/// A follow-up query for the host to run against the tab's connection.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchRequest {
    /// Results for an older generation (the graph was cleared since) are dropped.
    pub generation: u64,
    pub kind: FetchKind,
    pub script: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphViewEvent {
    Fetch(FetchRequest),
    /// "Show in table": switch to the Table view filtered to this id.
    ShowInTable(String),
    /// The layout choice or options changed (persist them for the tab).
    LayoutChanged(LayoutSettings),
    /// The user confirmed a data change; run it and answer with [`GraphView::apply_mutation`].
    Mutate(edit::MutationRequest),
}

/// Counts for the status text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphStats {
    pub nodes: usize,
    pub visible_nodes: usize,
    pub edges: usize,
    pub visible_edges: usize,
    pub selected: usize,
}

impl GraphStats {
    pub fn text(&self) -> String {
        let mut s = format!(
            "{} of {} nodes · {} of {} edges",
            self.visible_nodes, self.nodes, self.visible_edges, self.edges
        );
        if self.selected > 0 {
            s.push_str(&format!(" · {} selected", self.selected));
        }
        s
    }
}

struct RunState {
    run_id: u64,
    /// The run adds to an existing graph: new nodes are placed incrementally.
    incremental: bool,
    ingested: usize,
    has_vertices: bool,
    has_edges: bool,
    followed_up: bool,
}

/// The "N of M shown — load more" affordance after a capped expand.
#[derive(Clone, Debug, PartialEq)]
struct ExpandMore {
    node: ExtId,
    caption: String,
    direction: ExpandDirection,
    label: Option<String>,
    shown: usize,
    total: Option<u64>,
}

pub struct GraphView {
    canvas: Entity<GraphCanvas>,
    focus: FocusHandle,
    search: Entity<InputState>,
    filter_key: Entity<InputState>,
    filter_value: Entity<InputState>,
    filter_label: Option<String>,
    filter_op: FilterOp,
    show_filter: bool,
    show_legend: bool,
    show_style_editor: bool,
    layout: LayoutSettings,
    /// The stylesheet in use (an id in the shared [`StyleLibrary`]).
    stylesheet: String,
    /// The connection this graph's results come from (for its default stylesheet).
    connection: Option<String>,
    style_editor: Option<Entity<crate::style_editor::StyleEditor>>,
    accumulate: bool,
    edges_between: bool,
    expand_cap: usize,
    /// Original id values, for literals in follow-up queries.
    ids: HashMap<ExtId, GValue>,
    /// Placeholder vertices whose details were already requested.
    requested: HashSet<ExtId>,
    generation: u64,
    run: Option<RunState>,
    /// Fit the camera after merges until the user moves it.
    auto_fit: bool,
    more: Option<ExpandMore>,
    notice: Option<SharedString>,
    /// Edges between were skipped because there were too many vertices.
    edges_between_skipped: bool,
    pending: usize,
    style_dark: Option<bool>,
    matches: Vec<NodeIx>,
    match_ix: usize,
    edit: edit::EditState,
    /// Not backed by a connection (e.g. the schema graph): no follow-up queries and no edits.
    offline: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<GraphViewEvent> for GraphView {}

impl Focusable for GraphView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl GraphView {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let canvas = cx.new(|cx| GraphCanvas::new(GraphStore::new(), cx));
        let library = StyleLibrary::global(cx);
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find node by id or property"));
        let filter_key = cx.new(|cx| InputState::new(window, cx).placeholder("property"));
        let filter_value = cx.new(|cx| InputState::new(window, cx).placeholder("value"));
        let subscriptions =
            vec![
                cx.subscribe(&canvas, |this, _, event: &CanvasEvent, cx| match event {
                    CanvasEvent::NodeDoubleClicked(n) => {
                        this.expand(*n, ExpandDirection::Both, None, cx);
                    }
                    CanvasEvent::LinkRequested { from, to } => this.start_link(*from, *to, cx),
                }),
                cx.observe(&canvas, |_, _, cx| cx.notify()),
                cx.observe(&library, |this: &mut Self, _, cx| this.restyle(cx)),
                cx.subscribe_in(&search, window, |this, state, event: &InputEvent, _, cx| {
                    match event {
                        InputEvent::Change => {
                            let q = state.read(cx).value().to_string();
                            this.matches = this.store(cx).find(&q, 50);
                            this.match_ix = 0;
                            cx.notify();
                        }
                        InputEvent::PressEnter { .. } => {
                            let q = state.read(cx).value().to_string();
                            this.find_next(&q, cx);
                        }
                        _ => {}
                    }
                }),
                cx.subscribe_in(&filter_key, window, |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::PressEnter { .. }) {
                        this.apply_filter(cx);
                    }
                }),
                cx.subscribe_in(
                    &filter_value,
                    window,
                    |this, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Change | InputEvent::PressEnter { .. }) {
                            this.apply_filter(cx);
                        }
                    },
                ),
            ];
        Self {
            canvas,
            focus: cx.focus_handle(),
            search,
            filter_key,
            filter_value,
            filter_label: None,
            filter_op: FilterOp::Gt,
            show_filter: false,
            show_legend: true,
            show_style_editor: false,
            layout: LayoutSettings::default(),
            stylesheet: crate::style_library::DEFAULT_ID.to_owned(),
            connection: None,
            style_editor: None,
            accumulate: false,
            edges_between: true,
            expand_cap: DEFAULT_EXPAND_CAP,
            ids: HashMap::new(),
            requested: HashSet::new(),
            generation: 0,
            run: None,
            auto_fit: true,
            more: None,
            notice: None,
            edges_between_skipped: false,
            pending: 0,
            style_dark: None,
            matches: Vec::new(),
            match_ix: 0,
            edit: edit::EditState::new(window, cx),
            offline: false,
            _subscriptions: subscriptions,
        }
    }

    /// Offline graphs never ask the host for follow-up queries and offer no editing.
    pub fn set_offline(&mut self, offline: bool, cx: &mut Context<'_, Self>) {
        self.offline = offline;
        cx.notify();
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    pub fn canvas(&self) -> &Entity<GraphCanvas> {
        &self.canvas
    }

    pub fn store<'a>(&self, cx: &'a App) -> &'a GraphStore {
        self.canvas.read(cx).store()
    }

    fn update_store<R>(
        &mut self,
        cx: &mut Context<'_, Self>,
        f: impl FnOnce(&mut GraphStore) -> R,
    ) -> R {
        let r = self.canvas.update(cx, |c, cx| {
            let r = f(c.store_mut());
            cx.notify();
            r
        });
        cx.notify();
        r
    }

    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    pub fn filter_inputs(&self) -> (&Entity<InputState>, &Entity<InputState>) {
        (&self.filter_key, &self.filter_value)
    }

    pub fn accumulate(&self) -> bool {
        self.accumulate
    }

    pub fn set_accumulate(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        self.accumulate = on;
        cx.notify();
    }

    pub fn edges_between(&self) -> bool {
        self.edges_between
    }

    pub fn set_edges_between(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        self.edges_between = on;
        cx.notify();
    }

    pub fn set_expand_cap(&mut self, cap: usize) {
        self.expand_cap = cap.max(1);
    }

    /// Follow-up queries still running.
    pub fn pending_fetches(&self) -> usize {
        self.pending
    }

    pub fn notice(&self) -> Option<&SharedString> {
        self.notice.as_ref()
    }

    pub fn stats(&self, cx: &App) -> GraphStats {
        let g = self.store(cx);
        GraphStats {
            nodes: g.node_count(),
            visible_nodes: g.visible_node_count(),
            edges: g.edge_count(),
            visible_edges: g.visible_edge_count(),
            selected: g.selected_count(),
        }
    }

    /// The "N of M neighbours shown" text, when an expand was capped.
    pub fn more_text(&self) -> Option<String> {
        let m = self.more.as_ref()?;
        let total = m
            .total
            .map(|t| t.to_string())
            .unwrap_or_else(|| "many".into());
        Some(format!(
            "{} of {} neighbours of {} shown",
            m.shown, total, m.caption
        ))
    }

    /// Removes everything; results of in-flight follow-ups are dropped.
    pub fn clear(&mut self, cx: &mut Context<'_, Self>) {
        self.generation += 1;
        self.ids.clear();
        self.edit.forget_elements();
        self.requested.clear();
        self.more = None;
        self.notice = None;
        self.edges_between_skipped = false;
        self.matches.clear();
        self.auto_fit = true;
        self.canvas.update(cx, |c, cx| {
            c.store_mut().clear();
            c.request_fit();
            cx.notify();
        });
        cx.notify();
    }

    // ---- results -----------------------------------------------------------------------------

    /// Feed the items of run `run_id` (called repeatedly while it streams; only new items are merged).
    /// When the run has `finished`, follow-up queries are emitted once.
    pub fn ingest(
        &mut self,
        run_id: u64,
        items: &[GValue],
        finished: bool,
        cx: &mut Context<'_, Self>,
    ) {
        let new_run = self.run.as_ref().is_none_or(|r| r.run_id != run_id);
        if new_run {
            if !self.accumulate {
                self.clear(cx);
            } else if self.store(cx).node_count() == 0 {
                self.auto_fit = true;
            }
            let incremental = self.store(cx).node_count() > 0;
            self.run = Some(RunState {
                run_id,
                incremental,
                ingested: 0,
                has_vertices: false,
                has_edges: false,
                followed_up: false,
            });
        }
        let Some(run) = self.run.as_mut() else { return };
        if items.len() > run.ingested {
            let set = elements(&items[run.ingested..]);
            run.has_vertices |= !set.vertices.is_empty();
            run.has_edges |= !set.edges.is_empty();
            run.ingested = items.len();
            let incremental = run.incremental;
            let report = self.merge_set(set, cx);
            if incremental {
                self.place_new(&report, cx);
            } else if !report.added_nodes.is_empty() || !report.added_edges.is_empty() {
                self.relayout(LayoutScope::All, cx);
            }
        }
        if finished
            && let Some(run) = self.run.as_mut()
            && !run.followed_up
        {
            run.followed_up = true;
            let vertex_only = run.has_vertices && !run.has_edges;
            self.follow_up(vertex_only, cx);
        }
    }

    fn merge_set(&mut self, set: ElementSet, cx: &mut Context<'_, Self>) -> MergeReport {
        if set.is_empty() {
            return MergeReport::default();
        }
        let delta = self.delta_for(set);
        let report = self.update_store(cx, |g| g.merge(delta));
        self.restyle(cx);
        if self.auto_fit {
            let touched = self.canvas.read(cx).camera_touched();
            if touched {
                self.auto_fit = false;
            } else {
                self.canvas.update(cx, |c, _| c.request_fit());
            }
        }
        report
    }

    // ---- style -------------------------------------------------------------------------------

    pub fn stylesheet_id(&self) -> &str {
        &self.stylesheet
    }

    /// Switch to stylesheet `id` (re-renders right away).
    pub fn set_stylesheet(&mut self, id: &str, cx: &mut Context<'_, Self>) {
        self.stylesheet = id.to_owned();
        self.restyle(cx);
        cx.notify();
    }

    pub fn connection(&self) -> Option<&str> {
        self.connection.as_deref()
    }

    /// The connection results come from. The first time, its default stylesheet (if any) is applied.
    pub fn set_connection(&mut self, connection: &str, cx: &mut Context<'_, Self>) {
        if self.connection.as_deref() == Some(connection) {
            return;
        }
        let first = self.connection.is_none();
        self.connection = Some(connection.to_owned());
        if first {
            let library = StyleLibrary::global(cx);
            if let Some(id) = library.read(cx).default_for_connection(connection) {
                let id = id.to_owned();
                self.set_stylesheet(&id, cx);
            }
        }
        cx.notify();
    }

    /// Re-resolve the stylesheet against the graph (after merges and stylesheet edits).
    pub fn restyle(&mut self, cx: &mut Context<'_, Self>) {
        let library = StyleLibrary::global(cx);
        let sheet = library.read(cx).get_or_default(&self.stylesheet);
        self.canvas.update(cx, |c, cx| {
            let looks = library.update(cx, |l, _| {
                graph_store::style::resolve(&sheet, c.store_mut(), l.auto_palette_mut())
            });
            c.set_looks(looks);
            cx.notify();
        });
        cx.notify();
    }

    pub fn show_style_editor(&self) -> bool {
        self.show_style_editor
    }

    pub fn set_show_style_editor(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.show_style_editor = on;
        if on && self.style_editor.is_none() {
            let graph = cx.weak_entity();
            self.style_editor =
                Some(cx.new(|cx| crate::style_editor::StyleEditor::new(graph, window, cx)));
        }
        if let Some(editor) = &self.style_editor {
            editor.update(cx, |e, cx| e.reload_later(window, cx));
        }
        cx.notify();
    }

    pub fn style_editor(&self) -> Option<&Entity<crate::style_editor::StyleEditor>> {
        self.style_editor.as_ref()
    }

    // ---- layout ------------------------------------------------------------------------------

    pub fn layout_settings(&self) -> LayoutSettings {
        self.layout
    }

    /// Change the layout choice; `run` applies it to the graph now.
    pub fn set_layout_settings(
        &mut self,
        settings: LayoutSettings,
        run: bool,
        cx: &mut Context<'_, Self>,
    ) {
        let changed = settings != self.layout;
        self.layout = settings;
        if changed {
            cx.emit(GraphViewEvent::LayoutChanged(settings));
        }
        if run {
            self.rerun_layout(cx);
        }
        cx.notify();
    }

    /// Lay out the whole graph again with the current settings.
    pub fn rerun_layout(&mut self, cx: &mut Context<'_, Self>) {
        let fit = !self.canvas.read(cx).camera_touched() || self.auto_fit;
        let (kind, options) = (self.layout.kind, self.layout.options());
        self.canvas.update(cx, |c, cx| {
            c.run_layout(kind, options, LayoutScope::All, fit, cx)
        });
        cx.notify();
    }

    pub fn stop_layout(&mut self, cx: &mut Context<'_, Self>) {
        self.canvas.update(cx, |c, cx| c.stop_layout(cx));
        cx.notify();
    }

    pub fn is_layout_running(&self, cx: &App) -> bool {
        self.canvas.read(cx).is_layout_running()
    }

    fn relayout(&mut self, scope: LayoutScope, cx: &mut Context<'_, Self>) {
        let fit = self.auto_fit && scope == LayoutScope::All;
        let (kind, options) = (self.layout.kind, self.layout.options());
        self.canvas
            .update(cx, |c, cx| c.run_layout(kind, options, scope, fit, cx));
    }

    /// Settle newly added nodes around their neighbours without moving the rest.
    fn place_new(&mut self, report: &MergeReport, cx: &mut Context<'_, Self>) {
        if !report.added_nodes.is_empty() {
            self.relayout(LayoutScope::Only(report.added_nodes.clone()), cx);
        }
    }

    fn delta_for(&mut self, set: ElementSet) -> GraphDelta {
        let mut delta = GraphDelta::default();
        for v in set.vertices {
            let id = self.remember(&v.id);
            self.remember_vertex(&id, &v);
            let props: Props = v
                .properties
                .iter()
                .flat_map(|(k, vps)| {
                    let key: std::sync::Arc<str> = k.as_str().into();
                    vps.iter()
                        .map(move |vp| (key.clone(), prop_value(&vp.value)))
                })
                .collect();
            let caption = graph_store::style::default_caption(&props, &id.to_string());
            delta.nodes.push(NodeIn {
                id,
                label: v.label,
                caption,
                props,
            });
        }
        for e in set.edges {
            let id = ext_id(&e.id);
            self.remember_edge(&id, &e);
            let out_v = self.remember(&e.out_v);
            let in_v = self.remember(&e.in_v);
            let props: Props = e
                .properties
                .iter()
                .map(|(k, p)| (k.as_str().into(), prop_value(&p.value)))
                .collect();
            delta.edges.push(EdgeIn {
                id,
                label: e.label,
                out_v,
                out_label: e.out_v_label,
                in_v,
                in_label: e.in_v_label,
                props,
            });
        }
        delta
    }

    fn remember(&mut self, id: &GValue) -> ExtId {
        let ext = ext_id(id);
        self.ids.entry(ext.clone()).or_insert_with(|| id.clone());
        ext
    }

    fn literal(&self, id: &ExtId) -> Option<String> {
        self.ids.get(id).and_then(gremlin_literal)
    }

    fn emit_fetch(&mut self, kind: FetchKind, script: String, cx: &mut Context<'_, Self>) {
        if self.offline {
            return;
        }
        self.pending += 1;
        cx.emit(GraphViewEvent::Fetch(FetchRequest {
            generation: self.generation,
            kind,
            script,
        }));
        cx.notify();
    }

    /// Fetch the full vertices of placeholders (edge endpoints, vertices without properties) not yet
    /// requested.
    fn fetch_details(&mut self, cx: &mut Context<'_, Self>) {
        let g = self.canvas.read(cx).store();
        let wanted: Vec<ExtId> = (0..g.node_count())
            .map(|i| NodeIx(i as u32))
            .filter(|n| g.has_flag(*n, flags::PLACEHOLDER))
            .map(|n| g.node_ext(n).clone())
            .filter(|id| !self.requested.contains(id))
            .collect();
        let mut literals = Vec::with_capacity(wanted.len());
        for id in wanted {
            if let Some(lit) = self.literal(&id) {
                literals.push(lit);
            }
            self.requested.insert(id);
        }
        for chunk in literals.chunks(ID_BATCH) {
            self.emit_fetch(
                FetchKind::Endpoints,
                format!("g.V({})", chunk.join(", ")),
                cx,
            );
        }
    }

    fn follow_up(&mut self, vertex_only: bool, cx: &mut Context<'_, Self>) {
        self.fetch_details(cx);
        if self.edges_between && vertex_only {
            let count = self.store(cx).node_count();
            if count > EDGES_BETWEEN_LIMIT {
                self.edges_between_skipped = true;
                self.notice = Some(
                    format!(
                        "Edges between the {count} vertices were not fetched automatically (limit {EDGES_BETWEEN_LIMIT})."
                    )
                    .into(),
                );
            } else {
                self.fetch_edges_between(cx);
            }
        }
    }

    /// Fetch the edges among all displayed vertices (the "Fetch edges between" button).
    pub fn fetch_edges_between(&mut self, cx: &mut Context<'_, Self>) {
        self.edges_between_skipped = false;
        self.notice = None;
        let g = self.store(cx);
        let ids: Vec<String> = (0..g.node_count())
            .map(|i| NodeIx(i as u32))
            .filter(|n| !g.has_flag(*n, flags::PLACEHOLDER))
            .filter_map(|n| self.literal(g.node_ext(n)))
            .collect();
        if ids.len() < 2 {
            return;
        }
        let all = ids.join(", ");
        for chunk in ids.chunks(ID_BATCH) {
            self.emit_fetch(
                FetchKind::EdgesBetween,
                format!(
                    "g.V({}).outE().where(inV().hasId(within({all})))",
                    chunk.join(", ")
                ),
                cx,
            );
        }
    }

    /// Fetch up to the cap of `n`'s neighbours (by direction and optional edge label).
    pub fn expand(
        &mut self,
        n: NodeIx,
        direction: ExpandDirection,
        label: Option<String>,
        cx: &mut Context<'_, Self>,
    ) {
        let ext = self.store(cx).node_ext(n).clone();
        self.expand_page(ext, direction, label, 0, cx);
    }

    fn expand_page(
        &mut self,
        node: ExtId,
        direction: ExpandDirection,
        label: Option<String>,
        offset: usize,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(id) = self.literal(&node) else {
            self.notice = Some("This vertex id cannot be used in a query.".into());
            cx.notify();
            return;
        };
        let script = expand_script(&id, direction, label.as_deref(), offset, self.expand_cap);
        self.emit_fetch(
            FetchKind::Expand {
                node,
                direction,
                label,
                offset,
            },
            script,
            cx,
        );
    }

    /// Load the next page of the last capped expand.
    pub fn load_more(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(m) = self.more.clone() {
            self.expand_page(m.node, m.direction, m.label, m.shown, cx);
        }
    }

    /// The host's answer to a [`FetchRequest`].
    pub fn apply_fetch(
        &mut self,
        request: &FetchRequest,
        result: Result<Vec<GValue>, String>,
        cx: &mut Context<'_, Self>,
    ) {
        self.pending = self.pending.saturating_sub(1);
        cx.notify();
        if request.generation != self.generation {
            return;
        }
        let items = match result {
            Ok(items) => items,
            Err(message) => {
                let what = match request.kind {
                    FetchKind::Endpoints => "edge endpoints",
                    FetchKind::EdgesBetween => "edges between vertices",
                    FetchKind::Expand { .. } | FetchKind::ExpandCount { .. } => "neighbours",
                    FetchKind::Degree(_) => "the degree",
                    FetchKind::IncidentCount(_) => "incident edges",
                };
                self.notice = Some(format!("Couldn't fetch {what}: {message}").into());
                return;
            }
        };
        match &request.kind {
            FetchKind::Endpoints => {
                let report = self.merge_set(elements(&items), cx);
                self.place_new(&report, cx);
            }
            FetchKind::EdgesBetween => {
                let report = self.merge_set(elements(&items), cx);
                let incremental = self.run.as_ref().is_some_and(|r| r.incremental);
                if !report.added_edges.is_empty() && self.auto_fit && !incremental {
                    // Still settling the first picture: lay it out again with the new edges.
                    self.relayout(LayoutScope::All, cx);
                } else {
                    self.place_new(&report, cx);
                }
                self.fetch_details(cx);
            }
            FetchKind::Expand {
                node,
                direction,
                label,
                offset,
            } => {
                let cap = self.expand_cap;
                let more = items.len() > cap;
                // Expanding keeps the picture: no auto-fit from here on.
                self.auto_fit = false;
                let report = self.merge_set(elements(&items[..items.len().min(cap)]), cx);
                self.place_new(&report, cx);
                self.fetch_details(cx);
                if more {
                    let caption = self
                        .store(cx)
                        .node_by_ext(node)
                        .map(|n| self.store(cx).caption(n).to_string())
                        .unwrap_or_else(|| node.to_string());
                    let known_total = self
                        .more
                        .as_ref()
                        .filter(|m| {
                            m.node == *node && m.direction == *direction && m.label == *label
                        })
                        .and_then(|m| m.total);
                    self.more = Some(ExpandMore {
                        node: node.clone(),
                        caption,
                        direction: *direction,
                        label: label.clone(),
                        shown: offset + cap,
                        total: known_total,
                    });
                    if known_total.is_none()
                        && let Some(id) = self.literal(node)
                    {
                        let script = count_script(&id, *direction, label.as_deref());
                        self.emit_fetch(
                            FetchKind::ExpandCount {
                                node: node.clone(),
                                direction: *direction,
                                label: label.clone(),
                            },
                            script,
                            cx,
                        );
                    }
                } else if self.more.as_ref().is_some_and(|m| m.node == *node) {
                    self.more = None;
                }
            }
            FetchKind::ExpandCount {
                node,
                direction,
                label,
            } => {
                let total = items.first().and_then(GValue::as_i64);
                if let Some(m) = self.more.as_mut()
                    && m.node == *node
                    && m.direction == *direction
                    && m.label == *label
                {
                    m.total = total.and_then(|t| u64::try_from(t).ok());
                }
            }
            FetchKind::Degree(node) => self.apply_degree(node, &items),
            FetchKind::IncidentCount(proposal) => self.apply_incident_count(*proposal, &items),
        }
        cx.notify();
    }

    // ---- selection, visibility, find -------------------------------------------------------

    pub fn hide_selection(&mut self, cx: &mut Context<'_, Self>) {
        self.update_store(cx, GraphStore::hide_selected);
    }

    pub fn unhide_all(&mut self, cx: &mut Context<'_, Self>) {
        self.update_store(cx, GraphStore::unhide_all);
    }

    pub fn select_all(&mut self, cx: &mut Context<'_, Self>) {
        self.update_store(cx, GraphStore::select_all);
    }

    pub fn select_neighbours(&mut self, cx: &mut Context<'_, Self>) {
        self.update_store(cx, GraphStore::select_neighbours);
    }

    pub fn set_pinned(&mut self, pinned: bool, cx: &mut Context<'_, Self>) {
        self.update_store(cx, |g| g.set_flag_on_selected(flags::PINNED, pinned));
    }

    pub fn toggle_vertex_label(&mut self, name: &str, cx: &mut Context<'_, Self>) {
        self.update_store(cx, |g| {
            if let Some(l) = g.vertex_label_ix(name) {
                let hidden = g.is_vertex_label_hidden(l);
                g.set_vertex_label_hidden(l, !hidden);
            }
        });
    }

    pub fn toggle_edge_label(&mut self, name: &str, cx: &mut Context<'_, Self>) {
        self.update_store(cx, |g| {
            if let Some(l) = g.edge_label_ix(name) {
                let hidden = g.is_edge_label_hidden(l);
                g.set_edge_label_hidden(l, !hidden);
            }
        });
    }

    /// Show only vertices matching the filter panel's label and predicate (cleared when both are empty).
    pub fn set_filter(&mut self, filter: Option<Filter>, cx: &mut Context<'_, Self>) {
        self.update_store(cx, |g| g.set_filter(filter));
    }

    fn apply_filter(&mut self, cx: &mut Context<'_, Self>) {
        let key = self.filter_key.read(cx).value().trim().to_owned();
        let value = self.filter_value.read(cx).value().trim().to_owned();
        let filter = if self.filter_label.is_none() && (key.is_empty() || value.is_empty()) {
            None
        } else {
            Some(Filter {
                label: self.filter_label.clone(),
                key: if value.is_empty() { String::new() } else { key },
                op: self.filter_op,
                value,
            })
        };
        if self.store(cx).filter() != filter.as_ref() {
            self.set_filter(filter, cx);
        }
    }

    fn set_filter_label(&mut self, label: Option<String>, cx: &mut Context<'_, Self>) {
        self.filter_label = label;
        self.apply_filter(cx);
    }

    fn set_filter_op(&mut self, op: FilterOp, cx: &mut Context<'_, Self>) {
        self.filter_op = op;
        self.apply_filter(cx);
    }

    fn clear_filter(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.filter_label = None;
        self.filter_key
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.filter_value
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.set_filter(None, cx);
    }

    /// Select and centre the next match for `query`; returns false when nothing matches.
    pub fn find_next(&mut self, query: &str, cx: &mut Context<'_, Self>) -> bool {
        let matches = self.store(cx).find(query, 50);
        if matches != self.matches {
            self.matches = matches;
            self.match_ix = 0;
        } else if !self.matches.is_empty() {
            self.match_ix = (self.match_ix + 1) % self.matches.len();
        }
        let Some(&n) = self.matches.get(self.match_ix) else {
            cx.notify();
            return false;
        };
        self.canvas.update(cx, |c, cx| {
            let g = c.store_mut();
            if g.has_flag(n, flags::HIDDEN) {
                // A hidden match is revealed rather than silently skipped.
                g.set_flag(n, flags::USER_HIDDEN, false);
                g.recompute_visibility();
            }
            g.clear_flag(flags::SELECTED);
            g.set_flag(n, flags::SELECTED, true);
            c.focus_node(n);
            cx.notify();
        });
        self.auto_fit = false;
        cx.notify();
        true
    }

    fn sync_style(&mut self, cx: &mut Context<'_, Self>) {
        let dark = cx.theme().mode.is_dark();
        if self.style_dark != Some(dark) {
            self.style_dark = Some(dark);
            let style = if dark {
                CanvasStyle::dark()
            } else {
                CanvasStyle::light()
            };
            self.canvas.update(cx, |c, cx| {
                c.set_style(style);
                cx.notify();
            });
        }
    }

    // ---- rendering -------------------------------------------------------------------------

    fn render_toolbar(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let stats = self.stats(cx);
        let toggle = |id: &'static str, label: &'static str, on: bool| {
            Button::new(id)
                .label(label)
                .xsmall()
                .when(on, |b| b.primary())
                .when(!on, |b| b.ghost())
        };
        let minimap = self.canvas.read(cx).show_minimap();
        let running = self.is_layout_running(cx);
        let match_text = (!self.matches.is_empty())
            .then(|| format!("{}/{}", self.match_ix + 1, self.matches.len()));
        h_flex()
            .id("graph-toolbar")
            .flex_none()
            .gap(Tokens::space(1.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .child(
                Button::new("graph-fit")
                    .icon(Icon::new(IconName::Maximize))
                    .ghost()
                    .xsmall()
                    .tooltip("Fit to view (F)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.canvas.update(cx, |c, cx| {
                            c.request_fit();
                            cx.notify();
                        })
                    })),
            )
            .child(
                Button::new("graph-zoom-out")
                    .icon(Icon::new(IconName::Minus))
                    .ghost()
                    .xsmall()
                    .tooltip("Zoom out")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.canvas.update(cx, |c, cx| {
                            c.zoom_by(1.0 / 1.25);
                            cx.notify();
                        })
                    })),
            )
            .child(
                Button::new("graph-zoom-in")
                    .icon(Icon::new(IconName::Plus))
                    .ghost()
                    .xsmall()
                    .tooltip("Zoom in")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.canvas.update(cx, |c, cx| {
                            c.zoom_by(1.25);
                            cx.notify();
                        })
                    })),
            )
            .child(
                div()
                    .w(Tokens::BORDER)
                    .h(Tokens::space(4.0))
                    .bg(theme.border),
            )
            .child(self.render_layout_picker(cx))
            .child(self.render_style_picker(cx))
            .child(if running {
                Button::new("graph-layout-stop")
                    .icon(Icon::new(IconName::CircleX))
                    .ghost()
                    .xsmall()
                    .tooltip("Stop layout")
                    .on_click(cx.listener(|this, _, _, cx| this.stop_layout(cx)))
            } else {
                Button::new("graph-layout-run")
                    .icon(Icon::new(IconName::Redo))
                    .ghost()
                    .xsmall()
                    .tooltip("Re-run layout")
                    .on_click(cx.listener(|this, _, _, cx| this.rerun_layout(cx)))
            })
            .child(
                Button::new("graph-undo")
                    .icon(Icon::new(IconName::Undo))
                    .ghost()
                    .xsmall()
                    .disabled(!self.can_undo())
                    .tooltip("Undo the last edit (with preview)")
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                Button::new("graph-redo")
                    .icon(Icon::new(IconName::Redo))
                    .ghost()
                    .xsmall()
                    .disabled(!self.can_redo())
                    .tooltip("Redo (with preview)")
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
            .child(
                div()
                    .w(Tokens::BORDER)
                    .h(Tokens::space(4.0))
                    .bg(theme.border),
            )
            .child(
                toggle("graph-mode-clear", "Clear", !self.accumulate)
                    .tooltip("Each query replaces the graph")
                    .on_click(cx.listener(|this, _, _, cx| this.set_accumulate(false, cx))),
            )
            .child(
                toggle("graph-mode-accumulate", "Accumulate", self.accumulate)
                    .tooltip("Each query adds to the graph")
                    .on_click(cx.listener(|this, _, _, cx| this.set_accumulate(true, cx))),
            )
            .child(
                toggle("graph-edges-between", "Edges between", self.edges_between)
                    .tooltip("After a vertex-only result, also fetch the edges among the vertices")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let on = !this.edges_between;
                        this.set_edges_between(on, cx)
                    })),
            )
            .child(
                div()
                    .w(Tokens::BORDER)
                    .h(Tokens::space(4.0))
                    .bg(theme.border),
            )
            .child(
                toggle("graph-legend-toggle", "Legend", self.show_legend).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.show_legend = !this.show_legend;
                        cx.notify();
                    },
                )),
            )
            .child(
                toggle(
                    "graph-filter-toggle",
                    "Filter",
                    self.show_filter || self.store(cx).filter().is_some(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_filter = !this.show_filter;
                    cx.notify();
                })),
            )
            .child(
                toggle("graph-minimap-toggle", "Minimap", minimap).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.canvas.update(cx, |c, cx| {
                            let on = !c.show_minimap();
                            c.set_show_minimap(on);
                            cx.notify();
                        })
                    },
                )),
            )
            .child(
                div().w(Tokens::size(220.0)).child(
                    Input::new(&self.search)
                        .xsmall()
                        .prefix(Icon::new(IconName::Search).xsmall())
                        .when_some(match_text, |i, t| {
                            i.suffix(
                                div()
                                    .text_size(Tokens::text_xs())
                                    .text_color(theme.muted_foreground)
                                    .child(t),
                            )
                        }),
                ),
            )
            .child(div().flex_1())
            .when(self.pending > 0, |el| {
                el.child(
                    h_flex()
                        .gap(Tokens::space(1.0))
                        .text_size(Tokens::text_xs())
                        .text_color(theme.muted_foreground)
                        .child(Spinner::new().xsmall())
                        .child("Fetching…"),
                )
            })
            .child(
                div()
                    .id("graph-stats")
                    .test_support()
                    .text_size(Tokens::text_xs())
                    .text_color(theme.muted_foreground)
                    .child(stats.text()),
            )
    }

    fn render_style_picker(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        let library = StyleLibrary::global(cx);
        let current = library.read(cx).get_or_default(&self.stylesheet);
        let sheets: Vec<(String, String)> = library
            .read(cx)
            .sheets()
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();
        Button::new("graph-style-picker")
            .label(format!("Style: {}", current.name))
            .ghost()
            .xsmall()
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.label("Stylesheet");
                for (id, name) in &sheets {
                    let view = view.clone();
                    let id = id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(id == current.id)
                            .on_click(move |_, _, cx| {
                                let _ = view.update(cx, |v, cx| v.set_stylesheet(&id, cx));
                            }),
                    );
                }
                let view = view.clone();
                menu.separator()
                    .item(
                        PopupMenuItem::new("Edit stylesheet…").on_click(move |_, window, cx| {
                            let _ =
                                view.update(cx, |v, cx| v.set_show_style_editor(true, window, cx));
                        }),
                    )
            })
    }

    fn render_layout_picker(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        let current = self.layout;
        Button::new("graph-layout-picker")
            .label(format!("Layout: {}", current.kind.name()))
            .ghost()
            .xsmall()
            .dropdown_menu(move |menu, _, _| {
                let item = |label: &'static str, checked: bool, next: LayoutSettings| {
                    let view = view.clone();
                    PopupMenuItem::new(label)
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |v, cx| v.set_layout_settings(next, true, cx));
                        })
                };
                let mut menu = menu.label("Layout");
                for kind in LayoutKind::ALL {
                    menu = menu.item(item(
                        kind.name(),
                        kind == current.kind,
                        LayoutSettings { kind, ..current },
                    ));
                }
                menu = menu.separator().label("Spacing");
                for (label, spacing) in [
                    ("Compact", Spacing::Compact),
                    ("Normal", Spacing::Normal),
                    ("Wide", Spacing::Wide),
                ] {
                    menu = menu.item(item(
                        label,
                        spacing == current.spacing,
                        LayoutSettings { spacing, ..current },
                    ));
                }
                if current.kind == LayoutKind::ForceAtlas2 {
                    menu = menu.separator().label("Gravity");
                    for (label, gravity) in [
                        ("Weak", Gravity::Weak),
                        ("Normal", Gravity::Normal),
                        ("Strong", Gravity::Strong),
                    ] {
                        menu = menu.item(item(
                            label,
                            gravity == current.gravity,
                            LayoutSettings { gravity, ..current },
                        ));
                    }
                }
                menu
            })
    }

    fn render_legend(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let canvas = self.canvas.read(cx);
        let style = canvas.style().clone();
        let g = canvas.store();
        let vertices = g.vertex_label_counts();
        let edges = g.edge_label_counts();
        let row = |prefix: &'static str, lc: &LabelCount, swatch: Hsla, round: bool| {
            let name = lc.name.to_string();
            h_flex()
                .id(SharedString::from(format!("legend-{prefix}-{name}")))
                .test_support()
                .gap(Tokens::space(1.5))
                .px(Tokens::space(1.5))
                .py(Tokens::space(0.5))
                .rounded(Tokens::RADIUS_SM)
                .cursor_pointer()
                .hover(|s| s.bg(theme.accent))
                .when(lc.hidden, |el| el.opacity(0.45))
                .child(
                    div()
                        .size(Tokens::space(2.5))
                        .flex_none()
                        .when(round, |d| d.rounded_full())
                        .when(!round, |d| d.h(Tokens::space(0.75)))
                        .bg(swatch),
                )
                .child(div().flex_1().truncate().child(name))
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(lc.count.to_string()),
                )
                .child(
                    Icon::new(if lc.hidden {
                        IconName::EyeOff
                    } else {
                        IconName::Eye
                    })
                    .xsmall()
                    .text_color(theme.muted_foreground),
                )
        };
        let vertex_rows: Vec<_> = vertices
            .iter()
            .map(|lc| {
                let name = lc.name.to_string();
                row("v", lc, style.label_color(lc.label), true).on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_vertex_label(&name, cx)),
                )
            })
            .collect();
        let edge_rows: Vec<_> = edges
            .iter()
            .map(|lc| {
                let name = lc.name.to_string();
                row("e", lc, style.edge.opacity(1.0), false)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_edge_label(&name, cx)))
            })
            .collect();
        let header = |text: &'static str| {
            div()
                .px(Tokens::space(1.5))
                .pt(Tokens::space(1.0))
                .text_size(Tokens::text_xs())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(text)
        };
        v_flex()
            .id("graph-legend")
            .test_support()
            .absolute()
            .top(Tokens::space(2.0))
            .left(Tokens::space(2.0))
            .w(Tokens::size(200.0))
            .max_h(Tokens::size(320.0))
            .overflow_y_scroll()
            .p(Tokens::space(1.0))
            .gap(Tokens::space(0.25))
            .rounded(Tokens::RADIUS_MD)
            .border(Tokens::BORDER)
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            .text_size(Tokens::text_xs())
            .when(!vertex_rows.is_empty(), |el| {
                el.child(header("VERTICES")).children(vertex_rows)
            })
            .when(!edge_rows.is_empty(), |el| {
                el.child(header("EDGES")).children(edge_rows)
            })
    }

    fn render_filter(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let labels: Vec<String> = self
            .store(cx)
            .vertex_label_counts()
            .into_iter()
            .map(|l| l.name.to_string())
            .collect();
        let label_button = |id: String, text: String, on: bool| {
            Button::new(SharedString::from(id))
                .label(text)
                .xsmall()
                .when(on, |b| b.primary())
                .when(!on, |b| b.ghost())
        };
        let mut label_row = h_flex().flex_wrap().gap(Tokens::space(1.0)).child(
            label_button(
                "filter-label-any".into(),
                "Any label".into(),
                self.filter_label.is_none(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.set_filter_label(None, cx))),
        );
        for name in labels {
            let on = self.filter_label.as_deref() == Some(name.as_str());
            let value = name.clone();
            label_row = label_row.child(
                label_button(format!("filter-label-{name}"), name, on).on_click(cx.listener(
                    move |this, _, _, cx| this.set_filter_label(Some(value.clone()), cx),
                )),
            );
        }
        let ops = h_flex()
            .gap(Tokens::space(0.5))
            .children(FilterOp::ALL.into_iter().map(|op| {
                label_button(
                    format!("filter-op-{op:?}"),
                    op.symbol().into(),
                    op == self.filter_op,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_filter_op(op, cx)))
            }));
        v_flex()
            .id("graph-filter")
            .test_support()
            .absolute()
            .top(Tokens::space(2.0))
            .right(Tokens::space(2.0))
            .w(Tokens::size(300.0))
            .p(Tokens::space(2.0))
            .gap(Tokens::space(2.0))
            .rounded(Tokens::RADIUS_MD)
            .border(Tokens::BORDER)
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            .text_size(Tokens::text_xs())
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Show only vertices where"),
                    )
                    .child(
                        Button::new("filter-clear")
                            .label("Clear")
                            .ghost()
                            .xsmall()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.clear_filter(window, cx)),
                            ),
                    ),
            )
            .child(label_row)
            .child(Input::new(&self.filter_key).xsmall())
            .child(ops)
            .child(Input::new(&self.filter_value).xsmall())
    }

    fn render_more(&self, cx: &mut Context<'_, Self>) -> Option<impl IntoElement> {
        let text = self.more_text()?;
        let theme = cx.theme().clone();
        Some(
            h_flex()
                .id("graph-more")
                .test_support()
                .absolute()
                .bottom(Tokens::space(3.0))
                .left(Tokens::space(3.0))
                .gap(Tokens::space(2.0))
                .px(Tokens::space(2.0))
                .py(Tokens::space(1.0))
                .rounded(Tokens::RADIUS_MD)
                .border(Tokens::BORDER)
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_md()
                .text_size(Tokens::text_xs())
                .child(text)
                .child(
                    Button::new("graph-load-more")
                        .label("Load more")
                        .primary()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
                )
                .child(
                    Button::new("graph-more-dismiss")
                        .icon(Icon::new(IconName::Close))
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.more = None;
                            cx.notify();
                        })),
                ),
        )
    }

    fn render_notice(&self, cx: &mut Context<'_, Self>) -> Option<impl IntoElement> {
        let notice = self.notice.clone()?;
        let theme = cx.theme().clone();
        Some(
            h_flex()
                .id("graph-notice")
                .test_support()
                .absolute()
                .bottom(Tokens::space(3.0))
                .right(Tokens::space(3.0))
                .max_w(Tokens::size(420.0))
                .gap(Tokens::space(2.0))
                .px(Tokens::space(2.0))
                .py(Tokens::space(1.0))
                .rounded(Tokens::RADIUS_MD)
                .bg(theme.warning.opacity(0.15))
                .text_color(theme.warning)
                .text_size(Tokens::text_xs())
                .child(div().flex_1().child(notice))
                .when(self.edges_between_skipped, |el| {
                    el.child(
                        Button::new("graph-fetch-edges")
                            .label("Fetch edges")
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| this.fetch_edges_between(cx))),
                    )
                })
                .child(
                    Button::new("graph-notice-dismiss")
                        .icon(Icon::new(IconName::Close))
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                ),
        )
    }
}

/// Builds the canvas context menu (the node under the pointer was recorded by the canvas on right-click).
fn context_menu(
    view: WeakEntity<GraphView>,
    canvas: Entity<GraphCanvas>,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<'_, PopupMenu>,
) -> PopupMenu {
    let focus = canvas.read(cx).focus_handle(cx);
    let node = canvas.read(cx).context_node();
    let (pinned, edge_labels) = {
        let g = canvas.read(cx).store();
        let labels: Vec<String> = (0..g.edge_label_count())
            .map(|i| g.edge_label_name(graph_store::LabelIx(i as u16)).to_owned())
            .collect();
        (node.is_some_and(|n| g.has_flag(n, flags::PINNED)), labels)
    };
    let offline = view.upgrade().is_some_and(|v| v.read(cx).offline);
    let mut menu = menu.action_context(focus);
    if node.is_some() && offline {
        menu = menu
            .menu("Select neighbours", Box::new(SelectNeighbours))
            .menu("Hide", Box::new(HideSelection))
            .when(pinned, |m| m.menu("Unpin", Box::new(UnpinSelection)))
            .when(!pinned, |m| m.menu("Pin", Box::new(PinSelection)))
            .separator();
    } else if let Some(n) = node {
        let item_view = view.clone();
        let expand_item = move |label: String, direction: ExpandDirection, edge: Option<String>| {
            let view = item_view.clone();
            PopupMenuItem::new(label).on_click(move |_, _, cx| {
                let edge = edge.clone();
                let _ = view.update(cx, |v, cx| v.expand(n, direction, edge, cx));
            })
        };
        let submenu = |direction: ExpandDirection, labels: Vec<String>| {
            let expand_item = expand_item.clone();
            move |mut m: PopupMenu, _: &mut Window, _: &mut Context<'_, PopupMenu>| {
                m = m.item(expand_item("All edges".into(), direction, None));
                if !labels.is_empty() {
                    m = m.separator();
                }
                for l in &labels {
                    m = m.item(expand_item(l.clone(), direction, Some(l.clone())));
                }
                m
            }
        };
        menu = menu
            .item(expand_item(
                "Expand neighbours".into(),
                ExpandDirection::Both,
                None,
            ))
            .submenu(
                "Expand out",
                window,
                cx,
                submenu(ExpandDirection::Out, edge_labels.clone()),
            )
            .submenu(
                "Expand in",
                window,
                cx,
                submenu(ExpandDirection::In, edge_labels),
            )
            .separator()
            .menu("Select neighbours", Box::new(SelectNeighbours))
            .menu("Hide", Box::new(HideSelection))
            .menu("Delete from database…", Box::new(DeleteFromDatabase))
            .when(pinned, |m| m.menu("Unpin", Box::new(UnpinSelection)))
            .when(!pinned, |m| m.menu("Pin", Box::new(PinSelection)))
            .separator()
            .item({
                let view = view.clone();
                PopupMenuItem::new("Copy id").on_click(move |_, _, cx| {
                    if let Some(v) = view.upgrade() {
                        let id = v.read(cx).store(cx).node_ext(n).to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(id));
                    }
                })
            })
            .item({
                let view = view.clone();
                PopupMenuItem::new("Show in table").on_click(move |_, _, cx| {
                    let _ = view.update(cx, |v, cx| {
                        let id = v.store(cx).node_ext(n).to_string();
                        cx.emit(GraphViewEvent::ShowInTable(id));
                    });
                })
            })
            .separator();
    }
    menu.when(!offline, |m| m.menu("New vertex…", Box::new(NewVertex)))
        .menu("Select all", Box::new(SelectAll))
        .menu("Unhide all", Box::new(UnhideAll))
        .menu("Fit to view", Box::new(FitGraph))
}

impl Render for GraphView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        self.sync_style(cx);
        let theme = cx.theme().clone();
        let empty = self.store(cx).node_count() == 0;
        let view = cx.weak_entity();
        let canvas = self.canvas.clone();
        let canvas_area = div()
            .id("graph-canvas")
            .test_support()
            .key_context(CANVAS_CONTEXT)
            .size_full()
            .child(self.canvas.clone())
            .context_menu(move |menu, window, cx| {
                context_menu(view.clone(), canvas.clone(), menu, window, cx)
            });
        v_flex()
            .id("graph-view")
            .test_support()
            .key_context(VIEW_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .on_action(cx.listener(|this, _: &HideSelection, _, cx| this.hide_selection(cx)))
            .on_action(cx.listener(|this, _: &UnhideAll, _, cx| this.unhide_all(cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &SelectNeighbours, _, cx| this.select_neighbours(cx)))
            .on_action(cx.listener(|this, _: &PinSelection, _, cx| this.set_pinned(true, cx)))
            .on_action(cx.listener(|this, _: &UnpinSelection, _, cx| this.set_pinned(false, cx)))
            .on_action(cx.listener(|this, _: &ClearGraph, _, cx| this.clear(cx)))
            .on_action(cx.listener(|this, _: &RerunLayout, _, cx| this.rerun_layout(cx)))
            .on_action(cx.listener(|this, _: &StopLayout, _, cx| this.stop_layout(cx)))
            .on_action(cx.listener(|this, _: &UndoEdit, _, cx| this.undo(cx)))
            .on_action(cx.listener(|this, _: &RedoEdit, _, cx| this.redo(cx)))
            .on_action(cx.listener(|this, _: &DeleteFromDatabase, _, cx| this.delete_selection(cx)))
            .on_action(cx.listener(|this, _: &NewVertex, _, cx| this.start_new_vertex(cx)))
            .on_action(cx.listener(|this, _: &FitGraph, _, cx| {
                this.canvas.update(cx, |c, cx| {
                    c.request_fit();
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &ExpandSelection, _, cx| {
                let selected: Vec<NodeIx> = this.store(cx).selected().take(10).collect();
                for n in selected {
                    this.expand(n, ExpandDirection::Both, None, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FindInGraph, window, cx| {
                this.search.update(cx, |s, cx| s.focus(window, cx));
            }))
            .child(self.render_toolbar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_body(canvas_area, empty, cx))
                    .when_some(
                        self.style_editor.clone().filter(|_| self.show_style_editor),
                        |el, editor| {
                            el.child(
                                div()
                                    .w(Tokens::size(320.0))
                                    .h_full()
                                    .flex_none()
                                    .border_l(Tokens::BORDER)
                                    .border_color(theme.border)
                                    .child(editor),
                            )
                        },
                    ),
            )
    }
}

impl GraphView {
    fn render_body(
        &self,
        canvas_area: impl IntoElement,
        empty: bool,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .relative()
            .flex_1()
            .h_full()
            .min_w_0()
            .child(canvas_area)
            .when(empty, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.muted_foreground)
                        .child("No vertices or edges to draw in this result."),
                )
            })
            .when(self.show_legend && !empty, |el| {
                el.child(self.render_legend(cx))
            })
            .when(self.show_filter, |el| el.child(self.render_filter(cx)))
            .children(self.render_more(cx))
            .children(self.render_notice(cx))
            .children(self.render_edit_overlay(cx))
    }
}

// ---- values and scripts ------------------------------------------------------------------------

/// The store id for a Gremlin id value.
pub fn ext_id(id: &GValue) -> ExtId {
    match id {
        GValue::String(s) => ExtId::Str(s.as_str().into()),
        other => match other.as_i64() {
            Some(n) => ExtId::Int(n),
            None => ExtId::Str(console::format(other).into()),
        },
    }
}

fn prop_value(v: &GValue) -> PropValue {
    match v {
        GValue::Bool(b) => PropValue::Bool(*b),
        GValue::Int32(_) | GValue::Int64(_) | GValue::Short(_) | GValue::Byte(_) => {
            PropValue::Num(v.as_i64().unwrap_or_default() as f64)
        }
        GValue::Float(f) => PropValue::Num(f64::from(*f)),
        GValue::Double(f) => PropValue::Num(*f),
        GValue::String(s) => PropValue::Text(s.as_str().into()),
        other => PropValue::Text(console::format(other).into()),
    }
}

/// A Gremlin literal for an id value with its exact type (`1`, `1L`, `'a'`), or `None` when the type has
/// no portable literal.
pub fn gremlin_literal(id: &GValue) -> Option<String> {
    Some(match id {
        GValue::Int32(n) => n.to_string(),
        GValue::Int64(n) => format!("{n}L"),
        GValue::Short(n) => format!("(short) {n}"),
        GValue::Byte(n) => format!("(byte) {n}"),
        GValue::String(s) => quote(s),
        GValue::Uuid(u) => format!("UUID.fromString('{u}')"),
        _ => return None,
    })
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn steps(direction: ExpandDirection) -> (&'static str, &'static str) {
    match direction {
        ExpandDirection::Both => ("bothE", "otherV"),
        ExpandDirection::Out => ("outE", "inV"),
        ExpandDirection::In => ("inE", "outV"),
    }
}

/// One page of neighbours as `{e, v}` rows; fetches `cap + 1` rows to detect that more exist.
pub fn expand_script(
    id: &str,
    direction: ExpandDirection,
    label: Option<&str>,
    offset: usize,
    cap: usize,
) -> String {
    let (edge, vertex) = steps(direction);
    let label = label.map(quote).unwrap_or_default();
    format!(
        "g.V({id}).{edge}({label}).range({offset}, {}).as('e').{vertex}().as('v').select('e', 'v')",
        offset + cap + 1
    )
}

pub fn count_script(id: &str, direction: ExpandDirection, label: Option<&str>) -> String {
    let (edge, _) = steps(direction);
    let label = label.map(quote).unwrap_or_default();
    format!("g.V({id}).{edge}({label}).count()")
}

#[cfg(test)]
mod tests {
    use ::core::prelude::v1::test;

    use super::*;

    #[test]
    fn literals_keep_the_id_type() {
        assert_eq!(gremlin_literal(&GValue::Int32(1)).as_deref(), Some("1"));
        assert_eq!(gremlin_literal(&GValue::Int64(1)).as_deref(), Some("1L"));
        assert_eq!(
            gremlin_literal(&GValue::String("o'neil".into())).as_deref(),
            Some("'o\\'neil'")
        );
        assert_eq!(gremlin_literal(&GValue::Null), None);
    }

    #[test]
    fn expand_scripts() {
        assert_eq!(
            expand_script("1L", ExpandDirection::Out, Some("route"), 0, 100),
            "g.V(1L).outE('route').range(0, 101).as('e').inV().as('v').select('e', 'v')"
        );
        assert_eq!(
            expand_script("'a'", ExpandDirection::Both, None, 100, 100),
            "g.V('a').bothE().range(100, 201).as('e').otherV().as('v').select('e', 'v')"
        );
        assert_eq!(
            count_script("1", ExpandDirection::In, None),
            "g.V(1).inE().count()"
        );
    }

    #[test]
    fn scripts_parse_as_gremlin() {
        for script in [
            expand_script("1L", ExpandDirection::Both, Some("route"), 0, 100),
            count_script("'x'", ExpandDirection::Out, None),
            "g.V(1, 2L).outE().where(inV().hasId(within(1, 2L)))".to_owned(),
        ] {
            let diagnostics = gremlin_syntax::Document::parse(script.as_str()).diagnostics();
            assert!(diagnostics.is_empty(), "{script}: {diagnostics:?}");
        }
    }

    #[test]
    fn captions_prefer_name_then_code() {
        let props: Props = vec![
            ("code".into(), PropValue::Text("AUS".into())),
            ("desc".into(), PropValue::Text("Austin".into())),
        ];
        assert_eq!(graph_store::style::default_caption(&props, "3"), "AUS");
        assert_eq!(graph_store::style::default_caption(&Props::new(), "3"), "3");
        assert_eq!(ext_id(&GValue::Int32(4)), ExtId::Int(4));
        assert_eq!(
            ext_id(&GValue::String("v1".into())),
            ExtId::Str("v1".into())
        );
    }
}
