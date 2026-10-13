# Spec Delta

## Purpose

Defines the cross-cutting user-experience standards every surface must meet: helpful states, consistent motion, full keyboard access, onboarding and accessibility.

## ADDED Requirements

### Requirement: Empty, loading and error states
Every panel and view SHALL have a designed empty state with a next action, a loading state for operations longer than 150 ms, and an error state with a readable message and a recovery action.

#### Scenario: Empty results dock
- **WHEN** no query has been run in a tab
- **THEN** the results dock shows an empty state suggesting to run a query with the shortcut shown

#### Scenario: Error with recovery
- **WHEN** a connection attempt fails
- **THEN** the error state shows the reason and offers "Edit connection" and "Retry"

### Requirement: Consistent motion
Transitions SHALL use the standard motion durations, and SHALL be disabled when the operating system's reduce-motion preference is on.

#### Scenario: Reduce motion
- **WHEN** reduce motion is enabled in the OS
- **THEN** panel and graph-focus transitions happen instantly

### Requirement: Keyboard access
Every user action SHALL be reachable from the keyboard, focus SHALL be visible, and a keyboard shortcuts sheet SHALL list all bindings.

#### Scenario: Shortcuts sheet
- **WHEN** the user opens the keyboard shortcuts sheet
- **THEN** it lists every action that has a keybinding, grouped by area

#### Scenario: Keyboard-only connection
- **WHEN** a user creates a connection and runs a query using only the keyboard
- **THEN** every step is reachable and the focused element is always visibly indicated

### Requirement: First-run onboarding
On first launch, the application SHALL offer a short skippable tour of the main areas and a guided way to connect to a sample graph.

#### Scenario: Skip tour
- **WHEN** the user skips the tour on first launch
- **THEN** the tour does not appear again on later launches

### Requirement: Accessibility semantics
All interactive elements SHALL expose accessible roles and names, and the graph view SHALL expose a textual summary of the selection.

#### Scenario: Accessible names
- **WHEN** the accessibility snapshot test inspects the main window
- **THEN** every button, input and tab has a non-empty accessible name
