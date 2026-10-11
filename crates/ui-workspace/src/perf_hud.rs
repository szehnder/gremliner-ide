//! `--perf-hud`: a frame-time overlay. While shown it requests a frame every vsync, so it measures the
//! compositor-paced frame interval (p50/p95/max over the last 240 frames).

use std::collections::VecDeque;
use std::time::Instant;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

use crate::theme::Tokens;

const WINDOW: usize = 240;

#[derive(Default)]
pub struct PerfHud {
    last: Option<Instant>,
    samples: VecDeque<f32>,
}

impl PerfHud {
    /// Record a frame and return the overlay element.
    pub fn frame(&mut self, window: &mut Window, cx: &App) -> impl IntoElement + use<> {
        let now = Instant::now();
        if let Some(last) = self.last.replace(now) {
            if self.samples.len() == WINDOW {
                self.samples.pop_front();
            }
            self.samples
                .push_back(now.duration_since(last).as_secs_f32() * 1000.0);
        }
        window.request_animation_frame();
        let mut sorted: Vec<f32> = self.samples.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let pct = |p: f32| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let ix = ((sorted.len().saturating_sub(1)) as f32 * p).round() as usize;
            sorted.get(ix).copied().unwrap_or(0.0)
        };
        let text = format!(
            "p50 {:.1} ms  p95 {:.1} ms  max {:.1} ms",
            pct(0.5),
            pct(0.95),
            sorted.last().copied().unwrap_or(0.0)
        );
        let theme = cx.theme();
        div()
            .id("perf-hud")
            .absolute()
            .top(Tokens::space(12.0))
            .right(Tokens::space(3.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .rounded(Tokens::RADIUS_SM)
            .bg(theme.popover.opacity(0.92))
            .border_1()
            .border_color(theme.border)
            .font_family(crate::theme::fonts::MONO_FONT_FAMILY)
            .text_size(Tokens::text_xs())
            .text_color(theme.foreground)
            .child(text)
    }
}
