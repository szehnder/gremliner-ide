# graph-rendering Specification

## Purpose
Draws node-link graphs interactively and fast enough that panning, zooming and selecting stay smooth with tens of thousands of elements.

## Requirements

### Requirement: Frame-rate targets
On the reference machine, the graph canvas SHALL sustain at least 120 frames per second while panning and zooming a graph of 10,000 nodes and 20,000 edges, and at least 60 frames per second with 50,000 nodes and 100,000 edges.

#### Scenario: Medium graph benchmark
- **WHEN** the render benchmark runs its scripted pan/zoom path on a 10,000-node, 20,000-edge synthetic graph
- **THEN** the 95th percentile frame time is at most 8.3 ms

#### Scenario: Large graph benchmark
- **WHEN** the render benchmark runs on a 50,000-node, 100,000-edge synthetic graph
- **THEN** the 95th percentile frame time is at most 16.7 ms

### Requirement: Camera navigation
The canvas SHALL support panning by dragging empty space, zooming with the scroll wheel or trackpad pinch centred on the cursor, and a "Fit to view" action that frames all visible elements.

#### Scenario: Zoom keeps point under cursor
- **WHEN** the user scrolls to zoom in with the cursor over a node
- **THEN** the node stays under the cursor while the zoom level increases

#### Scenario: Fit to view
- **WHEN** the user invokes "Fit to view"
- **THEN** every visible node is inside the viewport with a margin and the zoom level is the largest that achieves this

### Requirement: Level of detail
The canvas SHALL reduce detail when elements are too small to read: labels SHALL be hidden below a configurable minimum on-screen size, and arrowheads SHALL be hidden below a zoom threshold.

#### Scenario: Labels hide when zoomed out
- **WHEN** the zoom level makes node labels smaller than the minimum label size
- **THEN** no labels are drawn, and they reappear when zooming back in past the threshold

### Requirement: Hover and selection hit-testing
The canvas SHALL identify the node or edge under the cursor, highlight it on hover, and select it on click. Shift-click SHALL add to the selection and dragging with Shift on empty space SHALL draw a selection rectangle.

#### Scenario: Click selects the topmost node
- **WHEN** the user clicks where two nodes overlap
- **THEN** the node drawn on top is selected and the previous selection is cleared

#### Scenario: Rectangle selection
- **WHEN** the user Shift-drags a rectangle over empty space enclosing five nodes
- **THEN** exactly those five nodes are added to the selection

### Requirement: Node dragging
The user SHALL be able to drag a node to move it; a dragged node SHALL become pinned at its new position.

#### Scenario: Drag pins a node
- **WHEN** the user drags a node to a new position and releases it
- **THEN** the node stays at the new position and is marked as pinned

### Requirement: Minimap
The canvas SHALL show an optional minimap of the whole graph with the current viewport outlined; clicking the minimap SHALL centre the viewport on that point.

#### Scenario: Navigate via minimap
- **WHEN** the user clicks the top-left corner of the minimap
- **THEN** the viewport centres on the corresponding top-left region of the graph
