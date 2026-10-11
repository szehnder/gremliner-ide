# Spec Delta

## Purpose

Speeds up writing correct Gremlin with context-aware completions, signature help and schema-based warnings in the editor.

## ADDED Requirements

### Requirement: Step completions
After a `.` following a traversal, the editor SHALL suggest Gremlin steps that are valid in that position, each with a one-line description, ordered by relevance.

#### Scenario: Suggest steps
- **WHEN** the user types `g.V().ha`
- **THEN** suggestions include `has`, `hasLabel`, `hasId`, `hasKey`, `hasValue` and `hasNot`, each with a description

#### Scenario: Source steps after g
- **WHEN** the user types `g.`
- **THEN** suggestions include traversal source steps such as `V`, `E`, `addV`, `inject` and `with`

### Requirement: Label completions from schema
Inside label arguments, the editor SHALL suggest labels from the discovered schema: vertex labels for vertex-label positions and edge labels for edge-traversal steps.

#### Scenario: Vertex label
- **WHEN** the user types `g.V().hasLabel('` on air-routes
- **THEN** suggestions include `airport`, `country` and `continent`

#### Scenario: Reachable edge labels ranked first
- **WHEN** the user types `g.V().hasLabel('airport').out('` on air-routes
- **THEN** `route` is suggested before edge labels that do not start at airport

### Requirement: Property key completions
Inside property-key arguments, the editor SHALL suggest property keys for the label inferred at that point in the traversal, falling back to all keys when the label is unknown.

#### Scenario: Keys for inferred label
- **WHEN** the user types `g.V().hasLabel('airport').values('`
- **THEN** suggestions include `code`, `city` and `runways` and exclude keys that only exist on other labels

### Requirement: Value completions
For the value argument of `has(key, …)`, the editor SHALL suggest sample values of that property from the schema.

#### Scenario: Sample value suggestion
- **WHEN** the user types `g.V().has('airport','code','A`
- **THEN** suggestions include sample codes starting with "A" if present in the samples

### Requirement: Signature help
While the cursor is inside a step's parentheses, the editor SHALL show that step's signatures with the current parameter highlighted.

#### Scenario: Signature for has
- **WHEN** the cursor is after the first comma in `has('airport', `
- **THEN** signature help shows the `has(label, key, value)` form with the second parameter highlighted

### Requirement: Enum, predicate and snippet completions
The editor SHALL complete enum and predicate names (for example `P.gt`, `TextP.containing`, `Order.desc`) and offer snippets for common patterns.

#### Scenario: Predicate completion
- **WHEN** the user types `has('age', P.`
- **THEN** suggestions include `gt`, `gte`, `lt`, `lte`, `between`, `within` and `without`

### Requirement: Schema warnings with quick fixes
The editor SHALL warn when a label or property key is not in the discovered schema and offer the closest known name as a quick fix.

#### Scenario: Misspelled label
- **WHEN** the query contains `hasLabel('airprt')` on air-routes
- **THEN** a warning says the label is unknown and the quick fix replaces it with `airport`

### Requirement: Completion latency
Completion suggestions SHALL appear within 30 ms of the triggering keystroke on the reference machine.

#### Scenario: Completion speed
- **WHEN** the benchmark triggers completions at 100 positions in a 200-line document with the air-routes schema loaded
- **THEN** the 95th percentile latency is at most 30 ms
