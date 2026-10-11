//! The command palette: every registered command, fuzzy-searchable, recent first, context-aware.
//!
//! Ranking is pure ([`rank`]) and tested without a window. The view is gpui-kit's `Command` in a dialog,
//! with local filtering off so our ranking and match highlighting are what the user sees.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex};
use gpui_kit::*;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::theme::Tokens;

/// Recently run commands kept for ordering; older entries fall off.
pub const RECENT_LIMIT: usize = 20;
/// `ui_state` key the recent list persists under.
pub const RECENT_KEY: &str = "palette.recent";

/// When a command is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Always,
    /// Only while an editor tab is open.
    EditorOpen,
    /// Only while connected to a graph.
    Connected,
}

/// What the palette knows about the current context when it opens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandContext {
    pub editor_open: bool,
    pub connected: bool,
}

impl When {
    pub fn applies(self, ctx: &CommandContext) -> bool {
        match self {
            When::Always => true,
            When::EditorOpen => ctx.editor_open,
            When::Connected => ctx.connected,
        }
    }
}

/// Palette metadata for one action.
pub struct CommandMeta {
    pub title: &'static str,
    pub category: &'static str,
    pub when: When,
    /// Extra search terms.
    pub keywords: &'static [&'static str],
    pub action: Box<dyn Action>,
}

impl CommandMeta {
    pub fn new(category: &'static str, title: &'static str, action: impl Action) -> Self {
        Self {
            title,
            category,
            when: When::Always,
            keywords: &[],
            action: Box::new(action),
        }
    }

    pub fn when(mut self, when: When) -> Self {
        self.when = when;
        self
    }

    pub fn keywords(mut self, keywords: &'static [&'static str]) -> Self {
        self.keywords = keywords;
        self
    }

    /// Stable identity used for the recent list.
    pub fn id(&self) -> &'static str {
        self.action.name()
    }
}

/// All commands known to the palette, in registration order.
#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<CommandMeta>,
}

impl Global for CommandRegistry {}

impl CommandRegistry {
    pub fn register(cx: &mut App, meta: CommandMeta) {
        cx.default_global::<CommandRegistry>().commands.push(meta);
    }

    pub fn commands(cx: &App) -> &[CommandMeta] {
        cx.try_global::<CommandRegistry>()
            .map(|r| r.commands.as_slice())
            .unwrap_or_default()
    }
}

/// Most-recent-first list of command ids, persisted in `ui_state`.
#[derive(Default, Clone)]
pub struct RecentCommands(pub VecDeque<String>);

impl Global for RecentCommands {}

impl RecentCommands {
    pub fn record(&mut self, id: &str) {
        self.0.retain(|r| r != id);
        self.0.push_front(id.to_owned());
        self.0.truncate(RECENT_LIMIT);
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.0.iter().position(|r| r == id)
    }
}

/// One palette row after ranking.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    /// Index into the registry.
    pub index: usize,
    /// Byte ranges of the title that matched the query.
    pub highlights: Vec<Range<usize>>,
}

/// The input to [`rank`]: the parts of a [`CommandMeta`] ranking looks at.
pub struct Candidate<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub category: &'a str,
    pub keywords: &'a [&'a str],
    pub when: When,
}

impl<'a> From<&'a CommandMeta> for Candidate<'a> {
    fn from(meta: &'a CommandMeta) -> Self {
        Candidate {
            id: meta.id(),
            title: meta.title,
            category: meta.category,
            keywords: meta.keywords,
            when: meta.when,
        }
    }
}

