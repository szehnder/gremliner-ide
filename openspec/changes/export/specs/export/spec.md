# Spec Delta

## Purpose

Lets users export graph visualisations as images and results as data files in standard formats.

## ADDED Requirements

### Requirement: Export graph as PNG
The user SHALL be able to export the graph as a PNG at 1×, 2× or 4× scale, of the visible area or the whole graph, with a theme or transparent background.

#### Scenario: Whole graph at 2x
- **WHEN** the user exports the whole graph as PNG at 2× scale
- **THEN** the file is a valid PNG containing every visible node, at twice the logical resolution

### Requirement: Export graph as SVG
The user SHALL be able to export the graph as an SVG with nodes, edges, arrowheads and captions as vector elements, using the current styles.

#### Scenario: SVG contents
- **WHEN** the user exports a 10-node graph as SVG
- **THEN** the SVG contains 10 node shapes and a text element for each visible caption

### Requirement: Export graph data
The user SHALL be able to export the current graph as GraphML or GraphSON, including element ids, labels and properties.

#### Scenario: GraphML round trip
- **WHEN** the user exports the modern graph view as GraphML and the file is loaded into an empty TinkerGraph
- **THEN** the loaded graph has the same vertices, edges, labels and properties

### Requirement: Export results
The user SHALL be able to export the current results as CSV, JSON or JSON Lines.

#### Scenario: JSON Lines
- **WHEN** the user exports 6 results as JSON Lines
- **THEN** the file has 6 lines, each a valid JSON value

### Requirement: Copy to clipboard
The user SHALL be able to copy the graph as an image and the selected elements as JSON to the clipboard.

#### Scenario: Copy selection as JSON
- **WHEN** the user selects two vertices and chooses "Copy as JSON"
- **THEN** the clipboard contains a JSON array with both vertices
