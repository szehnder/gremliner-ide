//! Profile view: each step with traverser count, time and a percentage bar, nested for child traversals,
//! with the bottleneck marked. Selecting a top-level step asks for its source to be highlighted.

use std::ops::Range;

use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::profile::{ProfileReport, StepMetric};
use ui_theme::Tokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileViewEvent {
    /// A step was selected; highlight this source range (`None`: the step has no source to show).
    StepSelected(Option<Range<usize>>),
}

/// One displayed row: nesting depth, the metric, and the top-level index for selectable rows.
struct Row {
    depth: usize,
    metric: StepMetric,
    top: Option<usize>,
}

pub struct ProfileView {
    report: ProfileReport,
    ranges: Vec<Option<Range<usize>>>,
    rows: Vec<Row>,
    bottleneck: Option<usize>,
    selected: Option<usize>,
}

impl EventEmitter<ProfileViewEvent> for ProfileView {}

fn flatten(steps: &[StepMetric], depth: usize, top_level: bool, out: &mut Vec<Row>) {
    for (i, s) in steps.iter().enumerate() {
        out.push(Row {
            depth,
            metric: s.clone(),
            top: top_level.then_some(i),
        });
        flatten(&s.children, depth + 1, false, out);
    }
}

impl ProfileView {
    pub fn new(report: ProfileReport, ranges: Vec<Option<Range<usize>>>) -> Self {
        let mut rows = Vec::new();
        flatten(&report.steps, 0, true, &mut rows);
        Self {
            bottleneck: report.bottleneck(),
            report,
            ranges,
            rows,
            selected: None,
        }
    }

    pub fn report(&self) -> &ProfileReport {
        &self.report
    }

    /// The top-level step marked as the bottleneck.
    pub fn bottleneck(&self) -> Option<usize> {
        self.bottleneck
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Select top-level step `i` and ask for its source to be highlighted.
    pub fn select(&mut self, i: usize, cx: &mut Context<'_, Self>) {
        self.selected = Some(i);
        let range = self.ranges.get(i).cloned().flatten();
        cx.emit(ProfileViewEvent::StepSelected(range));
        cx.notify();
    }
}

fn fmt_count(n: Option<i64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_else(|| "–".into())
}

impl Render for ProfileView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let header = |label: &'static str| {
            div()
                .text_size(Tokens::text_xs())
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.muted_foreground)
                .child(label)
        };
        let count_w = Tokens::size(72.0);
        let bar_w = Tokens::size(180.0);
        let summary = h_flex()
            .gap(Tokens::space(3.0))
            .px(Tokens::space(3.0))
            .py(Tokens::space(2.0))
            .text_size(Tokens::text_sm())
            .child(
                div()
                    .text_color(theme.foreground)
                    .child(format!("Total {:.2} ms", self.report.total_ms)),
            )
            .children(
                self.bottleneck
                    .and_then(|b| self.report.steps.get(b))
                    .map(|s| {
                        h_flex()
                            .gap(Tokens::space(1.0))
                            .text_color(theme.warning)
                            .child(Icon::new(IconName::TriangleAlert).xsmall())
                            .child(format!("Slowest: {} ({:.0}%)", s.name, s.pct))
                    }),
            );
        let head = h_flex()
            .gap(Tokens::space(3.0))
            .px(Tokens::space(3.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .child(div().flex_1().min_w_0().child(header("Step")))
            .child(div().w(count_w).child(header("Count")))
            .child(div().w(count_w).child(header("Traversers")))
            .child(div().w(count_w).child(header("Time (ms)")))
            .child(div().w(bar_w).child(header("% of time")));
        let rows = self.rows.iter().enumerate().map(|(n, row)| {
            let top = row.top;
            let is_bottleneck = top.is_some() && top == self.bottleneck;
            let selected = top.is_some() && top == self.selected;
            let pct = row.metric.pct.clamp(0.0, 100.0);
            let bar_color = if is_bottleneck {
                theme.warning
            } else {
                theme.primary
            };
            h_flex()
                .id(SharedString::from(format!("profile-row-{n}")))
                .test_support()
                .gap(Tokens::space(3.0))
                .px(Tokens::space(3.0))
                .py(Tokens::space(1.0))
                .text_size(Tokens::text_sm())
                .when(selected, |el| el.bg(theme.accent))
                .when(top.is_some(), |el| {
                    el.cursor_pointer()
                        .hover(|s| s.bg(theme.accent.opacity(0.5)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(i) = top {
                                this.select(i, cx);
                            }
                        }))
                })
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(Tokens::space(1.0))
                        .pl(Tokens::space(4.0 * row.depth as f32))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                                .text_color(if is_bottleneck {
                                    theme.warning
                                } else {
                                    theme.foreground
                                })
                                .child(row.metric.name.clone()),
                        )
                        .when(is_bottleneck, |el| {
                            el.child(
                                div()
                                    .id("profile-bottleneck")
                                    .test_support()
                                    .flex_none()
                                    .px(Tokens::space(1.5))
                                    .rounded(Tokens::RADIUS_SM)
                                    .bg(theme.warning.opacity(0.15))
                                    .text_color(theme.warning)
                                    .text_size(Tokens::text_xs())
                                    .child("Bottleneck"),
                            )
                        }),
                )
                .child(div().w(count_w).child(fmt_count(row.metric.count)))
                .child(div().w(count_w).child(fmt_count(row.metric.traversers)))
                .child(div().w(count_w).child(format!("{:.3}", row.metric.dur_ms)))
                .child(
                    h_flex()
                        .w(bar_w)
                        .gap(Tokens::space(2.0))
                        .child(
                            div()
                                .flex_1()
                                .h(Tokens::size(6.0))
                                .rounded(Tokens::RADIUS_SM)
                                .bg(theme.muted)
                                .child(
                                    div()
                                        .h_full()
                                        .rounded(Tokens::RADIUS_SM)
                                        .w(relative((pct / 100.0) as f32))
                                        .bg(bar_color),
                                ),
                        )
                        .child(
                            div()
                                .w(Tokens::size(44.0))
                                .text_color(theme.muted_foreground)
                                .child(format!("{pct:.1}%")),
                        ),
                )
        });
        let sections = self
            .report
            .optimized
            .iter()
            .map(|t| ("Optimized traversal".to_owned(), t.clone()))
            .chain(
                self.report
                    .sections
                    .iter()
                    .map(|s| (s.title.clone(), s.text.clone())),
            )
            .map(|(title, text)| {
                v_flex()
                    .gap(Tokens::space(1.0))
                    .px(Tokens::space(3.0))
                    .py(Tokens::space(2.0))
                    .child(
                        div()
                            .text_size(Tokens::text_sm())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                            .text_size(Tokens::text_xs())
                            .text_color(theme.muted_foreground)
                            .whitespace_normal()
                            .child(text),
                    )
            });
        v_flex()
            .id("profile-view")
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .child(summary)
            .child(head)
            .children(rows)
            .children(sections)
    }
}
