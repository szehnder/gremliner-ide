//! Inspecting and editing graph data from the Graph view: typed element details, the change preview
//! (every edit is shown as a Gremlin statement with bindings before it runs), local patches after the
//! server confirms, undo/redo, the Alt-drag edge and the new-vertex form.
//!
//! The view never runs statements itself: a confirmed preview becomes [`GraphViewEvent::Mutate`]; the
//! host adds its own confirmation for production and read-only connections, runs it, and answers with
//! [`GraphView::apply_mutation`].

use std::collections::{HashMap, HashSet};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use graph_store::{EdgeIx, ExtId, NodeIx, flags};
use gremlin_model::edits::{Cardinality, EditCommand, ElementKind, Statement, counts};
use gremlin_model::elements::elements;
use gremlin_model::{Edge, GValue, Property, Vertex, VertexProperty, new_map};
use ui_theme::Tokens;

use super::{FetchKind, GraphView, GraphViewEvent, gremlin_literal, prop_value};
use graph_canvas::LayoutScope;

/// A confirmed change for the host to run.
#[derive(Clone, Debug, PartialEq)]
pub struct MutationRequest {
    pub id: u64,
    pub statement: Statement,
    /// One line for confirmations ("Set age = 30 on vertex 1").
    pub description: String,
    pub destructive: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct UndoEntry {
    command: EditCommand,
    inverse: EditCommand,
}

#[derive(Clone, Debug, PartialEq)]
enum ProposalKind {
    Edit,
    Undo(UndoEntry),
    Redo(UndoEntry),
}

/// A change waiting for the user's confirmation (or for the server).
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub command: EditCommand,
    pub statement: Statement,
    /// The property's values before the edit: `Some(None)` absent, `None` unknown (no undo then).
    before: Option<Option<Vec<GValue>>>,
    kind: ProposalKind,
    /// Vertices and edges a delete removes (edges include those incident to deleted vertices).
    pub counts: Option<(usize, usize)>,
    pub waiting_for_counts: bool,
}

/// What the inspector shows.
#[derive(Clone, Debug, PartialEq)]
pub enum SelectedElement {
    Vertex {
        node: NodeIx,
        id: ExtId,
        label: String,
        detail: Option<Vertex>,
        degree: Option<(i64, i64)>,
    },
    Edge {
        edge: EdgeIx,
        id: ExtId,
        label: String,
        detail: Option<Edge>,
        out_v: (NodeIx, String),
        in_v: (NodeIx, String),
    },
    /// Several elements are selected.
    Many { vertices: usize, edges: usize },
}

/// Edit-related state of a graph view.
pub struct EditState {
    vertices: HashMap<ExtId, Vertex>,
    edges: HashMap<ExtId, Edge>,
    degrees: HashMap<ExtId, (i64, i64)>,
    degree_requested: HashSet<ExtId>,
    proposal: Option<Proposal>,
    in_flight: HashMap<u64, Proposal>,
    next_id: u64,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    /// Alt-drag target waiting for an edge label.
    link: Option<(NodeIx, NodeIx)>,
    link_label: Entity<InputState>,
    new_vertex: bool,
    new_label: Entity<InputState>,
    new_props: Entity<InputState>,
    error: Option<SharedString>,
}

impl EditState {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut App| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            vertices: HashMap::new(),
            edges: HashMap::new(),
            degrees: HashMap::new(),
            degree_requested: HashSet::new(),
            proposal: None,
            in_flight: HashMap::new(),
            next_id: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            link: None,
            link_label: input("Edge label, e.g. created", window, cx),
            new_vertex: false,
            new_label: input("Vertex label, e.g. person", window, cx),
            new_props: input("Properties: name=ann; age=31", window, cx),
            error: None,
        }
    }

    /// Forget element details (the graph was cleared); undo history stays.
    pub fn forget_elements(&mut self) {
        self.vertices.clear();
        self.edges.clear();
        self.degrees.clear();
        self.degree_requested.clear();
        self.link = None;
    }
}

