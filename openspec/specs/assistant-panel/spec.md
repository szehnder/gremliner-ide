# assistant-panel Specification

## Purpose
Provides a conversational, schema-aware assistant that explains, fixes and optimizes queries and describes results, without ever changing data on its own.

## Requirements

### Requirement: Conversation per tab
The assistant panel SHALL keep a separate conversation for each editor tab, stream answers as formatted Markdown, and allow clearing the conversation.

#### Scenario: Separate conversations
- **WHEN** the user asks a question in tab A and then switches to tab B
- **THEN** tab B's assistant conversation does not contain tab A's messages

### Requirement: Explain query
The "Explain query" action SHALL produce a step-by-step explanation of the current statement, and hovering an explanation item SHALL highlight the corresponding steps in the editor.

#### Scenario: Explain with step mapping
- **WHEN** the user invokes Explain on `g.V().has('code','AUS').out('route').values('code')`
- **THEN** the explanation has an item per step and hovering the `out('route')` item highlights that step in the editor

### Requirement: Fix error
When the last run failed, the "Fix error" action SHALL send the failing query and error to the model and present a corrected query that has passed validation and, if read-only, a dry run.

#### Scenario: Fix a failing query
- **WHEN** the last run failed with an error and the user clicks "Fix error"
- **THEN** the panel shows a corrected query with verification badges and an explanation of the cause

### Requirement: Optimize
The "Optimize" action SHALL send the query and, when available, its profile output, and present an alternative query with reasoning; the alternative SHALL be verified to return the same number of results as the original on a bounded sample.

#### Scenario: Optimize with profile
- **WHEN** the user invokes Optimize after profiling a query
- **THEN** the suggestion references the slowest steps from the profile and shows an "equivalent on sample" badge when counts match

### Requirement: Describe result
The "Describe result" action SHALL summarise the current result's size and shape; sample values SHALL be included only if the connection's privacy setting allows it.

#### Scenario: Describe without samples
- **WHEN** sample sharing is off and the user invokes Describe result
- **THEN** the request includes counts, labels and column names but no property values

### Requirement: Runnable code blocks
Gremlin code blocks in answers SHALL offer Insert, Run and Copy actions; Run SHALL follow the same mutation safety rules as the editor.

#### Scenario: Insert from answer
- **WHEN** the user clicks Insert on a code block in an answer
- **THEN** the query is inserted into the active editor as a new statement

### Requirement: Read-only tools
For models that support tool calling, the assistant SHALL be able to call read-only tools to fetch the schema, fetch sample values, validate a query, and dry-run a read-only query; each tool call and its result summary SHALL be visible in the conversation. Tools SHALL never execute mutating queries.

#### Scenario: Visible tool call
- **WHEN** the model calls the dry-run tool during an answer
- **THEN** the conversation shows the tool call with its query and a summary of the result

#### Scenario: Mutation refused by tool
- **WHEN** the model calls the dry-run tool with a mutating query
- **THEN** the tool returns a refusal and no query is sent to the database

### Requirement: Context transparency
Before sending, the panel SHALL show chips for each context item to be included (query, error, profile, schema, sample data), and the user SHALL be able to remove any of them.

#### Scenario: Remove context
- **WHEN** the user removes the "schema" chip and sends a question
- **THEN** the request does not include the schema summary
