# Spec Delta

## REMOVED Requirements

### Requirement: Default results view by shape
**Reason**: Element and path results now open in the Graph view (graph-view); replaced by "Initial results view".
**Migration**: None; the Table view stays one click away.

## ADDED Requirements

### Requirement: Initial results view
The results dock SHALL choose the initial view from the result shape: Graph for vertices, edges and paths, Table for maps and properties, JSON for nested or mixed structures, Console for scalars, unless the tab specifies a default view.

#### Scenario: Elements open in graph
- **WHEN** the user runs `g.V().hasLabel('person')` on the modern graph
- **THEN** the Graph view is selected and shows four person nodes, and the Table view lists four person rows
