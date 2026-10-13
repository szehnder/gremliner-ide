# Manual test: Neptune on LocalStack

LocalStack only emulates Neptune in **LocalStack Pro**, so CI uses recorded `DescribeDBClusters` responses
and a fake Neptune (ADR-0017, ADR-0026). Run this checklist when a Pro licence is available.

## Setup
```
export LOCALSTACK_AUTH_TOKEN=...            # Pro
localstack start -d
awslocal neptune create-db-cluster --db-cluster-identifier air-routes --engine neptune --engine-version 1.2.1.0
awslocal neptune describe-db-clusters       # note Endpoint and Port (e.g. localhost.localstack.cloud:4510)
```

## Checklist
Record the LocalStack version, the date and the outcome of each step.

1. **Discovery (CLI).** `gremliner-cli connections localstack-clusters` lists `air-routes` with engine
   1.2.1.0, `localhost.localstack.cloud` and its port.
2. **Discovery (app).** New connection → Neptune on LocalStack → Find clusters shows the same cluster as a
   chip. Pick it and Test connection. Expect "Connected · Neptune 1.2.1.0 · TinkerPop 3.6.2".
3. **Port changes.** Save the connection, restart LocalStack (`localstack restart`), and recreate the
   cluster if needed. Connecting again looks the port up afresh; nothing has to be edited.
4. **Manual port.** Choose "Enter host and port", enter `localhost` and the cluster port, and Test.
   Expect it to connect even with the LocalStack URL wrong.
5. **IAM emulation.** Start LocalStack with `ENFORCE_IAM=1`, turn on IAM in the connection, and Test.
   Expect it to connect, signed with `test`/`test`. With IAM off, expect a refusal that mentions IAM.
6. **Errors.**
   - With LocalStack stopped, Find clusters says "LocalStack is not reachable at http://localhost:4566."
   - On the community edition (no auth token), it says LocalStack does not provide Neptune and that Pro
     is needed.
   - After `awslocal neptune delete-db-cluster --db-cluster-identifier air-routes`, it says no clusters
     exist and shows the `awslocal neptune create-db-cluster` command.
7. **Restrictions.** In an editor on the connection, a lambda is flagged, and so is `asString()` (a
   TinkerPop 3.7 step) on engine 1.2.1.0.
