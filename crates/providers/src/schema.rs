//! Schema discovery on a live [`Session`]: the Gremlin client as a [`QueryRunner`], and the provider's
//! statistics summary when it has one (ADR-0015).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;
use gremlin_client::{Client, Error, Request};
use gremlin_model::GValue;
use schema::{Options, QueryError, QueryRunner, Schema, Summary};

use storage::{CachedSchema, Storage};
use uuid::Uuid;

use crate::provider::Session;

/// Runs discovery queries on a client, with the per-query timeout applied on both ends.
pub struct ClientRunner<'c>(pub &'c Client);

impl QueryRunner for ClientRunner<'_> {
    fn run<'a>(
        &'a self,
        script: &'a str,
        bindings: Vec<(String, GValue)>,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<Vec<GValue>, QueryError>> {
        Box::pin(async move {
            let mut request = Request::new(script).timeout(timeout);
            for (name, value) in bindings {
                request = request.binding(name, value);
            }
            match self.0.execute_request(request).await {
                Ok(result) => Ok(result.items),
                Err(err) => Err(query_error(&err)),
            }
        })
    }
}

fn query_error(err: &Error) -> QueryError {
    match err {
        Error::Timeout(_) => QueryError::Timeout,
        // 598: Gremlin Server's evaluation timeout.
        Error::Server { code: 598, .. } => QueryError::Timeout,
        Error::Server { message, .. } if message.to_lowercase().contains("timeout") => {
            QueryError::Timeout
        }
        other => QueryError::Failed(other.to_string()),
    }
}

impl Session {
    /// The database's statistics summary, when the provider offers one (Neptune's
    /// `/propertygraph/statistics/summary`). `None` when statistics are off or stale; discovery then
    /// samples with queries instead.
    pub async fn schema_summary(&self) -> Option<Summary> {
        let api = self.api.as_ref().filter(|_| self.capabilities.statistics)?;
        match api.statistics_summary().await {
            Ok(text) => Summary::from_neptune_json(&text),
            Err(err) => {
                log::info!("statistics summary unavailable: {err}");
                None
            }
        }
    }
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Discover the schema of a connected graph, reporting each intermediate snapshot.
pub async fn discover_schema(
    session: &Session,
    opts: &Options,
    progress: &(dyn Fn(&Schema) + Send + Sync),
) -> Schema {
    let summary = session.schema_summary().await;
    schema::discover(
        &ClientRunner(&session.client),
        summary,
        opts,
        now_ms(),
        progress,
    )
    .await
}

/// The cached schema of a connection, if one was saved and is still readable.
pub async fn load_cached(storage: &Storage, connection: Uuid) -> Option<Schema> {
    match storage.load_schema(&connection.to_string()).await {
        Ok(Some(row)) => Schema::from_json(&row.json),
        Ok(None) => None,
        Err(err) => {
            log::warn!("could not read the cached schema: {err}");
            None
        }
    }
}

/// Replace the cached schema of a connection.
pub async fn save_cached(
    storage: &Storage,
    connection: Uuid,
    schema: &Schema,
) -> Result<(), storage::StorageError> {
    storage
        .save_schema(CachedSchema {
            connection_id: connection.to_string(),
            discovered_at_ms: schema.discovered_at_ms,
            json: schema.to_json(),
        })
        .await
}