/// Filter by context, fuzzy-match `query` against titles (and keywords), and order the result.
///
/// With an empty query: recently used commands first (most recent first), then the rest in registration
/// order (which is curated: most useful first). With a query: by match score, ties broken by recency, then
/// registration order.
pub fn rank(
    candidates: &[Candidate<'_>],
    query: &str,
    recent: &RecentCommands,
    ctx: &CommandContext,
) -> Vec<Ranked> {
    let query = query.trim();
    let mut rows: Vec<(u32, Option<usize>, Ranked)> = Vec::new();
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut indices = Vec::new();
    for (index, c) in candidates.iter().enumerate() {
        if !c.when.applies(ctx) {
            continue;
        }
        let recency = recent.position(c.id);
        if query.is_empty() {
            rows.push((
                0,
                recency,
                Ranked {
                    index,
                    highlights: Vec::new(),
                },
            ));
            continue;
        }
        indices.clear();
        let title_score =
            pattern.indices(Utf32Str::new(c.title, &mut buf), &mut matcher, &mut indices);
        let highlights = char_indices_to_byte_ranges(c.title, &mut indices);
        // Keywords count, slightly below a title match, and highlight nothing.
        let keyword_score = c
            .keywords
            .iter()
            .filter_map(|k| pattern.score(Utf32Str::new(k, &mut buf), &mut matcher))
            .max()
            .map(|s| s.saturating_sub(s / 4));
        let Some(score) = title_score.max(keyword_score) else {
            continue;
        };
        let highlights = if title_score.is_some() {
            highlights
        } else {
            Vec::new()
        };
        rows.push((score, recency, Ranked { index, highlights }));
    }
    rows.sort_by(|(sa, ra, a), (sb, rb, b)| {
        sb.cmp(sa)
            .then_with(|| match (ra, rb) {
                (Some(x), Some(y)) => x.cmp(y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.index.cmp(&b.index))
    });
    rows.into_iter().map(|(_, _, r)| r).collect()
}

/// nucleo reports matched *char* indices; merge them into byte ranges for `StyledText`.
fn char_indices_to_byte_ranges(text: &str, indices: &mut [u32]) -> Vec<Range<usize>> {
    indices.sort_unstable();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut wanted = indices.iter().copied().peekable();
    for (char_ix, (byte_ix, ch)) in text.char_indices().enumerate() {
        let Some(&next) = wanted.peek() else { break };
        if next as usize != char_ix {
            continue;
        }
        wanted.next();
        let end = byte_ix + ch.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == byte_ix => last.end = end,
            _ => ranges.push(byte_ix..end),
        }
    }
    ranges
}

/// The live palette: its query and current ranking.
pub struct PaletteModel {
    pub query: String,
    pub ctx: CommandContext,
    pub rows: Vec<Ranked>,
}

impl PaletteModel {
    pub fn new(ctx: CommandContext, cx: &App) -> Self {
        let mut model = Self {
            query: String::new(),
            ctx,
            rows: Vec::new(),
        };
        model.rerank(cx);
        model
    }

    pub fn set_query(&mut self, query: &str, cx: &App) {
        if self.query != query {
            self.query = query.to_owned();
            self.rerank(cx);
        }
    }

    fn rerank(&mut self, cx: &App) {
        let candidates: Vec<Candidate<'_>> = CommandRegistry::commands(cx)
            .iter()
            .map(Candidate::from)
            .collect();
        let recent = cx
            .try_global::<RecentCommands>()
            .cloned()
            .unwrap_or_default();
        self.rows = rank(&candidates, &self.query, &recent, &self.ctx);
    }

    /// Titles in display order (tests and screenshots).
    pub fn titles(&self, cx: &App) -> Vec<&'static str> {
        let commands = CommandRegistry::commands(cx);
        self.rows.iter().map(|r| commands[r.index].title).collect()
    }
}

/// Called after the palette runs a command: records it and returns the action to dispatch.
pub type OnRun = Rc<dyn Fn(Box<dyn Action>, &mut Window, &mut App)>;

/// Open the palette as a dialog. `on_run` receives the chosen action after the dialog has closed.
pub fn open(ctx: CommandContext, on_run: OnRun, window: &mut Window, cx: &mut App) {
    let model = Rc::new(RefCell::new(PaletteModel::new(ctx, cx)));
    let state = cx.new(|cx| CommandState::new(window, cx));
    // The dialog focuses itself when it opens; move focus into the search field after that.
    let focus_state = state.clone();
    window.defer(cx, move |window, cx| {
        focus_state.update(cx, |state, cx| state.focus(window, cx));
    });
    window.open_dialog(cx, move |dialog, _, cx| {
        let commands = CommandRegistry::commands(cx);
        let rows = model.borrow().rows.clone();
        let items = rows.iter().map(|row| {
            let meta = &commands[row.index];
            let action = meta.action.boxed_clone();
            let title: SharedString = meta.title.into();
            let category: SharedString = meta.category.into();
            let highlights = row.highlights.clone();
            CommandItem::new()
                .label(title.clone())
                .child(move |window, cx| {
                    render_row(&category, &title, &highlights, action.as_ref(), window, cx)
                })
        });
        let query_model = model.clone();
        let confirm_model = model.clone();
        let on_run = on_run.clone();
        dialog
            .w(Tokens::PALETTE_WIDTH)
            .margin_top(Tokens::DIALOG_TOP)
            .p_0()
            .close_button(false)
            .child(
                Command::new(&state)
                    .bordered(false)
                    .filterable(false)
                    .max_h(Tokens::palette_list_height())
                    .placeholder("Type a command…")
                    .items(items)
                    .on_query(move |query, window, cx| {
                        query_model.borrow_mut().set_query(query, cx);
                        window.refresh();
                    })
                    .on_confirm(move |path, window, cx| {
                        let action = {
                            let model = confirm_model.borrow();
                            model.rows.get(path.row).map(|row| {
                                CommandRegistry::commands(cx)[row.index]
                                    .action
                                    .boxed_clone()
                            })
                        };
                        let Some(action) = action else { return };
                        cx.default_global::<RecentCommands>().record(action.name());
                        window.close_dialog(cx);
                        on_run(action, window, cx);
                    })
                    .empty(|_, _, cx| {
                        div()
                            .p(Tokens::space(4.0))
                            .text_color(cx.theme().muted_foreground)
                            .child("No matching commands")
                    }),
            )
    });
}

fn render_row(
    category: &SharedString,
    title: &SharedString,
    highlights: &[Range<usize>],
    action: &dyn Action,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let emphasis = HighlightStyle {
        color: Some(theme.primary),
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    let text = StyledText::new(title.clone())
        .with_highlights(highlights.iter().map(|r| (r.clone(), emphasis)));
    h_flex()
        .w_full()
        .gap(Tokens::space(2.0))
        .child(
            div()
                .text_color(theme.muted_foreground)
                .text_size(Tokens::text_sm())
                .child(format!("{category}:")),
        )
        .child(div().flex_1().child(text))
        .children(Kbd::binding_for_action(action, None, window))
}

#[cfg(test)]
mod tests {
    // `gpui_kit::*` (test-support) exports a `test` macro that would shadow the built-in one.
    use super::*;
    use ::core::prelude::v1::test;

    fn candidates() -> Vec<Candidate<'static>> {
        let c = |id, category, title, when| Candidate {
            id,
            title,
            category,
            keywords: &[],
            when,
        };
        vec![
            c("a::ToggleSidebar", "View", "Toggle Sidebar", When::Always),
            c(
                "a::ToggleResultsDock",
                "View",
                "Toggle Results Dock",
                When::Always,
            ),
            c(
                "a::ToggleRightPanel",
                "View",
                "Toggle Right Panel",
                When::Always,
            ),
            c(
                "a::OpenSettings",
                "Preferences",
                "Open Settings",
                When::Always,
            ),
            c("a::Format", "Editor", "Format Query", When::EditorOpen),
            c("a::Reset", "View", "Reset Layout", When::Always),
        ]
    }

    fn titles(rows: &[Ranked], c: &[Candidate<'_>]) -> Vec<String> {
        rows.iter().map(|r| c[r.index].title.to_owned()).collect()
    }

    #[test]
    fn scenario_command_palette_fuzzy_match() {
        let c = candidates();
        let rows = rank(
            &c,
            "tgres",
            &RecentCommands::default(),
            &CommandContext::default(),
        );
        assert_eq!(
            c[rows[0].index].title,
            "Toggle Results Dock",
            "{:?}",
            titles(&rows, &c)
        );
        // Matched characters are reported for highlighting: T, g, R, e, s.
        let title = c[rows[0].index].title;
        let highlighted: String = rows[0]
            .highlights
            .iter()
            .map(|r| &title[r.clone()])
            .collect();
        assert_eq!(highlighted.to_lowercase(), "tgres");
    }

    #[test]
    fn scenario_command_palette_recent_ordering() {
        let c = candidates();
        let mut recent = RecentCommands::default();
        recent.record("a::OpenSettings");
        recent.record("a::ToggleResultsDock");
        let rows = rank(&c, "", &recent, &CommandContext::default());
        let t = titles(&rows, &c);
        assert_eq!(t[0], "Toggle Results Dock");
        assert_eq!(t[1], "Open Settings");
    }

    #[test]
    fn scenario_command_palette_no_editor_open() {
        let c = candidates();
        let none = rank(
            &c,
            "",
            &RecentCommands::default(),
            &CommandContext::default(),
        );
        assert!(!titles(&none, &c).contains(&"Format Query".to_owned()));
        let searched = rank(
            &c,
            "format",
            &RecentCommands::default(),
            &CommandContext::default(),
        );
        assert!(searched.is_empty());
        let open = CommandContext {
            editor_open: true,
            ..Default::default()
        };
        let with_editor = rank(&c, "format", &RecentCommands::default(), &open);
        assert_eq!(titles(&with_editor, &c), vec!["Format Query"]);
    }

    #[test]
    fn empty_query_keeps_registration_order() {
        let c = candidates();
        let rows = rank(
            &c,
            "",
            &RecentCommands::default(),
            &CommandContext::default(),
        );
        assert_eq!(
            titles(&rows, &c),
            vec![
                "Toggle Sidebar",
                "Toggle Results Dock",
                "Toggle Right Panel",
                "Open Settings",
                "Reset Layout"
            ]
        );
    }

    #[test]
    fn recent_list_dedupes_and_caps() {
        let mut recent = RecentCommands::default();
        for i in 0..30 {
            recent.record(&format!("c{i}"));
        }
        recent.record("c25");
        assert_eq!(recent.0.len(), RECENT_LIMIT);
        assert_eq!(recent.0[0], "c25");
        assert_eq!(recent.0.iter().filter(|r| *r == "c25").count(), 1);
    }

    #[test]
    fn byte_ranges_merge_and_respect_utf8() {
        let mut idx = vec![0, 1, 3];
        assert_eq!(
            char_indices_to_byte_ranges("héllo", &mut idx),
            vec![0..3, 4..5]
        );
    }
}
