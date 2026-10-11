//! Paste-to-fill: turn whatever the user pasted into the host field into structured settings.
//!
//! Accepts `ws://`, `wss://`, `http(s)://` URLs, `host:port`, `host:port/path` and bare host names.
//! Neptune endpoints (`*.neptune.amazonaws.com`) are recognised and their region extracted.

/// What a pasted endpoint says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEndpoint {
    pub host: String,
    pub port: Option<u16>,
    pub path: Option<String>,
    /// `Some` when the input had a scheme that decides TLS.
    pub tls: Option<bool>,
    /// The AWS region when the host is a Neptune endpoint.
    pub neptune_region: Option<String>,
}

impl ParsedEndpoint {
    pub fn is_neptune(&self) -> bool {
        self.neptune_region.is_some()
    }
}

const NEPTUNE_SUFFIX: &str = ".neptune.amazonaws.com";

/// Parse a pasted endpoint. Returns `None` for input that is not an endpoint (empty, spaces inside...).
pub fn parse(input: &str) -> Option<ParsedEndpoint> {
    let input = input.trim();
    if input.is_empty() || input.contains(char::is_whitespace) {
        return None;
    }
    let (with_scheme, tls) = match input.split_once("://") {
        Some((scheme, _)) => {
            let tls = match scheme.to_ascii_lowercase().as_str() {
                "wss" | "https" => true,
                "ws" | "http" => false,
                _ => return None,
            };
            (input.to_owned(), Some(tls))
        }
        None => (format!("ws://{input}"), None),
    };
    let url = url::Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
    if host.is_empty() {
        return None;
    }
    let path = match url.path() {
        "" | "/" => None,
        p => Some(p.to_owned()),
    };
    // `Url` drops default ports (80/443) for http(s); keep explicit ones from the text.
    let port = url.port().or_else(|| explicit_port(input));
    Some(ParsedEndpoint {
        neptune_region: neptune_region(&host),
        host,
        port,
        path,
        tls,
    })
}

fn explicit_port(input: &str) -> Option<u16> {
    let after_scheme = input.split_once("://").map_or(input, |(_, rest)| rest);
    let authority = after_scheme.split('/').next()?;
    let (_, port) = authority.rsplit_once(':')?;
    port.parse().ok()
}

/// `my-cluster.cluster-abc123.us-east-1.neptune.amazonaws.com` → `us-east-1`.
pub fn neptune_region(host: &str) -> Option<String> {
    let lower = host.to_ascii_lowercase();
    let stem = lower.strip_suffix(NEPTUNE_SUFFIX)?;
    let region = stem.rsplit('.').next()?;
    let looks_like_region = region.split('-').count() >= 3
        && region
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    looks_like_region.then(|| region.to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn scenario_connections_paste_a_websocket_url() {
        let p = parse("wss://db.example.com:8182/gremlin").unwrap();
        assert_eq!(p.host, "db.example.com");
        assert_eq!(p.port, Some(8182));
        assert_eq!(p.path.as_deref(), Some("/gremlin"));
        assert_eq!(p.tls, Some(true));
        assert!(!p.is_neptune());
    }

    #[test]
    fn scenario_connections_paste_a_neptune_endpoint() {
        let p =
            parse("my-graph.cluster-c1a2b3c4d5e6.eu-west-2.neptune.amazonaws.com:8182").unwrap();
        assert_eq!(p.neptune_region.as_deref(), Some("eu-west-2"));
        assert_eq!(p.port, Some(8182));
        let ro = parse("wss://db-1.cluster-ro-xyz.us-east-1.neptune.amazonaws.com:8182/gremlin")
            .unwrap();
        assert_eq!(ro.neptune_region.as_deref(), Some("us-east-1"));
        assert_eq!(ro.tls, Some(true));
    }

    #[test]
    fn plain_hosts_and_host_ports() {
        let p = parse("localhost:8183").unwrap();
        assert_eq!(
            (p.host.as_str(), p.port, p.tls),
            ("localhost", Some(8183), None)
        );
        let p = parse("graph.internal").unwrap();
        assert_eq!((p.host.as_str(), p.port), ("graph.internal", None));
        let p = parse("[::1]:8182").unwrap();
        assert_eq!(p.host, "::1");
        let p = parse("https://example.com:443/gremlin").unwrap();
        assert_eq!(p.port, Some(443));
        assert_eq!(p.tls, Some(true));
    }

    #[test]
    fn rejects_non_endpoints() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("not an endpoint"), None);
        assert_eq!(parse("ftp://x"), None);
        assert_eq!(neptune_region("example.com"), None);
    }
}
