//! Table view: element groups by label (or one table for maps/scalars), quick filter, sortable headers,
//! resizable/reorderable columns (gpui-kit `DataTable`), copy and export.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::GValue;
use gremlin_model::normalize::{self, TableGroup, TableModel};
use ui_theme::ButtonTip as _;
use ui_theme::Tokens;

/// Sort state: column and descending flag. Clicking a header cycles ascending → descending → off.
pub type Sort = Option<(usize, bool)>;

pub struct GroupDelegate {
    group: TableGroup,
    /// Row indices into `group.rows`, in display order.
    order: Vec<usize>,
    sort: Sort,
    filter: String,
    owner: WeakEntity<TableView>,
}

impl GroupDelegate {
    fn new(group: TableGroup, owner: WeakEntity<TableView>) -> Self {
        let order = (0..group.rows.len()).collect();
        Self {
            group,
            order,
            sort: None,
            filter: String::new(),
            owner,
        }
    }

    fn refresh_order(&mut self) {
        self.order = normalize::view_rows(&self.group, self.sort, &self.filter);
    }

    pub fn cycle_sort(&mut self, col: usize) {
        self.sort = match self.sort {
            Some((c, false)) if c == col => Some((col, true)),
            Some((c, true)) if c == col => None,
            _ => Some((col, false)),
        };
        self.refresh_order();
    }

    pub fn group(&self) -> &TableGroup {
        &self.group
    }

    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// The text of a visible cell.
    pub fn cell(&self, row: usize, col: usize) -> &str {
        self.group.rows[self.order[row]][col].display()
    }
}

impl TableDelegate for GroupDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.group.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.order.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let name = &self.group.columns[col_ix];
        let width = if matches!(name.as_str(), "id" | "outV" | "inV") {
            90.0
        } else {
            160.0
        };
        Column::new(
            SharedString::from(name.clone()),
            SharedString::from(name.clone()),
        )
        .width(px(width)) // token-check: allow (initial column width; user-resizable)
        .resizable(true)
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<'_, TableState<Self>>,
    ) -> impl IntoElement {
        let name = self.group.columns[col_ix].clone();
        let icon = match self.sort {
            Some((c, false)) if c == col_ix => Some(IconName::SortAscending),
            Some((c, true)) if c == col_ix => Some(IconName::SortDescending),
            _ => None,
        };
        h_flex()
            .id(SharedString::from(format!("th-{name}")))
            .test_support()
            .size_full()
            .gap(Tokens::space(1.0))
            .cursor_pointer()
            .on_click(cx.listener(move |table, _, _, cx| {
                table.delegate_mut().cycle_sort(col_ix);
                table.refresh(cx);
                cx.notify();
            }))
            .child(div().truncate().child(name))
            .children(icon.map(|i| Icon::new(i).xsmall()))
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<'_, TableState<Self>>,
    ) -> impl IntoElement {
        div().truncate().child(self.cell(row_ix, col_ix).to_owned())
    }

    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        _: &mut Window,
        _: &mut Context<'_, TableState<Self>>,
    ) {
        if col_ix == to_ix
            || col_ix >= self.group.columns.len()
            || to_ix >= self.group.columns.len()
        {
            return;
        }
        let column = self.group.columns.remove(col_ix);
        self.group.columns.insert(to_ix, column);
        for row in &mut self.group.rows {
            let cell = row.remove(col_ix);
            row.insert(to_ix, cell);
        }
        self.sort = None;
        self.refresh_order();
    }

    fn visible_rows_changed(
        &mut self,
        _: std::ops::Range<usize>,
        _: &mut Window,
        _: &mut Context<'_, TableState<Self>>,
    ) {
        let _ = &self.owner;
    }
}

