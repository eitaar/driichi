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

#[path = "oauth_store.rs"]
mod oauth_store;
pub(crate) use oauth_store::{
    AccessGrant, CodeExchange, OAuthError, OAuthService, RefreshExchange, TokenPair,
};

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
    pub(crate) grants: OAuthService,
}

impl OAuthGatewayState {
    pub(crate) fn new(
        config: ChatgptOAuthConfig,
        storage: std::sync::Arc<crate::Storage>,
    ) -> Result<Self, CimdError> {
        Ok(Self {
            grants: OAuthService::new(config.clone(), storage),
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
            .is_some_and(|methods| methods.iter().any(|value| value.as_str() == Some("none")))
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use sha2::Digest;

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
            redirect_uri: Url::parse("https://chatgpt.com/connector_platform_oauth_redirect")
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
    fn accepts_callback_id_scoped_client_metadata_urls() {
        let mut config = test_config();
        config.client_id =
            Url::parse("https://chatgpt.com/oauth/callback_123/client.json").unwrap();
        let document = client_document(&config);
        assert!(validate_client_document(
            &config,
            &document,
            &config.redirect_uri
        ));
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
        let verifier =
            CimdClientMetadataVerifier::with_fetcher(fetcher.clone(), Duration::from_secs(60));

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
        let verifier = CimdClientMetadataVerifier::with_fetcher(too_large, Duration::from_secs(60));
        assert_eq!(
            verifier
                .validate_client(&config, &config.client_id, &config.redirect_uri)
                .await,
            Err(CimdError::TooLarge)
        );
    }

    const RESOURCE: &str = "https://driichi.example/chatgpt/mcp";
    const CLIENT_ID: &str = "https://chatgpt.com/oauth/client.json";
    const REDIRECT_URI: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
    const TEST_VERIFIER: &str = "a-very-long-test-verifier-which-is-at-least-43-characters";

    fn test_challenge(verifier: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use sha2::{Digest, Sha256};
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    fn test_data_root(name: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "driichi-oauth-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    async fn test_service(name: &str) -> (std::path::PathBuf, Arc<crate::Storage>, OAuthService) {
        let root = test_data_root(name);
        let storage = Arc::new(crate::Storage::connect(&root).await.unwrap());
        let service = OAuthService::new(test_config(), storage.clone());
        (root, storage, service)
    }

    async fn issue_test_code(service: &OAuthService) -> String {
        service
            .issue_authorization_code(
                CLIENT_ID,
                REDIRECT_URI,
                RESOURCE,
                "driichi:play",
                &test_challenge(TEST_VERIFIER),
            )
            .await
            .unwrap()
    }

    fn code_exchange(code: String, verifier: &str) -> CodeExchange {
        CodeExchange {
            client_id: CLIENT_ID.to_owned(),
            redirect_uri: REDIRECT_URI.to_owned(),
            code,
            verifier: verifier.to_owned(),
            resource: RESOURCE.to_owned(),
        }
    }

    fn refresh_exchange(refresh_token: String) -> RefreshExchange {
        RefreshExchange {
            client_id: CLIENT_ID.to_owned(),
            refresh_token,
            resource: RESOURCE.to_owned(),
        }
    }

    #[tokio::test]
    async fn code_exchange_enforces_pkce_client_redirect_resource_and_single_use() {
        let (root, storage, service) = test_service("code-exchange").await;
        let code = issue_test_code(&service).await;

        let wrong_verifier = code_exchange(
            code.clone(),
            "another-verifier-with-more-than-43-characters",
        );
        assert!(service.exchange_code(wrong_verifier).await.is_err());

        let mut wrong_client = code_exchange(code.clone(), TEST_VERIFIER);
        wrong_client.client_id = "https://chatgpt.com/other/client.json".to_owned();
        assert!(service.exchange_code(wrong_client).await.is_err());

        let mut wrong_redirect = code_exchange(code.clone(), TEST_VERIFIER);
        wrong_redirect.redirect_uri = "https://chatgpt.com/other/callback".to_owned();
        assert!(service.exchange_code(wrong_redirect).await.is_err());

        for resource in ["", "https://foreign.example/mcp"] {
            let mut wrong_resource = code_exchange(code.clone(), TEST_VERIFIER);
            wrong_resource.resource = resource.to_owned();
            assert!(service.exchange_code(wrong_resource).await.is_err());
        }

        let pair = service
            .exchange_code(code_exchange(code.clone(), TEST_VERIFIER))
            .await
            .unwrap();
        assert!(
            service
                .exchange_code(code_exchange(code.clone(), TEST_VERIFIER))
                .await
                .is_err()
        );
        assert_eq!(
            service
                .validate_access(&pair.access_token, RESOURCE, "driichi:play")
                .await
                .unwrap()
                .subject,
            "admin"
        );
        assert!(
            service
                .validate_access(
                    &pair.access_token,
                    "https://foreign.example/mcp",
                    "driichi:play"
                )
                .await
                .is_err()
        );
        assert!(
            service
                .validate_access(&pair.access_token, RESOURCE, "driichi:other")
                .await
                .is_err()
        );

        let code_hash: Vec<u8> = sqlx::query_scalar("SELECT code_hash FROM oauth_codes LIMIT 1")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert_eq!(code_hash.len(), 32);
        assert_ne!(code_hash.as_slice(), code.as_bytes());
        let refresh_hash: Vec<u8> =
            sqlx::query_scalar("SELECT token_hash FROM oauth_refresh_tokens LIMIT 1")
                .fetch_one(storage.pool())
                .await
                .unwrap();
        let access_hash: Vec<u8> =
            sqlx::query_scalar("SELECT token_hash FROM oauth_access_tokens LIMIT 1")
                .fetch_one(storage.pool())
                .await
                .unwrap();
        assert_eq!(refresh_hash.len(), 32);
        assert_eq!(access_hash.len(), 32);
        assert_ne!(refresh_hash.as_slice(), pair.refresh_token.as_bytes());
        assert_ne!(access_hash.as_slice(), pair.access_token.as_bytes());

        storage.close().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn simultaneous_code_redemptions_have_one_winner() {
        let (root, storage, service) = test_service("code-replay").await;
        let code = issue_test_code(&service).await;
        let first = service.exchange_code(code_exchange(code.clone(), TEST_VERIFIER));
        let second = service.exchange_code(code_exchange(code, TEST_VERIFIER));
        let (first, second) = tokio::join!(first, second);
        match (first, second) {
            (Ok(_), Err(_)) | (Err(_), Ok(_)) => {}
            _ => panic!("exactly one concurrent code redemption must win"),
        }
        let families: i64 = sqlx::query_scalar("SELECT count(*) FROM oauth_refresh_families")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert_eq!(families, 1);

        storage.close().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn access_expires_in_ten_minutes_and_refresh_family_in_thirty_days() {
        let (root, storage, service) = test_service("lifetimes").await;
        let code = issue_test_code(&service).await;
        let pair = service
            .exchange_code(code_exchange(code, TEST_VERIFIER))
            .await
            .unwrap();
        let access = sqlx::query(
            "SELECT issued_at, expires_at FROM oauth_access_tokens WHERE token_hash = ?",
        )
        .bind(sha2::Sha256::digest(pair.access_token.as_bytes()).to_vec())
        .fetch_one(storage.pool())
        .await
        .unwrap();
        let access_issued: i64 = access.try_get("issued_at").unwrap();
        let access_expires: i64 = access.try_get("expires_at").unwrap();
        assert_eq!(access_expires - access_issued, 10 * 60);

        let family = sqlx::query(
            "SELECT f.issued_at, f.expires_at \
             FROM oauth_refresh_families f \
             JOIN oauth_refresh_tokens r ON r.family_id = f.family_id \
             WHERE r.token_hash = ?",
        )
        .bind(sha2::Sha256::digest(pair.refresh_token.as_bytes()).to_vec())
        .fetch_one(storage.pool())
        .await
        .unwrap();
        let family_issued: i64 = family.try_get("issued_at").unwrap();
        let family_expires: i64 = family.try_get("expires_at").unwrap();
        assert_eq!(family_expires - family_issued, 30 * 24 * 60 * 60);

        sqlx::query("UPDATE oauth_access_tokens SET expires_at = 0")
            .execute(storage.pool())
            .await
            .unwrap();
        assert!(
            service
                .validate_access(&pair.access_token, RESOURCE, "driichi:play")
                .await
                .is_err()
        );
        sqlx::query("UPDATE oauth_refresh_families SET expires_at = 0")
            .execute(storage.pool())
            .await
            .unwrap();
        assert!(
            service
                .rotate_refresh(refresh_exchange(pair.refresh_token))
                .await
                .is_err()
        );

        storage.close().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn simultaneous_refresh_redeems_once_and_replay_revokes_the_family() {
        let (root, storage, service) = test_service("refresh-replay").await;
        let code = issue_test_code(&service).await;
        let original = service
            .exchange_code(code_exchange(code, TEST_VERIFIER))
            .await
            .unwrap();

        let wrong_client = RefreshExchange {
            client_id: "https://chatgpt.com/other/client.json".to_owned(),
            refresh_token: original.refresh_token.clone(),
            resource: RESOURCE.to_owned(),
        };
        assert!(service.rotate_refresh(wrong_client).await.is_err());
        let wrong_resource = RefreshExchange {
            client_id: CLIENT_ID.to_owned(),
            refresh_token: original.refresh_token.clone(),
            resource: "https://foreign.example/mcp".to_owned(),
        };
        assert!(service.rotate_refresh(wrong_resource).await.is_err());

        let first = service.rotate_refresh(refresh_exchange(original.refresh_token.clone()));
        let second = service.rotate_refresh(refresh_exchange(original.refresh_token.clone()));
        let (first, second) = tokio::join!(first, second);
        let winner = match (first, second) {
            (Ok(pair), Err(_)) | (Err(_), Ok(pair)) => pair,
            _ => panic!("exactly one concurrent refresh must win"),
        };
        assert!(
            service
                .validate_access(&winner.access_token, RESOURCE, "driichi:play")
                .await
                .is_err()
        );
        assert!(
            service
                .rotate_refresh(refresh_exchange(winner.refresh_token))
                .await
                .is_err()
        );

        storage.close().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authorization_code_and_refresh_grant_survive_storage_restarts() {
        let (root, storage, service) = test_service("restart").await;
        let code = issue_test_code(&service).await;
        storage.close().await;
        drop(service);
        drop(storage);

        let storage = Arc::new(crate::Storage::connect(&root).await.unwrap());
        let service = OAuthService::new(test_config(), storage.clone());
        let pair = service
            .exchange_code(code_exchange(code, TEST_VERIFIER))
            .await
            .unwrap();
        storage.close().await;
        drop(service);
        drop(storage);

        let storage = Arc::new(crate::Storage::connect(&root).await.unwrap());
        let service = OAuthService::new(test_config(), storage.clone());
        let next = service
            .rotate_refresh(refresh_exchange(pair.refresh_token))
            .await
            .unwrap();
        assert!(
            service
                .validate_access(&next.access_token, RESOURCE, "driichi:play")
                .await
                .is_ok()
        );

        storage.close().await;
        let _ = std::fs::remove_dir_all(root);
    }
}
