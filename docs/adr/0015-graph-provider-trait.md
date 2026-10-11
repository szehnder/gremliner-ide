# 0015. `GraphProvider` trait with a capability matrix

- Status: accepted
- Date: 2026-10-11

## Context
v1 ships Gremlin Server, Neptune (AWS) and Neptune (LocalStack); JanusGraph, Cosmos DB, Aerospike and others will
follow. Providers differ in auth, endpoints, supported steps, profile/explain mechanisms and schema discovery.

## Decision
`providers` defines:
```rust
#[async_trait] trait GraphProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> &Capabilities; // profile, explain, schema_summary, tx, script_lang, unsupported_steps…
    async fn connect(&self) -> Result<Arc<dyn GremlinSession>>;
    async fn profile(&self, q: &Query) -> Result<ProfileReport>;   // default: append .profile()
    async fn explain(&self, q: &Query) -> Result<ExplainReport>;   // default: append .explain()
    async fn schema_summary(&self) -> Result<Option<SchemaSummary>>; // Neptune: statistics API
    async fn server_info(&self) -> Result<ServerInfo>;
}
```
Each connection config is a tagged enum (`ProviderConfig::GremlinServer{..} | NeptuneAws{..} | NeptuneLocalStack{..}`)
persisted as JSON. The UI renders provider-specific form sections from the enum.

## Alternatives considered
- **Feature flags scattered through UI code** — leaks provider knowledge everywhere.

## Consequences
- The assistant and autocomplete read `capabilities().unsupported_steps` to avoid suggesting invalid steps.
- Adding a provider = new module + config variant + form section + capability entry.

## Revisit when
A provider needs a non-Gremlin query language (openCypher) — extend with a `languages()` capability.
