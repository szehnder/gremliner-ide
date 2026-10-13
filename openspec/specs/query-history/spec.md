# query-history Specification

## Purpose
Keeps an automatic, searchable record of executed queries so users can find, reopen and re-run past work.

## Requirements

### Requirement: Record executions
Every query execution SHALL be recorded with its text, connection, start time, duration, outcome (success, error, cancelled) and result count.

#### Scenario: Entry recorded
- **WHEN** the user runs `g.V().count()` successfully on "Local"
- **THEN** a history entry exists with that text, connection "Local", outcome success and result count 1

### Requirement: Search and filter
The history SHALL support full-text search over query text and filtering by connection and outcome, returning results within 100 ms for 10,000 entries.

#### Scenario: Search history
- **WHEN** the user searches "route" in history
- **THEN** only entries whose text contains "route" are listed, newest first

#### Scenario: Search speed
- **WHEN** the history contains 10,000 entries and the user searches
- **THEN** results appear within 100 ms

### Requirement: Reopen and re-run
The user SHALL be able to open a history entry in a new tab or re-run it on its original connection.

#### Scenario: Re-run from history
- **WHEN** the user chooses "Run again" on an entry
- **THEN** a tab opens bound to the entry's connection and the query runs

### Requirement: Retention
The history SHALL keep at most a configurable number of entries (default 10,000), removing the oldest first, and the user SHALL be able to clear history.

#### Scenario: Retention limit
- **WHEN** a new entry is recorded while the history is at its limit
- **THEN** the oldest entry is removed
