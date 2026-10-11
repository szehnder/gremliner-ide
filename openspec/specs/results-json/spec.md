# results-json Specification

## Purpose
Presents structured query results as a navigable, expandable tree with type annotations, suitable for nested and mixed results.

## Requirements

### Requirement: Expandable tree
The JSON view SHALL display results as an expandable tree showing keys, values and type annotations (for example `Int64`, `Vertex`, `Path`), with expand/collapse per node and "expand all" / "collapse all" actions.

#### Scenario: Expand a path
- **WHEN** the user runs `g.V(1).out().path()` and expands the first result
- **THEN** the tree shows a Path node whose children are the vertices, each with id and label

### Requirement: Lazy rendering
The JSON view SHALL render only visible rows, so that results with one million nodes open and scroll without freezing.

#### Scenario: Huge result opens quickly
- **WHEN** a result with one million tree nodes is shown
- **THEN** the view becomes interactive within 500 ms

### Requirement: Copy value and path
The user SHALL be able to copy any node's value as JSON and copy its access path.

#### Scenario: Copy value
- **WHEN** the user invokes "Copy value" on a vertex node
- **THEN** the clipboard contains that vertex as JSON
