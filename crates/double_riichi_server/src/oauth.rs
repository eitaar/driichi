use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{ConnectInfo, Extension, RawQuery, State, rejection::BytesRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::StreamExt;
use rand::random;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
    http::{self, RequestId, ServerState},
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

    pub(crate) async fn client_display_name(
        &self,
        config: &ChatgptOAuthConfig,
    ) -> Result<String, CimdError> {
        let document = self.validated_document(config).await?;
        let name = document["client_name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("ChatGPT");
        Ok(name
            .chars()
            .filter(|character| !character.is_control())
            .take(120)
            .collect())
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthorizationRequest {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    state: String,
    scope: String,
    resource: String,
    code_challenge: String,
    code_challenge_method: String,
}

#[derive(Clone)]
struct PendingAuthorization {
    request: AuthorizationRequest,
    client_name: String,
    expires_at: Instant,
}

pub(crate) struct OAuthGatewayState {
    pub(crate) config: ChatgptOAuthConfig,
    pub(crate) client_metadata: CimdClientMetadataVerifier,
    pub(crate) grants: OAuthService,
    pending_authorizations: Mutex<HashMap<[u8; 32], PendingAuthorization>>,
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
            pending_authorizations: Mutex::new(HashMap::new()),
        })
    }

    #[cfg(debug_assertions)]
    pub(crate) fn new_for_tests(
        config: ChatgptOAuthConfig,
        storage: std::sync::Arc<crate::Storage>,
    ) -> Result<Self, CimdError> {
        let document = json!({
            "client_id": config.client_id.as_str(),
            "redirect_uris": [config.redirect_uri.as_str()],
            "token_endpoint_auth_methods_supported": ["none"],
            "client_name": "ChatGPT <Driichi>"
        });
        let fetcher = Arc::new(StaticCimdMetadataFetcher {
            body: serde_json::to_vec(&document).map_err(|_| CimdError::InvalidDocument)?,
        });
        Ok(Self {
            grants: OAuthService::new(config.clone(), storage),
            client_metadata: CimdClientMetadataVerifier::with_fetcher(
                fetcher,
                CLIENT_METADATA_CACHE_TTL,
            ),
            config,
            pending_authorizations: Mutex::new(HashMap::new()),
        })
    }
}

#[cfg(debug_assertions)]
struct StaticCimdMetadataFetcher {
    body: Vec<u8>,
}

#[cfg(debug_assertions)]
impl CimdMetadataFetcher for StaticCimdMetadataFetcher {
    fn fetch<'a>(&'a self, _url: &'a Url) -> BoxFuture<'a, Result<Vec<u8>, CimdError>> {
        Box::pin(async move { Ok(self.body.clone()) })
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
        "authorization_response_iss_parameter_supported": true,
        "client_id_metadata_document_supported": true
    })
}

const AUTHORIZATION_FLOW_LIFETIME: Duration = Duration::from_secs(10 * 60);
const OAUTH_CSRF_COOKIE_LIFETIME_SECONDS: u64 = 10 * 60;
const MAX_PENDING_AUTHORIZATIONS: usize = 1024;
const MAX_OAUTH_FORM_BYTES: usize = 16 * 1024;
const MAX_OAUTH_QUERY_BYTES: usize = 8 * 1024;
const AUTHORIZATION_FIELDS: [&str; 9] = [
    "client_id",
    "redirect_uri",
    "response_type",
    "state",
    "scope",
    "resource",
    "code_challenge",
    "code_challenge_method",
    "ui_locales",
];

fn parse_urlencoded_fields(input: &[u8], limit: usize) -> Result<HashMap<String, String>, ()> {
    if input.len() > limit {
        return Err(());
    }
    let mut fields = HashMap::new();
    for (key, value) in url::form_urlencoded::parse(input) {
        if key.is_empty() || key.len() > 128 || value.len() > 4096 {
            return Err(());
        }
        if fields
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(());
        }
    }
    Ok(fields)
}

fn form_body(
    headers: &HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<HashMap<String, String>, ()> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.split(';').next().is_some_and(|value| {
        value
            .trim()
            .eq_ignore_ascii_case("application/x-www-form-urlencoded")
    }) {
        return Err(());
    }
    let body = body.map_err(|_| ())?;
    parse_urlencoded_fields(body.as_ref(), MAX_OAUTH_FORM_BYTES)
}

