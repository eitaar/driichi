use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{Json, extract::State};
use futures_util::StreamExt;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::sync::Mutex;
use url::Url;

use crate::{
    config::{
        ChatgptOAuthConfig, is_trusted_chatgpt_metadata_url, is_trusted_chatgpt_redirect_uri,
    },
    http::ServerState,
};

const CLIENT_METADATA_TIMEOUT: Duration = Duration::from_secs(3);
const CLIENT_METADATA_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_CLIENT_METADATA_BYTES: usize = 64 * 1024;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum CimdError {
    #[error("client metadata is unavailable")]
    Unavailable,
    #[error("client metadata response is too large")]
    TooLarge,
    #[error("client metadata is invalid")]
    InvalidDocument,
}

pub(crate) trait CimdMetadataFetcher: Send + Sync {
    fn fetch<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Vec<u8>, CimdError>>;
}

struct HttpCimdMetadataFetcher {
    client: reqwest::Client,
}

impl HttpCimdMetadataFetcher {
    fn new() -> Result<Self, CimdError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(CLIENT_METADATA_TIMEOUT)
            .build()
            .map_err(|_| CimdError::Unavailable)?;
        Ok(Self { client })
    }
}

impl CimdMetadataFetcher for HttpCimdMetadataFetcher {
    fn fetch<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Vec<u8>, CimdError>> {
        Box::pin(async move {
            let response = self
                .client
                .get(url.as_str())
                .send()
                .await
                .map_err(|_| CimdError::Unavailable)?;
            if !response.status().is_success() {
                return Err(CimdError::Unavailable);
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_CLIENT_METADATA_BYTES as u64)
            {
                return Err(CimdError::TooLarge);
            }

            let mut body = Vec::new();
            let mut chunks = response.bytes_stream();
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk.map_err(|_| CimdError::Unavailable)?;
                if body
                    .len()
                    .checked_add(chunk.len())
                    .is_none_or(|length| length > MAX_CLIENT_METADATA_BYTES)
                {
                    return Err(CimdError::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(body)
        })
    }
}

#[derive(Clone)]
struct CachedClientDocument {
    document: Value,
    fetched_at: Instant,
}

#[derive(Clone)]
pub(crate) struct CimdClientMetadataVerifier {
    fetcher: Arc<dyn CimdMetadataFetcher>,
    cache: Arc<Mutex<Option<CachedClientDocument>>>,
    cache_ttl: Duration,
}

impl CimdClientMetadataVerifier {
    pub(crate) fn new_default() -> Result<Self, CimdError> {
        Ok(Self::with_fetcher(
            Arc::new(HttpCimdMetadataFetcher::new()?),
            CLIENT_METADATA_CACHE_TTL,
        ))
    }

    fn with_fetcher(fetcher: Arc<dyn CimdMetadataFetcher>, cache_ttl: Duration) -> Self {
        Self {
            fetcher,
            cache: Arc::new(Mutex::new(None)),
            cache_ttl,
        }
    }

    pub(crate) async fn validate_client(
        &self,
        config: &ChatgptOAuthConfig,
        requested_client_id: &Url,
        requested_redirect_uri: &Url,
    ) -> Result<(), CimdError> {
        if requested_client_id.as_str() != config.client_id.as_str()
            || !is_trusted_chatgpt_metadata_url(&config.client_id)
            || !is_trusted_chatgpt_redirect_uri(&config.redirect_uri)
        {
            return Err(CimdError::InvalidDocument);
        }

        let document = self.validated_document(config).await?;
        if validate_client_document(config, &document, requested_redirect_uri) {
            Ok(())
        } else {
            Err(CimdError::InvalidDocument)
        }
    }

    async fn validated_document(&self, config: &ChatgptOAuthConfig) -> Result<Value, CimdError> {
        if !is_trusted_chatgpt_metadata_url(&config.client_id)
            || !is_trusted_chatgpt_redirect_uri(&config.redirect_uri)
        {
            return Err(CimdError::InvalidDocument);
        }

        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache
            .as_ref()
            .filter(|cached| cached.fetched_at.elapsed() < self.cache_ttl)
            && validate_client_document(config, &cached.document, &config.redirect_uri)
        {
            return Ok(cached.document.clone());
        }

        let body = self.fetcher.fetch(&config.client_id).await?;
        if body.len() > MAX_CLIENT_METADATA_BYTES {
            return Err(CimdError::TooLarge);
        }
        let document: Value =
            serde_json::from_slice(&body).map_err(|_| CimdError::InvalidDocument)?;
        if !validate_client_document(config, &document, &config.redirect_uri) {
            return Err(CimdError::InvalidDocument);
        }

        *cache = Some(CachedClientDocument {
            document: document.clone(),
            fetched_at: Instant::now(),
        });
        Ok(document)
    }
}

pub(crate) struct OAuthGatewayState {
    pub(crate) config: ChatgptOAuthConfig,
    pub(crate) client_metadata: CimdClientMetadataVerifier,
}

impl OAuthGatewayState {
    pub(crate) fn new(config: ChatgptOAuthConfig) -> Result<Self, CimdError> {
        Ok(Self {
            config,
            client_metadata: CimdClientMetadataVerifier::new_default()?,
        })
    }
}

pub(crate) fn validate_client_document(
    config: &ChatgptOAuthConfig,
    document: &Value,
    redirect: &Url,
) -> bool {
    if !is_trusted_chatgpt_metadata_url(&config.client_id)
        || !is_trusted_chatgpt_redirect_uri(&config.redirect_uri)
        || redirect.as_str() != config.redirect_uri.as_str()
    {
        return false;
    }

    document["client_id"].as_str() == Some(config.client_id.as_str())
        && document["redirect_uris"].as_array().is_some_and(|uris| {
            uris.iter()
                .any(|value| value.as_str() == Some(redirect.as_str()))
        })
        && document["token_endpoint_auth_methods_supported"]
            .as_array()
            .is_some_and(|methods| {
                methods
                    .iter()
                    .any(|value| value.as_str() == Some("none"))
            })
}

fn protected_resource_document(config: &ChatgptOAuthConfig) -> Value {
    json!({
        "resource": config.resource.as_str(),
        "authorization_servers": [config.issuer_identifier()],
        "scopes_supported": ["driichi:play"],
        "bearer_methods_supported": ["header"]
    })
}

fn authorization_server_document(config: &ChatgptOAuthConfig) -> Value {
    let issuer = config.issuer_identifier();
    json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/api/v1/admin/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "scopes_supported": ["driichi:play"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": true
    })
}

pub(crate) async fn get_protected_resource_metadata(
    State(state): State<Arc<ServerState>>,
) -> Json<Value> {
    let config = &state
        .chatgpt_oauth
        .as_ref()
        .expect("OAuth metadata route is mounted only when OAuth is enabled")
        .config;
    Json(protected_resource_document(config))
}

pub(crate) async fn get_authorization_server_metadata(
    State(state): State<Arc<ServerState>>,
) -> Json<Value> {
    let config = &state
        .chatgpt_oauth
        .as_ref()
        .expect("OAuth metadata route is mounted only when OAuth is enabled")
        .config;
    Json(authorization_server_document(config))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct FakeFetcher {
        body: Vec<u8>,
        calls: AtomicUsize,
        requested_urls: std::sync::Mutex<Vec<String>>,
    }

    impl FakeFetcher {
        fn new(body: Vec<u8>) -> Self {
            Self {
                body,
                calls: AtomicUsize::new(0),
                requested_urls: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl CimdMetadataFetcher for FakeFetcher {
        fn fetch<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Vec<u8>, CimdError>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.requested_urls
                    .lock()
                    .unwrap()
                    .push(url.as_str().to_owned());
                Ok(self.body.clone())
            })
        }
    }

    fn test_config() -> ChatgptOAuthConfig {
        let issuer = Url::parse("https://driichi.example/").unwrap();
        let mut resource = issuer.clone();
        resource.set_path("/chatgpt/mcp");
        ChatgptOAuthConfig {
            issuer,
            resource,
            client_id: Url::parse("https://chatgpt.com/oauth/client.json").unwrap(),
            redirect_uri: Url::parse(
                "https://chatgpt.com/connector_platform_oauth_redirect",
            )
            .unwrap(),
            allowed_origins: vec![Url::parse("https://chatgpt.com/").unwrap()],
        }
    }

    fn client_document(config: &ChatgptOAuthConfig) -> Value {
        json!({
            "client_id": config.client_id.as_str(),
            "redirect_uris": [config.redirect_uri.as_str()],
            "token_endpoint_auth_method": "private_key_jwt",
            "token_endpoint_auth_methods_supported": ["none", "private_key_jwt"]
        })
    }

    #[test]
    fn validates_exact_client_redirect_and_plural_none_method() {
        let config = test_config();
        let document = client_document(&config);
        assert!(validate_client_document(
            &config,
            &document,
            &config.redirect_uri
        ));

        let mut wrong_client = document.clone();
        wrong_client["client_id"] = json!("https://chatgpt.com/other/client.json");
        assert!(!validate_client_document(
            &config,
            &wrong_client,
            &config.redirect_uri
        ));

        let mut wrong_redirect = document.clone();
        wrong_redirect["redirect_uris"] = json!(["https://chatgpt.com/other/callback"]);
        assert!(!validate_client_document(
            &config,
            &wrong_redirect,
            &config.redirect_uri
        ));

        let mut wrong_auth = document;
        wrong_auth["token_endpoint_auth_methods_supported"] = json!(["private_key_jwt"]);
        assert!(!validate_client_document(
            &config,
            &wrong_auth,
            &config.redirect_uri
        ));
    }

    #[tokio::test]
    async fn fetches_only_the_configured_document_and_rechecks_redirect_on_cached_use() {
        let config = test_config();
        let fetcher = Arc::new(FakeFetcher::new(
            serde_json::to_vec(&client_document(&config)).unwrap(),
        ));
        let verifier = CimdClientMetadataVerifier::with_fetcher(
            fetcher.clone(),
            Duration::from_secs(60),
        );

        assert!(
            verifier
                .validate_client(&config, &config.client_id, &config.redirect_uri)
                .await
                .is_ok()
        );
        assert!(
            verifier
                .validate_client(&config, &config.client_id, &config.redirect_uri)
                .await
                .is_ok()
        );
        let wrong_redirect = Url::parse("https://chatgpt.com/not-the-configured-callback").unwrap();
        assert!(
            verifier
                .validate_client(&config, &config.client_id, &wrong_redirect)
                .await
                .is_err()
        );
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            fetcher.requested_urls.lock().unwrap().as_slice(),
            ["https://chatgpt.com/oauth/client.json"]
        );
    }

    #[tokio::test]
    async fn rejects_private_hosts_and_metadata_bodies_over_the_size_cap() {
        let mut private_config = test_config();
        private_config.client_id = Url::parse("https://127.0.0.1/oauth/client.json").unwrap();
        let never_used_fetcher = Arc::new(FakeFetcher::new(Vec::new()));
        let verifier = CimdClientMetadataVerifier::with_fetcher(
            never_used_fetcher.clone(),
            Duration::from_secs(60),
        );
        assert!(
            verifier
                .validate_client(
                    &private_config,
                    &private_config.client_id,
                    &private_config.redirect_uri
                )
                .await
                .is_err()
        );
        assert_eq!(never_used_fetcher.calls.load(Ordering::SeqCst), 0);

        let config = test_config();
        let too_large = Arc::new(FakeFetcher::new(vec![b'x'; MAX_CLIENT_METADATA_BYTES + 1]));
        let verifier =
            CimdClientMetadataVerifier::with_fetcher(too_large, Duration::from_secs(60));
        assert_eq!(
            verifier
                .validate_client(&config, &config.client_id, &config.redirect_uri)
                .await,
            Err(CimdError::TooLarge)
        );
    }
}