/// Type names offered by the value editor.
pub fn value_type_names() -> [&'static str; 5] {
    ["String", "Int32", "Int64", "Double", "Boolean"]
}

/// Parses `text` as a value of GraphSON type `ty` (one of [`value_type_names`]).
pub fn parse_typed(text: &str, ty: &str) -> Result<GValue, String> {
    let t = text.trim();
    let bad = || format!("{t:?} is not a valid {ty}");
    Ok(match ty {
        "Int32" => GValue::Int32(t.parse().map_err(|_| bad())?),
        "Int64" => GValue::Int64(t.parse().map_err(|_| bad())?),
        "Double" => GValue::Double(t.parse().map_err(|_| bad())?),
        "Boolean" => match t.to_ascii_lowercase().as_str() {
            "true" => GValue::Bool(true),
            "false" => GValue::Bool(false),
            _ => return Err(bad()),
        },
        _ => GValue::String(text.to_owned()),
    })
}

/// `name=ann; age=31` → typed pairs (integers, decimals and booleans are inferred; quotes force text).
fn parse_props(text: &str) -> Result<Vec<(String, GValue)>, String> {
    let mut out = Vec::new();
    for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| format!("{part:?} needs the form key=value"))?;
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() {
            return Err(format!("{part:?} has no key"));
        }
        let value = if let Some(q) = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            GValue::String(q.to_owned())
        } else if let Ok(n) = v.parse::<i32>() {
            GValue::Int32(n)
        } else if let Ok(n) = v.parse::<i64>() {
            GValue::Int64(n)
        } else if let Ok(f) = v.parse::<f64>() {
            GValue::Double(f)
        } else if v == "true" || v == "false" {
            GValue::Bool(v == "true")
        } else {
            GValue::String(v.to_owned())
        };
        out.push((k.to_owned(), value));
    }
    Ok(out)
}

impl GraphView {
    pub(super) fn remember_vertex(&mut self, id: &ExtId, v: &Vertex) {
        let keep = self
            .edit
            .vertices
            .get(id)
            .is_some_and(|old| old.properties.len() > v.properties.len());
        if !keep {
            self.edit.vertices.insert(id.clone(), v.clone());
        }
    }

    pub(super) fn remember_edge(&mut self, id: &ExtId, e: &Edge) {
        self.edit.edges.insert(id.clone(), e.clone());
    }

    /// The single selected element (or a count of several), with typed details when known.
    pub fn selection(&mut self, cx: &mut Context<'_, Self>) -> Option<SelectedElement> {
        let (nodes, edges): (Vec<NodeIx>, Vec<EdgeIx>) = {
            let g = self.store(cx);
            (g.selected().collect(), g.selected_edges().collect())
        };
        match (nodes.as_slice(), edges.as_slice()) {
            ([], []) => None,
            ([n], []) => {
                let g = self.store(cx);
                let id = g.node_ext(*n).clone();
                let label = g.vertex_label_name(g.node_label(*n)).to_owned();
                if !self.edit.degree_requested.contains(&id) {
                    self.request_degree(&id, cx);
                }
                Some(SelectedElement::Vertex {
                    node: *n,
                    detail: self.edit.vertices.get(&id).cloned(),
                    degree: self.edit.degrees.get(&id).copied(),
                    id,
                    label,
                })
            }
            ([], [e]) => {
                let g = self.store(cx);
                let id = g.edge_ext(*e).clone();
                let (a, b) = g.edge_endpoints_of(*e);
                Some(SelectedElement::Edge {
                    edge: *e,
                    label: g.edge_label_name(g.edge_label(*e)).to_owned(),
                    detail: self.edit.edges.get(&id).cloned(),
                    out_v: (a, g.caption(a).to_string()),
                    in_v: (b, g.caption(b).to_string()),
                    id,
                })
            }
            (n, e) => Some(SelectedElement::Many {
                vertices: n.len(),
                edges: e.len(),
            }),
        }
    }

    fn request_degree(&mut self, id: &ExtId, cx: &mut Context<'_, Self>) {
        self.edit.degree_requested.insert(id.clone());
        if let Some(lit) = self.literal(id) {
            self.emit_fetch(
                FetchKind::Degree(id.clone()),
                format!("g.V({lit}).project('in', 'out').by(inE().count()).by(outE().count())"),
                cx,
            );
        }
    }

