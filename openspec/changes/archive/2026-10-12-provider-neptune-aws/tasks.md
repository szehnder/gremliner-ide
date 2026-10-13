# Tasks

## 1. Settings and credentials

- [x] 1.1 Neptune config variant with region inference and AWS profile listing; verify `scenario_provider_neptune_aws_region_inferred` and `scenario_provider_neptune_aws_profiles_listed` (fixture AWS config)
- [x] 1.2 Credential resolution with SSO expiry mapping and refresh; verify `scenario_provider_neptune_aws_expired_sso_session` and `scenario_provider_neptune_aws_refresh_before_expiry`

## 2. Signing and transport

- [x] 2.1 SigV4 handshake signer with temporary credentials; verify `scenario_provider_neptune_aws_signed_handshake` with the test verifier
- [x] 2.2 Tunnel host override (Host and SNI); verify `scenario_provider_neptune_aws_connect_through_localhost_tunnel` against a local TLS test server

## 3. Capabilities

- [x] 3.1 Status parsing and compliance table; verify `scenario_provider_neptune_aws_status_parsed` and `scenario_provider_neptune_aws_lambda_flagged`
- [x] 3.2 Neptune form section in the connection dialog; verify headless UI test

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate provider-neptune-aws --strict` passes
- [x] 4.3 Write `docs/manual-tests/neptune-aws.md`
