# saved-queries Specification

## Purpose
Lets users save named queries with their settings and organise them in folders for reuse.

## Requirements

### Requirement: Save a query with settings
Saving a tab SHALL store its query text and settings (timeout, fetch edges between vertices, default results view, parameter definitions) under a name, globally or under a connection.

#### Scenario: Save and reopen
- **WHEN** the user saves a tab as "Texas airports" with a 10-second timeout and reopens it later
- **THEN** the tab opens with the same text and the 10-second timeout

### Requirement: Folders
Saved queries SHALL be organisable into nested folders that can be created, renamed, moved by drag and deleted; deleting a non-empty folder SHALL require confirmation.

#### Scenario: Move into folder
- **WHEN** the user drags "Texas airports" into the folder "Airports"
- **THEN** the query appears under "Airports" and persists there after restart

### Requirement: Unsaved changes indicator
A tab opened from a saved query SHALL show an indicator when its content differs from the saved version, and Ctrl/Cmd+S SHALL update the saved query.

#### Scenario: Dirty indicator
- **WHEN** the user edits a tab opened from a saved query
- **THEN** the tab shows an unsaved indicator until the user saves
