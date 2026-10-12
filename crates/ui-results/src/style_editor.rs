//! The stylesheet editor side panel: pick a stylesheet, a label (or the label/caption settings) and edit
//! colour, shape, icon, caption template, size or width. Every change is saved to the shared library at
//! once, so the open graphs restyle live.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use graph_store::style::{EdgeStyle, Rgb, Shape, Size, Stylesheet, VertexStyle, WILDCARD};
use ui_theme::Tokens;

use crate::graph_view::GraphView;
use crate::style_library::{DEFAULT_ID, StyleLibrary};

/// What the panel is editing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleTarget {
    Vertex(String),
    Edge(String),
    /// Label density and caption settings.
    Globals,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Name,
    Hex,
    Icon,
    Caption,
    SizePx,
    SizeKey,
    SizeMin,
    SizeMax,
    MinLabel,
    MaxCaption,
}

pub struct StyleEditor {
    graph: WeakEntity<GraphView>,
    target: StyleTarget,
    inputs: Vec<(Field, Entity<InputState>)>,
    _subscriptions: Vec<Subscription>,
}

impl StyleEditor {
    pub fn new(
        graph: WeakEntity<GraphView>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let fields = [
            (Field::Name, "Stylesheet name"),
            (Field::Hex, "#rrggbb"),
            (Field::Icon, "Lucide icon, e.g. plane"),
            (Field::Caption, "e.g. {code} – {city}"),
            (Field::SizePx, "px"),
            (Field::SizeKey, "numeric property"),
            (Field::SizeMin, "min px"),
            (Field::SizeMax, "max px"),
            (Field::MinLabel, "px"),
            (Field::MaxCaption, "characters (0: no limit)"),
        ];
        let mut inputs = Vec::new();
        let mut subscriptions = Vec::new();
        for (field, placeholder) in fields {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.apply(field, cx);
                    }
                },
            ));
            inputs.push((field, input));
        }
        let library = StyleLibrary::global(cx);
        subscriptions.push(cx.observe(&library, |_, _, cx| cx.notify()));
        let this = Self {
            graph,
            target: StyleTarget::Globals,
            inputs,
            _subscriptions: subscriptions,
        };
        // The graph view is usually mid-update when it creates the editor; read it afterwards.
        cx.defer_in(window, |this, window, cx| this.reload(window, cx));
        this
    }

    /// [`Self::reload`] once the current update is over (callers may be updating the graph view).
    pub fn reload_later(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        cx.defer_in(window, |this, window, cx| this.reload(window, cx));
    }

    fn input(&self, field: Field) -> &Entity<InputState> {
        self.inputs
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, i)| i)
            .unwrap_or(&self.inputs[0].1)
    }

    /// The caption template input (tests and automation).
    pub fn caption_input(&self) -> &Entity<InputState> {
        self.input(Field::Caption)
    }

    pub fn target(&self) -> &StyleTarget {
        &self.target
    }

    /// The stylesheet the graph uses.
    fn sheet(&self, cx: &App) -> Option<Stylesheet> {
        let graph = self.graph.upgrade()?;
        let id = graph.read(cx).stylesheet_id().to_owned();
        Some(StyleLibrary::global_ref(cx)?.read(cx).get_or_default(&id))
    }

    /// Edit the graph's stylesheet and save it (live preview everywhere).
    fn edit(&self, cx: &mut Context<'_, Self>, f: impl FnOnce(&mut Stylesheet, &StyleTarget)) {
        let Some(mut sheet) = self.sheet(cx) else {
            return;
        };
        f(&mut sheet, &self.target);
        let library = StyleLibrary::global(cx);
        library.update(cx, |l, cx| l.save(sheet, cx));
    }

    pub fn set_target(
        &mut self,
        target: StyleTarget,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.target = target;
        self.reload(window, cx);
    }

    /// Fill the inputs from the stylesheet for the current target.
    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(sheet) = self.sheet(cx) else { return };
        let mut values: Vec<(Field, String)> = vec![(Field::Name, sheet.name.clone())];
        let size_values = |size: &Size| match size {
            Size::Fixed { px } => vec![(Field::SizePx, fmt_num(*px))],
            Size::Property { key, min, max } => vec![
                (Field::SizeKey, key.clone()),
                (Field::SizeMin, fmt_num(*min)),
                (Field::SizeMax, fmt_num(*max)),
            ],
        };
        match &self.target {
            StyleTarget::Vertex(l) => {
                let st = sheet.vertex.get(l).cloned().unwrap_or_default();
                values.push((Field::Hex, st.color.map(Rgb::hex).unwrap_or_default()));
                values.push((Field::Icon, st.icon.unwrap_or_default()));
                values.push((Field::Caption, st.caption.unwrap_or_default()));
                values.extend(size_values(&st.size));
            }
            StyleTarget::Edge(l) => {
                let st = sheet.edge.get(l).cloned().unwrap_or_default();
                values.push((Field::Hex, st.color.map(Rgb::hex).unwrap_or_default()));
                values.push((Field::Caption, st.caption.unwrap_or_default()));
                values.extend(size_values(&st.width));
            }
            StyleTarget::Globals => {
                values.push((Field::MinLabel, fmt_num(sheet.globals.min_label_px)));
                values.push((Field::MaxCaption, sheet.globals.max_caption_len.to_string()));
            }
        }
        for (field, value) in values {
            let input = self.input(field).clone();
            input.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        cx.notify();
    }

    fn apply(&mut self, field: Field, cx: &mut Context<'_, Self>) {
        let value = self.input(field).read(cx).value().trim().to_owned();
        if field == Field::Name {
            if let Some(graph) = self.graph.upgrade() {
                let id = graph.read(cx).stylesheet_id().to_owned();
                StyleLibrary::global(cx).update(cx, |l, cx| l.rename(&id, &value, cx));
            }
            return;
        }
        let num = value.parse::<f32>().ok();
        self.edit(cx, move |sheet, target| match target {
            StyleTarget::Vertex(l) => {
                let st = sheet.vertex.entry(l.clone()).or_default();
                match field {
                    Field::Hex => st.color = Rgb::parse(&value),
                    Field::Icon => st.icon = (!value.is_empty()).then(|| value.clone()),
                    Field::Caption => st.caption = (!value.is_empty()).then(|| value.clone()),
                    _ => edit_size(&mut st.size, field, &value, num),
                }
            }
            StyleTarget::Edge(l) => {
                let st = sheet.edge.entry(l.clone()).or_default();
                match field {
                    Field::Hex => st.color = Rgb::parse(&value),
                    Field::Caption => st.caption = (!value.is_empty()).then(|| value.clone()),
                    _ => edit_size(&mut st.width, field, &value, num),
                }
            }
            StyleTarget::Globals => match field {
                Field::MinLabel => {
                    if let Some(n) = num {
                        sheet.globals.min_label_px = n.clamp(4.0, 32.0);
                    }
                }
                Field::MaxCaption => {
                    if let Ok(n) = value.parse::<usize>() {
                        sheet.globals.max_caption_len = n;
                    }
                }
                _ => {}
            },
        });
    }

    fn graph_labels(&self, cx: &App) -> (Vec<String>, Vec<String>) {
        let Some(graph) = self.graph.upgrade() else {
            return (Vec::new(), Vec::new());
        };
        let g = graph.read(cx).store(cx);
        let mut v: Vec<String> = g
            .vertex_label_counts()
            .iter()
            .map(|l| l.name.to_string())
            .collect();
        let mut e: Vec<String> = g
            .edge_label_counts()
            .iter()
            .map(|l| l.name.to_string())
            .collect();
        if let Some(sheet) = self.sheet(cx) {
            for k in sheet.vertex.keys() {
                if !v.contains(k) {
                    v.push(k.clone());
                }
            }
            for k in sheet.edge.keys() {
                if !e.contains(k) {
                    e.push(k.clone());
                }
            }
        }
        (v, e)
    }

    fn export(&self, cx: &mut Context<'_, Self>) {
        let Some(sheet) = self.sheet(cx) else { return };
        let json = sheet.to_json();
        let file = format!("{}.gremliner-style.json", sheet.name);
        let dir = std::env::current_dir().unwrap_or_default();
        let prompt = cx.prompt_for_new_path(&dir, Some(&file));
        cx.background_spawn(async move {
            if let Ok(Ok(Some(path))) = prompt.await
                && let Err(err) = std::fs::write(&path, json)
            {
                log::warn!("stylesheet export to {} failed: {err}", path.display());
            }
        })
        .detach();
    }

    fn import(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import stylesheet".into()),
        });
        let graph = self.graph.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(err) => {
                    log::warn!("cannot read {}: {err}", path.display());
                    return;
                }
            };
            let sheet = match Stylesheet::from_json(&text) {
                Ok(s) => s,
                Err(err) => {
                    log::warn!("{} is not a stylesheet: {err}", path.display());
                    return;
                }
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let id = StyleLibrary::global(cx).update(cx, |l, cx| l.import(sheet, cx));
                let _ = graph.update(cx, |g, cx| g.set_stylesheet(&id, cx));
                this.reload(window, cx);
            });
        })
        .detach();
    }

    fn switch_to(&mut self, id: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        let _ = self.graph.update(cx, |g, cx| g.set_stylesheet(&id, cx));
        self.reload(window, cx);
    }

    fn render_header(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let current = self
            .graph
            .upgrade()
            .map(|g| g.read(cx).stylesheet_id().to_owned())
            .unwrap_or_else(|| DEFAULT_ID.into());
        let connection = self
            .graph
            .upgrade()
            .and_then(|g| g.read(cx).connection().map(str::to_owned));
        let library = StyleLibrary::global_ref(cx).map(|l| l.read(cx));
        let sheets: Vec<(String, String)> = library
            .map(|l| {
                l.sheets()
                    .iter()
                    .map(|s| (s.id.clone(), s.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let is_conn_default = connection.as_deref().is_some_and(|c| {
            library.and_then(|l| l.default_for_connection(c)) == Some(current.as_str())
        });
        let small =
            |id: &'static str, label: &'static str| Button::new(id).label(label).ghost().xsmall();
        v_flex()
            .gap(Tokens::space(1.5))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(Tokens::space(1.0))
                    .children(sheets.into_iter().map(|(id, name)| {
                        let on = id == current;
                        Button::new(SharedString::from(format!("style-sheet-{id}")))
                            .label(name)
                            .xsmall()
                            .when(on, |b| b.primary())
                            .when(!on, |b| b.ghost())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.switch_to(id.clone(), window, cx)
                            }))
                    })),
            )
            .child(Input::new(self.input(Field::Name)).xsmall())
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(Tokens::space(0.5))
                    .child(small("style-new", "New").on_click(cx.listener(
                        |this, _, window, cx| {
                            let id = StyleLibrary::global(cx)
                                .update(cx, |l, cx| l.create("Untitled", cx));
                            this.switch_to(id, window, cx);
                        },
                    )))
                    .child(small("style-duplicate", "Duplicate").on_click(cx.listener(
                        move |this, _, window, cx| {
                            let Some(graph) = this.graph.upgrade() else {
                                return;
                            };
                            let id = graph.read(cx).stylesheet_id().to_owned();
                            if let Some(new_id) =
                                StyleLibrary::global(cx).update(cx, |l, cx| l.duplicate(&id, cx))
                            {
                                this.switch_to(new_id, window, cx);
                            }
                        },
                    )))
                    .when(current != DEFAULT_ID, |el| {
                        el.child(small("style-delete", "Delete").on_click(cx.listener(
                            |this, _, window, cx| {
                                let Some(graph) = this.graph.upgrade() else {
                                    return;
                                };
                                let id = graph.read(cx).stylesheet_id().to_owned();
                                StyleLibrary::global(cx).update(cx, |l, cx| l.delete(&id, cx));
                                this.switch_to(DEFAULT_ID.into(), window, cx);
                            },
                        )))
                    })
                    .child(
                        small("style-import", "Import…")
                            .on_click(cx.listener(|this, _, window, cx| this.import(window, cx))),
                    )
                    .child(
                        small("style-export", "Export…")
                            .on_click(cx.listener(|this, _, _, cx| this.export(cx))),
                    ),
            )
            .when_some(connection, |el, conn| {
                el.child(
                    Button::new("style-conn-default")
                        .label("Default for this connection")
                        .xsmall()
                        .when(is_conn_default, |b| b.primary())
                        .when(!is_conn_default, |b| b.ghost())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let Some(graph) = this.graph.upgrade() else {
                                return;
                            };
                            let id = graph.read(cx).stylesheet_id().to_owned();
                            let set = (!is_conn_default).then_some(id.as_str());
                            StyleLibrary::global(cx)
                                .update(cx, |l, cx| l.set_connection_default(&conn, set, cx));
                        })),
                )
            })
            .child(div().h(Tokens::BORDER).bg(theme.border))
            .into_any_element()
    }

    fn render_targets(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let (vertex, edge) = self.graph_labels(cx);
        let chip = |id: String, text: String, on: bool| {
            Button::new(SharedString::from(id))
                .label(text)
                .xsmall()
                .when(on, |b| b.primary())
                .when(!on, |b| b.ghost())
        };
        let section = |text: &'static str| {
            div()
                .text_size(Tokens::text_xs())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(text)
        };
        let label_text = |l: &str| {
            if l == WILDCARD {
                "All labels".to_owned()
            } else {
                l.to_owned()
            }
        };
        let mut v_row = h_flex().flex_wrap().gap(Tokens::space(1.0));
        for l in vertex {
            let on = self.target == StyleTarget::Vertex(l.clone());
            let t = StyleTarget::Vertex(l.clone());
            v_row = v_row.child(chip(format!("style-v-{l}"), label_text(&l), on).on_click(
                cx.listener(move |this, _, window, cx| this.set_target(t.clone(), window, cx)),
            ));
        }
        let mut e_row = h_flex().flex_wrap().gap(Tokens::space(1.0));
        for l in edge {
            let on = self.target == StyleTarget::Edge(l.clone());
            let t = StyleTarget::Edge(l.clone());
            e_row = e_row.child(chip(format!("style-e-{l}"), label_text(&l), on).on_click(
                cx.listener(move |this, _, window, cx| this.set_target(t.clone(), window, cx)),
            ));
        }
        v_flex()
            .gap(Tokens::space(1.0))
            .child(section("VERTEX LABELS"))
            .child(v_row)
            .child(section("EDGE LABELS"))
            .child(e_row)
            .child(
                chip(
                    "style-globals".into(),
                    "Labels & captions".into(),
                    self.target == StyleTarget::Globals,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_target(StyleTarget::Globals, window, cx)
                })),
            )
            .child(div().h(Tokens::BORDER).bg(theme.border))
            .into_any_element()
    }

    fn row(label: &'static str, cx: &App) -> Div {
        h_flex().gap(Tokens::space(2.0)).child(
            div()
                .w(Tokens::size(64.0))
                .flex_none()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
    }

    fn render_swatches(&self, current: Option<Rgb>, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let palette: Vec<Hsla> = self
            .graph
            .upgrade()
            .map(|g| g.read(cx).canvas().read(cx).style().palette.to_vec())
            .unwrap_or_default();
        let mut row = h_flex().flex_wrap().gap(Tokens::space(1.0)).child(
            Button::new("style-swatch-auto")
                .label("Auto")
                .xsmall()
                .when(current.is_none(), |b| b.primary())
                .when(current.is_some(), |b| b.ghost())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_color(None, cx);
                    this.reload(window, cx);
                })),
        );
        for (i, c) in palette.into_iter().enumerate() {
            let rgb = Rgb(graph_canvas::theme::bytes_from_color(c));
            let on = current == Some(rgb);
            row = row.child(
                div()
                    .id(SharedString::from(format!("style-swatch-{i}")))
                    .test_support()
                    .size(Tokens::space(4.0))
                    .rounded_full()
                    .bg(c)
                    .border(Tokens::BORDER)
                    .border_color(if on { theme.foreground } else { theme.border })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_color(Some(rgb), cx);
                        this.reload(window, cx);
                    })),
            );
        }
        row.into_any_element()
    }

    fn set_color(&mut self, color: Option<Rgb>, cx: &mut Context<'_, Self>) {
        self.edit(cx, move |sheet, target| match target {
            StyleTarget::Vertex(l) => sheet.vertex.entry(l.clone()).or_default().color = color,
            StyleTarget::Edge(l) => sheet.edge.entry(l.clone()).or_default().color = color,
            StyleTarget::Globals => {}
        });
    }

    fn render_size(&self, size: &Size, cx: &mut Context<'_, Self>) -> AnyElement {
        let by_property = matches!(size, Size::Property { .. });
        let mode = |id: &'static str, label: &'static str, on: bool| {
            Button::new(id)
                .label(label)
                .xsmall()
                .when(on, |b| b.primary())
                .when(!on, |b| b.ghost())
        };
        v_flex()
            .gap(Tokens::space(1.0))
            .child(
                h_flex()
                    .gap(Tokens::space(1.0))
                    .child(
                        mode("style-size-fixed", "Fixed", !by_property).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.edit_size_mode(false, cx);
                                this.reload(window, cx);
                            },
                        )),
                    )
                    .child(
                        mode("style-size-property", "By property", by_property).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.edit_size_mode(true, cx);
                                this.reload(window, cx);
                            }),
                        ),
                    ),
            )
            .when(!by_property, |el| {
                el.child(Input::new(self.input(Field::SizePx)).xsmall())
            })
            .when(by_property, |el| {
                el.child(Input::new(self.input(Field::SizeKey)).xsmall())
                    .child(
                        h_flex()
                            .gap(Tokens::space(1.0))
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(self.input(Field::SizeMin)).xsmall()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(self.input(Field::SizeMax)).xsmall()),
                            ),
                    )
            })
            .into_any_element()
    }

    fn edit_size_mode(&mut self, by_property: bool, cx: &mut Context<'_, Self>) {
        self.edit(cx, move |sheet, target| {
            let (size, base) = match target {
                StyleTarget::Vertex(l) => (
                    &mut sheet.vertex.entry(l.clone()).or_default().size,
                    graph_store::style::DEFAULT_NODE_PX,
                ),
                StyleTarget::Edge(l) => (
                    &mut sheet.edge.entry(l.clone()).or_default().width,
                    graph_store::style::DEFAULT_EDGE_PX,
                ),
                StyleTarget::Globals => return,
            };
            *size = if by_property {
                Size::Property {
                    key: String::new(),
                    min: base * 0.66,
                    max: base * 2.66,
                }
            } else {
                Size::Fixed { px: base }
            };
        });
    }

    fn render_target(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let Some(sheet) = self.sheet(cx) else {
            return div().into_any_element();
        };
        match self.target.clone() {
            StyleTarget::Vertex(l) => {
                let st: VertexStyle = sheet.vertex.get(&l).cloned().unwrap_or_default();
                let shapes = h_flex()
                    .gap(Tokens::space(1.0))
                    .children(Shape::ALL.into_iter().map(|shape| {
                        let on = st.shape == shape;
                        Button::new(SharedString::from(format!("style-shape-{}", shape.name())))
                            .label(shape.name())
                            .xsmall()
                            .when(on, |b| b.primary())
                            .when(!on, |b| b.ghost())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let l = l_for(&this.target);
                                this.edit(cx, move |sheet, _| {
                                    if let Some(l) = l {
                                        sheet.vertex.entry(l).or_default().shape = shape;
                                    }
                                });
                            }))
                    }));
                v_flex()
                    .gap(Tokens::space(2.0))
                    .child(Self::row("Colour", cx).child(self.render_swatches(st.color, cx)))
                    .child(
                        Self::row("Custom", cx).child(Input::new(self.input(Field::Hex)).xsmall()),
                    )
                    .child(Self::row("Shape", cx).child(shapes))
                    .child(
                        Self::row("Icon", cx).child(Input::new(self.input(Field::Icon)).xsmall()),
                    )
                    .child(
                        Self::row("Caption", cx)
                            .child(Input::new(self.input(Field::Caption)).xsmall()),
                    )
                    .child(Self::row("Size", cx).child(self.render_size(&st.size, cx)))
                    .into_any_element()
            }
            StyleTarget::Edge(l) => {
                let st: EdgeStyle = sheet.edge.get(&l).cloned().unwrap_or_default();
                let dashed = st.dashed;
                v_flex()
                    .gap(Tokens::space(2.0))
                    .child(Self::row("Colour", cx).child(self.render_swatches(st.color, cx)))
                    .child(
                        Self::row("Custom", cx).child(Input::new(self.input(Field::Hex)).xsmall()),
                    )
                    .child(Self::row("Width", cx).child(self.render_size(&st.width, cx)))
                    .child(
                        Self::row("Caption", cx)
                            .child(Input::new(self.input(Field::Caption)).xsmall()),
                    )
                    .child(
                        Self::row("Line", cx).child(
                            Button::new("style-dashed")
                                .label("Dashed")
                                .xsmall()
                                .when(dashed, |b| b.primary())
                                .when(!dashed, |b| b.ghost())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit(cx, move |sheet, target| {
                                        if let StyleTarget::Edge(l) = target {
                                            sheet.edge.entry(l.clone()).or_default().dashed =
                                                !dashed;
                                        }
                                    });
                                })),
                        ),
                    )
                    .into_any_element()
            }
            StyleTarget::Globals => {
                let g = sheet.globals.clone();
                let density = h_flex().gap(Tokens::space(1.0)).children(
                    [("Low", 0.2f32), ("Normal", 0.5), ("High", 0.9)]
                        .into_iter()
                        .map(|(name, v)| {
                            let on = (g.label_density - v).abs() < 0.05;
                            Button::new(SharedString::from(format!("style-density-{name}")))
                                .label(name)
                                .xsmall()
                                .when(on, |b| b.primary())
                                .when(!on, |b| b.ghost())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit(cx, move |sheet, _| sheet.globals.label_density = v);
                                }))
                        }),
                );
                let edge_captions = g.show_edge_captions;
                v_flex()
                    .gap(Tokens::space(2.0))
                    .child(Self::row("Density", cx).child(density))
                    .child(
                        Self::row("Min size", cx)
                            .child(Input::new(self.input(Field::MinLabel)).xsmall()),
                    )
                    .child(
                        Self::row("Max length", cx)
                            .child(Input::new(self.input(Field::MaxCaption)).xsmall()),
                    )
                    .child(
                        Self::row("Edges", cx).child(
                            Button::new("style-edge-captions")
                                .label("Show edge captions")
                                .xsmall()
                                .when(edge_captions, |b| b.primary())
                                .when(!edge_captions, |b| b.ghost())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit(cx, move |sheet, _| {
                                        sheet.globals.show_edge_captions = !edge_captions
                                    });
                                })),
                        ),
                    )
                    .into_any_element()
            }
        }
    }
}

