//! Profile and explain on any session: Neptune's HTTP endpoints when the session has them, otherwise
//! the TinkerPop `profile()` / `explain()` steps appended to the statement (ADR-0015).

use std::time::Duration;

use gremlin_client::Request;
use gremlin_model::profile::{ExplainReport, ProfileReport};

use crate::error::{FailureKind, ProviderError};
use crate::provider::Session;

/// The statement without a trailing terminal step or semicolon, ready for `.profile()`.
pub fn traversal_part(statement: &str) -> &str {
    let mut s = statement.trim().trim_end_matches(';').trim_end();
    for terminal in [
        ".toList()",
        ".iterate()",
        ".toSet()",
        ".next()",
        ".toBulkSet()",
    ] {
        if let Some(stripped) = s.strip_suffix(terminal) {
            s = stripped.trim_end();
            break;
        }
    }
    s
}

fn server_error(err: gremlin_client::Error) -> ProviderError {
    ProviderError {
        kind: FailureKind::Server,
        reason: match &err {
            gremlin_client::Error::Server { message, .. } => message.clone(),
            other => other.to_string(),
        },
        suggestion: String::new(),
        detail: err.to_string(),
    }
}

fn unexpected(what: &str) -> ProviderError {
    ProviderError {
        kind: FailureKind::Server,
        reason: format!("The server did not return {what}."),
        suggestion: String::new(),
        detail: String::new(),
    }
}

/// Run the statement with profiling. This executes the traversal (callers confirm mutations first).
pub async fn profile(
    session: &Session,
    statement: &str,
    timeout: Duration,
) -> Result<ProfileReport, ProviderError> {
    let traversal = traversal_part(statement);
    if session.capabilities.neptune
        && let Some(api) = &session.api
    {
        let text = api.profile(traversal).await?;
        return Ok(ProfileReport::from_neptune_text(&text));
    }
    let result = session
        .client
        .execute_request(Request::new(format!("{traversal}.profile()")).timeout(timeout))
        .await
        .map_err(server_error)?;
    ProfileReport::from_items(&result.items).ok_or_else(|| unexpected("traversal metrics"))
}

/// How the database plans the statement. Does not execute it.
pub async fn explain(
    session: &Session,
    statement: &str,
    timeout: Duration,
) -> Result<ExplainReport, ProviderError> {
    let traversal = traversal_part(statement);
    if session.capabilities.neptune
        && let Some(api) = &session.api
    {
        let text = api.explain(traversal).await?;
        return Ok(ExplainReport::from_neptune_text(&text));
    }
    let result = session
        .client
        .execute_request(Request::new(format!("{traversal}.explain()")).timeout(timeout))
        .await
        .map_err(server_error)?;
    ExplainReport::from_items(&result.items).ok_or_else(|| unexpected("an explanation"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_steps_are_stripped() {
        assert_eq!(traversal_part("g.V().toList();"), "g.V()");
        assert_eq!(traversal_part(" g.V().count() "), "g.V().count()");
        assert_eq!(traversal_part("g.V().next()"), "g.V()");
    }
}
