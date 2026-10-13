//! Neptune's `/status` response.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeptuneStatus {
    /// `healthy`, `recovery`, ...
    pub status: String,
    /// e.g. `1.4.5.0.R2`.
    pub engine_version: String,
    /// e.g. `3.7.3` (the `tinkerpop-` prefix removed).
    pub tinkerpop_version: Option<String>,
    /// `writer` or `reader`.
    pub role: Option<String>,
    pub iam_auth: Option<bool>,
    pub audit_log: Option<bool>,
    /// Slow-query logging level (`disabled`, `info`, `debug`).
    pub slow_query_logs: Option<String>,
    pub query_timeout_ms: Option<u64>,
}

impl NeptuneStatus {
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let str_at = |path: &[&str]| {
            path.iter()
                .try_fold(&v, |v, k| v.get(k))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let engine_version = str_at(&["dbEngineVersion"])
            .ok_or_else(|| "not a Neptune status response (no dbEngineVersion)".to_owned())?;
        let flag = |path: &[&str]| str_at(path).map(|s| s.eq_ignore_ascii_case("enabled"));
        Ok(Self {
            status: str_at(&["status"]).unwrap_or_default(),
            engine_version,
            tinkerpop_version: str_at(&["gremlin", "version"])
                .map(|s| s.trim_start_matches("tinkerpop-").to_owned()),
            role: str_at(&["role"]),
            iam_auth: flag(&["features", "IAMAuthentication"]),
            audit_log: flag(&["features", "AuditLog"]),
            slow_query_logs: str_at(&["features", "SlowQueryLogs"]),
            query_timeout_ms: str_at(&["settings", "clusterQueryTimeoutInMs"])
                .and_then(|s| s.parse().ok()),
        })
    }
}
