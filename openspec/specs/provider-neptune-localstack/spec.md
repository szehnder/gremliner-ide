# provider-neptune-localstack Specification

## Purpose
Connects the IDE to Neptune clusters emulated by LocalStack, discovering cluster endpoints automatically.

## Requirements

### Requirement: Cluster discovery
Given a LocalStack URL and region, the provider SHALL list the emulated Neptune clusters with their identifiers, engine versions, endpoints and ports, and let the user pick one.

#### Scenario: List clusters
- **WHEN** the provider parses the recorded LocalStack DescribeDBClusters response with one cluster on port 4510
- **THEN** the picker shows that cluster with endpoint `localhost.localstack.cloud`, port 4510 and its engine version

### Requirement: Helpful discovery errors
The provider SHALL distinguish LocalStack not running, Neptune not available in the running LocalStack edition, and no clusters existing, with a specific message and next step for each.

#### Scenario: LocalStack not running
- **WHEN** nothing listens on the LocalStack URL
- **THEN** the error says LocalStack is not reachable at that URL

#### Scenario: No clusters
- **WHEN** LocalStack responds with an empty cluster list
- **THEN** the message explains that no clusters exist and shows the command to create one

### Requirement: Version-aware capabilities
The provider SHALL map the cluster's Neptune engine version to its TinkerPop version and expose the corresponding capabilities.

#### Scenario: Engine to TinkerPop mapping
- **WHEN** the selected cluster has engine version 1.2.1.0
- **THEN** the connection reports TinkerPop 3.6.2

### Requirement: Optional IAM emulation
When IAM is enabled for the connection, requests SHALL be signed with SigV4 using the configured (test) credentials.

#### Scenario: Signed requests with test credentials
- **WHEN** IAM is enabled with access key `test` and secret `test`
- **THEN** the connection handshake is SigV4-signed for service `neptune-db`

### Requirement: Manual override
The user SHALL be able to bypass discovery and enter a host and port directly.

#### Scenario: Manual port
- **WHEN** the user enters host `localhost` and port 4510 manually
- **THEN** the provider connects to that address without calling DescribeDBClusters
