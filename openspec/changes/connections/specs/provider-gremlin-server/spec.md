# Spec Delta

## Purpose

Connects the IDE to Apache TinkerPop Gremlin Server and compatible endpoints, with optional TLS and username/password authentication.

## ADDED Requirements

### Requirement: Gremlin Server connection settings
The Gremlin Server provider SHALL accept host, port, path (default `/gremlin`), TLS on/off, optional "accept invalid certificates" for development, optional username and password, and traversal source alias (default `g`).

#### Scenario: Defaults
- **WHEN** the user selects the Gremlin Server provider for a new connection
- **THEN** port defaults to 8182, path to `/gremlin`, TLS off and alias `g`

### Requirement: Capability detection
On connect, the provider SHALL detect the TinkerPop version where possible and record the capabilities the rest of the IDE uses (profile support, explain support, script evaluation).

#### Scenario: Detect TinkerPop version
- **WHEN** connecting to the test server running TinkerPop 3.7
- **THEN** the connection reports TinkerPop version 3.7.x and profile and explain as supported

### Requirement: Alternative traversal source
Queries SHALL be executed against the configured traversal source alias, so that servers hosting multiple graphs can be targeted.

#### Scenario: Use gModern alias
- **WHEN** a connection is configured with alias `gModern` and the user runs `g.V().count()`
- **THEN** the query is evaluated against the modern graph and returns 6
