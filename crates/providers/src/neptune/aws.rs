//! AWS profiles and credentials for Neptune IAM authentication (ADR-0016).
//!
//! Profiles are listed from the user's config and credentials files. Credentials come from the selected
//! profile (static keys, SSO, assume-role, credential_process) or the default chain, and are cached per
//! connection and refreshed shortly before they expire.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use aws_config::BehaviorVersion;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_config::provider_config::ProviderConfig;
use aws_credential_types::Credentials;
use aws_credential_types::provider::error::CredentialsError;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};
use aws_smithy_http_client::tls::{Provider as TlsProvider, rustls_provider::CryptoMode};

use crate::error::{FailureKind, ProviderError};

/// Credentials this close to expiry are replaced before signing.
pub const REFRESH_BEFORE: Duration = Duration::from_secs(5 * 60);
/// SSO and STS calls can hang on a dead network; give up after this.
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// The AWS shared config and credentials files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsFiles {
    pub config: PathBuf,
    pub credentials: PathBuf,
}

impl AwsFiles {
    /// `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`, else `~/.aws/config` and `~/.aws/credentials`.
    pub fn from_env() -> Option<Self> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let pick = |var: &str, file: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".aws").join(file)))
        };
        Some(Self {
            config: pick("AWS_CONFIG_FILE", "config")?,
            credentials: pick("AWS_SHARED_CREDENTIALS_FILE", "credentials")?,
        })
    }

    fn profile_files(&self) -> EnvConfigFiles {
        EnvConfigFiles::builder()
            .with_file(EnvConfigFileKind::Config, &self.config)
            .with_file(EnvConfigFileKind::Credentials, &self.credentials)
            .build()
    }
}

/// One profile from the AWS files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsProfile {
    pub name: String,
    /// Signs in through IAM Identity Center (`sso_session` or `sso_start_url`).
    pub sso: bool,
    pub region: Option<String>,
}

/// Profiles in both files, `default` first, then by name. Missing or unreadable files list nothing.
pub fn list_profiles(files: &AwsFiles) -> Vec<AwsProfile> {
    let read = |p: &PathBuf| std::fs::read_to_string(p).unwrap_or_default();
    parse_profiles(&read(&files.config), &read(&files.credentials))
}

/// Parse the two files' text (INI). Config sections are `[default]` and `[profile name]`; credentials
/// sections are `[name]`. `[sso-session x]` and `[services x]` are not profiles.
pub fn parse_profiles(config: &str, credentials: &str) -> Vec<AwsProfile> {
    let mut out: Vec<AwsProfile> = Vec::new();
    let mut add = |text: &str, is_config: bool| {
        let mut current: Option<usize> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let section = section.trim();
                let name = if is_config {
                    match section.strip_prefix("profile") {
                        Some(rest) if rest.starts_with(char::is_whitespace) => Some(rest.trim()),
                        _ if section == "default" => Some("default"),
                        _ => None,
                    }
                } else {
                    Some(section)
                };
                current = name.filter(|n| !n.is_empty()).map(|name| {
                    match out.iter().position(|p| p.name == name) {
                        Some(i) => i,
                        None => {
                            out.push(AwsProfile {
                                name: name.to_owned(),
                                sso: false,
                                region: None,
                            });
                            out.len() - 1
                        }
                    }
                });
                continue;
            }
            let (Some(i), Some((key, value))) = (current, line.split_once('=')) else {
                continue;
            };
            match key.trim() {
                "sso_session" | "sso_start_url" => out[i].sso = true,
                "region" => out[i].region = Some(value.trim().to_owned()),
                _ => {}
            }
        }
    };
    add(config, true);
    add(credentials, false);
    out.sort_by(|a, b| {
        (a.name != "default")
            .cmp(&(b.name != "default"))
            .then(a.name.cmp(&b.name))
    });
    out
}

/// HTTP client for SSO and STS calls, on the same rustls/ring stack as the rest of the app.
fn http_client() -> aws_smithy_runtime_api::client::http::SharedHttpClient {
    aws_smithy_http_client::Builder::new()
        .tls_provider(TlsProvider::Rustls(CryptoMode::Ring))
        .build_https()
}

/// A connection's credentials: resolved once, reused, and refreshed before they expire.
pub struct CredentialCache {
    provider: SharedCredentialsProvider,
    /// The profile, for messages; `None` for the default chain.
    profile: Option<String>,
    sso: bool,
    refresh_before: Duration,
    cached: tokio::sync::Mutex<Option<Credentials>>,
}

impl CredentialCache {
    pub fn new(provider: SharedCredentialsProvider, profile: Option<String>, sso: bool) -> Self {
        Self {
            provider,
            profile,
            sso,
            refresh_before: REFRESH_BEFORE,
            cached: tokio::sync::Mutex::new(None),
        }
    }

