# Spec Delta

## Purpose

Defines measurable performance guarantees for the IDE and enforces them continuously so regressions are caught automatically.

## ADDED Requirements

### Requirement: Fast cold start
A release build SHALL render its first frame within 300 ms of launch on the reference machine.

#### Scenario: Cold start measurement
- **WHEN** the start-up benchmark launches the release build 10 times
- **THEN** the median time to first frame is at most 300 ms

### Requirement: Low idle memory
With one connection open and an empty editor, the application SHALL use at most 150 MB of resident memory after 30 seconds idle.

#### Scenario: Idle memory measurement
- **WHEN** the memory benchmark samples resident memory after 30 seconds idle
- **THEN** it is at most 150 MB

### Requirement: Budget enforcement
The budget check SHALL fail when any measurement exceeds its absolute budget locally, or regresses by more than 15% against the main-branch baseline in CI.

#### Scenario: Regression detected
- **WHEN** a benchmark is 20% slower than the stored baseline
- **THEN** the budget check fails and names the benchmark and both values

### Requirement: Scenario coverage enforced
The CI check SHALL fail when an archived spec scenario has no test with the matching scenario name.

#### Scenario: Missing scenario test
- **WHEN** an archived spec contains a scenario with no matching test
- **THEN** the coverage check fails and lists the scenario
