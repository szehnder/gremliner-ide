# app-shell Specification

## Purpose
Provides the main application window and the dockable workspace layout that hosts every other feature of the IDE.

## Requirements

### Requirement: Dockable workspace layout
The application SHALL present a workspace with a left sidebar, a central tabbed editor area, a bottom results dock, a right-side panel area and a status bar. Each dock SHALL be resizable by dragging its splitter and collapsible via a toggle action.

#### Scenario: Toggle the results dock
- **WHEN** the user invokes the "Toggle Results Dock" action
- **THEN** the bottom dock collapses, the editor area grows to fill the space, and invoking the action again restores the previous size

#### Scenario: Resize the sidebar
- **WHEN** the user drags the sidebar splitter 100 logical pixels to the right
- **THEN** the sidebar width increases by 100 logical pixels, clamped to its minimum and maximum widths

### Requirement: Layout persistence
The application SHALL restore the window size, position, dock sizes, collapsed states and panel arrangement from the previous session on startup.

#### Scenario: Restart restores layout
- **WHEN** the user collapses the right panel, quits, and relaunches the application
- **THEN** the right panel is collapsed and all other dock sizes match the previous session

#### Scenario: Corrupt layout falls back to default
- **WHEN** the stored layout cannot be parsed
- **THEN** the application starts with the default layout and logs a warning without showing an error dialog

### Requirement: Welcome empty state
When no editor tab is open, the editor area SHALL show a welcome view with actions to add a connection, open a recent connection, and view keyboard shortcuts.

#### Scenario: First launch
- **WHEN** the application starts with no saved connections and no open tabs
- **THEN** the welcome view is shown with an "Add connection" primary action

### Requirement: Responsive main thread
The application SHALL never block the UI thread on network, disk or LLM I/O; such work SHALL run in the background and report progress in the status bar.

#### Scenario: Slow background task
- **WHEN** a background task takes 5 seconds to complete
- **THEN** the UI keeps responding to input during those 5 seconds and the status bar shows an activity indicator until it finishes

### Requirement: Automation startup flags
The application SHALL accept command-line flags `--perf-hud` (show a frame-time overlay) and `--exit-after <seconds>` (quit automatically) to support automated verification.

#### Scenario: Exit after timeout
- **WHEN** the application is launched with `--exit-after 2`
- **THEN** it renders its first frame and exits with status 0 after about 2 seconds