    pub(super) fn apply_degree(&mut self, node: &ExtId, items: &[GValue]) {
        if let Some(GValue::Map(m)) = items.first() {
            let get = |k: &str| {
                m.get(&gremlin_model::GKey::String(k.into()))
                    .and_then(GValue::as_i64)
                    .unwrap_or(0)
            };
            self.edit
                .degrees
                .insert(node.clone(), (get("in"), get("out")));
        }
    }

    /// Select node `n` alone and centre it (inspector endpoint links).
    pub fn select_and_focus(&mut self, n: NodeIx, cx: &mut Context<'_, Self>) {
        self.canvas.update(cx, |c, cx| {
            let g = c.store_mut();
            g.clear_selection();
            g.set_flag(n, flags::SELECTED, true);
            c.focus_node(n);
            cx.notify();
        });
        cx.notify();
    }

    /// The original id value of an element (for bindings).
    pub fn id_value(&self, id: &ExtId) -> Option<GValue> {
        self.ids.get(id).cloned().or_else(|| match id {
            ExtId::Int(n) => Some(GValue::Int64(*n)),
            ExtId::Str(s) => Some(GValue::String(s.to_string())),
        })
    }

    // ---- proposals -----------------------------------------------------------------------------

    pub fn proposal(&self) -> Option<&Proposal> {
        self.edit.proposal.as_ref()
    }

    pub fn edit_error(&self) -> Option<&SharedString> {
        self.edit.error.as_ref()
    }

    /// Confirmed changes still waiting for the server.
    pub fn edit_in_flight(&self) -> usize {
        self.edit.in_flight.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.edit.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.edit.redo.is_empty()
    }

    fn before_values(&self, command: &EditCommand) -> Option<Option<Vec<GValue>>> {
        let (element, key) = match command {
            EditCommand::SetProperty { element, key, .. }
            | EditCommand::RemoveProperty { element, key } => (element, key),
            _ => return Some(None),
        };
        let ext = super::ext_id(&element.id);
        match element.kind {
            ElementKind::Vertex => {
                let v = self.edit.vertices.get(&ext)?;
                Some(
                    v.properties
                        .get(key)
                        .map(|vps| vps.iter().map(|vp| (*vp.value).clone()).collect()),
                )
            }
            ElementKind::Edge => {
                let e = self.edit.edges.get(&ext)?;
                Some(e.properties.get(key).map(|p| vec![(*p.value).clone()]))
            }
        }
    }

    /// Show the preview for `command`.
    pub fn propose(&mut self, command: EditCommand, cx: &mut Context<'_, Self>) {
        self.propose_kind(command, ProposalKind::Edit, cx);
    }

    fn propose_kind(
        &mut self,
        command: EditCommand,
        kind: ProposalKind,
        cx: &mut Context<'_, Self>,
    ) {
        if self.offline {
            return;
        }
        let id = self.edit.next_id;
        self.edit.next_id += 1;
        let before = self.before_values(&command);
        let mut proposal = Proposal {
            id,
            statement: command.statement(),
            command,
            before,
            kind,
            counts: None,
            waiting_for_counts: false,
        };
        if let EditCommand::DropElements { vertices, edges } = &proposal.command {
            // Count incident edges on the server: the graph may show only some of them.
            let lits: Vec<String> = vertices.iter().filter_map(gremlin_literal).collect();
            if lits.is_empty() {
                proposal.counts = Some((0, edges.len()));
            } else {
                proposal.waiting_for_counts = true;
                let script = format!("g.V({}).bothE().dedup().id()", lits.join(", "));
                self.emit_fetch(FetchKind::IncidentCount(id), script, cx);
            }
        }
        self.edit.error = None;
        self.edit.proposal = Some(proposal);
        cx.notify();
    }

