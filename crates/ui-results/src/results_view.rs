//! The results dock content: summary bar, view switcher (Table / JSON / Console), error view.

use std::time::{Duration, Instant};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::normalize::PreferredView;
use ui_theme::Tokens;

use crate::console_view::ConsoleView;
use crate::json_view::JsonView;
use crate::result_set::{ResultSet, RunStatus};
use crate::table_view::TableView;

/// While streaming, views rebuild at most this often (the summary updates on every batch).
const STREAM_REFRESH: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Table,
    Json,
    Console,
}

impl From<PreferredView> for ViewKind {
    fn from(v: PreferredView) -> Self {
        match v {
            PreferredView::Table => ViewKind::Table,
            PreferredView::Json => ViewKind::Json,
            PreferredView::Console => ViewKind::Console,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultsViewEvent {
    CancelRequested,
}

pub struct ResultsView {
    result: Option<Entity<ResultSet>>,
    view: ViewKind,
    /// The user picked a view; stop choosing by shape.
    view_pinned: bool,
    table: Option<Entity<TableView>>,
    json: Option<Entity<JsonView>>,
    console: Option<Entity<ConsoleView>>,
    /// Items count the views were last built from.
    built_for: Option<(u64, usize)>,
    /// When the last refresh finished, and how long it took (the stream throttle adapts to it).
    last_refresh: Option<Instant>,
    refresh_cost: Duration,
    pending_refresh: Option<Task<()>>,
    _result_subscription: Option<Subscription>,
}

impl EventEmitter<ResultsViewEvent> for ResultsView {}

impl Default for ResultsView {
    fn default() -> Self {
        Self {
            result: None,
            view: ViewKind::Table,
            view_pinned: false,
            table: None,
            json: None,
            console: None,
            built_for: None,
            last_refresh: None,
            refresh_cost: Duration::ZERO,
            pending_refresh: None,
            _result_subscription: None,
        }
    }
}

impl ResultsView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn result(&self) -> Option<&Entity<ResultSet>> {
        self.result.as_ref()
    }

    pub fn view(&self) -> ViewKind {
        self.view
    }

    pub fn table(&self) -> Option<&Entity<TableView>> {
        self.table.as_ref()
    }

    pub fn json(&self) -> Option<&Entity<JsonView>> {
        self.json.as_ref()
    }

    pub fn console(&self) -> Option<&Entity<ConsoleView>> {
        self.console.as_ref()
    }

    /// Show a (new) result. Views are rebuilt from scratch, so nothing from a previous run remains.
    pub fn set_result(
        &mut self,
        result: Entity<ResultSet>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self._result_subscription = Some(cx.observe_in(&result, window, |this, _, window, cx| {
            this.on_result_changed(window, cx)
        }));
        self.result = Some(result);
        self.table = None;
        self.json = None;
        self.console = None;
        self.built_for = None;
        self.last_refresh = None;
        self.pending_refresh = None;
        self.view_pinned = false;
        self.refresh(window, cx);
    }

    pub fn set_view(&mut self, view: ViewKind, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.view = view;
        self.view_pinned = true;
        self.refresh(window, cx);
    }

    fn on_result_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let running = self
            .result
            .as_ref()
            .is_some_and(|r| r.read(cx).is_running());
        // Rebuilding costs O(items); never spend more than ~20% of the time rebuilding while streaming.
        let interval = STREAM_REFRESH.max(self.refresh_cost * 4);
        let recent = self.last_refresh.is_some_and(|t| t.elapsed() < interval);
        if running && recent {
            if self.pending_refresh.is_none() {
                self.pending_refresh = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(interval).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.pending_refresh = None;
                        this.refresh(window, cx);
                    });
                }));
            }
            cx.notify(); // the summary still updates
            return;
        }
        self.refresh(window, cx);
    }

    /// (Re)build the active view from the current items if they changed.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let started = Instant::now();
        self.rebuild(window, cx);
        self.refresh_cost = started.elapsed();
        self.last_refresh = Some(Instant::now());
    }

    fn rebuild(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(result) = self.result.clone() else {
            cx.notify();
            return;
        };
        let (run_id, count, shape) = {
            let r = result.read(cx);
            (r.run_id, r.items.len(), r.shape())
        };
        if !self.view_pinned && count > 0 {
            self.view = shape.preferred_view().into();
        }
        let stale = self.built_for != Some((run_id, count));
        if stale {
            // Other views rebuild when they are next shown.
            self.table = None.or(self.table.take().filter(|_| self.view == ViewKind::Table));
            self.json = None.or(self.json.take().filter(|_| self.view == ViewKind::Json));
            self.console = None.or(self
                .console
                .take()
                .filter(|_| self.view == ViewKind::Console));
        }
        // Views read the items in place (moved out and back, no copy); only the JSON tree keeps its own.
        let (items, raw) = result.update(cx, |r, _| {
            (std::mem::take(&mut r.items), std::mem::take(&mut r.raw))
        });
        match self.view {
            ViewKind::Table => match &self.table {
                Some(t) if stale => t.update(cx, |t, cx| t.set_items(&items, window, cx)),
                Some(_) => {}
                None => self.table = Some(cx.new(|cx| TableView::new(&items, window, cx))),
            },
            ViewKind::Json => match &self.json {
                Some(j) if stale => {
                    let copy = items.clone();
                    j.update(cx, |j, cx| j.set_items(copy, cx));
                }
                Some(_) => {}
                None => {
                    let copy = items.clone();
                    self.json = Some(cx.new(|cx| JsonView::new(copy, cx)));
                }
            },
            ViewKind::Console => match &self.console {
                Some(c) if stale => c.update(cx, |c, cx| c.set_items(&items, &raw, cx)),
                Some(_) => {}
                None => {
                    self.console = Some(cx.new(|cx| ConsoleView::new(&items, &raw, window, cx)))
                }
            },
        }
        result.update(cx, |r, _| {
            r.items = items;
            r.raw = raw;
        });
        self.built_for = Some((run_id, count));
        cx.notify();
    }

    fn render_summary(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(result) = self.result.as_ref() else {
            return div().into_any_element();
        };
        let r = result.read(cx);
        let count = r.items.len();
        let items = if count == 1 {
            "1 item".to_owned()
        } else {
            format!("{} items", group_digits(count))
        };
        let ms = r.elapsed_so_far().as_millis();
        let (icon, status, color) = match &r.status {
            RunStatus::Running => (None, "Running…".to_owned(), theme.muted_foreground),
            RunStatus::Succeeded => (
                Some(IconName::CircleCheck),
                "Success".to_owned(),
                theme.success,
            ),
            RunStatus::Failed(_) => (Some(IconName::CircleX), "Error".to_owned(), theme.danger),
            RunStatus::Cancelled => (Some(IconName::Ban), "Cancelled".to_owned(), theme.warning),
        };
        let running = r.is_running();
        let truncated = r
            .truncated
            .then(|| format!("Truncated at {} items", group_digits(r.cap)));
        let server = r
            .server_time
            .map(|t| format!("server {} ms", t.as_millis()));
        let view_button = |id: &'static str,
                           label: &'static str,
                           kind: ViewKind,
                           current: ViewKind,
                           cx: &mut Context<'_, Self>| {
            Button::new(id)
                .label(label)
                .xsmall()
                .when(kind == current, |b| b.primary())
                .when(kind != current, |b| b.ghost())
                .on_click(cx.listener(move |this, _, window, cx| this.set_view(kind, window, cx)))
        };
        h_flex()
            .id("results-summary")
            .test_support()
            .flex_none()
            .gap(Tokens::space(3.0))
            .px(Tokens::space(3.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .text_size(Tokens::text_sm())
            .child(
                h_flex()
                    .id("results-status")
                    .test_support()
                    .gap(Tokens::space(1.5))
                    .text_color(color)
                    .when(running, |el| el.child(Spinner::new().xsmall()))
                    .children(icon.map(|i| Icon::new(i).xsmall()))
                    .child(status),
            )
            .child(
                div()
                    .id("results-count")
                    .test_support()
                    .text_color(theme.foreground)
                    .child(items),
            )
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(format!("{ms} ms")),
            )
            .children(server.map(|s| div().text_color(theme.muted_foreground).child(s)))
            .children(truncated.map(|t| {
                div()
                    .id("results-truncated")
                    .test_support()
                    .px(Tokens::space(1.5))
                    .rounded(Tokens::RADIUS_SM)
                    .bg(theme.warning.opacity(0.15))
                    .text_color(theme.warning)
                    .child(t)
            }))
            .when(running, |el| {
                el.child(
                    Button::new("results-cancel")
                        .label("Cancel")
                        .icon(IconName::CircleX)
                        .ghost()
                        .xsmall()
                        .tooltip("Cancel (Esc)")
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(ResultsViewEvent::CancelRequested)),
                        ),
                )
            })
            .child(div().flex_1())
            .child(view_button(
                "view-table",
                "Table",
                ViewKind::Table,
                self.view,
                cx,
            ))
            .child(view_button(
                "view-json",
                "JSON",
                ViewKind::Json,
                self.view,
                cx,
            ))
            .child(view_button(
                "view-console",
                "Console",
                ViewKind::Console,
                self.view,
                cx,
            ))
            .into_any_element()
    }

    fn render_error(&self, cx: &App) -> Option<AnyElement> {
        let r = self.result.as_ref()?.read(cx);
        let RunStatus::Failed(error) = &r.status else {
            return None;
        };
        let theme = cx.theme();
        let code = error
            .code
            .map(|c| format!("Error {c}"))
            .unwrap_or_else(|| "Error".into());
        let exception = error.exceptions.first().cloned();
        Some(
            v_flex()
                .id("results-error")
                .test_support()
                .size_full()
                .gap(Tokens::space(2.0))
                .p(Tokens::space(4.0))
                .child(
                    h_flex()
                        .gap(Tokens::space(2.0))
                        .text_color(theme.danger)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Icon::new(IconName::CircleX).small())
                        .child(code)
                        .children(exception.map(|e| {
                            div()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.muted_foreground)
                                .text_size(Tokens::text_sm())
                                .child(e)
                        })),
                )
                .child(
                    div()
                        .id("results-error-message")
                        .test_support()
                        .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                        .text_size(Tokens::text_sm())
                        .text_color(theme.foreground)
                        .child(error.message.clone()),
                )
                .child(
                    div()
                        .text_size(Tokens::text_sm())
                        .text_color(theme.muted_foreground)
                        .child("The assistant will be able to suggest a fix here."),
                )
                .into_any_element(),
        )
    }
}

fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl Render for ResultsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let body: AnyElement = if let Some(error) = self.render_error(cx) {
            error
        } else {
            match self.view {
                ViewKind::Table => self.table.clone().map(|t| t.into_any_element()),
                ViewKind::Json => self.json.clone().map(|j| j.into_any_element()),
                ViewKind::Console => self.console.clone().map(|c| c.into_any_element()),
            }
            .unwrap_or_else(|| div().into_any_element())
        };
        v_flex()
            .id("results-view")
            .test_support()
            .size_full()
            .child(self.render_summary(cx))
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::group_digits;

    #[test]
    fn digits_are_grouped() {
        assert_eq!(group_digits(6), "6");
        assert_eq!(group_digits(10_000), "10,000");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }
}
