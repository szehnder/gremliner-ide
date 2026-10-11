# Design

## Context

gpui-kit's `InputState::code_editor(language)` provides a code editor with tree-sitter highlighting (custom
languages registered via its highlighter registry), diagnostics, completion/hover provider traits (LSP-shaped),
multi-cursor and search. See proposal.md; ADR-0014 for the language layer.

## Goals / Non-Goals

**Goals:**
- Reuse the gpui-kit editor fully; add Gremlin intelligence through its provider traits.

**Non-Goals:**
- Vim mode (possible later via gpui-kit if offered).

## Decisions

- **Language registration:** register `tree-sitter-gremlin` language + `highlights.scm` with gpui-kit's highlighter
  registry under name `gremlin` (feature `tree-sitter` on gpui-kit).
- **Analysis loop:** on each edit, `ui-editor` forwards the edit to a `gremlin_syntax::Document` (incremental) and
  publishes diagnostics through gpui-kit's diagnostics API. The analysis runs on the foreground thread because it is
  < 1 ms typical; if it exceeds 4 ms (measured), it moves to the background executor with a revision check.
- **Hover:** implement gpui-kit's hover provider using the step catalogue.
- **Current statement highlight:** a background highlight range from `statement_at(cursor)`.
- **Execution entry point:** the editor emits `RunRequested { tab_id, text, range }`; `query-execution` handles it.
- **Tab persistence:** `open_tabs` table (id, order, column, connection_id, content, cursor, options JSON), saved debounced 500 ms after changes and on quit.

## Risks / Trade-offs

- [gpui-kit editor API differences from expectations] → spike first in task 1.1; record findings in `docs/gpui-notes.md`.

## Verification

- Headless UI tests (`#[gpui_kit::test]`) for each scenario.
- Benchmark `editor_typing_10k_lines` via the gpui-kit test harness timing.
- `cargo xtask screenshot -- --open-connection local` shows the editor with a highlighted query.

## As built

- **Crates:** `ui-editor` holds `QueryEditor` (editor state, analysis mirror, problems list, hover, format,
  comment toggle, run requests) and its actions; `ui-workspace` holds the dockable `EditorPanel` (tab
  toolbar with connection picker, Run and Format buttons; title with colour, environment badge and dirty
  marker), the `EditorTabs` registry, and persistence. Tokens moved to a shared `ui-theme` crate (ADR-0035).
- **Analysis loop:** each edit is diffed into one incremental `Document::edit`; documents over 64 KB are
  analysed on the background executor with a revision check (Run re-parses synchronously if stale).
  Measured at 10k lines: p95 15 ms keystroke-to-frame in the headless harness (gpui-kit alone: 6.5 ms).
- **Persistence:** `open_tabs` (id, title, content, cursor, connection) saved 500 ms after changes and on
  close/quit; the dock layout stores only the tab id. An unreadable layout still reopens every stored tab.
- **Run:** `EditorTabs` emits `EditorTabsEvent::RunRequested(RunRequest { tab_id, connection_id, text,
  range })`; `query-execution` subscribes. Until then Run only records the request.
- **Dirty marker:** set when the text differs from the last Save (Ctrl+S); saved queries arrive with
  `query-history-saved`.
- **Keys:** Ctrl/Cmd+Enter and Alt+Enter run, Shift+Alt+F formats, Ctrl/Cmd+/ toggles comments (all in
  `QueryEditor > Input`), Ctrl/Cmd+T new tab, Ctrl/Cmd+\ split right.
- The welcome view hides while any editor tab is open.
