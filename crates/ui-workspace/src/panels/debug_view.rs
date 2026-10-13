//! The debugger's panel inside an editor tab (query-debugger): navigation, the step strip with capped
//! traverser counts, traversal simulation and single-element inspection. Intermediate results go to the
//! results dock through the runner, which cancels a stale step's run when the user moves on.

use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

use debugger::{ElementKind, IDS, Session, format_count};
use futures::StreamExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_client::{Client, Request};
use gremlin_model::GValue;
use uuid::Uuid;

use super::RunRequest;
use crate::connections::ConnectionManager;
use crate::io::spawn_io;
use crate::run::Runner;
use crate::settings::AppSettings;
use crate::theme::Tokens;

/// Counts stop at this many traversers ("≥ 100,000").
pub const COUNT_CAP: u64 = 100_000;
/// Traversers listed by the simulation.
pub const SIM_MAX: usize = 1_000;
pub const SIM_PAGE: usize = 50;
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Count queries in flight at once.
const COUNT_CONCURRENCY: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugViewEvent {
    /// The current step moved; highlight it.
    CurrentChanged,
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimItem {
    pub id: GValue,
    pub label: String,
    /// Leads to a final result; `None` while unknown.
    pub survives: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct Simulation {
    pub step: usize,
    pub kind: Option<ElementKind>,
    pub items: Vec<SimItem>,
    pub page: usize,
    pub hide_failed: bool,
    pub loading: bool,
    /// Why the simulation is unavailable or failed.
    pub message: Option<String>,
}

impl Simulation {
    /// Items shown, after "hide failed".
    pub fn visible(&self) -> Vec<(usize, &SimItem)> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| !(self.hide_failed && it.survives == Some(false)))
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Inspection {
    pub label: String,
    pub from: usize,
    /// Remaining main steps and whether the element is still there after each.
    pub rows: Vec<(usize, Option<bool>)>,
    pub dropped: Option<usize>,
    pub done: bool,
    pub error: Option<String>,
}

pub struct DebugView {
    session: Session,
    /// Offset of the statement in the editor document.
    base: usize,
    tab_id: Uuid,
    connection_id: Uuid,
    counts: HashMap<usize, Result<u64, String>>,
    sim: Option<Simulation>,
    inspection: Option<Inspection>,
    _count_task: Option<Task<()>>,
    sim_task: Option<Task<()>>,
    inspect_task: Option<Task<()>>,
}

impl EventEmitter<DebugViewEvent> for DebugView {}

/// The session's client, connecting first if needed.
async fn client(connection_id: Uuid, cx: &mut AsyncApp) -> Option<Client> {
    let manager = cx.update(|cx| ConnectionManager::global(cx));
    if manager.read_with(cx, |m, _| m.session(connection_id).is_none()) {
        manager
            .update(cx, |m, cx| m.connect(connection_id, cx))
            .await;
    }
    manager.read_with(cx, |m, _| {
        m.session(connection_id).map(|s| s.client.clone())
    })
}

async fn query(
    client: &Client,
    script: String,
    ids: Option<Vec<GValue>>,
) -> Result<Vec<GValue>, String> {
    let mut request = Request::new(script).timeout(QUERY_TIMEOUT);
    if let Some(ids) = ids {
        request = request.binding(IDS, GValue::List(ids));
    }
    client
        .execute_request(request)
        .await
        .map(|r| r.items)
        .map_err(|e| e.to_string())
}

/// A short name for an element: its `code`, `name` or `title` property, else its label and id.
fn element_label(v: &GValue) -> String {
    let (label, id, props) = match v {
        GValue::Vertex(v) => (
            v.label.clone(),
            v.id.clone(),
            ["code", "name", "title"].iter().find_map(|k| {
                v.properties
                    .get(*k)
                    .and_then(|p| p.first())
                    .map(|p| p.value.clone())
            }),
        ),
        GValue::Edge(e) => (e.label.clone(), e.id.clone(), None),
        other => return gremlin_model::console::format(other),
    };
    let id = gremlin_model::console::format(&id);
    match props {
        Some(p) => format!("{} ({label} {id})", gremlin_model::console::format(&p)),
        None => format!("{label} {id}"),
    }
}

impl DebugView {
    pub fn new(
        session: Session,
        base: usize,
        tab_id: Uuid,
        connection_id: Uuid,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut this = Self {
            session,
            base,
            tab_id,
            connection_id,
            counts: HashMap::new(),
            sim: None,
            inspection: None,
            _count_task: None,
            sim_task: None,
            inspect_task: None,
        };
        this.start_counts(cx);
        this
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn counts(&self) -> &HashMap<usize, Result<u64, String>> {
        &self.counts
    }

    pub fn simulation(&self) -> Option<&Simulation> {
        self.sim.as_ref()
    }

    pub fn inspection(&self) -> Option<&Inspection> {
        self.inspection.as_ref()
    }

    /// The current step's range in the editor document.
    pub fn current_range(&self) -> Range<usize> {
        let span = &self.session.step(self.session.current()).span;
        self.base + span.start..self.base + span.end
    }

    /// Count the traversers after every navigable step, a few queries at a time.
    fn start_counts(&mut self, cx: &mut Context<'_, Self>) {
        let queries: Vec<(usize, String)> = (0..self.session.steps().len())
            .filter(|&i| !self.session.step(i).folded)
            .map(|i| (i, self.session.count_query(i, COUNT_CAP)))
            .collect();
        let connection_id = self.connection_id;
        self._count_task = Some(cx.spawn(async move |this, cx| {
            let Some(client) = client(connection_id, cx).await else {
                return;
            };
            let (tx, mut rx) = futures::channel::mpsc::unbounded();
            let io = cx.update(|cx| {
                spawn_io(cx, async move {
                    futures::stream::iter(queries)
                        .map(|(i, q)| {
                            let client = client.clone();
                            async move {
                                let r = query(&client, q, None).await.and_then(|items| {
                                    items
                                        .first()
                                        .and_then(GValue::as_i64)
                                        .map(|n| n.max(0) as u64)
                                        .ok_or_else(|| "no count".to_owned())
                                });
                                (i, r)
                            }
                        })
                        .buffer_unordered(COUNT_CONCURRENCY)
                        .for_each(|r| {
                            let _ = tx.unbounded_send(r);
                            futures::future::ready(())
                        })
                        .await;
                })
            });
            while let Some((i, r)) = rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    this.counts.insert(i, r);
                    cx.notify();
                });
            }
            drop(io);
        }));
    }

    /// Show the current step: highlight it and run its prefix into the results dock.
    pub fn show_current(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.sim = None;
        self.sim_task = None;
        self.inspection = None;
        self.inspect_task = None;
        let limit = AppSettings::general(cx).result_limit.max(1);
        let script = self.session.results_query(self.session.current(), limit);
        let request = RunRequest {
            tab_id: self.tab_id,
            connection_id: Some(self.connection_id),
            text: script,
            range: 0..0,
            mode: ui_editor::RunMode::Run,
            generated: true,
            bindings: Vec::new(),
            settings: Default::default(),
        };
        // A new run cancels the previous step's, so stepping quickly never piles up queries.
        Runner::global(cx).update(cx, |r, cx| r.execute(request, window, cx));
        cx.emit(DebugViewEvent::CurrentChanged);
        cx.notify();
    }

    fn moved(&mut self, before: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.session.current() != before || self.session.note().is_some() {
            self.show_current(window, cx);
        }
    }

    pub fn step_over(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let before = self.session.current();
        self.session.step_over();
        self.moved(before, window, cx);
    }

    pub fn step_into(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let before = self.session.current();
        self.session.step_into();
        self.moved(before, window, cx);
    }

    pub fn step_out(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let before = self.session.current();
        self.session.step_out();
        self.moved(before, window, cx);
    }

    pub fn step_back(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let before = self.session.current();
        self.session.step_back();
        self.moved(before, window, cx);
    }

    pub fn jump(&mut self, i: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        let before = self.session.current();
        self.session.jump(i);
        self.moved(before, window, cx);
    }

    /// List the traversers at the current step and find which lead to a final result.
    pub fn simulate(&mut self, cx: &mut Context<'_, Self>) {
        let step = self.session.current();
        let mut sim = Simulation {
            step,
            loading: true,
            ..Default::default()
        };
        if let Some(why) = self.session.simulation_unavailable(step) {
            sim.loading = false;
            sim.message = Some(why);
            self.sim = Some(sim);
            cx.notify();
            return;
        }
        self.sim = Some(sim);
        cx.notify();
        let at = self.session.results_query(step, SIM_MAX);
        let session = self.session.clone();
        let connection_id = self.connection_id;
        self.sim_task = Some(cx.spawn(async move |this, cx| {
            let Some(client) = client(connection_id, cx).await else {
                let _ = this.update(cx, |this, cx| this.sim_failed("Not connected.".into(), cx));
                return;
            };
            let work = cx.update(|cx| {
                spawn_io(cx, async move {
                    let items = query(&client, at, None).await?;
                    let kind = match items.first() {
                        Some(GValue::Vertex(_)) => Some(ElementKind::Vertex),
                        Some(GValue::Edge(_)) => Some(ElementKind::Edge),
                        _ => None,
                    };
                    let survivors = match kind {
                        Some(kind) => {
                            let ids: Vec<GValue> =
                                items.iter().filter_map(|v| v.element_id().cloned()).collect();
                            Some(query(&client, session.survivors_query(step, kind), Some(ids)).await?)
                        }
                        None => None,
                    };
                    Ok::<_, String>((items, kind, survivors))
                })
            });
            let outcome = match work.await {
                Ok(r) => r,
                Err(e) => Err(e.to_string()),
            };
            let _ = this.update(cx, |this, cx| match outcome {
                Ok((items, kind, survivors)) => {
                    let Some(sim) = this.sim.as_mut().filter(|s| s.step == step) else {
                        return;
                    };
                    sim.loading = false;
                    sim.kind = kind;
                    sim.items = items
                        .iter()
                        .map(|v| SimItem {
                            id: v.element_id().cloned().unwrap_or_else(|| v.clone()),
                            label: element_label(v),
                            survives: survivors.as_ref().map(|s| {
                                v.element_id().is_some_and(|id| s.contains(id))
                            }),
                        })
                        .collect();
                    if kind.is_none() && !sim.items.is_empty() {
                        sim.message = Some(
                            "These are values, not vertices or edges, so they cannot be traced further."
                                .into(),
                        );
                    }
                    cx.notify();
                }
                Err(err) => this.sim_failed(err, cx),
            });
        }));
    }

    fn sim_failed(&mut self, message: String, cx: &mut Context<'_, Self>) {
        if let Some(sim) = self.sim.as_mut() {
            sim.loading = false;
            sim.message = Some(message);
        }
        cx.notify();
    }

    pub fn set_hide_failed(&mut self, hide: bool, cx: &mut Context<'_, Self>) {
        if let Some(sim) = self.sim.as_mut() {
            sim.hide_failed = hide;
            sim.page = 0;
        }
        cx.notify();
    }

    pub fn page(&mut self, delta: isize, cx: &mut Context<'_, Self>) {
        if let Some(sim) = self.sim.as_mut() {
            let pages = sim.visible().len().div_ceil(SIM_PAGE).max(1);
            sim.page = (sim.page as isize + delta).clamp(0, pages as isize - 1) as usize;
        }
        cx.notify();
    }

    /// Trace simulation item `n` through the remaining steps to find where it drops out.
    pub fn inspect(&mut self, n: usize, cx: &mut Context<'_, Self>) {
        let Some(sim) = self.sim.as_ref() else { return };
        let (Some(item), Some(kind)) = (sim.items.get(n).cloned(), sim.kind) else {
            return;
        };
        let from = sim.step;
        let queries = self.session.inspection_queries(from, kind);
        self.inspection = Some(Inspection {
            label: item.label.clone(),
            from,
            rows: queries.iter().map(|(s, _)| (*s, None)).collect(),
            ..Default::default()
        });
        cx.notify();
        let connection_id = self.connection_id;
        self.inspect_task = Some(cx.spawn(async move |this, cx| {
            let Some(client) = client(connection_id, cx).await else {
                return;
            };
            for (row, (step, q)) in queries.into_iter().enumerate() {
                let id = item.id.clone();
                let c = client.clone();
                let work =
                    cx.update(|cx| spawn_io(cx, async move { query(&c, q, Some(vec![id])).await }));
                let result = match work.await {
                    Ok(r) => r,
                    Err(e) => Err(e.to_string()),
                };
                let mut stop = false;
                let _ = this.update(cx, |this, cx| {
                    let Some(insp) = this.inspection.as_mut() else {
                        stop = true;
                        return;
                    };
                    match result {
                        Ok(items) => {
                            let present = items.first().and_then(GValue::as_i64).unwrap_or(0) > 0;
                            insp.rows[row].1 = Some(present);
                            if !present {
                                insp.dropped = Some(step);
                                insp.done = true;
                                stop = true;
                            }
                        }
                        Err(err) => {
                            insp.error = Some(err);
                            insp.done = true;
                            stop = true;
                        }
                    }
                    cx.notify();
                });
                if stop {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(insp) = this.inspection.as_mut() {
                    insp.done = true;
                }
                cx.notify();
            });
        }));
    }

    /// The inspection's conclusion in words.
    pub fn inspection_summary(&self) -> Option<String> {
        let insp = self.inspection.as_ref()?;
        if let Some(err) = &insp.error {
            return Some(format!("Inspection failed: {err}"));
        }
        if !insp.done {
            return None;
        }
        Some(match insp.dropped {
            Some(step) => format!(
                "{} is filtered out at `{}`.",
                insp.label,
                self.session.call_text(step)
            ),
            None => format!("{} reaches the end of the traversal.", insp.label),
        })
    }
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    }
}