fn l_for(target: &StyleTarget) -> Option<String> {
    match target {
        StyleTarget::Vertex(l) => Some(l.clone()),
        _ => None,
    }
}

fn edit_size(size: &mut Size, field: Field, value: &str, num: Option<f32>) {
    match (size, field) {
        (Size::Fixed { px }, Field::SizePx) => {
            if let Some(n) = num {
                *px = n.clamp(0.5, 200.0);
            }
        }
        (Size::Property { key, .. }, Field::SizeKey) => *key = value.to_owned(),
        (Size::Property { min, .. }, Field::SizeMin) => {
            if let Some(n) = num {
                *min = n.clamp(0.5, 200.0);
            }
        }
        (Size::Property { max, .. }, Field::SizeMax) => {
            if let Some(n) = num {
                *max = n.clamp(0.5, 200.0);
            }
        }
        _ => {}
    }
}

fn fmt_num(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

impl Render for StyleEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        v_flex()
            .id("style-editor")
            .test_support()
            .size_full()
            .p(Tokens::space(2.0))
            .gap(Tokens::space(2.0))
            .overflow_y_scroll()
            .text_size(Tokens::text_xs())
            .bg(theme.background)
            .child(
                h_flex()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Stylesheet"))
                    .child(
                        Button::new("style-close")
                            .icon(Icon::new(IconName::Close))
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, window, cx| {
                                let _ = this
                                    .graph
                                    .update(cx, |g, cx| g.set_show_style_editor(false, window, cx));
                            })),
                    ),
            )
            .child(self.render_header(cx))
            .child(self.render_targets(cx))
            .child(self.render_target(cx))
    }
}
