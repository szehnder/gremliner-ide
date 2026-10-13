//! Console view: Gremlin Console-style lines (`==>v[1]`) or the raw GraphSON response, with text or regex
//! search, match highlighting and next/previous navigation.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::{GValue, console};
use regex::{Regex, RegexBuilder};
use ui_theme::ButtonTip as _;
use ui_theme::Tokens;

/// Every match of `query` in `lines`: (line, byte range). Invalid regexes match nothing (and report it).
pub fn find_matches(
    lines: &[String],
    query: &str,
    regex: bool,
) -> Result<Vec<(usize, Range<usize>)>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = if regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let re: Regex = RegexBuilder::new(&pattern)
        .case_insensitive(!regex)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(lines
        .iter()
        .enumerate()
        .flat_map(|(i, line)| {
            re.find_iter(line)
                .filter(|m| !m.is_empty())
                .map(move |m| (i, m.range()))
                .collect::<Vec<_>>()
        })
        .collect())
}

pub struct ConsoleView {
    items_lines: Rc<Vec<String>>,
    raw_lines: Rc<Vec<String>>,
    raw: bool,
    regex: bool,
    search: Entity<InputState>,
    matches: Rc<Vec<(usize, Range<usize>)>>,
    current: usize,
    search_error: Option<String>,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ConsoleView {
    pub fn new(
        items: &[GValue],
        raw: &[String],
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search output"));
        let subscriptions =
            vec![
                cx.subscribe(&search, |this, _, event: &InputEvent, cx| match event {
                    InputEvent::Change => this.update_matches(cx),
                    InputEvent::PressEnter { shift, .. } => {
                        this.step(if *shift { -1 } else { 1 }, cx)
                    }
                    _ => {}
                }),
            ];
        let mut this = Self {
            items_lines: Rc::new(Vec::new()),
            raw_lines: Rc::new(Vec::new()),
            raw: false,
            regex: false,
            search,
            matches: Rc::new(Vec::new()),
            current: 0,
            search_error: None,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        this.set_items(items, raw, cx);
        this
    }

    pub fn set_items(&mut self, items: &[GValue], raw: &[String], cx: &mut Context<'_, Self>) {
        self.items_lines = Rc::new(items.iter().map(console::line).collect());
        self.raw_lines = Rc::new(
            raw.iter()
                .flat_map(|frame| {
                    let pretty = serde_json::from_str::<serde_json::Value>(frame)
                        .ok()
                        .and_then(|v| serde_json::to_string_pretty(&v).ok())
                        .unwrap_or_else(|| frame.clone());
                    pretty.lines().map(str::to_owned).collect::<Vec<_>>()
                })
                .collect(),
        );
        self.update_matches(cx);
    }

    pub fn lines(&self) -> &[String] {
        if self.raw {
            &self.raw_lines
        } else {
            &self.items_lines
        }
    }

    pub fn set_raw(&mut self, raw: bool, cx: &mut Context<'_, Self>) {
        self.raw = raw;
        self.update_matches(cx);
    }

    pub fn is_raw(&self) -> bool {
        self.raw
    }

    pub fn set_regex(&mut self, regex: bool, cx: &mut Context<'_, Self>) {
        self.regex = regex;
        self.update_matches(cx);
    }

    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    pub fn matches(&self) -> &[(usize, Range<usize>)] {
        &self.matches
    }

    pub fn current_match(&self) -> Option<usize> {
        (!self.matches.is_empty()).then_some(self.current)
    }

    fn update_matches(&mut self, cx: &mut Context<'_, Self>) {
        let query = self.search.read(cx).value().to_string();
        match find_matches(self.lines(), &query, self.regex) {
            Ok(m) => {
                self.matches = Rc::new(m);
                self.search_error = None;
            }
            Err(e) => {
                self.matches = Rc::new(Vec::new());
                self.search_error = Some(e);
            }
        }
        self.current = 0;
        self.reveal_current();
        cx.notify();
    }

    /// Move to the next (+1) or previous (-1) match.
    pub fn step(&mut self, delta: isize, cx: &mut Context<'_, Self>) {
        let n = self.matches.len();
        if n == 0 {
            return;
        }
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let next = (self.current as isize + delta).rem_euclid(n as isize) as usize;
        self.current = next;
        self.reveal_current();
        cx.notify();
    }

    fn reveal_current(&self) {
        if let Some((line, _)) = self.matches.get(self.current) {
            self.scroll.scroll_to_item(*line, ScrollStrategy::Center);
        }
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let count = match (&self.search_error, self.matches.len()) {
            (Some(_), _) => "Invalid pattern".to_owned(),
            (None, 0) => String::new(),
            (None, n) => format!("{} of {n}", self.current + 1),
        };
        let toolbar = h_flex()
            .flex_none()
            .gap(Tokens::space(1.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .child(
                Button::new("console-raw")
                    .label(if self.raw { "Raw GraphSON" } else { "Console" })
                    .ghost()
                    .xsmall()
                    .tip("Toggle between console text and the raw server response")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let raw = !this.raw;
                        this.set_raw(raw, cx);
                    })),
            )
            .child(div().flex_1())
            .child(
                div().w(Tokens::size(220.0)).child(
                    Input::new(&self.search)
                        .xsmall()
                        .prefix(Icon::new(IconName::Search).xsmall()),
                ),
            )
            .child(
                Button::new("console-regex")
                    .label(".*")
                    .xsmall()
                    .when(self.regex, |b| b.primary())
                    .when(!self.regex, |b| b.ghost())
                    .tip("Regular expression")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let regex = !this.regex;
                        this.set_regex(regex, cx);
                    })),
            )
            .child(
                div()
                    .id("console-match-count")
                    .test_support()
                    .min_w(Tokens::size(70.0))
                    .text_size(Tokens::text_xs())
                    .text_color(if self.search_error.is_some() {
                        theme.danger
                    } else {
                        theme.muted_foreground
                    })
                    .child(count),
            )
            .child(
                Button::new("console-prev")
                    .icon(IconName::ChevronUp)
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
            )
            .child(
                Button::new("console-next")
                    .icon(IconName::ChevronDown)
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, _, cx| this.step(1, cx))),
            );

        let lines = if self.raw {
            self.raw_lines.clone()
        } else {
            self.items_lines.clone()
        };
        let matches = self.matches.clone();
        let current = self.matches.get(self.current).cloned();
        let list = uniform_list("console-lines", lines.len(), move |range, _, cx| {
            let theme = cx.theme();
            let mark = HighlightStyle {
                background_color: Some(theme.warning.opacity(0.25)),
                ..Default::default()
            };
            let active = HighlightStyle {
                background_color: Some(theme.warning.opacity(0.6)),
                ..Default::default()
            };
            range
                .map(|ix| {
                    let highlights: Vec<(Range<usize>, HighlightStyle)> = matches
                        .iter()
                        .filter(|(l, _)| *l == ix)
                        .map(|(l, r)| {
                            let style = if current.as_ref() == Some(&(*l, r.clone())) {
                                active
                            } else {
                                mark
                            };
                            (r.clone(), style)
                        })
                        .collect();
                    div()
                        .id(("console-line", ix))
                        .px(Tokens::space(2.0))
                        .whitespace_nowrap()
                        .child(StyledText::new(lines[ix].clone()).with_highlights(highlights))
                        .into_any_element()
                })
                .collect()
        })
        .track_scroll(&self.scroll)
        .size_full();

        v_flex()
            .id("console-view")
            .test_support()
            .size_full()
            .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
            .text_size(Tokens::text_sm())
            .child(toolbar)
            .child(div().flex_1().min_h_0().child(list))
    }
}

#[cfg(test)]
mod tests {
    use super::find_matches;

    #[test]
    fn text_and_regex_search() {
        let lines: Vec<String> = ["==>v[1]", "==>v[22] v[3]", "==>e[7][1-knows->2]"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let m = find_matches(&lines, r"v\[\d+\]", true).unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m[1], (1, 3..8));
        assert_eq!(
            find_matches(&lines, "KNOWS", false).unwrap().len(),
            1,
            "plain text is case-insensitive"
        );
        assert!(find_matches(&lines, "v[", true).is_err());
        assert_eq!(
            find_matches(&lines, "v[", false).unwrap().len(),
            3,
            "plain text is literal"
        );
    }
}
