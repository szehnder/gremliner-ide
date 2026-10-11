# Spec Delta

## Purpose

Shows query results as fast, sortable, filterable tables, grouping graph elements by label and flattening maps into columns.

## ADDED Requirements

### Requirement: Element tables by label
When results contain vertices or edges, the table view SHALL group them by label with a label selector, showing id, label and one column per property key. Edge tables SHALL also show the out-vertex and in-vertex ids.

#### Scenario: Group by label
- **WHEN** the user runs `g.V()` on the modern graph
- **THEN** the table offers labels "person (4)" and "software (2)", and selecting "software" shows two rows with name and lang columns

### Requirement: Map and scalar tables
When results are maps, the table SHALL use the union of keys as columns; when results are scalars, the table SHALL show a single value column.

#### Scenario: valueMap results
- **WHEN** the user runs `g.V().hasLabel('person').valueMap('name','age')`
- **THEN** the table has name and age columns and four rows

### Requirement: Sort, filter and resize
The user SHALL be able to sort by clicking a column header, filter rows with a quick-search box, and resize and reorder columns.

#### Scenario: Sort by column
- **WHEN** the user clicks the age header twice
- **THEN** rows are sorted by age descending

#### Scenario: Quick filter
- **WHEN** the user types "mar" in the quick-search box
- **THEN** only rows with a cell containing "mar" remain visible

### Requirement: Large result performance
The table SHALL scroll smoothly through 100,000 rows with no frame exceeding 16 ms.

#### Scenario: Scroll 100k rows
- **WHEN** the benchmark scrolls through a 100,000-row result
- **THEN** no frame exceeds 16 ms

### Requirement: Copy and export
The user SHALL be able to copy selected cells as tab-separated text and export the current table to CSV or JSON.

#### Scenario: Export CSV
- **WHEN** the user exports the person table to CSV
- **THEN** the file has a header row and four data rows matching the table
