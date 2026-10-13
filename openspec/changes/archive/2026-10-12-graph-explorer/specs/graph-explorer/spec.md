# Spec Delta

## Purpose

Lets users explore the graph by building path searches visually from schema labels and typed property filters, without writing Gremlin.

## ADDED Requirements

### Requirement: Path builder
The explorer SHALL let the user build a path of alternating vertex and edge steps that starts and ends with a vertex step, choosing each step's label from the schema or "any", and choosing the direction (out, in, both) for edge steps.

#### Scenario: Build a two-hop path
- **WHEN** the user builds airport → route (out) → airport → contains (in) → country
- **THEN** the generated query traverses outgoing route edges and incoming contains edges between those labels

#### Scenario: Path must end on a vertex
- **WHEN** the last step in the builder is an edge step
- **THEN** the run button is disabled with a hint to add a vertex step

### Requirement: Typed filters
Each step SHALL accept property filters whose available operators depend on the property's data type: comparison and range operators for numbers and dates, and text operators for strings.

#### Scenario: Numeric filter operators
- **WHEN** the user adds a filter on the numeric `runways` property
- **THEN** the operator list includes greater than, less than, between, inside and outside and excludes starting with

#### Scenario: Text filter
- **WHEN** the user filters airport `city` starting with "San"
- **THEN** the generated query uses a text predicate for "starts with" and the results contain only cities starting with "San"

### Requirement: Search options
The explorer SHALL support a result limit (default 200), randomised results, and a choice to clear the graph on each new search or accumulate.

#### Scenario: Limit applied
- **WHEN** the user runs a search with limit 50
- **THEN** at most 50 paths are returned

### Requirement: Generated Gremlin
The explorer SHALL display the Gremlin query it generates, updating as the path changes, with values passed as parameters, and SHALL offer to open it in an editor tab.

#### Scenario: Open in editor
- **WHEN** the user clicks "Open in editor"
- **THEN** a new tab opens with the generated query and its parameter values

### Requirement: Search from a vertex
The graph view's context menu SHALL offer "Search path from here", which opens the explorer with the start step fixed to that vertex.

#### Scenario: Start from a node
- **WHEN** the user chooses "Search path from here" on AUS
- **THEN** the explorer opens with the first step set to the AUS vertex by id