    pub(super) fn apply_incident_count(&mut self, proposal: u64, items: &[GValue]) {
        let Some(p) = self.edit.proposal.as_mut().filter(|p| p.id == proposal) else {
            return;
        };
        if let EditCommand::DropElements { vertices, edges } = &p.command {
            let incident: HashSet<String> =
                items.iter().map(gremlin_model::console::format).collect();
            let extra = edges
                .iter()
                .filter(|e| !incident.contains(&gremlin_model::console::format(e)))
                .count();
            p.counts = Some((vertices.len(), incident.len() + extra));
            p.waiting_for_counts = false;
        }
    }

    pub fn cancel_proposal(&mut self, cx: &mut Context<'_, Self>) {
        self.edit.proposal = None;
        self.edit.link = None;
        cx.notify();
    }

    /// The user confirmed the preview: hand the statement to the host.
    pub fn confirm_proposal(&mut self, cx: &mut Context<'_, Self>) {
        let Some(p) = self.edit.proposal.take() else {
            return;
        };
        if p.waiting_for_counts {
            self.edit.proposal = Some(p);
            return;
        }
        let request = MutationRequest {
            id: p.id,
            statement: p.statement.clone(),
            description: p.command.describe(),
            destructive: p.command.is_destructive(),
        };
        self.edit.in_flight.insert(p.id, p);
        cx.emit(GraphViewEvent::Mutate(request));
        cx.notify();
    }

    /// The host's answer to a [`MutationRequest`]: on success the graph is patched locally and the edit
    /// is recorded for undo; on failure nothing changes and the error is shown.
    pub fn apply_mutation(
        &mut self,
        id: u64,
        result: Result<Vec<GValue>, String>,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(p) = self.edit.in_flight.remove(&id) else {
            return;
        };
        let items = match result {
            Ok(items) => items,
            Err(message) => {
                self.edit.error =
                    Some(format!("{} failed: {message}", p.command.describe()).into());
                cx.notify();
                return;
            }
        };
        self.edit.error = None;
        let created = self.patch(&p.command, &items, cx);
        let entry = match (&p.kind, &p.before) {
            (ProposalKind::Undo(e) | ProposalKind::Redo(e), _) => Some(e.clone()),
            (ProposalKind::Edit, before) => {
                let inverse = match (&created, before) {
                    (Some(new_id), _) => p.command.undo_creation(new_id),
                    (None, Some(before)) => p.command.inverse(before.as_deref()),
                    (None, None) => None,
                };
                inverse.map(|inverse| UndoEntry {
                    command: p.command.clone(),
                    inverse,
                })
            }
        };
        match (&p.kind, entry) {
            (ProposalKind::Edit, Some(e)) => {
                self.edit.undo.push(e);
                self.edit.redo.clear();
            }
            (ProposalKind::Undo(_), Some(e)) => self.edit.redo.push(e),
            (ProposalKind::Redo(_), Some(e)) => self.edit.undo.push(e),
            _ => {}
        }
        self.restyle(cx);
        cx.notify();
    }

    /// Preview undoing the last edit.
    pub fn undo(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(e) = self.edit.undo.pop() {
            let inverse = e.inverse.clone();
            self.propose_kind(inverse, ProposalKind::Undo(e), cx);
        }
    }

    /// Preview redoing the last undone edit.
    pub fn redo(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(e) = self.edit.redo.pop() {
            let command = e.command.clone();
            self.propose_kind(command, ProposalKind::Redo(e), cx);
        }
    }

