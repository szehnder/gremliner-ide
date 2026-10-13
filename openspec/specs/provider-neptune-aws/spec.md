# provider-neptune-aws Specification

## Purpose
Connects the IDE to Amazon Neptune clusters using the user's AWS credentials with IAM SigV4 authentication, and exposes Neptune-specific capabilities.

## Requirements

### Requirement: Neptune connection settings
The Neptune provider SHALL accept a cluster or instance endpoint, port (default 8182), region (inferred from the endpoint when possible), IAM authentication on or off, and an AWS profile selected from the user's AWS configuration.

#### Scenario: Region inferred
- **WHEN** the user enters `mydb.cluster-abc123.us-east-2.neptune.amazonaws.com`
- **THEN** the region field is set to `us-east-2`

#### Scenario: Profiles listed
- **WHEN** the user opens the AWS profile picker
- **THEN** it lists the profiles defined in the user's AWS config and credentials files, including SSO profiles

### Requirement: IAM SigV4 authentication
With IAM enabled, every connection handshake and HTTP request to Neptune SHALL be signed with SigV4 for the `neptune-db` service using credentials resolved from the selected profile or the default credential chain, including temporary session tokens.

#### Scenario: Signed handshake
- **WHEN** the provider opens a connection with IAM enabled using temporary credentials
- **THEN** the handshake request carries Authorization, X-Amz-Date and X-Amz-Security-Token headers that a SigV4 verifier accepts for service `neptune-db`

#### Scenario: Expired SSO session
- **WHEN** the selected profile's SSO session has expired
- **THEN** the connection fails with a message telling the user to run `aws sso login --profile <name>`

### Requirement: Tunnel host override
The provider SHALL support connecting to a local tunnel address while signing requests and validating TLS for the real cluster hostname.

#### Scenario: Connect through localhost tunnel
- **WHEN** the connection uses tunnel address `localhost:8182` with the cluster hostname set as the signing host
- **THEN** requests are sent to localhost and signed with the cluster hostname as the Host header

### Requirement: Neptune capabilities
On connect, the provider SHALL read the Neptune engine and TinkerPop versions from the status endpoint and expose Neptune restrictions (no lambdas, unsupported steps) and support for profile, explain and statistics summaries to the rest of the IDE.

#### Scenario: Status parsed
- **WHEN** the provider reads the recorded Neptune status response
- **THEN** it reports the engine version, the TinkerPop version and that profile and explain are supported

#### Scenario: Lambda flagged
- **WHEN** a query containing a lambda is validated on a Neptune connection
- **THEN** validation reports that Neptune does not support lambdas

### Requirement: Credential refresh
The provider SHALL refresh expiring temporary credentials automatically and re-sign on reconnect without user action.

#### Scenario: Refresh before expiry
- **WHEN** temporary credentials expire during a long session
- **THEN** the next reconnect uses refreshed credentials and succeeds without prompting the user
