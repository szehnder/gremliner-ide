# Tasks

## 1. Discovery

- [ ] 1.1 DescribeDBClusters call and XML parsing; verify `scenario_provider_neptune_localstack_list_clusters`
- [ ] 1.2 Error classification; verify `scenario_provider_neptune_localstack_localstack_not_running` and `scenario_provider_neptune_localstack_no_clusters`
- [ ] 1.3 Engine version mapping; verify `scenario_provider_neptune_localstack_engine_to_tinkerpop_mapping`

## 2. Connection

- [ ] 2.1 IAM with test credentials; verify `scenario_provider_neptune_localstack_signed_requests_with_test_credentials`
- [ ] 2.2 Manual override and the form section with cluster picker; verify `scenario_provider_neptune_localstack_manual_port`

## 3. Verification

- [ ] 3.1 `cargo xtask ci` is green
- [ ] 3.2 `openspec validate provider-neptune-localstack --strict` passes
- [ ] 3.3 Write `docs/manual-tests/localstack-neptune.md`
