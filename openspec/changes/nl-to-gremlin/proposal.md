# Proposal

## Why

The signature feature: describe what you want to see in plain English and get a Gremlin query that has been
checked to actually work against the connected graph, with a preview, before you run it. Unchecked text-to-query
(Compass, G.V()'s OpenAI option) often yields plausible but wrong queries; grounding and verification fix that.

## What Changes

- Prompt bar above the editor (Ctrl/Cmd+K): type a request, press Enter.
- The code-driven pipeline (ADR-0019): context (schema per ADR-0021, provider dialect, current query, conversation) →
  structured generation → static validation (syntax, schema, unsupported steps, mutation) → read-only dry run with an injected limit and timeout (ADR-0020) → repair loop (≤ 3) → result card.
- Result card: query (highlighted), explanation, badges (Syntax ✓, Schema ✓, Dry run ✓ "12 rows · 34 ms"), preview (mini table, or mini graph for element results), and actions Insert / Run / Copy / Refine.
- Refine: follow-up prompts modify the previous query in context ("only those in Texas", "also show the country").
- Live status in the card while the pipeline runs (Generating → Validating → Test-running → Repairing 1/3).
- Write intent: queries classified as mutating are shown with a warning and never dry-run.
- Golden evaluation set (30 prompts on air-routes, 10 on modern) and `cargo xtask eval-assistant`; `gremliner-cli nl`.

Out of scope: the conversational side panel (assistant-panel) and the autonomous agent (later change).

## Capabilities

### New Capabilities
- `nl-to-gremlin`: turning natural-language requests into verified Gremlin queries with previews and iterative refinement.

### Modified Capabilities
- None.

## Impact

- Crates: `assistant` (pipeline), `ui-assistant` (prompt bar, card), `schema`, `gremlin-syntax`, `llm`, `gremliner-cli`, `xtask`.
- ADRs: 0019, 0020, 0021.
