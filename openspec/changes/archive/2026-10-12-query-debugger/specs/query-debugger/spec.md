# Spec Delta

## Purpose

Lets users step through a traversal to see intermediate results and counts at each step and find out exactly where individual traversers are filtered out.

## ADDED Requirements

### Requirement: Start a debug session
The user SHALL be able to start a debug session on the current statement with Ctrl+F5 when the statement is syntactically valid and read-only; during the session the editor SHALL be read-only and the current step highlighted.

#### Scenario: Start debugging
- **WHEN** the user presses Ctrl+F5 on `g.V().has('code','AUS').out('route').has('country','US').values('code')`
- **THEN** a debug session starts at the first step, the editor becomes read-only, and the first step is highlighted

#### Scenario: Mutating query refused
- **WHEN** the user starts debugging `g.V().drop()`
- **THEN** the session does not start and a message explains that mutating queries cannot be debugged

### Requirement: Step navigation
The debugger SHALL support Step Over (F10), Step Into (F11), Step Out (Shift+F11), Step Back (F9) and jumping to any step by clicking it.

#### Scenario: Step into a nested traversal
- **WHEN** the current step is `where(out('route').has('code','LHR'))` and the user presses F11
- **THEN** the current step becomes `out('route')` inside the `where`

#### Scenario: Step back
- **WHEN** the user presses F9 at the third step
- **THEN** the current step becomes the second step and its results are shown

### Requirement: Intermediate results
At the current step, the debugger SHALL execute the traversal up to and including that step, with a result limit and timeout, and show the results in the results views.

#### Scenario: Results at a step
- **WHEN** the current step is `out('route')` in the session above
- **THEN** the results views show the airports reachable from AUS, limited to the configured maximum

### Requirement: Inline step counts
During a session, the editor SHALL show the number of traversers after each step inline, computed with a cap and shown as "≥ cap" when exceeded.

#### Scenario: Counts after each step
- **WHEN** the debug session above starts
- **THEN** each step shows a count, for example 1 after `has('code','AUS')` and the number of routes after `out('route')`

### Requirement: Traversal simulation
The debugger SHALL list the traversers at the current step with paging and mark which ones lead to a final result, with an option to hide those that do not.

#### Scenario: Mark failed traversers
- **WHEN** at `out('route')` the simulation lists destinations
- **THEN** destinations outside the US are marked as not producing a result, and "hide failed" removes them from the list

### Requirement: Traverser inspection
For a selected element at any step, the debugger SHALL show its path through the remaining steps and identify the step at which it is filtered out, if any.

#### Scenario: Find where an element drops
- **WHEN** the user inspects LHR from the `out('route')` step
- **THEN** the inspection reports that LHR is filtered out at `has('country','US')`

### Requirement: Unsupported steps explained
For steps whose intermediate results cannot be shown independently (modulators such as `by()`, and the internals of `project`, `group` and `groupCount`), the debugger SHALL show results at the enclosing step and explain why.

#### Scenario: Modulator step
- **WHEN** the user clicks a `by('code')` modulator
- **THEN** the debugger selects the parent step and shows a note that modulators are evaluated as part of it
