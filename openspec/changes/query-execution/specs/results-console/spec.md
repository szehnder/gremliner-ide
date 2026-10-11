# Spec Delta

## Purpose

Shows the raw server output in a Gremlin Console style, with search, for users who want exactly what the server returned.

## ADDED Requirements

### Requirement: Raw output
The console view SHALL show each result item as Gremlin Console-style text (for example `v[1]`, `e[7][1-knows->2]`, `==>29`) and offer a toggle to show the raw GraphSON response.

#### Scenario: Console formatting
- **WHEN** the user runs `g.V(1)` on the modern graph and opens the console view
- **THEN** it shows `==>v[1]`

#### Scenario: Raw GraphSON
- **WHEN** the user toggles raw mode
- **THEN** the console shows the server's GraphSON JSON exactly as received, pretty-printed

### Requirement: Search output
The console view SHALL support text and regular-expression search with match highlighting and next/previous navigation.

#### Scenario: Regex search
- **WHEN** the user searches `v\[\d+\]` with regex enabled
- **THEN** every vertex token is highlighted and the match count is shown
