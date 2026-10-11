# Spec Delta

## Purpose

Arranges graph nodes automatically using a choice of algorithms, animating smoothly and never blocking interaction.

## ADDED Requirements

### Requirement: Force-directed default layout
New graphs SHALL be laid out with a force-directed layout by default, animating until it converges or the user stops it.

#### Scenario: Air-routes converges quickly
- **WHEN** the layout benchmark runs the default layout on the full air-routes graph
- **THEN** it reaches its convergence threshold within 2 seconds on the reference machine

#### Scenario: Interaction during layout
- **WHEN** the default layout is running on a 10,000-node graph
- **THEN** panning and zooming stay above 60 frames per second

### Requirement: Alternative layouts
The user SHALL be able to choose hierarchical (top-down or left-right), radial, circular, grid and concentric layouts from the graph toolbar.

#### Scenario: Hierarchical layout of a tree
- **WHEN** the user applies the top-down hierarchical layout to a tree-shaped result
- **THEN** every edge points from a higher layer to a lower layer and no two nodes overlap

#### Scenario: Radial around selection
- **WHEN** the user selects a node and applies the radial layout
- **THEN** the selected node is at the centre and nodes are placed in rings by hop distance

### Requirement: Pinned nodes respected
Layouts SHALL keep pinned nodes at their positions.

#### Scenario: Pinned node stays
- **WHEN** a node is pinned and the user re-runs the force layout
- **THEN** the pinned node's position is unchanged

### Requirement: Incremental placement
When nodes are added to an existing graph, the layout SHALL place them near their connected nodes and SHALL NOT move existing nodes by more than a small distance.

#### Scenario: Expand keeps context
- **WHEN** the user expands a node in a laid-out graph
- **THEN** new nodes appear around the expanded node and previously placed nodes move by less than 10% of the viewport width

### Requirement: Layout controls
The user SHALL be able to stop a running layout, re-run it, and adjust per-layout options; the chosen layout and options SHALL persist per tab.

#### Scenario: Stop layout
- **WHEN** the user clicks "Stop layout" during animation
- **THEN** nodes stop moving within one frame

### Requirement: Deterministic layouts
Given the same graph, options and seed, a layout SHALL produce the same positions.

#### Scenario: Same seed same result
- **WHEN** the force layout runs twice on the same graph with the same seed
- **THEN** the resulting positions are identical
