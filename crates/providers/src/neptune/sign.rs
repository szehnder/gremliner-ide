//! SigV4 signing for Neptune (service `neptune-db`): the WebSocket upgrade and the HTTP endpoints.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use aws_credential_types::Credentials;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use gremlin_client::HandshakeSigner;

use super::aws::CredentialCache;

pub const SERVICE: &str = "neptune-db";

/// Sign `request` in place: adds Authorization, X-Amz-Date and (for temporary credentials)
/// X-Amz-Security-Token. The URI's authority is the signed Host, so it must name the real endpoint.
pub fn sign_request<B>(
    request: &mut http::Request<B>,
    body: &[u8],
    creds: &Credentials,
    region: &str,
    time: SystemTime,
) -> Result<(), String> {
    let identity: Identity = creds.clone().into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name(SERVICE)
        .time(time)
        .settings(SigningSettings::default())
        .build()
        .map_err(|e| e.to_string())?
        .into();
    let uri = request.uri().to_string();
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
        .collect();
    let signable = SignableRequest::new(
        request.method().as_str(),
        uri,
        headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        SignableBody::Bytes(body),
    )
    .map_err(|e| e.to_string())?;
    let (instructions, _) = sign(signable, &params)
        .map_err(|e| e.to_string())?
        .into_parts();
    instructions.apply_to_request_http1x(request);
    Ok(())
}

/// Signs every WebSocket (re)connect with the connection's current credentials.
pub struct NeptuneSigner {
    pub credentials: Arc<CredentialCache>,
    pub region: String,
}

impl HandshakeSigner for NeptuneSigner {
    fn sign<'a>(
        &'a self,
        request: &'a mut http::Request<()>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let creds = self.credentials.get().await.map_err(|e| {
                if e.suggestion.is_empty() {
                    e.reason.clone()
                } else {
                    format!("{} {}", e.reason, e.suggestion)
                }
            })?;
            // SigV4 signs https:// even for wss:// (ADR-0016).
            let uri = request.uri().to_string();
            let https = uri
                .replacen("wss://", "https://", 1)
                .replacen("ws://", "http://", 1);
            *request.uri_mut() = https
                .parse()
                .map_err(|e: http::uri::InvalidUri| e.to_string())?;
            let result = sign_request(request, &[], &creds, &self.region, SystemTime::now());
            *request.uri_mut() = uri
                .parse()
                .map_err(|e: http::uri::InvalidUri| e.to_string())?;
            result
        })
    }
}