pub struct TableView {
    model: TableModel,
    selected_group: usize,
    table: Option<Entity<TableState<GroupDelegate>>>,
    filter: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl TableView {
    pub fn new(items: &[GValue], window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter rows"));
        let subscriptions =
            vec![
                cx.subscribe_in(&filter, window, |this, state, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = state.read(cx).value().to_string();
                        this.set_filter(&text, cx);
                    }
                }),
            ];
        let mut this = Self {
            model: TableModel::default(),
            selected_group: 0,
            table: None,
            filter,
            _subscriptions: subscriptions,
        };
        this.set_items(items, window, cx);
        this
    }

    /// Rebuild from new items, keeping the selected label when it still exists.
    pub fn set_items(&mut self, items: &[GValue], window: &mut Window, cx: &mut Context<'_, Self>) {
        let previous = self
            .model
            .groups
            .get(self.selected_group)
            .map(|g| g.label.clone());
        self.model = normalize::table(items);
        self.selected_group = previous
            .and_then(|label| self.model.groups.iter().position(|g| g.label == label))
            .unwrap_or(0);
        self.rebuild_table(window, cx);
    }

    fn rebuild_table(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(group) = self.model.groups.get(self.selected_group).cloned() else {
            self.table = None;
            cx.notify();
            return;
        };
        let filter = self.filter.read(cx).value().to_string();
        let owner = cx.weak_entity();
        let (sort, keep_sort) = self
            .table
            .as_ref()
            .map(|t| {
                let d = t.read(cx).delegate();
                (
                    d.sort,
                    d.group.label == group.label && d.group.columns == group.columns,
                )
            })
            .unwrap_or((None, false));
        let mut delegate = GroupDelegate::new(group, owner);
        delegate.filter = filter;
        if keep_sort {
            delegate.sort = sort;
        }
        delegate.refresh_order();
        match &self.table {
            Some(table) if keep_sort => table.update(cx, |t, cx| {
                *t.delegate_mut() = delegate;
                t.refresh(cx);
            }),
            _ => self.table = Some(cx.new(|cx| TableState::new(delegate, window, cx))),
        }
        cx.notify();
    }

    pub fn select_group(&mut self, ix: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        if ix < self.model.groups.len() && ix != self.selected_group {
            self.selected_group = ix;
            self.table = None;
            self.rebuild_table(window, cx);
        }
    }

    fn set_filter(&mut self, text: &str, cx: &mut Context<'_, Self>) {
        if let Some(table) = &self.table {
            let text = text.to_owned();
            table.update(cx, |t, cx| {
                t.delegate_mut().filter = text;
                t.delegate_mut().refresh_order();
                t.refresh(cx);
            });
        }
        cx.notify();
    }

    pub fn model(&self) -> &TableModel {
        &self.model
    }

    pub fn table(&self) -> Option<&Entity<TableState<GroupDelegate>>> {
        self.table.as_ref()
    }

    pub fn filter_input(&self) -> &Entity<InputState> {
        &self.filter
    }

    /// Apply the filter input's current text (`set_value` emits no change event).
    pub fn apply_filter_input(&mut self, cx: &mut Context<'_, Self>) {
        let text = self.filter.read(cx).value().to_string();
        self.set_filter(&text, cx);
    }

    /// The visible table as CSV (current label, filter and sort).
    pub fn csv(&self, cx: &App) -> Option<String> {
        let table = self.table.as_ref()?.read(cx);
        let d = table.delegate();
        Some(normalize::to_csv(&d.group, &d.order))
    }

    /// The visible rows as JSON objects keyed by column.
    pub fn json(&self, cx: &App) -> Option<String> {
        let table = self.table.as_ref()?.read(cx);
        let d = table.delegate();
        let rows: Vec<serde_json::Value> = d
            .order
            .iter()
            .map(|&r| {
                let obj: serde_json::Map<String, serde_json::Value> = d
                    .group
                    .columns
                    .iter()
                    .zip(&d.group.rows[r])
                    .map(|(c, cell)| {
                        let v = match cell {
                            normalize::Cell::Empty => serde_json::Value::Null,
                            normalize::Cell::Number(n, _) => serde_json::json!(n),
                            normalize::Cell::Text(t) => serde_json::Value::String(t.clone()),
                        };
                        (c.clone(), v)
                    })
                    .collect();
                serde_json::Value::Object(obj)
            })
            .collect();
        serde_json::to_string_pretty(&rows).ok()
    }

    /// Ask for a file and write the visible table there.
    fn export(&mut self, json: bool, cx: &mut Context<'_, Self>) {
        let Some(content) = (if json { self.json(cx) } else { self.csv(cx) }) else {
            return;
        };
        let name = self
            .model
            .groups
            .get(self.selected_group)
            .map(|g| g.label.clone())
            .unwrap_or_else(|| "results".into());
        let file = format!("{name}.{}", if json { "json" } else { "csv" });
        let dir = std::env::current_dir().unwrap_or_default();
        let prompt = cx.prompt_for_new_path(&dir, Some(&file));
        cx.background_spawn(async move {
            if let Ok(Ok(Some(path))) = prompt.await
                && let Err(err) = std::fs::write(&path, content)
            {
                log::warn!("export to {} failed: {err}", path.display());
            }
        })
        .detach();
    }
}

impl Render for TableView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let many = self.model.groups.len() > 1;
        let labels =
            h_flex()
                .gap(Tokens::space(1.0))
                .children(self.model.groups.iter().enumerate().map(|(i, g)| {
                    let selected = i == self.selected_group;
                    Button::new(SharedString::from(format!("label-{}", g.label)))
                        .label(format!("{} ({})", g.label, g.rows.len()))
                        .xsmall()
                        .when(selected, |b| b.primary())
                        .when(!selected, |b| b.ghost())
                        .on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.select_group(i, window, cx)
                            }),
                        )
                }));
        let toolbar = h_flex()
            .flex_none()
            .gap(Tokens::space(2.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .when(many, |el| el.child(labels))
            .child(div().flex_1())
            .child(
                div().w(Tokens::size(200.0)).child(
                    Input::new(&self.filter)
                        .xsmall()
                        .prefix(Icon::new(IconName::Search).xsmall()),
                ),
            )
            .child(
                Button::new("export-csv")
                    .label("CSV")
                    .ghost()
                    .xsmall()
                    .tip("Export the visible rows as CSV")
                    .on_click(cx.listener(|this, _, _, cx| this.export(false, cx))),
            )
            .child(
                Button::new("export-json")
                    .label("JSON")
                    .ghost()
                    .xsmall()
                    .tip("Export the visible rows as JSON")
                    .on_click(cx.listener(|this, _, _, cx| this.export(true, cx))),
            );
        v_flex()
            .id("table-view")
            .test_support()
            .size_full()
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .children(self.table.as_ref().map(|t| {
                        DataTable::new(t)
                            .stripe(true)
                            .bordered(false)
                            .into_any_element()
                    })),
            )
    }
}
