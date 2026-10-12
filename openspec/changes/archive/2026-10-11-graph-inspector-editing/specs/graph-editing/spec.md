# Spec Delta

## Purpose

Lets users inspect graph elements in detail and safely create, change and delete graph data from the visual graph.

## ADDED Requirements

### Requirement: Inspect selection
Selecting a single element SHALL show an inspector with its id, label, all properties with their types (including multi-properties and meta-properties for vertices), and degree; edges SHALL also show their endpoints with links to select them.

#### Scenario: Inspect a vertex
- **WHEN** the user selects marko in the modern graph
- **THEN** the inspector shows id 1, label person, name "marko" (String) and age 29 (Int32), with out-degree 3

#### Scenario: Navigate to endpoint
- **WHEN** the user selects an edge and clicks its in-vertex in the inspector
- **THEN** the in-vertex becomes selected and centred

### Requirement: Edit with preview
Every data change made from the UI SHALL be translated into a Gremlin statement that is shown to the user for confirmation before it runs, using parameter bindings for values.

#### Scenario: Edit a property
- **WHEN** the user changes marko's age to 30 and confirms the preview
- **THEN** the statement sets age to 30 on vertex 1 using bindings, and the inspector and graph show the new value after success

### Requirement: Create vertices and edges
The user SHALL be able to create a vertex with a label and properties, and create an edge by Alt-dragging from one node to another and choosing a label.

#### Scenario: Draw an edge
- **WHEN** the user Alt-drags from marko to lop, chooses label "created" and confirms
- **THEN** a new created edge from marko to lop is added to the database and drawn in the graph

### Requirement: Delete elements
The user SHALL be able to delete selected elements; the confirmation SHALL state how many vertices and edges will be removed, including incident edges of deleted vertices.

#### Scenario: Delete a vertex
- **WHEN** the user deletes a vertex with three incident edges
- **THEN** the confirmation states 1 vertex and 3 edges will be removed, and after confirming they disappear from the graph

### Requirement: Undo and redo
Edits made in the session SHALL be undoable and redoable when an inverse operation is known; the undo action SHALL also show a preview before running.

#### Scenario: Undo property change
- **WHEN** the user undoes the age change from 29 to 30
- **THEN** a preview to set age back to 29 is shown and, after confirming, age is 29

### Requirement: Production safety
On connections marked production or read-only, edits SHALL require an additional explicit confirmation naming the connection.

#### Scenario: Production confirmation
- **WHEN** the user confirms an edit on a production connection
- **THEN** a second confirmation that names the connection must be accepted before the statement runs