fn authorization_request(fields: &HashMap<String, String>) -> Option<AuthorizationRequest> {
    if fields
        .keys()
        .any(|key| !AUTHORIZATION_FIELDS.contains(&key.as_str()))
    {
        return None;
    }
    let request = AuthorizationRequest {
        client_id: fields.get("client_id")?.clone(),
        redirect_uri: fields.get("redirect_uri")?.clone(),
        response_type: fields.get("response_type")?.clone(),
        state: fields.get("state")?.clone(),
        scope: fields.get("scope")?.clone(),
        resource: fields.get("resource")?.clone(),
        code_challenge: fields.get("code_challenge")?.clone(),
        code_challenge_method: fields.get("code_challenge_method")?.clone(),
    };
    let challenge_is_s256 = request.code_challenge.len() == 43
        && request
            .code_challenge
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if request.response_type != "code"
        || request.state.is_empty()
        || request.state.len() > 2048
        || request.scope != "driichi:play"
        || request.code_challenge_method != "S256"
        || !challenge_is_s256
    {
        return None;
    }
    Some(request)
}

fn authorization_fields_from_form(
    fields: &HashMap<String, String>,
    extra_allowed: &[&str],
) -> Option<AuthorizationRequest> {
    if fields.keys().any(|key| {
        !AUTHORIZATION_FIELDS.contains(&key.as_str()) && !extra_allowed.contains(&key.as_str())
    }) {
        return None;
    }
    let auth_fields = fields
        .iter()
        .filter(|(key, _)| AUTHORIZATION_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    authorization_request(&auth_fields)
}

fn redirect_is_configured(
    fields: &HashMap<String, String>,
    config: &ChatgptOAuthConfig,
) -> Option<String> {
    (fields.get("client_id").map(String::as_str) == Some(config.client_id.as_str())
        && fields.get("redirect_uri").map(String::as_str) == Some(config.redirect_uri.as_str()))
    .then(|| {
        fields
            .get("state")
            .filter(|state| !state.is_empty() && state.len() <= 2048)
            .cloned()
    })
    .flatten()
}

fn oauth_redirect(
    config: &ChatgptOAuthConfig,
    state: &str,
    code: Option<&str>,
    error: Option<&str>,
) -> Response {
    let mut target = config.redirect_uri.clone();
    {
        let mut query = target.query_pairs_mut();
        if let Some(code) = code {
            query.append_pair("code", code);
        }
        if let Some(error) = error {
            query.append_pair("error", error);
        }
        query.append_pair("state", state);
        query.append_pair("iss", &config.issuer_identifier());
    }
    redirect_to(target)
}

fn redirect_to(target: Url) -> Response {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(
            header::LOCATION,
            HeaderValue::from_str(target.as_str()).expect("redirect URL is valid"),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::PRAGMA, "no-cache")
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn oauth_json_error(status: StatusCode, error: &'static str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::PRAGMA, "no-cache")
        .body(Body::from(json!({"error": error}).to_string()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn strict_same_origin(headers: &HeaderMap, state: &ServerState) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let Some(value) = origins.next() else {
        return false;
    };
    if origins.next().is_some() {
        return false;
    }
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Ok(origin) = Url::parse(value) else {
        return false;
    };
    origin.path() == "/"
        && origin.query().is_none()
        && origin.fragment().is_none()
        && http::same_origin(&origin, state.public_origin_url())
}

fn csrf_digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn new_csrf_token() -> String {
    URL_SAFE_NO_PAD.encode(random::<[u8; 32]>())
}

fn csrf_cookie_matches(headers: &HeaderMap, submitted: &str) -> bool {
    http::cookie_value(headers, "driichi_oauth_csrf")
        .is_some_and(|cookie| cookie == submitted && !submitted.is_empty())
}

async fn register_pending_authorization(
    gateway: &OAuthGatewayState,
    csrf: &str,
    request: AuthorizationRequest,
    client_name: String,
) {
    let now = Instant::now();
    let mut pending = gateway.pending_authorizations.lock().await;
    pending.retain(|_, flow| flow.expires_at > now);
    let key = csrf_digest(csrf);
    if pending.len() >= MAX_PENDING_AUTHORIZATIONS && !pending.contains_key(&key) {
        if let Some(oldest) = pending
            .iter()
            .min_by_key(|(_, flow)| flow.expires_at)
            .map(|(key, _)| *key)
        {
            pending.remove(&oldest);
        }
    }
    pending.insert(
        key,
        PendingAuthorization {
            request,
            client_name,
            expires_at: now + AUTHORIZATION_FLOW_LIFETIME,
        },
    );
}

async fn pending_authorization(
    gateway: &OAuthGatewayState,
    csrf: &str,
) -> Option<PendingAuthorization> {
    let now = Instant::now();
    let mut pending = gateway.pending_authorizations.lock().await;
    pending.retain(|_, flow| flow.expires_at > now);
    pending.get(&csrf_digest(csrf)).cloned()
}

async fn consume_pending_authorization(
    gateway: &OAuthGatewayState,
    csrf: &str,
    request: &AuthorizationRequest,
) -> Option<PendingAuthorization> {
    let mut pending = gateway.pending_authorizations.lock().await;
    let key = csrf_digest(csrf);
    if pending
        .get(&key)
        .is_some_and(|flow| flow.expires_at > Instant::now() && flow.request == *request)
    {
        pending.remove(&key)
    } else {
        None
    }
}

fn html_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn hidden_field(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        html_escape(name),
        html_escape(value)
    )
}

fn authorization_hidden_fields(request: &AuthorizationRequest, csrf: &str) -> String {
    let mut fields = hidden_field("csrf", csrf);
    for (name, value) in [
        ("client_id", request.client_id.as_str()),
        ("redirect_uri", request.redirect_uri.as_str()),
        ("response_type", request.response_type.as_str()),
        ("state", request.state.as_str()),
        ("scope", request.scope.as_str()),
        ("resource", request.resource.as_str()),
        ("code_challenge", request.code_challenge.as_str()),
        (
            "code_challenge_method",
            request.code_challenge_method.as_str(),
        ),
    ] {
        fields.push_str(&hidden_field(name, value));
    }
    fields
}

fn html_response(body: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::PRAGMA, "no-cache")
        .header(
            "content-security-policy",
            "default-src 'none'; form-action 'self' https://chatgpt.com; base-uri 'none'; frame-ancestors 'none'",
        )
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "origin")
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn sign_in_page(request: &AuthorizationRequest, client_name: &str, csrf: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Sign in</title><main><h1>Sign in to Driichi</h1><p>{} is requesting access to this server.</p><form method=\"post\" action=\"/api/v1/admin/oauth/login\">{}<label>Username <input name=\"username\" autocomplete=\"username\" required></label><label>Password <input name=\"password\" type=\"password\" autocomplete=\"current-password\" required></label><button type=\"submit\">Sign in</button></form></main></html>",
        html_escape(client_name),
        authorization_hidden_fields(request, csrf)
    )
}

fn consent_page(request: &AuthorizationRequest, client_name: &str, csrf: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Authorize ChatGPT</title><main><h1>Authorize connection</h1><p><strong>{}</strong> is requesting:</p><ul><li>Scope: <code>{}</code></li><li>Resource: <code>{}</code></li><li>Access: play games and read match state on this server</li></ul><form method=\"post\" action=\"/api/v1/admin/oauth/authorize\">{}<button name=\"decision\" value=\"approve\" type=\"submit\">Approve</button><button name=\"decision\" value=\"deny\" type=\"submit\">Deny</button></form></main></html>",
        html_escape(client_name),
        html_escape(&request.scope),
        html_escape(&request.resource),
        authorization_hidden_fields(request, csrf)
    )
}