    /// Apply a successful command to the local graph. Returns the id of a created element.
    fn patch(
        &mut self,
        command: &EditCommand,
        items: &[GValue],
        cx: &mut Context<'_, Self>,
    ) -> Option<GValue> {
        match command {
            EditCommand::SetProperty {
                element,
                key,
                value,
                cardinality,
            } => {
                let ext = super::ext_id(&element.id);
                match element.kind {
                    ElementKind::Vertex => {
                        let values: Vec<GValue> = {
                            let v =
                                self.edit
                                    .vertices
                                    .entry(ext.clone())
                                    .or_insert_with(|| Vertex {
                                        id: Box::new(element.id.clone()),
                                        label: String::new(),
                                        properties: new_map(),
                                    });
                            let vps = v.properties.entry(key.clone()).or_default();
                            let vp = VertexProperty {
                                id: Box::new(GValue::Null),
                                label: key.clone(),
                                value: Box::new(value.clone()),
                                properties: new_map(),
                            };
                            match cardinality {
                                Cardinality::Single => *vps = vec![vp],
                                Cardinality::List => vps.push(vp),
                                Cardinality::Set => {
                                    if !vps.iter().any(|x| *x.value == *value) {
                                        vps.push(vp);
                                    }
                                }
                            }
                            vps.iter().map(|vp| (*vp.value).clone()).collect()
                        };
                        let pv = values.iter().map(prop_value).collect();
                        self.update_store(cx, |g| {
                            if let Some(n) = g.node_by_ext(&ext) {
                                g.set_node_prop(n, key, Some(pv));
                            }
                        });
                    }
                    ElementKind::Edge => {
                        if let Some(e) = self.edit.edges.get_mut(&ext) {
                            e.properties.insert(
                                key.clone(),
                                Property {
                                    key: key.clone(),
                                    value: Box::new(value.clone()),
                                },
                            );
                        }
                        let pv = prop_value(value);
                        self.update_store(cx, |g| {
                            if let Some(e) = g.edge_by_ext(&ext) {
                                g.set_edge_prop(e, key, Some(pv));
                            }
                        });
                    }
                }
                None
            }
            EditCommand::RemoveProperty { element, key } => {
                let ext = super::ext_id(&element.id);
                match element.kind {
                    ElementKind::Vertex => {
                        if let Some(v) = self.edit.vertices.get_mut(&ext) {
                            v.properties.shift_remove(key);
                        }
                        self.update_store(cx, |g| {
                            if let Some(n) = g.node_by_ext(&ext) {
                                g.set_node_prop(n, key, None);
                            }
                        });
                    }
                    ElementKind::Edge => {
                        if let Some(e) = self.edit.edges.get_mut(&ext) {
                            e.properties.shift_remove(key);
                        }
                        self.update_store(cx, |g| {
                            if let Some(e) = g.edge_by_ext(&ext) {
                                g.set_edge_prop(e, key, None);
                            }
                        });
                    }
                }
                None
            }
            EditCommand::AddVertex { .. } | EditCommand::AddEdge { .. } => {
                let set = elements(items);
                let created = set
                    .vertices
                    .first()
                    .map(|v| (*v.id).clone())
                    .or_else(|| set.edges.first().map(|e| (*e.id).clone()));
                let report = self.merge_set(set, cx);
                if !report.added_nodes.is_empty() {
                    let nodes = report.added_nodes.clone();
                    self.relayout(LayoutScope::Only(nodes), cx);
                }
                created
            }
            EditCommand::DropElements { vertices, edges } => {
                let vs: Vec<ExtId> = vertices.iter().map(super::ext_id).collect();
                let es: Vec<ExtId> = edges.iter().map(super::ext_id).collect();
                for v in &vs {
                    self.edit.vertices.remove(v);
                    self.edit.degrees.remove(v);
                }
                for e in &es {
                    self.edit.edges.remove(e);
                }
                self.canvas.update(cx, |c, cx| {
                    c.stop_layout(cx);
                    let g = c.store_mut();
                    let nodes: Vec<NodeIx> = vs.iter().filter_map(|v| g.node_by_ext(v)).collect();
                    let edges: Vec<EdgeIx> = es.iter().filter_map(|e| g.edge_by_ext(e)).collect();
                    g.remove(&nodes, &edges);
                    c.forget_indices();
                    cx.notify();
                });
                self.matches.clear();
                None
            }
        }
    }

    // ---- delete, new vertex, link --------------------------------------------------------------

