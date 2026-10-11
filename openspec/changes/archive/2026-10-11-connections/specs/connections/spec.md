# Spec Delta

## Purpose

Lets users create, test, organise and securely store connections to graph databases, and see at a glance which one they are working against.

## ADDED Requirements

### Requirement: Create and edit connections
The user SHALL be able to create, edit, duplicate and delete connections. Each connection SHALL have a unique name, a provider type, provider-specific settings, an optional colour tag, and an environment badge (local, dev, staging, production).

#### Scenario: Create a connection
- **WHEN** the user fills in name "Local", provider Gremlin Server, host localhost, port 8182 and saves
- **THEN** "Local" appears in the connections sidebar and persists across restarts

#### Scenario: Duplicate name rejected
- **WHEN** the user saves a connection with a name that already exists
- **THEN** the form shows an inline error on the name field and nothing is saved

#### Scenario: Delete requires confirmation
- **WHEN** the user deletes a connection
- **THEN** a confirmation is requested, and only after confirming are the connection and its stored secrets removed

### Requirement: Paste-to-fill
When the user pastes a connection URL into the host field, the form SHALL parse it and fill in scheme, host, port, path and TLS settings, and select the matching provider.

#### Scenario: Paste a WebSocket URL
- **WHEN** the user pastes `wss://db.example.com:8182/gremlin` into the host field
- **THEN** host is `db.example.com`, port is 8182, path is `/gremlin` and TLS is enabled

#### Scenario: Paste a Neptune endpoint
- **WHEN** the user pastes a hostname ending in `.neptune.amazonaws.com`
- **THEN** the provider switches to Amazon Neptune and the region is filled in from the hostname

### Requirement: Test connection
The connection form SHALL offer a "Test connection" action that reports reachability, round-trip latency, server/TinkerPop version when detectable, and authentication result, without saving the connection.

#### Scenario: Successful test
- **WHEN** the user tests a connection to the running test server
- **THEN** the form shows success with latency in milliseconds and the detected version

#### Scenario: Unreachable host
- **WHEN** the user tests a connection to a port where nothing listens
- **THEN** the form shows a failure within the connect timeout with a human-readable reason and a suggestion

### Requirement: Secure secret storage
Passwords, tokens and keys SHALL be stored in the operating system's credential store and never written in plain text to application data files or logs.

#### Scenario: Password not on disk
- **WHEN** a connection with a password is saved
- **THEN** the application database and log files do not contain the password string

### Requirement: Environment awareness
The active connection's name, colour tag and environment badge SHALL be visible in the status bar and on every editor tab using it. Production connections SHALL be visually distinct.

#### Scenario: Production badge
- **WHEN** an editor tab is bound to a connection marked production
- **THEN** the tab and status bar show a production badge in the danger colour

### Requirement: Read-only connections
A connection marked read-only SHALL refuse to execute queries that the language analysis identifies as mutating, unless the user explicitly overrides for that single execution.

#### Scenario: Mutation blocked
- **WHEN** the user runs `g.addV('person')` on a read-only connection
- **THEN** execution is blocked with an explanation and an "Run anyway" override that requires confirmation

### Requirement: Connections sidebar
The sidebar SHALL list connections with a status indicator (disconnected, connecting, connected, error), support search by name, and offer connect, disconnect, edit, duplicate and delete actions.

#### Scenario: Status indicator
- **WHEN** the user connects to a reachable connection
- **THEN** its indicator changes from disconnected to connecting to connected

### Requirement: CLI connection management
The headless CLI SHALL list, add, test and remove saved connections using the same storage as the application.

#### Scenario: CLI test
- **WHEN** the user runs the CLI connection test command for a saved connection named "Local"
- **THEN** it prints success with latency and exits with status 0, or prints the failure reason and exits non-zero
