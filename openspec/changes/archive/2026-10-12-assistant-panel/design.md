# Design

## Context

Builds on the nl-to-gremlin pipeline (verification, privacy, prompt builders) and llm tool calling. See proposal.md; ADR-0019/0020/0021.

## Goals / Non-Goals

**Goals:**
- A single `assistant::Session` per tab with modes sharing context builders.

**Non-Goals:**
- Persisting conversations across restarts (v1 keeps them for the session; history may come later).

## Decisions

- **Modes:** `Mode::{Chat, Explain, Fix, Optimize, Describe}` each with a prompt template (snapshot-tested) and a context recipe.
- **Explain step mapping:** the model is asked to return JSON items `{steps: [step_index…], text}` using step indices we provide
  (we send the statement annotated with `[#n]` markers per step from `gremlin-syntax`); the UI maps indices back to spans.
- **Code-block verification:** fenced ```gremlin blocks in streamed Markdown are detected when closed and run through
  validation (+ dry run if read-only and allowed) asynchronously, adding badges in place.
- **Tools:** JSON-schema tool specs; executor enforces `is_mutating == false` and dry-run limits; tool loop capped at 6 calls per answer.
- **Optimize equivalence:** run `original.limit(K).count()` vs `candidate.limit(K).count()` for K=1000 (not proof; labelled "equivalent on sample").
- **Markdown:** gpui-kit Markdown renderer with a custom code-block component for actions.

## Risks / Trade-offs

- [Step-index JSON from weak models] → fallback to plain explanation without hover mapping.

## Verification

- Fake-LLM tests for each mode and tool behaviour (including mutation refusal).
- Headless UI tests for per-tab conversations, chips and code-block actions.

## As built

- **`assistant::chat`.**
  - Modes: `Mode::{Chat, Explain, Fix, Optimize, Describe}`. `turn(session, input, ctx, generator, runner, dry_runs, emit)`
    emits `TurnEvent`s: `Status`, streamed `Text`, `Tool`, `Block`, `Explain`, `Error` and `Done`.
  - `StreamingGenerator` streams text deltas from `LlmClient`; the scripted LLM streams in three pieces.
- **What each mode does.**
  - Explain sends the statement's steps numbered in text order (nested steps included), using
    `numbered_steps` from the tree-sitter step tree. It asks for `{items:[{steps,text}]}`, maps step
    numbers to spans, and falls back to plain text.
  - Fix and Optimize reuse the verified `pipeline::run`, so their suggestions are validated and dry-run.
  - Optimize first runs `<query>.profile()` read-only. It summarises `TraversalMetrics` slowest first, then
    checks equivalence with `limit(1000).count()` on both queries ("Equivalent on sample").
  - Describe sends a `ResultSummary`: count, shape, label counts and columns. Rows are added only when
    sample data is included and allowed.
  - In Chat, ```gremlin blocks in the answer are verified after it finishes, not while it streams, and get
    badges.
- **Tools.**
  - `get_schema`, `sample_values`, `validate_query` and `dry_run`, offered only when the profile supports
    tool calling, with at most 6 calls per answer.
  - `dry_run` refuses mutations before reaching the database. `sample_values` refuses when sample data is
    off. Each call appears in the transcript as `name: input → summary`.
- **UI.**
  - `ui_assistant::ChatView` shows quick actions (Explain, Fix error, Optimize, Describe result), Clear,
    the transcript, chips and the input:
    - Answer text uses gpui-kit's `TextView::markdown`. Code blocks are our own component (highlighted
      query, badges, Insert/Run/Copy) rather than a custom Markdown block renderer.
    - Hovering an explanation item asks the editor to highlight the item's step spans; this is a new
      `QueryEditor::set_hint_ranges`, with a `hint_highlight` token.
    - Context chips (Query, Error, Result, Schema, Sample data) appear when the context exists. Clicking
      one removes or restores it.
  - `ui_workspace::panels::AssistantPanel` sits in the right dock, behind the inspector:
    - Ctrl/Cmd+Shift+A and the palette's "Show Assistant Panel" open it. `Workspace::toggle_assistant`
      adds it when a saved layout lacks it.
    - It keeps one `ChatView` and `Session` per editor tab and follows `EditorTabs::active`, which is new:
      it is set from `BasePanel::set_active` and on tab creation.
- **Not done:** conversations are not persisted across restarts (a design non-goal), and the transcript
  doesn't scroll to the newest answer automatically.
