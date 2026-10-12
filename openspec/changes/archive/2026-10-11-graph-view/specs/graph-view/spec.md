# Spec Delta

## Purpose

Turns query results into an interactive node-link graph that users can expand, filter, search and navigate to explore their data.

## ADDED Requirements

### Requirement: Visualise element results
When a query returns vertices, edges, paths or maps containing elements, the Graph view SHALL display the distinct vertices as nodes and edges as directed links, deduplicated by element id.

#### Scenario: Paths become a graph
- **WHEN** the user runs `g.V().has('code','AUS').outE('route').inV().path().limit(10)` on air-routes
- **THEN** the Graph view shows AUS connected to 10 destination airports with 10 directed edges and no duplicate AUS node

### Requirement: Fetch edges between vertices
When the per-tab option is enabled, after a vertex-only result the Graph view SHALL fetch and display edges that connect the displayed vertices.

#### Scenario: Edges between results
- **WHEN** the option is enabled and the user runs `g.V().hasLabel('person')` on the modern graph
- **THEN** the knows edges between marko, vadas and josh are displayed

### Requirement: Fetch missing endpoints
When results contain edges whose endpoint vertices are not in the result, the Graph view SHALL fetch those vertices so every edge is drawn between two nodes.

#### Scenario: Edge-only result
- **WHEN** the user runs `g.E().hasLabel('created')` on the modern graph
- **THEN** all four created edges are drawn with their person and software endpoints

### Requirement: Clear or accumulate
The Graph view SHALL offer a per-tab toggle between clearing the graph on each new query and accumulating results into the existing graph.

#### Scenario: Accumulate results
- **WHEN** accumulation is on and the user runs two queries returning different vertices
- **THEN** the graph contains the union of both results and previously placed nodes keep their positions

### Requirement: Expand neighbours
Double-clicking a node SHALL fetch and add its adjacent edges and vertices in both directions, up to a configurable cap (default 100); the context menu SHALL offer expanding by direction and edge label, and loading the next page beyond the cap.

#### Scenario: Double-click expand
- **WHEN** the user double-clicks AUS on air-routes
- **THEN** up to 100 neighbours are added near AUS and a "100 of N shown — load more" affordance appears when more exist

#### Scenario: Expand by label
- **WHEN** the user chooses "Expand out → route" from the context menu of a node
- **THEN** only outgoing route edges and their target vertices are added

### Requirement: Hide, pin and select
The user SHALL be able to hide selected elements, unhide all, pin and unpin nodes, select all, and select the neighbours of the selection.

#### Scenario: Hide selection
- **WHEN** the user selects two nodes and presses Delete
- **THEN** the two nodes and their edges are hidden but remain in the result tables

### Requirement: Legend and filters
The Graph view SHALL show a legend of vertex and edge labels with counts and visibility toggles, and a filter panel to show only elements matching a label and property predicate.

#### Scenario: Toggle label visibility
- **WHEN** the user turns off the "software" label in the legend
- **THEN** software vertices and their edges disappear and the legend shows them as hidden

#### Scenario: Property filter
- **WHEN** the user filters airports where runways > 3
- **THEN** only airports with more than three runways remain visible

### Requirement: Find in graph
The Graph view SHALL provide a search box that matches nodes by id or property value and centres and highlights the selected match.

#### Scenario: Find a node
- **WHEN** the user searches "LHR" in an air-routes graph containing LHR
- **THEN** the viewport animates to centre LHR and it is highlighted

### Requirement: Graph statistics
The Graph view SHALL display counts of visible and total nodes and edges and the current selection size.

#### Scenario: Counts update
- **WHEN** the user hides 3 of 20 nodes
- **THEN** the status shows 17 of 20 nodes visible