impl Render for DebugView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = self.session.current();
        let nav =
            |id: &'static str,
             label: &'static str,
             tip: &'static str,
             cx: &mut Context<'_, Self>,
             f: fn(&mut DebugView, &mut Window, &mut Context<'_, DebugView>)| {
                Button::new(id)
                    .label(label)
                    .ghost()
                    .xsmall()
                    .tooltip(tip)
                    .on_click(cx.listener(move |this, _, window, cx| f(this, window, cx)))
            };
        let toolbar = h_flex()
            .gap(Tokens::space(1.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .child(
                div()
                    .text_size(Tokens::text_sm())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.primary)
                    .mr(Tokens::space(2.0))
                    .child("Debugging"),
            )
            .child(nav(
                "debug-back",
                "Back",
                "Step Back (F9)",
                cx,
                Self::step_back,
            ))
            .child(nav(
                "debug-over",
                "Over",
                "Step Over (F10)",
                cx,
                Self::step_over,
            ))
            .child(nav(
                "debug-into",
                "Into",
                "Step Into (F11)",
                cx,
                Self::step_into,
            ))
            .child(nav(
                "debug-out",
                "Out",
                "Step Out (Shift+F11)",
                cx,
                Self::step_out,
            ))
            .child(div().flex_1())
            .child(
                Button::new("debug-stop")
                    .label("Stop")
                    .icon(IconName::CircleX)
                    .ghost()
                    .xsmall()
                    .tooltip("Stop debugging (Shift+F5)")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DebugViewEvent::Stop))),
            );
        let notes: Vec<String> = self
            .session
            .note()
            .map(str::to_owned)
            .into_iter()
            .chain(self.session.prefix_note(current))
            .collect();
        let chips = (0..self.session.steps().len())
            .filter(|&i| !self.session.step(i).folded)
            .map(|i| {
                let step = self.session.step(i);
                let count = match self.counts.get(&i) {
                    Some(Ok(n)) => format_count(*n, COUNT_CAP),
                    Some(Err(_)) => "error".into(),
                    None => "…".into(),
                };
                let label = format!(
                    "{}{} · {count}",
                    "› ".repeat(step.depth),
                    short(self.session.call_text(i), 36)
                );
                let is_current = i == current;
                Button::new(SharedString::from(format!("debug-step-{i}")))
                    .label(label)
                    .xsmall()
                    .when(is_current, |b| b.primary())
                    .when(!is_current, |b| b.outline())
                    .on_click(cx.listener(move |this, _, window, cx| this.jump(i, window, cx)))
            });
        let strip = h_flex()
            .id("debug-steps")
            .test_support()
            .flex_wrap()
            .gap(Tokens::space(1.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.5))
            .children(chips);

        let sim_section = {
            let header = h_flex()
                .gap(Tokens::space(2.0))
                .child(
                    div()
                        .text_size(Tokens::text_sm())
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(format!(
                            "Traversers at `{}`",
                            short(self.session.call_text(current), 40)
                        )),
                )
                .child(
                    Button::new("debug-simulate")
                        .label("List traversers")
                        .outline()
                        .xsmall()
                        .loading(self.sim.as_ref().is_some_and(|s| s.loading))
                        .on_click(cx.listener(|this, _, _, cx| this.simulate(cx))),
                )
                .when_some(
                    self.sim.as_ref().filter(|s| !s.items.is_empty()),
                    |el, sim| {
                        el.child(
                            Switch::new("debug-hide-failed")
                                .label("Hide failed")
                                .checked(sim.hide_failed)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.set_hide_failed(*checked, cx)
                                })),
                        )
                    },
                );
            let mut body = v_flex().gap(Tokens::space(0.5));
            if let Some(sim) = &self.sim {
                if let Some(msg) = &sim.message {
                    body = body.child(
                        div()
                            .id("debug-sim-message")
                            .test_support()
                            .text_size(Tokens::text_sm())
                            .text_color(theme.muted_foreground)
                            .child(msg.clone()),
                    );
                }
                let visible = sim.visible();
                let pages = visible.len().div_ceil(SIM_PAGE).max(1);
                let page_items: Vec<(usize, SimItem)> = visible
                    .iter()
                    .skip(sim.page * SIM_PAGE)
                    .take(SIM_PAGE)
                    .map(|(n, it)| (*n, (*it).clone()))
                    .collect();
                for (n, item) in page_items {
                    let (mark, color) = match item.survives {
                        Some(true) => ("✓", theme.success),
                        Some(false) => ("✗", theme.danger),
                        None => ("·", theme.muted_foreground),
                    };
                    body = body.child(
                        h_flex()
                            .id(SharedString::from(format!("debug-sim-{n}")))
                            .test_support()
                            .gap(Tokens::space(2.0))
                            .text_size(Tokens::text_sm())
                            .child(div().w(Tokens::size(14.0)).text_color(color).child(mark))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(theme.foreground)
                                    .child(item.label.clone()),
                            )
                            .when(sim.kind.is_some(), |el| {
                                el.child(
                                    Button::new(SharedString::from(format!("debug-inspect-{n}")))
                                        .label("Inspect")
                                        .ghost()
                                        .xsmall()
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| this.inspect(n, cx)),
                                        ),
                                )
                            }),
                    );
                }
                if visible.len() > SIM_PAGE {
                    body = body.child(
                        h_flex()
                            .gap(Tokens::space(2.0))
                            .text_size(Tokens::text_xs())
                            .text_color(theme.muted_foreground)
                            .child(
                                Button::new("debug-sim-prev")
                                    .label("Previous")
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(|this, _, _, cx| this.page(-1, cx))),
                            )
                            .child(format!("Page {} of {pages}", sim.page + 1))
                            .child(
                                Button::new("debug-sim-next")
                                    .label("Next")
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(|this, _, _, cx| this.page(1, cx))),
                            ),
                    );
                }
                if sim.items.len() >= SIM_MAX {
                    body = body.child(
                        div()
                            .text_size(Tokens::text_xs())
                            .text_color(theme.muted_foreground)
                            .child(format!("Showing the first {SIM_MAX} traversers.")),
                    );
                }
            }
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(Tokens::space(1.5))
                .child(header)
                .child(body)
        };

        let inspect_section = self.inspection.as_ref().map(|insp| {
            let rows = insp.rows.iter().map(|(step, present)| {
                let (mark, color, note) = match present {
                    Some(true) => ("✓", theme.success, ""),
                    Some(false) => ("✗", theme.danger, "filtered out here"),
                    None if insp.done => ("", theme.muted_foreground, ""),
                    None => ("…", theme.muted_foreground, ""),
                };
                h_flex()
                    .gap(Tokens::space(2.0))
                    .text_size(Tokens::text_sm())
                    .child(div().w(Tokens::size(14.0)).text_color(color).child(mark))
                    .child(
                        div()
                            .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                            .text_color(theme.foreground)
                            .child(short(self.session.call_text(*step), 48)),
                    )
                    .child(if *present == Some(false) {
                        div()
                            .id("debug-inspection-dropped")
                            .test_support()
                            .text_color(color)
                            .child(note)
                            .into_any_element()
                    } else {
                        div().text_color(color).child(note).into_any_element()
                    })
            });
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(Tokens::space(1.0))
                .child(
                    div()
                        .text_size(Tokens::text_sm())
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(format!(
                            "{} from `{}`",
                            insp.label,
                            short(self.session.call_text(insp.from), 40)
                        )),
                )
                .children(rows)
                .when(!insp.done, |el| el.child(Spinner::new().xsmall()))
                .children(self.inspection_summary().map(|s| {
                    div()
                        .id("debug-inspection-result")
                        .test_support()
                        .text_size(Tokens::text_sm())
                        .text_color(theme.foreground)
                        .child(s)
                }))
        });

        v_flex()
            .id("debug-view")
            .test_support()
            .size_full()
            .bg(theme.background)
            .border_t(Tokens::BORDER)
            .border_color(theme.border)
            .child(toolbar)
            .children(notes.into_iter().map(|n| {
                div()
                    .id("debug-note")
                    .test_support()
                    .px(Tokens::space(2.0))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.warning)
                    .child(n)
            }))
            .child(strip)
            .child(
                h_flex()
                    .id("debug-details")
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .gap(Tokens::space(4.0))
                    .px(Tokens::space(2.0))
                    .py(Tokens::space(1.5))
                    .overflow_y_scroll()
                    .child(sim_section)
                    .children(inspect_section),
            )
    }
}