fn with_csrf_cookie(mut response: Response, state: &ServerState, csrf: &str) -> Response {
    response.headers_mut().append(
        header::SET_COOKIE,
        http::oauth_csrf_cookie(state, csrf, OAUTH_CSRF_COOKIE_LIFETIME_SECONDS),
    );
    response
}

fn clear_csrf_cookie(response: &mut Response, state: &ServerState) {
    response
        .headers_mut()
        .append(header::SET_COOKIE, http::oauth_csrf_cookie(state, "", 0));
}

pub(crate) async fn get_authorize(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let Some(gateway) = &state.chatgpt_oauth else {
        return oauth_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    let Some(raw_query) = raw_query else {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    let fields = match parse_urlencoded_fields(raw_query.as_bytes(), MAX_OAUTH_QUERY_BYTES) {
        Ok(fields) => fields,
        Err(()) => return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let safe_state = redirect_is_configured(&fields, &gateway.config);
    let Some(request) = authorization_request(&fields) else {
        return safe_state
            .map(|state| oauth_redirect(&gateway.config, &state, None, Some("invalid_request")))
            .unwrap_or_else(|| oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request"));
    };
    if request.client_id != gateway.config.client_id.as_str()
        || request.redirect_uri != gateway.config.redirect_uri.as_str()
    {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    if request.resource != gateway.config.resource.as_str() {
        return oauth_redirect(
            &gateway.config,
            &request.state,
            None,
            Some("invalid_request"),
        );
    }
    let client_id = Url::parse(&request.client_id).expect("configured client URL is valid");
    let redirect_uri = Url::parse(&request.redirect_uri).expect("configured redirect URL is valid");
    if gateway
        .client_metadata
        .validate_client(&gateway.config, &client_id, &redirect_uri)
        .await
        .is_err()
    {
        return oauth_redirect(&gateway.config, &request.state, None, Some("server_error"));
    }
    let client_name = gateway
        .client_metadata
        .client_display_name(&gateway.config)
        .await
        .unwrap_or_else(|_| "ChatGPT".to_owned());
    let csrf = new_csrf_token();
    register_pending_authorization(gateway, &csrf, request.clone(), client_name.clone()).await;
    let is_admin = http::require_admin(&state, &headers, &request_id).is_ok();
    let page = if is_admin {
        consent_page(&request, &client_name, &csrf)
    } else {
        sign_in_page(&request, &client_name, &csrf)
    };
    with_csrf_cookie(html_response(page), &state, &csrf)
}

pub(crate) async fn post_login(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    if !strict_same_origin(&headers, &state) {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let mut fields = match form_body(&headers, body) {
        Ok(fields) => fields,
        Err(()) => return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let Some(csrf) = fields.get("csrf").cloned() else {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    };
    if !csrf_cookie_matches(&headers, &csrf) {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let Some(request) = authorization_fields_from_form(&fields, &["csrf", "username", "password"])
    else {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    };
    let Some(username) = fields.get("username").filter(|value| value.len() <= 256) else {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    let Some(password) = fields.get("password").filter(|value| value.len() <= 4096) else {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    let Some(gateway) = &state.chatgpt_oauth else {
        return oauth_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    let Some(pending) = pending_authorization(gateway, &csrf).await else {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    };
    if pending.request != request {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    }

    let username = username.clone();
    let password = password.clone();
    let login = http::authenticate_admin_login(
        &state,
        &headers,
        &request_id,
        peer.as_ref().map(|value| value.0.0),
        move || Ok((username, password)),
    )
    .await;
    let login = match login {
        Ok(login) => login,
        Err(response) => return response,
    };
    let mut response = html_response(consent_page(&request, &pending.client_name, &csrf));
    response
        .headers_mut()
        .append(header::SET_COOKIE, login.cookie);
    response.headers_mut().append(
        header::SET_COOKIE,
        http::oauth_csrf_cookie(&state, &csrf, OAUTH_CSRF_COOKIE_LIFETIME_SECONDS),
    );
    response
}

pub(crate) async fn post_consent(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    if !strict_same_origin(&headers, &state) {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let fields = match form_body(&headers, body) {
        Ok(fields) => fields,
        Err(()) => return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let Some(csrf) = fields.get("csrf").cloned() else {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    };
    if !csrf_cookie_matches(&headers, &csrf) {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let Some(request) = authorization_fields_from_form(&fields, &["csrf", "decision"]) else {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    };
    let Some(decision) = fields.get("decision").map(String::as_str) else {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    let Some(gateway) = &state.chatgpt_oauth else {
        return oauth_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    if http::require_admin(&state, &headers, &request_id).is_err() {
        return oauth_json_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let Some(pending) = consume_pending_authorization(gateway, &csrf, &request).await else {
        return oauth_json_error(StatusCode::FORBIDDEN, "forbidden");
    };
    let mut response = match decision {
        "deny" => oauth_redirect(
            &gateway.config,
            &pending.request.state,
            None,
            Some("access_denied"),
        ),
        "approve" => match gateway
            .grants
            .issue_authorization_code(
                &pending.request.client_id,
                &pending.request.redirect_uri,
                &pending.request.resource,
                &pending.request.scope,
                &pending.request.code_challenge,
            )
            .await
        {
            Ok(code) => oauth_redirect(&gateway.config, &pending.request.state, Some(&code), None),
            Err(_) => oauth_redirect(
                &gateway.config,
                &pending.request.state,
                None,
                Some("server_error"),
            ),
        },
        _ => oauth_redirect(
            &gateway.config,
            &pending.request.state,
            None,
            Some("invalid_request"),
        ),
    };
    clear_csrf_cookie(&mut response, &state);
    response
}

pub(crate) async fn post_token(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some(gateway) = &state.chatgpt_oauth else {
        return oauth_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    if raw_query.as_deref().is_some_and(|query| !query.is_empty()) {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let fields = match form_body(&headers, body) {
        Ok(fields) => fields,
        Err(()) => return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let Some(grant_type) = fields.get("grant_type").map(String::as_str) else {
        return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    let pair = match grant_type {
        "authorization_code" => {
            if fields.keys().any(|key| {
                ![
                    "grant_type",
                    "client_id",
                    "resource",
                    "code",
                    "redirect_uri",
                    "code_verifier",
                ]
                .contains(&key.as_str())
            }) {
                return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
            }
            let (Some(client_id), Some(resource), Some(code), Some(redirect_uri), Some(verifier)) = (
                fields.get("client_id"),
                fields.get("resource"),
                fields.get("code"),
                fields.get("redirect_uri"),
                fields.get("code_verifier"),
            ) else {
                return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
            };
            gateway
                .grants
                .exchange_code(CodeExchange {
                    client_id: client_id.clone(),
                    redirect_uri: redirect_uri.clone(),
                    resource: resource.clone(),
                    code: code.clone(),
                    verifier: verifier.clone(),
                })
                .await
        }
        "refresh_token" => {
            if fields.keys().any(|key| {
                !["grant_type", "client_id", "resource", "refresh_token"].contains(&key.as_str())
            }) {
                return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
            }
            let (Some(client_id), Some(resource), Some(refresh_token)) = (
                fields.get("client_id"),
                fields.get("resource"),
                fields.get("refresh_token"),
            ) else {
                return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request");
            };
            gateway
                .grants
                .rotate_refresh(RefreshExchange {
                    client_id: client_id.clone(),
                    resource: resource.clone(),
                    refresh_token: refresh_token.clone(),
                })
                .await
        }
        _ => return oauth_json_error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    };
    match pair {
        Ok(pair) => {
            let body = serde_json::to_vec(&pair).expect("OAuth token response can be serialized");
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::CACHE_CONTROL, "no-store")
                .header(header::PRAGMA, "no-cache")
                .body(Body::from(body))
                .unwrap_or_else(|_| Response::new(Body::empty()))
        }
        Err(_) => oauth_json_error(StatusCode::BAD_REQUEST, "invalid_grant"),
    }
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
    use sqlx::Row;

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
    async fn refreshed_access_lifetime_stops_at_family_expiry() {
        let (root, storage, service) = test_service("family-deadline").await;
        let code = issue_test_code(&service).await;
        let original = service
            .exchange_code(code_exchange(code, TEST_VERIFIER))
            .await
            .unwrap();

        sqlx::query(
            "UPDATE oauth_refresh_families \
             SET expires_at = CAST(strftime('%s', 'now') AS INTEGER) + 300",
        )
        .execute(storage.pool())
        .await
        .unwrap();

        let refreshed = service
            .rotate_refresh(refresh_exchange(original.refresh_token))
            .await
            .unwrap();
        let (access_issued, access_expires, family_expires): (i64, i64, i64) = sqlx::query_as(
            "SELECT a.issued_at, a.expires_at, f.expires_at \
             FROM oauth_access_tokens a \
             JOIN oauth_refresh_families f ON f.family_id = a.family_id \
             WHERE a.token_hash = ?",
        )
        .bind(sha2::Sha256::digest(refreshed.access_token.as_bytes()).to_vec())
        .fetch_one(storage.pool())
        .await
        .unwrap();

        assert!(access_expires <= family_expires);
        assert_eq!(access_expires, family_expires);
        assert_eq!(
            refreshed.expires_in,
            (family_expires - access_issued) as u64
        );
        assert!(refreshed.expires_in < 10 * 60);

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
