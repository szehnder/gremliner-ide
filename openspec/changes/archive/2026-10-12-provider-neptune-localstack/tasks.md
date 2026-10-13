# Tasks

## 1. Discovery

- [x] 1.1 DescribeDBClusters call and XML parsing; verify `scenario_provider_neptune_localstack_list_clusters`
- [x] 1.2 Error classification; verify `scenario_provider_neptune_localstack_localstack_not_running` and `scenario_provider_neptune_localstack_no_clusters`
- [x] 1.3 Engine version mapping; verify `scenario_provider_neptune_localstack_engine_to_tinkerpop_mapping`

## 2. Connection

- [x] 2.1 IAM with test credentials; verify `scenario_provider_neptune_localstack_signed_requests_with_test_credentials`
- [x] 2.2 Manual override and the form section with cluster picker; verify `scenario_provider_neptune_localstack_manual_port`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green
- [x] 3.2 `openspec validate provider-neptune-localstack --strict` passes
- [x] 3.3 Write `docs/manual-tests/localstack-neptune.md`
