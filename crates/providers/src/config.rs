//! Saved connection settings (ADR-0015). Persisted as JSON in the `connections` table; secrets are
//! referenced, never embedded.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Default request timeout for a new connection.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub color: ColorTag,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    pub provider: ProviderConfig,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

impl ConnectionConfig {
    pub fn new(name: impl Into<String>, provider: ProviderConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            color: ColorTag::default(),
            environment: Environment::default(),
            read_only: false,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            provider,
        }
    }

    /// Keychain account holding this connection's password.
    pub fn password_account(&self) -> String {
        format!("{}/password", self.id)
    }

    /// A short `host:port` description for lists.
    pub fn endpoint_label(&self) -> String {
        match &self.provider {
            ProviderConfig::GremlinServer(c) => format!("{}:{}", c.host, c.port),
            ProviderConfig::NeptuneAws(c) => format!("{}:{}", c.host, c.port),
        }
    }
}

/// Where the connection points, used for badges. Production is drawn in the danger colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Local,
    Dev,
    Staging,
    Production,
}

impl Environment {
    pub const ALL: [Environment; 4] = [
        Environment::Local,
        Environment::Dev,
        Environment::Staging,
        Environment::Production,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Environment::Local => "Local",
            Environment::Dev => "Dev",
            Environment::Staging => "Staging",
            Environment::Production => "Production",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Dev => "dev",
            Environment::Staging => "staging",
            Environment::Production => "production",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }
}

/// A colour label; the theme decides the actual colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorTag {
    #[default]
    None,
    Green,
    Blue,
    Purple,
    Yellow,
    Orange,
    Red,
}

impl ColorTag {
    pub const ALL: [ColorTag; 7] = [
        ColorTag::None,
        ColorTag::Green,
        ColorTag::Blue,
        ColorTag::Purple,
        ColorTag::Yellow,
        ColorTag::Orange,
        ColorTag::Red,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ColorTag::None => "none",
            ColorTag::Green => "green",
            ColorTag::Blue => "blue",
            ColorTag::Purple => "purple",
            ColorTag::Yellow => "yellow",
            ColorTag::Orange => "orange",
            ColorTag::Red => "red",
        }
    }
}

/// Provider-specific settings. Adding a provider adds a variant (ADR-0015).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderConfig {
    GremlinServer(GremlinServerConfig),
    /// Amazon Neptune, with IAM SigV4 authentication.
    NeptuneAws(NeptuneConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    GremlinServer,
    NeptuneAws,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::GremlinServer => "Gremlin Server",
            ProviderKind::NeptuneAws => "Amazon Neptune",
        }
    }
}

impl ProviderConfig {
    pub fn kind(&self) -> ProviderKind {
        match self {
            ProviderConfig::GremlinServer(_) => ProviderKind::GremlinServer,
            ProviderConfig::NeptuneAws(_) => ProviderKind::NeptuneAws,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GremlinServerConfig {
    pub host: String,
    pub port: u16,
    pub path: String,
    pub tls: bool,
    pub accept_invalid_certs: bool,
    pub username: Option<String>,
    /// True when a password is stored in the credential store under
    /// [`ConnectionConfig::password_account`].
    pub has_password: bool,
    /// Server-side traversal source bound to `g`.
    pub alias: String,
}

impl Default for GremlinServerConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8182,
            path: "/gremlin".into(),
            tls: false,
            accept_invalid_certs: false,
            username: None,
            has_password: false,
            alias: "g".into(),
        }
    }
}

impl GremlinServerConfig {
    pub fn url(&self) -> String {
        let scheme = if self.tls { "wss" } else { "ws" };
        let path = if self.path.starts_with('/') {
            self.path.clone()
        } else {
            format!("/{}", self.path)
        };
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host) // IPv6 literal
        } else {
            self.host.clone()
        };
        format!("{scheme}://{host}:{}{path}", self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NeptuneConfig {
    /// Cluster, reader or instance endpoint. Requests are signed for this host even through a tunnel.
    pub host: String,
    pub port: u16,
    /// AWS region; inferred from the endpoint when it is a `*.neptune.amazonaws.com` name.
    pub region: String,
    /// Sign requests with AWS SigV4 (IAM database authentication).
    pub iam_auth: bool,
    /// AWS profile from `~/.aws/config` or `~/.aws/credentials`; `None` uses the default credential chain.
    pub profile: Option<String>,
    /// Connect to this `host:port` (a local SSH/SSM tunnel) instead of the endpoint.
    pub tunnel: Option<String>,
    /// Skip certificate checks, for tunnels whose certificate does not match.
    pub accept_invalid_certs: bool,
    /// Neptune always uses TLS; off only for stand-ins such as a local Gremlin Server.
    pub tls: bool,
}

impl Default for NeptuneConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 8182,
            region: String::new(),
            iam_auth: true,
            profile: None,
            tunnel: None,
            accept_invalid_certs: false,
            tls: true,
        }
    }
}

impl NeptuneConfig {
    /// The region to sign for: the configured one, else the one in the endpoint name.
    pub fn signing_region(&self) -> Option<String> {
        let region = self.region.trim();
        if region.is_empty() {
            crate::endpoint::neptune_region(&self.host)
        } else {
            Some(region.to_owned())
        }
    }

    fn authority(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The WebSocket URL, always naming the real endpoint.
    pub fn url(&self) -> String {
        let scheme = if self.tls { "wss" } else { "ws" };
        format!("{scheme}://{}/gremlin", self.authority())
    }

    /// Base URL for Neptune's HTTP endpoints (`/status`, `/gremlin/profile`, ...).
    pub fn http_base(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        format!("{scheme}://{}", self.authority())
    }

    /// The tunnel address, when one is set.
    pub fn tunnel_addr(&self) -> Option<&str> {
        self.tunnel
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn scenario_provider_gremlin_server_defaults() {
        let c = GremlinServerConfig::default();
        assert_eq!(c.port, 8182);
        assert_eq!(c.path, "/gremlin");
        assert!(!c.tls);
        assert_eq!(c.alias, "g");
        assert_eq!(c.url(), "ws://localhost:8182/gremlin");
    }

    #[test]
    fn config_json_round_trips_and_has_no_secret_fields() {
        let mut c = ConnectionConfig::new(
            "Local",
            ProviderConfig::GremlinServer(GremlinServerConfig {
                username: Some("stephen".into()),
                has_password: true,
                ..Default::default()
            }),
        );
        c.environment = Environment::Production;
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"kind\":\"gremlin_server\""), "{json}");
        assert!(!json.contains("password\":\""), "{json}");
        let back: ConnectionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn neptune_urls_and_old_configs() {
        let c = NeptuneConfig {
            host: "db.cluster-abc.us-east-2.neptune.amazonaws.com".into(),
            ..Default::default()
        };
        assert_eq!(
            c.url(),
            "wss://db.cluster-abc.us-east-2.neptune.amazonaws.com:8182/gremlin"
        );
        assert_eq!(c.signing_region().as_deref(), Some("us-east-2"));
        // Settings saved before profiles and tunnels existed still load.
        let old: NeptuneConfig = serde_json::from_str(
            r#"{"host":"h","port":8182,"region":"eu-west-1","iam_auth":true}"#,
        )
        .unwrap();
        assert!(old.tls && old.profile.is_none() && old.tunnel_addr().is_none());
    }

    #[test]
    fn urls_bracket_ipv6_and_fix_paths() {
        let c = GremlinServerConfig {
            host: "::1".into(),
            path: "gremlin".into(),
            tls: true,
            ..Default::default()
        };
        assert_eq!(c.url(), "wss://[::1]:8182/gremlin");
    }
}
