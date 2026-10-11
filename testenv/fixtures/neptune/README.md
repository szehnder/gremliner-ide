# Neptune fixtures

**Provenance: illustrative, not recorded.** These files were written by hand from the response formats described in
the Amazon Neptune user guide (status, statistics summary, Gremlin profile/explain, RDS DescribeDBClusters), using
air-routes-like content. No live Neptune cluster or LocalStack Pro instance was available when they were created
(see ADR-0026). Parsers must be tolerant of extra or missing sections.

When a real cluster or LocalStack Pro is available, capture real responses, add them next to these files with an
`-recorded-<engine-version>` suffix, and keep both sets in the tests.

| File | Endpoint | Used by change |
|---|---|---|
| `status.json` | `GET /status` | provider-neptune-aws |
| `statistics-summary-detailed.json` | `GET /propertygraph/statistics/summary?mode=detailed` | schema-discovery |
| `gremlin-profile.txt` | `POST /gremlin/profile` | query-profile-explain |
| `gremlin-explain.txt` | `POST /gremlin/explain` | query-profile-explain |
| `describe-db-clusters-localstack.xml` | LocalStack `DescribeDBClusters` | provider-neptune-localstack |
| `describe-db-clusters-empty.xml` | LocalStack `DescribeDBClusters` with no clusters | provider-neptune-localstack |