    /// Replace credentials this long before they expire (default [`REFRESH_BEFORE`]).
    pub fn refresh_before(mut self, window: Duration) -> Self {
        self.refresh_before = window;
        self
    }

    /// Credentials for `profile` (or the default chain), reading profiles from `files` (default: the
    /// usual locations).
    pub async fn resolve(
        profile: Option<&str>,
        region: Option<&str>,
        files: Option<&AwsFiles>,
    ) -> Arc<Self> {
        let region = region.map(|r| aws_config::Region::new(r.to_owned()));
        let base = ProviderConfig::without_region()
            .with_http_client(http_client())
            .with_region(region.clone());
        let files = files.cloned().or_else(AwsFiles::from_env);
        let profile = profile.map(str::trim).filter(|p| !p.is_empty());
        match profile {
            Some(name) => {
                let sso = files
                    .as_ref()
                    .map(list_profiles)
                    .unwrap_or_default()
                    .iter()
                    .any(|p| p.name == name && p.sso);
                let mut builder = ProfileFileCredentialsProvider::builder()
                    .configure(&base)
                    .profile_name(name);
                if let Some(files) = &files {
                    builder = builder.profile_files(files.profile_files());
                }
                let provider = SharedCredentialsProvider::new(builder.build());
                Arc::new(Self::new(provider, Some(name.to_owned()), sso))
            }
            None => {
                let chain = aws_config::defaults(BehaviorVersion::latest())
                    .http_client(http_client())
                    .region(region)
                    .load()
                    .await;
                let provider = chain.credentials_provider().unwrap_or_else(|| {
                    SharedCredentialsProvider::new(Credentials::new("", "", None, None, "none"))
                });
                Arc::new(Self::new(provider, None, false))
            }
        }
    }

    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    /// Valid credentials: the cached ones unless they expire within [`REFRESH_BEFORE`].
    pub async fn get(&self) -> Result<Credentials, ProviderError> {
        let mut slot = self.cached.lock().await;
        if let Some(creds) = slot.as_ref()
            && !expiring(creds, SystemTime::now(), self.refresh_before)
        {
            return Ok(creds.clone());
        }
        let fresh =
            match tokio::time::timeout(LOAD_TIMEOUT, self.provider.provide_credentials()).await {
                Ok(Ok(c)) => c,
                Ok(Err(err)) => return Err(credentials_error(self.profile(), self.sso, &err)),
                Err(_) => {
                    return Err(credentials_error(
                        self.profile(),
                        self.sso,
                        &CredentialsError::provider_timed_out(LOAD_TIMEOUT),
                    ));
                }
            };
        if fresh.access_key_id().is_empty() {
            return Err(credentials_error(
                self.profile(),
                false,
                &CredentialsError::not_loaded_no_source(),
            ));
        }
        *slot = Some(fresh.clone());
        Ok(fresh)
    }
}

fn expiring(creds: &Credentials, now: SystemTime, window: Duration) -> bool {
    creds
        .expiry()
        .is_some_and(|at| match at.duration_since(now) {
            Ok(left) => left < window,
            Err(_) => true, // already expired
        })
}

/// The error and its causes, one per line.
fn chain(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    let mut last = text.clone();
    while let Some(e) = source {
        let message = e.to_string();
        // SDK errors often wrap a cause with the same message.
        if message != last {
            text.push_str(": ");
            text.push_str(&message);
        }
        last = message;
        source = e.source();
    }
    text
}

/// Explain a credential failure. SSO profiles fail when the session expired or was never started.
pub fn credentials_error(
    profile: Option<&str>,
    sso: bool,
    err: &CredentialsError,
) -> ProviderError {
    let detail = chain(err);
    let lower = detail.to_ascii_lowercase();
    let looks_sso = sso || lower.contains("sso") || lower.contains("token has expired");
    let name = profile.unwrap_or("default");
    if looks_sso {
        return ProviderError {
            kind: FailureKind::AuthExpired,
            reason: format!(
                "The AWS SSO session for profile \"{name}\" has expired or was never started."
            ),
            suggestion: format!("Run `aws sso login --profile {name}`, then try again."),
            detail,
        };
    }
    let reason = match profile {
        Some(p) => format!("Could not load AWS credentials for profile \"{p}\"."),
        None => "No AWS credentials were found.".to_owned(),
    };
    ProviderError {
        kind: FailureKind::Auth,
        reason,
        suggestion: "Pick a profile from your AWS config, or set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY.".into(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn expiry_window() {
        let now = SystemTime::now();
        let with = |secs: u64| {
            Credentials::new("a", "b", None, Some(now + Duration::from_secs(secs)), "t")
        };
        assert!(expiring(&with(60), now, REFRESH_BEFORE));
        assert!(!expiring(&with(3600), now, REFRESH_BEFORE));
        assert!(!expiring(
            &Credentials::new("a", "b", None, None, "t"),
            now,
            REFRESH_BEFORE
        ));
    }
}
