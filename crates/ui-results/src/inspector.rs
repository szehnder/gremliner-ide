//! The inspector: the selected vertex or edge of the active graph with typed properties (multi- and
//! meta-properties), degree and endpoints, plus inline editing. Every change goes through the graph
//! view's preview.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use graph_store::ExtId;
use gremlin_model::edits::{Cardinality, EditCommand, ElementRef};
use gremlin_model::{GValue, console};
use ui_theme::Tokens;

use crate::graph_view::{GraphView, SelectedElement, parse_typed, value_type_names};

/// One property value as the inspector lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorRow {
    pub key: String,
    /// Position among the key's values (multi-properties).
    pub index: usize,
    pub value: String,
    pub type_name: String,
    /// Meta-properties of this value: (key, value, type).
    pub meta: Vec<(String, String, String)>,
    raw: GValue,
}

pub struct Inspector {
    graph: Option<Entity<GraphView>>,
    editing: Option<(String, usize)>,
    edit_value: Entity<InputState>,
    edit_type: &'static str,
    add_key: Entity<InputState>,
    add_value: Entity<InputState>,
    add_type: &'static str,
    add_cardinality: Cardinality,
    error: Option<SharedString>,
    _graph_subscription: Option<Subscription>,
}

impl Inspector {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        Self {
            graph: None,
            editing: None,
            edit_value: cx.new(|cx| InputState::new(window, cx)),
            edit_type: "String",
            add_key: cx.new(|cx| InputState::new(window, cx).placeholder("key")),
            add_value: cx.new(|cx| InputState::new(window, cx).placeholder("value")),
            add_type: "String",
            add_cardinality: Cardinality::Single,
            error: None,
            _graph_subscription: None,
        }
    }

    /// Inspect `graph`'s selection from now on.
    pub fn set_graph(&mut self, graph: Entity<GraphView>, cx: &mut Context<'_, Self>) {
        if self.graph.as_ref() == Some(&graph) {
            return;
        }
        self._graph_subscription = Some(cx.observe(&graph, |_, _, cx| cx.notify()));
        self.graph = Some(graph);
        self.editing = None;
        cx.notify();
    }

    pub fn graph(&self) -> Option<&Entity<GraphView>> {
        self.graph.as_ref()
    }

    pub fn edit_value_input(&self) -> &Entity<InputState> {
        &self.edit_value
    }

    pub fn add_inputs(&self) -> (&Entity<InputState>, &Entity<InputState>) {
        (&self.add_key, &self.add_value)
    }

    fn selection(&self, cx: &mut Context<'_, Self>) -> Option<SelectedElement> {
        self.graph.clone()?.update(cx, |g, cx| g.selection(cx))
    }

    /// The selected element's property rows, in display order.
    pub fn rows(&self, cx: &mut Context<'_, Self>) -> Vec<InspectorRow> {
        let row = |key: &str, index: usize, v: &GValue, meta: Vec<(String, String, String)>| {
            InspectorRow {
                key: key.to_owned(),
                index,
                value: console::format(v),
                type_name: v.type_name().to_owned(),
                meta,
                raw: v.clone(),
            }
        };
        match self.selection(cx) {
            Some(SelectedElement::Vertex {
                detail: Some(v), ..
            }) => v
                .properties
                .iter()
                .flat_map(|(k, vps)| {
                    vps.iter().enumerate().map(move |(i, vp)| {
                        let meta = vp
                            .properties
                            .iter()
                            .map(|(mk, mv)| {
                                (mk.clone(), console::format(mv), mv.type_name().to_owned())
                            })
                            .collect();
                        row(k, i, &vp.value, meta)
                    })
                })
                .collect(),
            Some(SelectedElement::Edge {
                detail: Some(e), ..
            }) => e
                .properties
                .iter()
                .map(|(k, p)| row(k, 0, &p.value, Vec::new()))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn element(&self, cx: &mut Context<'_, Self>) -> Option<ElementRef> {
        let graph = self.graph.clone()?;
        let id_value =
            |graph: &Entity<GraphView>, id: &ExtId, cx: &App| graph.read(cx).id_value(id);
        match self.selection(cx)? {
            SelectedElement::Vertex { id, .. } => {
                Some(ElementRef::vertex(id_value(&graph, &id, cx)?))
            }
            SelectedElement::Edge { id, .. } => Some(ElementRef::edge(id_value(&graph, &id, cx)?)),
            SelectedElement::Many { .. } => None,
        }
    }

    fn propose(&mut self, command: EditCommand, cx: &mut Context<'_, Self>) {
        if let Some(graph) = self.graph.clone() {
            graph.update(cx, |g, cx| g.propose(command, cx));
        }
    }

    pub fn start_edit(
        &mut self,
        row: &InspectorRow,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.editing = Some((row.key.clone(), row.index));
        self.edit_type = value_type_names()
            .into_iter()
            .find(|t| *t == row.type_name)
            .unwrap_or("String");
        let text = match &row.raw {
            GValue::String(s) => s.clone(),
            other => console::format(other),
        };
        self.edit_value
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.error = None;
        cx.notify();
    }

    /// Preview setting the edited value (single cardinality).
    pub fn save_edit(&mut self, cx: &mut Context<'_, Self>) {
        let Some((key, _)) = self.editing.clone() else {
            return;
        };
        let text = self.edit_value.read(cx).value().to_string();
        match (parse_typed(&text, self.edit_type), self.element(cx)) {
            (Ok(value), Some(element)) => {
                self.editing = None;
                self.error = None;
                self.propose(
                    EditCommand::SetProperty {
                        element,
                        key,
                        value,
                        cardinality: Cardinality::Single,
                    },
                    cx,
                );
            }
            (Err(e), _) => self.error = Some(e.into()),
            (_, None) => {}
        }
        cx.notify();
    }

    fn add_property(&mut self, cx: &mut Context<'_, Self>) {
        let key = self.add_key.read(cx).value().trim().to_owned();
        let text = self.add_value.read(cx).value().to_string();
        if key.is_empty() {
            self.error = Some("Enter a property key.".into());
            cx.notify();
            return;
        }
        match (parse_typed(&text, self.add_type), self.element(cx)) {
            (Ok(value), Some(element)) => {
                self.error = None;
                self.propose(
                    EditCommand::SetProperty {
                        element,
                        key,
                        value,
                        cardinality: self.add_cardinality,
                    },
                    cx,
                );
            }
            (Err(e), _) => self.error = Some(e.into()),
            (_, None) => {}
        }
        cx.notify();
    }

    fn remove_property(&mut self, key: String, cx: &mut Context<'_, Self>) {
        if let Some(element) = self.element(cx) {
            self.propose(EditCommand::RemoveProperty { element, key }, cx);
        }
    }

    fn render_rows(&self, rows: Vec<InspectorRow>, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut list = v_flex().gap(Tokens::space(0.5));
        for row in rows {
            let editing = self.editing.as_ref() == Some(&(row.key.clone(), row.index));
            let id = format!("inspector-prop-{}-{}", row.key, row.index);
            if editing {
                let types =
                    h_flex()
                        .gap(Tokens::space(0.5))
                        .children(value_type_names().into_iter().map(|t| {
                            let on = t == self.edit_type;
                            Button::new(SharedString::from(format!("inspector-type-{t}")))
                                .label(t)
                                .xsmall()
                                .when(on, |b| b.primary())
                                .when(!on, |b| b.ghost())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit_type = t;
                                    cx.notify();
                                }))
                        }));
                list = list.child(
                    v_flex()
                        .id(SharedString::from(id))
                        .test_support()
                        .gap(Tokens::space(1.0))
                        .p(Tokens::space(1.0))
                        .rounded(Tokens::RADIUS_SM)
                        .bg(theme.muted)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(row.key.clone()),
                        )
                        .child(Input::new(&self.edit_value).xsmall())
                        .child(types)
                        .child(
                            h_flex()
                                .justify_end()
                                .gap(Tokens::space(1.0))
                                .child(
                                    Button::new("inspector-edit-cancel")
                                        .label("Cancel")
                                        .ghost()
                                        .xsmall()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.editing = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("inspector-edit-save")
                                        .label("Preview")
                                        .primary()
                                        .xsmall()
                                        .on_click(cx.listener(|this, _, _, cx| this.save_edit(cx))),
                                ),
                        ),
                );
                continue;
            }
            let key = row.key.clone();
            let edit_row = row.clone();
            list =
                list.child(
                    v_flex()
                        .id(SharedString::from(id))
                        .test_support()
                        .px(Tokens::space(1.0))
                        .py(Tokens::space(0.5))
                        .rounded(Tokens::RADIUS_SM)
                        .hover(|s| s.bg(theme.accent))
                        .child(
                            h_flex()
                                .gap(Tokens::space(1.5))
                                .child(
                                    div()
                                        .w(Tokens::size(84.0))
                                        .flex_none()
                                        .truncate()
                                        .text_color(theme.muted_foreground)
                                        .child(row.key.clone()),
                                )
                                .child(div().flex_1().min_w_0().truncate().child(row.value.clone()))
                                .child(
                                    div()
                                        .text_size(Tokens::text_xs())
                                        .text_color(theme.muted_foreground)
                                        .child(row.type_name.clone()),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "inspector-edit-{}-{}",
                                        row.key, row.index
                                    )))
                                    .icon(Icon::new(IconName::Settings))
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Edit value")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_edit(&edit_row, window, cx)
                                    })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "inspector-remove-{}-{}",
                                        row.key, row.index
                                    )))
                                    .icon(Icon::new(IconName::Delete))
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Remove property")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_property(key.clone(), cx)
                                    })),
                                ),
                        )
                        .children(row.meta.iter().map(|(mk, mv, mt)| {
                            h_flex()
                                .pl(Tokens::space(4.0))
                                .gap(Tokens::space(1.5))
                                .text_size(Tokens::text_xs())
                                .text_color(theme.muted_foreground)
                                .child(format!("↳ {mk}"))
                                .child(div().text_color(theme.foreground).child(mv.clone()))
                                .child(mt.clone())
                        })),
                );
        }
        list.into_any_element()
    }

    fn render_add(&self, vertex: bool, cx: &mut Context<'_, Self>) -> AnyElement {
        let types = h_flex().flex_wrap().gap(Tokens::space(0.5)).children(
            value_type_names().into_iter().map(|t| {
                let on = t == self.add_type;
                Button::new(SharedString::from(format!("inspector-add-type-{t}")))
                    .label(t)
                    .xsmall()
                    .when(on, |b| b.primary())
                    .when(!on, |b| b.ghost())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.add_type = t;
                        cx.notify();
                    }))
            }),
        );
        let cards = h_flex().gap(Tokens::space(0.5)).children(
            [Cardinality::Single, Cardinality::List, Cardinality::Set]
                .into_iter()
                .map(|c| {
                    let on = c == self.add_cardinality;
                    Button::new(SharedString::from(format!("inspector-card-{}", c.token())))
                        .label(c.token())
                        .xsmall()
                        .when(on, |b| b.primary())
                        .when(!on, |b| b.ghost())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.add_cardinality = c;
                            cx.notify();
                        }))
                }),
        );
        v_flex()
            .gap(Tokens::space(1.0))
            .child(
                div()
                    .text_size(Tokens::text_xs())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("ADD PROPERTY"),
            )
            .child(
                h_flex()
                    .gap(Tokens::space(1.0))
                    .child(div().flex_1().child(Input::new(&self.add_key).xsmall()))
                    .child(div().flex_1().child(Input::new(&self.add_value).xsmall())),
            )
            .child(types)
            .when(vertex, |el| el.child(cards))
            .child(
                h_flex().justify_end().child(
                    Button::new("inspector-add")
                        .label("Preview")
                        .primary()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.add_property(cx))),
                ),
            )
            .into_any_element()
    }
}

