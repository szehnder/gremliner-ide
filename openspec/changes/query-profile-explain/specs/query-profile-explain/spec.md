# Spec Delta

## Purpose

Shows where a traversal spends its time and how the database plans it, using profile and explain output rendered as visual breakdowns.

## ADDED Requirements

### Requirement: Profile a query
The user SHALL be able to profile the current statement; the Profile view SHALL show each step with its traverser count, time in milliseconds and percentage of total time as a bar, nested for child traversals.

#### Scenario: Profile on Gremlin Server
- **WHEN** the user profiles `g.V().out().out().path()` on the modern graph
- **THEN** the Profile view lists the steps with counts, times and percentages that sum to about 100%

### Requirement: Highlight the bottleneck
The Profile view SHALL visually mark the step with the highest time share, and selecting a step SHALL highlight the matching part of the query in the editor when the step can be matched.

#### Scenario: Slowest step marked
- **WHEN** a profile has one step taking 70% of the time
- **THEN** that step is marked as the bottleneck

#### Scenario: Step to source
- **WHEN** the user selects the profile row for a `has` step
- **THEN** the corresponding `has(...)` call is highlighted in the editor

### Requirement: Neptune profile
On Neptune connections, profiling SHALL use Neptune's profile endpoint and present its parsed output in the same view, including Neptune-specific sections such as the optimized traversal and index operations.

#### Scenario: Parse Neptune profile
- **WHEN** the recorded Neptune profile output is parsed
- **THEN** the view shows the optimized traversal and per-step metrics

### Requirement: Explain a query
The user SHALL be able to explain the current statement; the Explain view SHALL show the applied traversal strategies and the final optimized traversal, or the provider's explain output.

#### Scenario: Explain on Gremlin Server
- **WHEN** the user explains `g.V().hasLabel('person').count()`
- **THEN** the Explain view lists the strategies applied and shows the final traversal

### Requirement: Profile mutation safety
Profiling a mutating statement SHALL require confirmation, because profiling executes the traversal.

#### Scenario: Confirm profiling a mutation
- **WHEN** the user profiles `g.V().drop()`
- **THEN** a confirmation explains the query will modify data before anything runs

### Requirement: CLI profile and explain
The headless CLI SHALL print profile and explain output for a query, as a readable table or JSON.

#### Scenario: CLI profile JSON
- **WHEN** the user runs the CLI profile command with `--json` on the modern graph
- **THEN** the output is valid JSON with a list of steps containing counts and durations
