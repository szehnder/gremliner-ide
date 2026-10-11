# Spec Delta

## Purpose

Understands Gremlin query text so the IDE can highlight it, report errors, format it, find statements and steps, and detect mutations and parameters.

## ADDED Requirements

### Requirement: Parse the Gremlin language
The language service SHALL parse the TinkerPop Gremlin language, including traversal sources, chained steps, anonymous traversals, predicates, enums, typed numeric literals, map and list literals, variables, comments and multiple statements, without reporting errors for valid queries.

#### Scenario: Valid corpus parses cleanly
- **WHEN** the language service parses each query in the reference corpus of valid Gremlin queries
- **THEN** no query produces a diagnostic

#### Scenario: Nested anonymous traversal
- **WHEN** parsing `g.V().where(__.out('knows').has('age', P.gt(30))).values('name')`
- **THEN** the step list contains `V`, `where`, `values` at the top level and `out`, `has` nested inside `where`

### Requirement: Syntax diagnostics
The language service SHALL report syntax errors with precise ranges and readable messages, and SHALL keep analysing the rest of the text after an error.

#### Scenario: Unclosed parenthesis
- **WHEN** parsing `g.V().has('name', 'marko'`
- **THEN** one diagnostic is reported at the end of the text saying a closing parenthesis is missing

#### Scenario: Error recovery
- **WHEN** parsing two statements where only the first has an error
- **THEN** the second statement is still parsed and its steps are available

### Requirement: Statement at cursor
The language service SHALL split text into statements separated by blank lines or semicolons and identify the statement containing a given cursor position.

#### Scenario: Pick the statement under the cursor
- **WHEN** the text contains three statements separated by blank lines and the cursor is inside the second
- **THEN** the second statement's text and range are returned

### Requirement: Mutation detection
The language service SHALL classify a statement as mutating if it contains any step that writes or deletes data, or if it cannot be fully parsed.

#### Scenario: Detect drop
- **WHEN** analysing `g.V().has('name','x').drop()`
- **THEN** the statement is classified as mutating

#### Scenario: Read-only query
- **WHEN** analysing `g.V().hasLabel('airport').values('code')`
- **THEN** the statement is classified as read-only

#### Scenario: Unparseable treated as mutating
- **WHEN** analysing a statement with a syntax error
- **THEN** the statement is classified as mutating for safety purposes

### Requirement: Parameter detection
The language service SHALL report free identifiers used as step arguments that are not Gremlin keywords, enums or step names as query parameters, in order of first appearance.

#### Scenario: Detect parameters
- **WHEN** analysing `g.V().has('airport','code',code).out().limit(n)`
- **THEN** the parameters are `code` and `n`

### Requirement: Formatting
The language service SHALL format Gremlin consistently: one step per line when a line exceeds the configured width, indentation for nested traversals, and normalised spacing, without changing the query's meaning.

#### Scenario: Format long query
- **WHEN** formatting a single-line query longer than 80 characters
- **THEN** the output breaks it into one step per line with continuation indentation, and parsing the output yields the same step structure

#### Scenario: Idempotent formatting
- **WHEN** formatting already-formatted output
- **THEN** the output is unchanged

### Requirement: Prefix at step
The language service SHALL produce, for any step in a statement, a valid executable query consisting of the traversal up to and including that step, including steps nested inside anonymous traversals.

#### Scenario: Top-level prefix
- **WHEN** requesting the prefix at step `out` in `g.V().has('code','AUS').out('route').values('code')`
- **THEN** the result is `g.V().has('code','AUS').out('route')`

### Requirement: Step documentation
The language service SHALL provide a short description, signature list and documentation link for every standard Gremlin step.

#### Scenario: Hover documentation
- **WHEN** requesting documentation for the step `repeat`
- **THEN** a description, its signatures, and a link to the TinkerPop reference are returned

### Requirement: Fast incremental analysis
Re-analysing a 200-line document after a single-character edit SHALL complete within 8 ms on the reference machine.

#### Scenario: Keystroke latency
- **WHEN** the benchmark applies single-character edits to a 200-line document
- **THEN** the 95th percentile time to updated diagnostics is at most 8 ms

### Requirement: Command-line language tools
The headless CLI SHALL lint, format and print the parse tree of Gremlin files or standard input.

#### Scenario: CLI lint exit code
- **WHEN** the user lints a file containing a syntax error
- **THEN** the CLI prints the diagnostic with line and column and exits with a non-zero status
