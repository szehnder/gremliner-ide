# Spec Delta

## Purpose

Makes every application action discoverable and runnable from the keyboard through a searchable command palette.

## ADDED Requirements

### Requirement: Open and search the palette
Pressing Ctrl/Cmd+Shift+P SHALL open a command palette listing all available actions with their keybindings, filtered by fuzzy match as the user types.

#### Scenario: Fuzzy match
- **WHEN** the user opens the palette and types "tgres"
- **THEN** "Toggle Results Dock" appears among the top results with its matched characters highlighted

#### Scenario: Run an action
- **WHEN** the user selects an action with arrow keys and presses Enter
- **THEN** the palette closes and the action runs

### Requirement: Recently used actions first
With an empty query, the palette SHALL list the most recently used actions first.

#### Scenario: Recent ordering
- **WHEN** the user runs "Toggle Results Dock" from the palette and reopens it
- **THEN** "Toggle Results Dock" is the first entry

### Requirement: Context-aware availability
Actions that are not applicable in the current context SHALL be hidden from the palette.

#### Scenario: No editor open
- **WHEN** no editor tab is open
- **THEN** editor-only actions such as "Format Query" are not listed
