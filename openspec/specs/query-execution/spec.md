# query-execution Specification

## Purpose
Runs queries from the editor against the tab's connection with safety checks, streaming results, cancellation, result limits and clear status reporting.

## Requirements

### Requirement: Implicit terminal step
When a statement has no terminal step, execution SHALL append `.toList()` before sending; statements with an explicit terminal step SHALL be sent unchanged.

#### Scenario: Append toList
- **WHEN** the user runs `g.V().limit(2)`
- **THEN** the server receives `g.V().limit(2).toList()`

#### Scenario: Keep explicit terminal
- **WHEN** the user runs `g.V().count().next()`
- **THEN** the server receives the statement unchanged

### Requirement: Streaming display
Results SHALL appear in the results dock as soon as the first batch arrives, and the item count SHALL update as further batches stream in.

#### Scenario: Progressive rendering
- **WHEN** the user runs `g.E()` on air-routes
- **THEN** rows appear before the query completes and the count increases until the final total is shown

### Requirement: Cancellation
The user SHALL be able to cancel a running query with Esc or the Cancel button; results received so far SHALL remain visible and be marked as cancelled.

#### Scenario: Cancel a long query
- **WHEN** the user presses Esc while a query is streaming
- **THEN** streaming stops within 200 ms and the summary shows "Cancelled" with the partial count

### Requirement: Result cap
Execution SHALL stop collecting results after a configurable maximum number of items (default 10,000) and SHALL indicate that the result was truncated.

#### Scenario: Truncated result
- **WHEN** a query returns more items than the cap
- **THEN** exactly the cap number of items is shown and the summary indicates truncation with the cap value

### Requirement: Result summary
After completion, the results dock SHALL show the outcome (success, error, cancelled), item count, client-side elapsed time and, when provided by the server, server-side execution time.

#### Scenario: Summary after success
- **WHEN** a query completes successfully with 6 items
- **THEN** the summary shows success, "6 items" and the elapsed time in milliseconds

### Requirement: Error presentation
When a query fails, the results dock SHALL show the server's error message and code in an error view; if the error refers to a position in the query, the editor SHALL highlight that range.

#### Scenario: Server error view
- **WHEN** the user runs `g.V().nosuchstep()`
- **THEN** the results dock shows an error view with the server message, and no stale results from a previous run

### Requirement: Default results view by shape
The results dock SHALL choose the initial view from the result shape: Table for elements and maps, JSON for nested or mixed structures, Console for scalars, unless the tab specifies a default view.

#### Scenario: Elements open in table
- **WHEN** the user runs `g.V().hasLabel('person')` on the modern graph
- **THEN** the Table view is selected and shows four person rows