    /// Preview deleting the selected vertices and edges.
    pub fn delete_selection(&mut self, cx: &mut Context<'_, Self>) {
        let (vertices, edges) = {
            let g = self.store(cx);
            let v: Vec<ExtId> = g.selected().map(|n| g.node_ext(n).clone()).collect();
            let e: Vec<ExtId> = g.selected_edges().map(|e| g.edge_ext(e).clone()).collect();
            (v, e)
        };
        if vertices.is_empty() && edges.is_empty() {
            return;
        }
        let vertices = vertices.iter().filter_map(|v| self.id_value(v)).collect();
        let edges = edges.iter().filter_map(|e| self.id_value(e)).collect();
        self.propose(EditCommand::DropElements { vertices, edges }, cx);
    }

    /// Ask for the label of an edge from `from` to `to` (after an Alt-drag).
    pub fn start_link(&mut self, from: NodeIx, to: NodeIx, cx: &mut Context<'_, Self>) {
        if self.offline {
            return;
        }
        self.edit.link = Some((from, to));
        self.edit.proposal = None;
        cx.notify();
    }

    pub fn link_label_input(&self) -> &Entity<InputState> {
        &self.edit.link_label
    }

    fn propose_link(&mut self, cx: &mut Context<'_, Self>) {
        let Some((from, to)) = self.edit.link.take() else {
            return;
        };
        let label = self.edit.link_label.read(cx).value().trim().to_owned();
        if label.is_empty() {
            self.edit.link = Some((from, to));
            self.edit.error = Some("Choose an edge label.".into());
            cx.notify();
            return;
        }
        let (a, b) = {
            let g = self.store(cx);
            (g.node_ext(from).clone(), g.node_ext(to).clone())
        };
        let (Some(from), Some(to)) = (self.id_value(&a), self.id_value(&b)) else {
            return;
        };
        self.propose(
            EditCommand::AddEdge {
                from,
                to,
                label,
                props: vec![],
            },
            cx,
        );
    }

    /// Open the new-vertex form.
    pub fn start_new_vertex(&mut self, cx: &mut Context<'_, Self>) {
        if self.offline {
            return;
        }
        self.edit.new_vertex = true;
        self.edit.proposal = None;
        cx.notify();
    }

    pub fn new_vertex_inputs(&self) -> (&Entity<InputState>, &Entity<InputState>) {
        (&self.edit.new_label, &self.edit.new_props)
    }

    fn propose_new_vertex(&mut self, cx: &mut Context<'_, Self>) {
        let label = self.edit.new_label.read(cx).value().trim().to_owned();
        let props = self.edit.new_props.read(cx).value().to_string();
        if label.is_empty() {
            self.edit.error = Some("Choose a vertex label.".into());
            cx.notify();
            return;
        }
        match parse_props(&props) {
            Ok(props) => {
                self.edit.new_vertex = false;
                self.propose(EditCommand::AddVertex { label, props }, cx);
            }
            Err(e) => {
                self.edit.error = Some(e.into());
                cx.notify();
            }
        }
    }

    // ---- rendering -------------------------------------------------------------------------------

