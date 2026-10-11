# Spec Delta

## Purpose

Provides tabbed, Gremlin-aware query editing with highlighting, diagnostics, formatting and keyboard-driven execution.

## ADDED Requirements

### Requirement: Syntax highlighting
The editor SHALL highlight Gremlin step names, traversal sources, predicates and enums, strings, numbers and comments using theme colours, updating on every keystroke.

#### Scenario: Highlight a query
- **WHEN** the user types `g.V().has('code','AUS')`
- **THEN** `g` is styled as a source, `V` and `has` as steps, and both arguments as strings

### Requirement: Inline diagnostics
The editor SHALL underline syntax errors and warnings at their ranges, mark affected lines in the gutter, and list all problems in a panel below the editor; clicking a problem SHALL move the cursor to it.

#### Scenario: Show an error
- **WHEN** the user types `g.V().has('name'`
- **THEN** an error underline appears at the end of the text and the problems list shows one entry

#### Scenario: Jump to problem
- **WHEN** the user clicks an entry in the problems list
- **THEN** the cursor moves to the start of that diagnostic's range

### Requirement: Hover documentation
Hovering over a step name SHALL show its description and signatures.

#### Scenario: Hover a step
- **WHEN** the user hovers over `repeat`
- **THEN** a tooltip shows the description and signatures of `repeat`

### Requirement: Format and comment
The editor SHALL format the whole document or the selection with Shift+Alt+F and toggle line comments with Ctrl/Cmd+/.

#### Scenario: Format document
- **WHEN** the user presses Shift+Alt+F on a long single-line query
- **THEN** the query is reformatted one step per line and the change can be undone in one step

### Requirement: Run at cursor
Ctrl/Cmd+Enter (and Alt+Enter) SHALL run the statement at the cursor, or the selected text if a selection exists. The statement that will run SHALL be subtly highlighted.

#### Scenario: Run second statement
- **WHEN** the tab has two statements and the cursor is in the second when the user presses Ctrl/Cmd+Enter
- **THEN** only the second statement is executed

#### Scenario: Run selection
- **WHEN** the user selects `g.V().limit(3)` inside a longer statement and presses Ctrl/Cmd+Enter
- **THEN** exactly the selected text is executed

### Requirement: Tabs bound to connections
Each editor tab SHALL be bound to one connection chosen from a picker in the tab toolbar; new tabs SHALL default to the most recently used connection.

#### Scenario: New tab defaults
- **WHEN** the user opens a new tab after using connection "Local"
- **THEN** the new tab is bound to "Local"

### Requirement: Tab management and restore
Tabs SHALL support reordering by drag, splitting into two columns, a dirty marker for unsaved changes, and SHALL be restored with their content, cursor and connection after restart.

#### Scenario: Restore tabs
- **WHEN** the user has three tabs with content, quits and relaunches
- **THEN** the three tabs reopen with the same content, cursor positions and connections

#### Scenario: Split editor
- **WHEN** the user invokes "Split Right" on a tab
- **THEN** the editor area shows two columns and the tab moves to the right column

### Requirement: Large document performance
The editor SHALL remain responsive with documents of 10,000 lines, with keystroke-to-render latency under 16 ms.

#### Scenario: Large file typing
- **WHEN** the benchmark types into a 10,000-line document
- **THEN** the 95th percentile keystroke-to-frame latency is under 16 ms
