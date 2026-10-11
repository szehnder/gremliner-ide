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
