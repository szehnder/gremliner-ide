# Proposal

## Why

Beyond generating queries, users want help understanding and fixing them: why did it fail, what does this
traversal do, why is it slow, what does this result mean. A conversational side panel, grounded in the same schema
and verification machinery, turns the IDE into a Gremlin mentor.

## What Changes

- Assistant panel (right dock, Ctrl/Cmd+Shift+A) with a conversation per editor tab, streamed Markdown answers, and code blocks that carry Insert / Run / Copy actions (verified like nl-to-gremlin when they are queries).
- Quick actions with rich context:
  - **Explain query**: step-by-step explanation mapped to step ranges (hovering an explanation item highlights the steps).
  - **Fix error**: sends the failing query and server error; proposes a verified fix.
  - **Optimize**: sends the query plus `profile()`/Neptune profile output (when available); proposes a faster query with reasoning, verified to return the same result count on a sample.
  - **Describe result**: summarises the current results (counts, distributions; sample rows only if privacy allows).
- Read-only tools for capable models (`get_schema`, `sample_values`, `validate_query`, `dry_run`) per ADR-0019, with each tool call visible in the transcript.
- Context chips showing what will be sent (query, error, profile, schema, sample data), each removable.

Out of scope: autonomous multi-query exploration and write actions (later `assistant-autonomous-agent`).

## Capabilities

### New Capabilities
- `assistant-panel`: a conversational, schema-aware assistant that explains, fixes and optimizes queries and describes results.

### Modified Capabilities
- None.

## Impact

- Crates: `assistant` (modes, tools), `ui-assistant` (panel, Markdown via gpui-kit), `ui-results` (context sources).
- ADRs: 0019, 0020, 0021.