impl Render for Inspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selection = self.selection(cx);
        let rows = self.rows(cx);
        let header = |label: String, id: String, kind: &'static str| {
            v_flex()
                .gap(Tokens::space(0.5))
                .child(
                    h_flex()
                        .gap(Tokens::space(1.5))
                        .child(
                            div()
                                .px(Tokens::space(1.5))
                                .rounded(Tokens::RADIUS_SM)
                                .bg(theme.accent)
                                .text_size(Tokens::text_xs())
                                .child(kind),
                        )
                        .child(
                            div()
                                .id("inspector-label")
                                .test_support()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(label),
                        ),
                )
                .child(
                    div()
                        .id("inspector-id")
                        .test_support()
                        .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                        .text_size(Tokens::text_xs())
                        .text_color(theme.muted_foreground)
                        .child(format!("id {id}")),
                )
        };
        let error = self.error.clone().map(|e| {
            div()
                .text_size(Tokens::text_xs())
                .text_color(theme.danger)
                .child(e)
        });
        let delete = |label: &'static str, cx: &mut Context<'_, Self>| {
            Button::new("inspector-delete")
                .label(label)
                .danger()
                .xsmall()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(g) = this.graph.clone() {
                        g.update(cx, |g, cx| g.delete_selection(cx));
                    }
                }))
        };
        let body: AnyElement = match selection {
            None => v_flex()
                .gap(Tokens::space(2.0))
                .items_center()
                .pt(Tokens::space(8.0))
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Info))
                .child("Select a vertex or edge to see its properties.")
                .when(self.graph.is_some(), |el| {
                    el.child(
                        Button::new("inspector-new-vertex")
                            .label("New vertex…")
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(g) = this.graph.clone() {
                                    g.update(cx, |g, cx| g.start_new_vertex(cx));
                                }
                            })),
                    )
                })
                .into_any_element(),
            Some(SelectedElement::Many { vertices, edges }) => v_flex()
                .gap(Tokens::space(2.0))
                .child(format!(
                    "{} selected",
                    gremlin_model::edits::counts(vertices, edges)
                ))
                .child(h_flex().child(delete("Delete from database…", cx)))
                .into_any_element(),
            Some(SelectedElement::Vertex {
                id,
                label,
                degree,
                detail,
                ..
            }) => {
                let degree = degree
                    .map(|(i, o)| format!("in {i} · out {o}"))
                    .unwrap_or_else(|| "degree …".into());
                v_flex()
                    .gap(Tokens::space(2.0))
                    .child(header(label, id.to_string(), "Vertex"))
                    .child(
                        div()
                            .id("inspector-degree")
                            .test_support()
                            .text_size(Tokens::text_xs())
                            .text_color(theme.muted_foreground)
                            .child(degree),
                    )
                    .when(detail.is_none(), |el| {
                        el.child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child("Loading properties…"),
                        )
                    })
                    .child(self.render_rows(rows, cx))
                    .children(error)
                    .child(self.render_add(true, cx))
                    .child(h_flex().child(delete("Delete vertex…", cx)))
                    .into_any_element()
            }
            Some(SelectedElement::Edge {
                id,
                label,
                out_v,
                in_v,
                ..
            }) => {
                let endpoint = |id: &'static str,
                                prefix: &'static str,
                                n: graph_store::NodeIx,
                                text: String,
                                cx: &mut Context<'_, Self>| {
                    Button::new(id)
                        .label(format!("{prefix} {text}"))
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(g) = this.graph.clone() {
                                g.update(cx, |g, cx| g.select_and_focus(n, cx));
                            }
                        }))
                };
                v_flex()
                    .gap(Tokens::space(2.0))
                    .child(header(label, id.to_string(), "Edge"))
                    .child(
                        h_flex()
                            .gap(Tokens::space(1.0))
                            .child(endpoint("inspector-out", "From", out_v.0, out_v.1, cx))
                            .child(Icon::new(IconName::ArrowRight).xsmall())
                            .child(endpoint("inspector-in", "To", in_v.0, in_v.1, cx)),
                    )
                    .child(self.render_rows(rows, cx))
                    .children(error)
                    .child(self.render_add(false, cx))
                    .child(h_flex().child(delete("Delete edge…", cx)))
                    .into_any_element()
            }
        };
        v_flex()
            .id("inspector")
            .test_support()
            .size_full()
            .p(Tokens::space(3.0))
            .gap(Tokens::space(2.0))
            .overflow_y_scroll()
            .text_size(Tokens::text_sm())
            .child(body)
    }
}
