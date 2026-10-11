# 0028. No telemetry; local-only diagnostics

- Status: accepted
- Date: 2026-10-11

## Context
Users connect to production graph databases; trust matters.

## Decision
No analytics or crash reporting leaves the machine. Logs (`tracing`) go to a rotating file in the data dir; a
"Copy diagnostics" action bundles logs + versions for bug reports, with query text redaction option.

## Consequences
- We learn about issues only through user reports.

## Revisit when
An opt-in crash reporter is explicitly requested.
