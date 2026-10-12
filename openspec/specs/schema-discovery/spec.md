# schema-discovery Specification

## Purpose
Discovers, caches and presents the effective schema of a connected graph (labels, properties, types, connections and samples) for users and for IDE features.

## Requirements

### Requirement: Discover labels, properties and connectivity
On connect, the IDE SHALL discover vertex labels, edge labels, property keys per label with inferred value types, and which vertex labels each edge label connects, in the background.

#### Scenario: Modern graph schema
- **WHEN** schema discovery runs against the modern graph
- **THEN** it reports vertex labels person and software, edge labels knows and created, person properties name (String) and age (Int32), and the triples person-knows->person and person-created->software

### Requirement: Approximate counts and samples
Discovery SHALL record approximate element counts per label and up to five distinct sample values per property, truncated to 64 characters each.

#### Scenario: Sample values
- **WHEN** discovery runs against air-routes
- **THEN** the airport `code` property has up to five sample values such as "AUS" and the airport label has an approximate count

### Requirement: Bounded discovery cost
Sampling-based discovery SHALL limit each discovery query by a sample size and a timeout, and SHALL report partial results when limits are hit rather than failing.

#### Scenario: Timeout yields partial schema
- **WHEN** the property-discovery query exceeds its timeout
- **THEN** the schema includes the labels already discovered and marks property information as partial

### Requirement: Fast path when the database offers statistics
When the connected database offers a schema statistics summary, discovery SHALL use it instead of sampling.

#### Scenario: Neptune summary used
- **WHEN** discovery runs against a Neptune connection whose statistics summary is available
- **THEN** labels and property keys come from the summary and no sampling queries are issued for them

### Requirement: Cache and refresh
The schema SHALL be cached per connection with its discovery time, shown immediately on reconnect, refreshed in the background, and refreshable on demand.

#### Scenario: Cached schema on reconnect
- **WHEN** the user reconnects to a connection whose schema was discovered earlier
- **THEN** the cached schema is available immediately and a background refresh updates it

### Requirement: Schema browser
The sidebar SHALL show the schema under each connected connection as a tree of labels with counts and their properties with types; selecting a label SHALL offer to open a sample query in a new tab.

#### Scenario: Open sample query from schema
- **WHEN** the user chooses "Sample 25" on the airport label
- **THEN** a new tab opens with `g.V().hasLabel('airport').limit(25)` and runs it

### Requirement: Schema graph
The IDE SHALL render the schema as a graph where vertex labels are nodes and edge labels are directed links between the labels they connect.

#### Scenario: Schema graph of air-routes
- **WHEN** the user opens the schema graph for air-routes
- **THEN** it shows airport, country and continent nodes with route and contains links

### Requirement: CLI schema output
The headless CLI SHALL print the discovered schema for a connection, as text or JSON.

#### Scenario: CLI schema JSON
- **WHEN** the user runs the CLI schema command with `--json` on the modern graph
- **THEN** the output is valid JSON listing the labels, properties and triples
