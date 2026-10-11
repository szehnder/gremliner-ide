# 0021. Schema context sent to LLMs: typed summary by default, sample values opt-in

- Status: accepted
- Date: 2026-10-11

## Context
Grounding needs schema; users may not want data sent to a cloud LLM.

## Decision
By default the LLM receives only: labels, edge connectivity triples, property keys with inferred types and
cardinality, and approximate counts. **Sample values** (a few distinct values per property, truncated) and **dry-run
result previews** are sent only if the connection's "Share sample data with the assistant" toggle is on (default:
on for local LLM endpoints like localhost/127.0.0.1, off for cloud). The UI always shows a "what will be sent" disclosure.

## Alternatives considered
- **Always send samples** — best quality, unacceptable privacy default for production data.

## Consequences
- Prompt builder is a pure function with insta snapshots for each privacy mode.

## Revisit when
Users ask for per-label/per-property redaction rules.
