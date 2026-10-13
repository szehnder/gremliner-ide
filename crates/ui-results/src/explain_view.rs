//! Explain view: the original traversal, each strategy applied (TinkerPop), the final traversal, and any
//! provider sections (Neptune's converted traversal, physical pipeline, predicates).

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;
use gremlin_model::profile::ExplainReport;
use ui_theme::Tokens;

pub struct ExplainView {
    report: ExplainReport,
}

impl ExplainView {
    pub fn new(report: ExplainReport) -> Self {
        Self { report }
    }

    pub fn report(&self) -> &ExplainReport {
        &self.report
    }
}

impl Render for ExplainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let block = |id: &'static str, title: String, text: String| {
            v_flex()
                .id(id)
                .test_support()
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
                        .text_size(Tokens::text_sm())
                        .text_color(theme.foreground)
                        .whitespace_normal()
                        .child(text),
                )
        };
        let strategies = (!self.report.strategies.is_empty()).then(|| {
            v_flex()
                .id("explain-strategies")
                .test_support()
                .px(Tokens::space(3.0))
                .py(Tokens::space(2.0))
                .gap(Tokens::space(1.0))
                .child(
                    div()
                        .text_size(Tokens::text_sm())
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(format!(
                            "Strategies applied ({})",
                            self.report.strategies.len()
                        )),
                )
                .children(self.report.strategies.iter().enumerate().map(|(i, s)| {
                    h_flex()
                        .id(SharedString::from(format!("explain-strategy-{i}")))
                        .gap(Tokens::space(3.0))
                        .items_start()
                        .text_size(Tokens::text_sm())
                        .child(
                            div()
                                .w(Tokens::size(220.0))
                                .flex_none()
                                .text_color(theme.foreground)
                                .child(s.strategy.clone()),
                        )
                        .child(
                            div()
                                .w(Tokens::size(96.0))
                                .flex_none()
                                .text_color(theme.muted_foreground)
                                .child(s.category.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                                .text_size(Tokens::text_xs())
                                .text_color(theme.muted_foreground)
                                .whitespace_normal()
                                .child(s.traversal.clone()),
                        )
                }))
        });
        v_flex()
            .id("explain-view")
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .children(
                self.report
                    .original
                    .clone()
                    .map(|t| block("explain-original", "Original traversal".into(), t)),
            )
            .children(strategies)
            .children(
                self.report
                    .final_traversal
                    .clone()
                    .map(|t| block("explain-final", "Final traversal".into(), t)),
            )
            .children(self.report.sections.iter().map(|s| {
                v_flex()
                    .gap(Tokens::space(1.0))
                    .px(Tokens::space(3.0))
                    .py(Tokens::space(2.0))
                    .child(
                        div()
                            .text_size(Tokens::text_sm())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child(s.title.clone()),
                    )
                    .child(
                        div()
                            .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                            .text_size(Tokens::text_xs())
                            .text_color(theme.muted_foreground)
                            .whitespace_normal()
                            .child(s.text.clone()),
                    )
            }))
    }
}