    /// The preview card, link-label prompt or new-vertex form, centred over the canvas.
    pub(super) fn render_edit_overlay(&self, cx: &mut Context<'_, Self>) -> Option<AnyElement> {
        let theme = cx.theme().clone();
        let card = |id: &'static str| {
            v_flex()
                .id(id)
                .test_support()
                .w(Tokens::size(460.0))
                .max_w_full()
                .p(Tokens::space(3.0))
                .gap(Tokens::space(2.0))
                .rounded(Tokens::RADIUS_LG)
                .border(Tokens::BORDER)
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_lg()
                .text_size(Tokens::text_sm())
        };
        let error = self.edit.error.clone().map(|e| {
            div()
                .id("edit-error")
                .test_support()
                .text_color(theme.danger)
                .text_size(Tokens::text_xs())
                .child(e)
        });
        let body: AnyElement = if let Some(p) = &self.edit.proposal {
            let title = match p.kind {
                ProposalKind::Undo(_) => format!("Undo: {}", p.command.describe()),
                ProposalKind::Redo(_) => format!("Redo: {}", p.command.describe()),
                ProposalKind::Edit => p.command.describe(),
            };
            let destructive = p.command.is_destructive();
            let counts_text = p
                .counts
                .map(|(v, e)| format!("This removes {} from the database.", counts(v, e)));
            card("edit-preview")
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .when(p.waiting_for_counts, |el| {
                    el.child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child("Counting incident edges…"),
                    )
                })
                .children(counts_text.map(|t| {
                    div()
                        .id("edit-counts")
                        .test_support()
                        .text_color(theme.warning)
                        .child(t)
                }))
                .child(
                    div()
                        .id("edit-statement")
                        .test_support()
                        .p(Tokens::space(2.0))
                        .rounded(Tokens::RADIUS_SM)
                        .bg(theme.muted)
                        .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                        .text_size(Tokens::text_xs())
                        .whitespace_normal()
                        .child(p.statement.preview()),
                )
                .children(error)
                .child(
                    h_flex()
                        .justify_end()
                        .gap(Tokens::space(2.0))
                        .child(
                            Button::new("edit-cancel")
                                .label("Cancel")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_proposal(cx))),
                        )
                        .child(
                            Button::new("edit-run")
                                .label("Run")
                                .small()
                                .when(destructive, |b| b.danger())
                                .when(!destructive, |b| b.primary())
                                .on_click(cx.listener(|this, _, _, cx| this.confirm_proposal(cx))),
                        ),
                )
                .into_any_element()
        } else if let Some((from, to)) = self.edit.link {
            let g = self.store(cx);
            let (a, b) = (g.caption(from).to_string(), g.caption(to).to_string());
            card("edit-link")
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("New edge from {a} to {b}")),
                )
                .child(Input::new(&self.edit.link_label).small())
                .children(error)
                .child(
                    h_flex()
                        .justify_end()
                        .gap(Tokens::space(2.0))
                        .child(
                            Button::new("edit-link-cancel")
                                .label("Cancel")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_proposal(cx))),
                        )
                        .child(
                            Button::new("edit-link-next")
                                .label("Preview")
                                .primary()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.propose_link(cx))),
                        ),
                )
                .into_any_element()
        } else if self.edit.new_vertex {
            card("edit-new-vertex")
                .child(div().font_weight(FontWeight::SEMIBOLD).child("New vertex"))
                .child(Input::new(&self.edit.new_label).small())
                .child(Input::new(&self.edit.new_props).small())
                .children(error)
                .child(
                    h_flex()
                        .justify_end()
                        .gap(Tokens::space(2.0))
                        .child(
                            Button::new("edit-new-cancel")
                                .label("Cancel")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.edit.new_vertex = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("edit-new-next")
                                .label("Preview")
                                .primary()
                                .small()
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.propose_new_vertex(cx)),
                                ),
                        ),
                )
                .into_any_element()
        } else {
            // A failed change: show the error until dismissed.
            let e = error?;
            card("edit-failed")
                .child(e)
                .child(
                    h_flex().justify_end().child(
                        Button::new("edit-error-dismiss")
                            .label("Dismiss")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit.error = None;
                                cx.notify();
                            })),
                    ),
                )
                .into_any_element()
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background.opacity(0.35))
                .child(body)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use ::core::prelude::v1::test;

    use super::*;

    #[test]
    fn typed_values_and_property_lists() {
        assert_eq!(parse_typed("30", "Int32"), Ok(GValue::Int32(30)));
        assert_eq!(parse_typed("30", "Int64"), Ok(GValue::Int64(30)));
        assert!(parse_typed("x", "Int32").is_err());
        assert_eq!(parse_typed("TRUE", "Boolean"), Ok(GValue::Bool(true)));
        assert_eq!(
            parse_typed(" a ", "String"),
            Ok(GValue::String(" a ".into()))
        );
        assert_eq!(
            parse_props(r#"name=ann; age=31; score=1.5; ok=true; code="7""#),
            Ok(vec![
                ("name".into(), GValue::String("ann".into())),
                ("age".into(), GValue::Int32(31)),
                ("score".into(), GValue::Double(1.5)),
                ("ok".into(), GValue::Bool(true)),
                ("code".into(), GValue::String("7".into())),
            ])
        );
        assert!(parse_props("novalue").is_err());
        assert_eq!(parse_props("  "), Ok(vec![]));
    }
}
