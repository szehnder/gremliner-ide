# Spec Delta

## Purpose

Lets users view and change application preferences in one place and keeps those preferences across sessions.

## ADDED Requirements

### Requirement: Settings window
Pressing Ctrl/Cmd+, SHALL open a settings view organised into pages, starting with a General page for theme and font sizes. Other features SHALL add their own pages.

#### Scenario: Open settings
- **WHEN** the user presses Ctrl/Cmd+,
- **THEN** the settings view opens on the General page

### Requirement: Immediate apply and persistence
Changes in settings SHALL apply immediately without a save button and SHALL persist across restarts.

#### Scenario: Persist a change
- **WHEN** the user changes the editor font size to 15 and restarts the application
- **THEN** the editor font size is 15

### Requirement: Reset to defaults
Each settings page SHALL offer a "Reset to defaults" action that restores that page's values after confirmation.

#### Scenario: Reset general settings
- **WHEN** the user confirms "Reset to defaults" on the General page
- **THEN** theme and font sizes return to their default values
