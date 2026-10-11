//! JSON tree view: expandable nodes with type annotations, rendered lazily (only visible rows exist), with
//! copy value / copy path and expand/collapse all.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::normalize::{self, TreeRow};
use gremlin_model::{GValue, plain};
use ui_theme::Tokens;

/// "Expand all" stops after this many nodes.
const EXPAND_ALL_LIMIT: usize = 50_000;

pub struct JsonView {
    roots: Rc<Vec<GValue>>,
    expanded: HashSet<Vec<u32>>,
    rows: Rc<Vec<TreeRow>>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
}

impl JsonView {
    pub fn new(items: Vec<GValue>, cx: &mut Context<'_, Self>) -> Self {
        let mut this = Self {
            roots: Rc::new(Vec::new()),
            expanded: HashSet::new(),
            rows: Rc::new(Vec::new()),
            selected: None,
            scroll: UniformListScrollHandle::new(),
        };
        this.set_items(items, cx);
        this
    }

    pub fn set_items(&mut self, items: Vec<GValue>, cx: &mut Context<'_, Self>) {
        self.roots = Rc::new(items);
        self.refresh_rows(cx);
    }

    fn refresh_rows(&mut self, cx: &mut Context<'_, Self>) {
        self.rows = Rc::new(normalize::visible_rows(&self.roots, &self.expanded));
        if self.selected.is_some_and(|s| s >= self.rows.len()) {
            self.selected = None;
        }
        cx.notify();
    }

    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    pub fn toggle(&mut self, row: usize, cx: &mut Context<'_, Self>) {
        let Some(r) = self.rows.get(row) else { return };
        if !r.expandable {
            return;
        }
        let path = r.path.clone();
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.selected = Some(row);
        self.refresh_rows(cx);
    }

    pub fn select(&mut self, row: usize, cx: &mut Context<'_, Self>) {
        self.selected = Some(row);
        cx.notify();
    }

    pub fn expand_all(&mut self, cx: &mut Context<'_, Self>) {
        self.expanded = normalize::all_expandable(&self.roots, EXPAND_ALL_LIMIT);
        self.refresh_rows(cx);
    }

    pub fn collapse_all(&mut self, cx: &mut Context<'_, Self>) {
        self.expanded.clear();
        self.refresh_rows(cx);
    }

    fn selected_value(&self) -> Option<&GValue> {
        let row = self.rows.get(self.selected?)?;
        normalize::node_at(&self.roots, &row.path)
    }

    /// The selected node as JSON.
    pub fn selected_json(&self) -> Option<String> {
        serde_json::to_string_pretty(&plain::to_json(self.selected_value()?)).ok()
    }

    pub fn copy_value(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(json) = self.selected_json() {
            cx.write_to_clipboard(ClipboardItem::new_string(json));
        }
    }

    pub fn copy_path(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(row) = self.selected.and_then(|s| self.rows.get(s)) {
            let path = normalize::access_path(&self.roots, &row.path);
            cx.write_to_clipboard(ClipboardItem::new_string(path));
        }
    }
}

impl Render for JsonView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let has_selection = self.selected.is_some();
        let toolbar = h_flex()
            .flex_none()
            .gap(Tokens::space(1.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .child(
                Button::new("json-expand-all")
                    .label("Expand all")
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, _, cx| this.expand_all(cx))),
            )
            .child(
                Button::new("json-collapse-all")
                    .label("Collapse all")
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, _, cx| this.collapse_all(cx))),
            )
            .child(div().flex_1())
            .child(
                Button::new("json-copy-value")
                    .label("Copy value")
                    .ghost()
                    .xsmall()
                    .disabled(!has_selection)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_value(cx))),
            )
            .child(
                Button::new("json-copy-path")
                    .label("Copy path")
                    .ghost()
                    .xsmall()
                    .disabled(!has_selection)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_path(cx))),
            );

        let rows = self.rows.clone();
        let roots = self.roots.clone();
        let selected = self.selected;
        let entity = cx.entity().downgrade();
        let list = uniform_list("json-rows", rows.len(), move |range, _, cx| {
            let theme = cx.theme();
            range
                .map(|ix| {
                    let row = &rows[ix];
                    let value = normalize::node_at(&roots, &row.path);
                    let type_name = value.map(|v| v.type_name().to_owned()).unwrap_or_default();
                    let preview = value.map(normalize::preview).unwrap_or_default();
                    let open = row.expandable
                        && rows.get(ix + 1).is_some_and(|n| {
                            n.path.starts_with(&row.path) && n.path.len() > row.path.len()
                        });
                    let arrow = if !row.expandable {
                        None
                    } else if open {
                        Some(IconName::ChevronDown)
                    } else {
                        Some(IconName::ChevronRight)
                    };
                    let entity = entity.clone();
                    #[allow(clippy::cast_precision_loss)]
                    let indent = Tokens::space(4.0 * row.depth() as f32);
                    h_flex()
                        .id(("json-row", ix))
                        .test_support()
                        .w_full()
                        .gap(Tokens::space(1.5))
                        .pl(indent)
                        .pr(Tokens::space(2.0))
                        .cursor_pointer()
                        .when(selected == Some(ix), |r| r.bg(theme.list_active))
                        .hover(|s| s.bg(theme.list_hover))
                        // Click selects the row and opens or closes it.
                        .on_click(move |_, _, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                this.select(ix, cx);
                                this.toggle(ix, cx);
                            });
                        })
                        .child(div().w(Tokens::space(3.0)).children(
                            arrow.map(|a| Icon::new(a).xsmall().text_color(theme.muted_foreground)),
                        ))
                        .child(div().text_color(theme.foreground).child(row.key.clone()))
                        .child(
                            div()
                                .px(Tokens::space(1.0))
                                .rounded(Tokens::RADIUS_SM)
                                .bg(theme.muted)
                                .text_size(Tokens::text_xs())
                                .text_color(theme.muted_foreground)
                                .child(type_name),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_color(theme.muted_foreground)
                                .child(preview),
                        )
                        .into_any_element()
                })
                .collect()
        })
        .track_scroll(&self.scroll)
        .size_full();

        v_flex()
            .id("json-view")
            .test_support()
            .size_full()
            .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
            .text_size(Tokens::text_sm())
            .child(toolbar)
            .child(div().flex_1().min_h_0().child(list))
    }
}
