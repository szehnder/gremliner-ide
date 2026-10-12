# nl-to-gremlin Specification

## Purpose
Turns natural-language requests into Gremlin queries that are checked against the connected schema and test-run read-only, with a preview, before the user runs them.

## Requirements

### Requirement: Prompt bar
The editor SHALL provide a prompt bar, opened with Ctrl/Cmd+K, where the user types a natural-language request about the connected graph and submits it with Enter.

#### Scenario: Open and submit
- **WHEN** the user presses Ctrl/Cmd+K, types "airports in Texas" and presses Enter
- **THEN** a result card appears showing pipeline progress

#### Scenario: No LLM configured
- **WHEN** the user opens the prompt bar with no LLM profile configured
- **THEN** the bar explains that a model is needed and offers to open the AI settings

### Requirement: Schema-grounded generation
Generation SHALL include the connected graph's schema summary and provider dialect in the model context, so generated queries use the actual labels, property keys and edge directions.

#### Scenario: Uses real labels
- **WHEN** the user asks "which airports can I fly to from Austin" on air-routes
- **THEN** the generated query uses the `airport` label, the `route` edge label in the outgoing direction, and an Austin-identifying property such as `code` or `city`

### Requirement: Static validation
Every generated query SHALL be checked for syntax errors, labels and property keys not present in the schema, edge labels used between label pairs that the schema does not connect, and steps the connected database does not support.

#### Scenario: Schema mismatch detected
- **WHEN** the model returns a query using the label `airports` that is not in the schema
- **THEN** validation reports the unknown label and the pipeline attempts a repair

### Requirement: Read-only dry run
A generated query classified as read-only SHALL be test-run with a result limit (default 25) and a timeout (default 5 seconds) before being presented; queries classified as mutating SHALL NOT be executed automatically.

#### Scenario: Dry run preview
- **WHEN** a valid read-only query is generated
- **THEN** it is executed with the limit and timeout and the card shows the row count, elapsed time and a preview of the rows

#### Scenario: Mutation never auto-executed
- **WHEN** the user asks "delete all airports in Texas"
- **THEN** the card shows the generated query with a "writes data" warning, no dry run is performed, and Run requires confirmation

### Requirement: Automatic repair
When validation fails, the dry run errors, or it returns no rows where rows were expected, the pipeline SHALL send the diagnostics back to the model and retry, up to 3 repair attempts, showing which attempt is in progress.

#### Scenario: Repair after server error
- **WHEN** the first generated query fails the dry run with a server error
- **THEN** a corrected query is generated using that error and the card shows "Repairing 1/3"

#### Scenario: Give up gracefully
- **WHEN** all repair attempts fail
- **THEN** the card shows the best attempt, the remaining problems, and options to insert anyway or rephrase

### Requirement: Result card actions
The result card SHALL show the query with syntax highlighting, a short explanation, verification badges, and actions to insert it into the editor, run it, copy it, or refine it.

#### Scenario: Insert into editor
- **WHEN** the user clicks Insert on a result card
- **THEN** the query is inserted at the cursor as a new statement and the card closes

#### Scenario: Run from card
- **WHEN** the user clicks Run on a verified read-only query
- **THEN** the query is inserted and executed without the dry-run limit, and the results appear in the results dock

### Requirement: Refinement
Follow-up prompts submitted from a result card SHALL modify the previous query using the conversation so far.

#### Scenario: Refine a query
- **WHEN** after "airports in Texas" the user refines with "only those with more than 3 runways"
- **THEN** the new query keeps the Texas filter and adds a runways filter

### Requirement: Privacy disclosure
The prompt bar SHALL show which LLM profile will be used and whether sample data and preview rows will be shared with it, following the connection's privacy setting.

#### Scenario: Cloud model without sample sharing
- **WHEN** the connection does not allow sharing sample data and the profile is a cloud model
- **THEN** the request contains only schema structure, and dry-run preview rows are not sent back to the model during repair

### Requirement: Quality bar
On the golden evaluation set against air-routes, at least 90% of prompts SHALL produce a query that passes validation and returns a non-empty dry-run result when using the reference cloud model.

#### Scenario: Golden set evaluation
- **WHEN** the assistant evaluation runs the golden set with the reference model
- **THEN** the success rate is at least 90%

### Requirement: CLI natural-language query
The headless CLI SHALL run the full pipeline for a prompt against a connection and print the final query, validation results and dry-run preview, with a JSON output option.

#### Scenario: CLI nl
- **WHEN** the user runs the CLI natural-language command "count airports per country, top 5" on the testenv connection
- **THEN** it prints a verified query, the badges, and a five-row preview
