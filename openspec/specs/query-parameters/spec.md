# query-parameters Specification

## Purpose
Lets users write reusable queries with named parameters, supply typed values, and run saved queries as simple parameterised reports.

## Requirements

### Requirement: Detect parameters
The editor SHALL detect parameters in the current statement and list them in a parameters panel in order of first appearance, updating as the user types.

#### Scenario: Parameters listed
- **WHEN** the statement is `g.V().has('airport','code',code).out().limit(n)`
- **THEN** the panel lists `code` and `n`

### Requirement: Typed values
Parameter values SHALL be parsed as JSON: unquoted numbers become numbers, `true`/`false` become booleans, `null` becomes null, and quoted text becomes a string; unparseable unquoted text SHALL be treated as a string.

#### Scenario: Number and string values
- **WHEN** the user enters `"AUS"` for code and `10` for n
- **THEN** code is bound as the string AUS and n as the number 10

### Requirement: Bindings, not substitution
Parameter values SHALL be sent to the server as bindings separate from the query text.

#### Scenario: Query text unchanged
- **WHEN** the user runs the statement with code = "AUS"
- **THEN** the server receives the original query text and a binding for code

### Requirement: Missing values prompt
Running a statement with parameters that have neither a value nor a default SHALL prompt for the missing values before executing.

#### Scenario: Prompt for missing
- **WHEN** the user runs the statement with no value for n and no default
- **THEN** a prompt asks for n and the query runs after it is provided

### Requirement: Defaults and labels saved
Parameter defaults and display labels SHALL be saved with saved queries and restored when they are opened.

#### Scenario: Default restored
- **WHEN** the user saves a query with default n = 10 and reopens it
- **THEN** the parameters panel shows n with default 10

### Requirement: Run as report
A saved query with parameters SHALL be runnable as a report: a form showing the parameter labels and defaults, a run button, and the results below; several report tabs for the same query SHALL be allowed.

#### Scenario: Run report
- **WHEN** the user chooses "Run as report" on a saved query with parameter "Airport code"
- **THEN** a report tab shows an "Airport code" field and, after running, the results
