# Spec Delta

## Purpose

Gives the application a consistent, polished visual language in light and dark modes, built from shared design tokens.

## ADDED Requirements

### Requirement: Light, dark and system themes
The application SHALL offer Light, Dark and System theme modes. System mode SHALL follow the operating system appearance and update live when it changes.

#### Scenario: Switch to dark
- **WHEN** the user selects the Dark theme in settings
- **THEN** every visible surface re-renders in the dark palette without restarting

#### Scenario: Follow system appearance
- **WHEN** the theme is System and the OS switches from light to dark appearance
- **THEN** the application switches to the dark palette within one second

### Requirement: Readable contrast
Text colour roles in both themes SHALL meet WCAG 2.1 AA contrast (4.5:1 for body text, 3:1 for large text and UI glyphs) against the surfaces they are used on.

#### Scenario: Contrast check
- **WHEN** the automated contrast test evaluates every text role against every surface role in both themes
- **THEN** all pairs meet the AA thresholds

### Requirement: Bundled typography
The application SHALL bundle its UI and monospace fonts so the appearance is identical on every platform, independent of installed system fonts.

#### Scenario: Fonts available offline
- **WHEN** the application runs on a machine without Inter or JetBrains Mono installed
- **THEN** UI text renders in Inter and code renders in JetBrains Mono

### Requirement: Adjustable font size
The user SHALL be able to change the UI font size and editor font size from settings, and with zoom actions (Ctrl/Cmd + `=` / `-` / `0`).

#### Scenario: Zoom in
- **WHEN** the user presses Ctrl/Cmd+= twice
- **THEN** UI text grows by two steps and the new size persists across restarts
