# Tasks

## 1. Containers

- [x] 1.1 Write `testenv/docker-compose.yml` (pinned gremlin-server 3.7.x on 8182; SASL instance on 8183) and `testenv/conf/gremlin-server.yaml` with graphs `g`, `gModern`, `gCrew`, `gSynth`; verify `docker compose -f testenv/docker-compose.yml up -d` becomes healthy
- [x] 1.2 Implement `cargo xtask testenv up|down|status` (status runs `g.inject(1)` over HTTP and prints the server version); verify against the running containers

## 2. Data

- [x] 2.1 Implement air-routes download with checksum and cache in `testenv/data/`; verify a second run doesn't re-download
- [x] 2.2 Implement `cargo xtask testenv seed --dataset <modern|crew|air-routes|synthetic|all>`; verify vertex/edge counts printed match expectations
- [x] 2.3 Implement the synthetic GraphML generator (`--nodes --avg-degree --labels --seed`), deterministic for a given seed; verify two runs with the same seed produce identical files (hash compare test)

## 3. Test helpers and fixtures

- [x] 3.1 Add a `testenv` helper module (endpoint discovery from `GREMLINER_TESTENV` env or the default, skip-with-message when unreachable); verify a dummy integration test skips with the env down and passes with it up
- [x] 3.2 Add `testenv/fixtures/neptune/` illustrative responses (from documented formats; recorded ones added later) (profile, explain, status, statistics summary, DescribeDBClusters) with a README stating their source and engine version; verify the files are valid JSON or text as documented
- [x] 3.3 Add the testenv service to the CI Linux job; verify the workflow YAML references `cargo xtask testenv up && seed`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green with the testenv running
- [x] 4.2 `openspec validate testenv --strict` passes
