# graph-stylesheets Specification

## Purpose
Lets users control how graph elements look by label, through reusable named stylesheets with live preview.

## Requirements

### Requirement: Automatic label colours
Labels without an explicit style SHALL receive a colour from the theme's categorical graph palette, assigned deterministically so the same label always gets the same colour.

#### Scenario: Stable auto colour
- **WHEN** the user runs two different queries that both return airport vertices
- **THEN** airport nodes have the same colour in both results

### Requirement: Vertex label styles
For each vertex label the user SHALL be able to set colour, shape, icon, caption template and size; size SHALL be either fixed or proportional to a numeric property within a min/max range.

#### Scenario: Caption template
- **WHEN** the user sets the airport caption to `{code} – {city}`
- **THEN** the AUS node is captioned "AUS – Austin"

#### Scenario: Size by property
- **WHEN** the user binds airport size to `runways` with range 8–32 px
- **THEN** the airport with the most runways is drawn at 32 px and the one with the fewest at 8 px

### Requirement: Edge label styles
For each edge label the user SHALL be able to set colour, width (fixed or bound to a numeric property), caption template and dashed style.

#### Scenario: Edge width by distance
- **WHEN** the user binds route width to `dist`
- **THEN** longer routes are drawn with thicker lines within the configured range

### Requirement: Label density controls
The user SHALL be able to set label density, the minimum on-screen node size at which captions appear, and the maximum caption length (0 means no truncation).

#### Scenario: Truncate captions
- **WHEN** maximum caption length is 5 and a caption is "Austin Bergstrom"
- **THEN** the caption is shown as "Austi…"

### Requirement: Named stylesheets
The user SHALL be able to create, rename, duplicate, delete and switch between named stylesheets, set a default per connection, and changes SHALL apply live to the open graph.

#### Scenario: Switch stylesheet
- **WHEN** the user switches from "Default" to "Presentation"
- **THEN** the graph re-renders immediately with the Presentation styles

#### Scenario: Connection default
- **WHEN** a connection's default stylesheet is "Air" and the user opens a new graph on that connection
- **THEN** the "Air" stylesheet is applied

### Requirement: Import and export
Stylesheets SHALL be exportable to and importable from JSON files.

#### Scenario: Round trip
- **WHEN** the user exports a stylesheet and imports the file
- **THEN** the imported stylesheet's rules equal the original's
