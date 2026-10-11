# gremlin-client Specification

## Purpose
Sends Gremlin scripts to TinkerPop-compatible servers over WebSockets and returns typed results as they stream in, with timeouts, cancellation and authentication.

## Requirements

### Requirement: Execute a script
The client SHALL submit a Gremlin script with optional parameter bindings to the server and return the complete typed result, the server status, and the elapsed time.

#### Scenario: Count vertices
- **WHEN** the client executes `g.V().count()` against the seeded modern graph
- **THEN** the result is a single 64-bit integer equal to 6 and the elapsed time is reported

#### Scenario: Bindings
- **WHEN** the client executes `g.V().has('name', n).values('age')` with binding `n = "marko"`
- **THEN** the result is the single value 29

### Requirement: Typed decoding
The client SHALL decode all GraphSON v3 types returned by Gremlin Server into typed values, preserving numeric types, map key types, element ids, labels, properties and paths.

#### Scenario: Vertex with properties
- **WHEN** the client executes `g.V().has('name','marko').elementMap()` against the modern graph
- **THEN** the result is a map containing id, label `person`, name `marko` and age 29 as a 32-bit integer

#### Scenario: Path result
- **WHEN** the client executes `g.V(1).out().out().path()` against the modern graph
- **THEN** each result is a path whose objects are vertices with ids and labels

### Requirement: Streaming partial results
The client SHALL deliver results incrementally as the server sends partial response batches, before the final batch arrives.

#### Scenario: Large result streams
- **WHEN** the client executes `g.E()` against air-routes with a batch size of 64
- **THEN** the caller receives the first batch before the final batch, and the total item count equals the number of edges

### Requirement: Timeouts and cancellation
Each request SHALL have a configurable timeout; when it expires or the caller cancels, the client SHALL stop delivering results, report the reason, and keep the connection usable for further requests.

#### Scenario: Timeout
- **WHEN** a request with a 100 ms timeout runs a query that takes longer
- **THEN** the call fails with a timeout error and a subsequent `g.inject(1)` on the same client succeeds

#### Scenario: Cancel
- **WHEN** the caller cancels a streaming request after the first batch
- **THEN** no further batches are delivered and the cancellation is reported

### Requirement: Server errors
The client SHALL surface server-side errors with the server's status code, message and exception details.

#### Scenario: Syntax error
- **WHEN** the client executes `g.V().nosuchstep()`
- **THEN** the call fails with a server evaluation error whose message names the problem

### Requirement: Authentication
The client SHALL support SASL PLAIN username/password authentication and SHALL allow custom headers to be added to the connection handshake for signed authentication schemes.

#### Scenario: SASL credentials
- **WHEN** the client connects to the authenticated test server with valid credentials and runs `g.inject(1)`
- **THEN** the result is 1

#### Scenario: Wrong credentials
- **WHEN** the client connects with an invalid password
- **THEN** the call fails with an authentication error that does not contain the password

### Requirement: Resilient connections
The client SHALL reconnect automatically on the next request after the connection drops, and SHALL support multiple concurrent requests over pooled connections.

#### Scenario: Concurrent requests
- **WHEN** ten requests are issued concurrently
- **THEN** all ten complete with results matching their own request

#### Scenario: Reconnect after server restart
- **WHEN** the server restarts between two requests
- **THEN** the second request succeeds after an automatic reconnect

### Requirement: Command-line query
The headless CLI SHALL execute a query against a server URL and print the results as readable text or, with `--json`, as JSON.

#### Scenario: CLI JSON output
- **WHEN** the user runs the CLI query command with `--json` and `g.V().count()` against the modern graph
- **THEN** standard output is valid JSON containing the result 6 and the elapsed milliseconds
