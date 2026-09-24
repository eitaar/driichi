use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use axum::{
    Extension, Router,
    body::{Body, Bytes},
    extract::{
        ConnectInfo, DefaultBodyLimit, MatchedPath, Path, RawQuery, State, WebSocketUpgrade,
        rejection::BytesRejection,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::{any, get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use double_riichi_core::{
    AudienceProjection, CharacterCatalog, CharacterUsage as CoreCharacterUsage, GameAction,
    GameEvent, GameMode, MatchPlayerSnapshot, MatchRole, Participant, ParticipantId,
    ParticipantKind, Presence, RoomCommand, RoomConfig, RoomController, RoomError, RoomEvent,
    RoomHandle, RoomJoinCode, RoomPhase, RoomRegistry, RoomRegistryError, RoomRemoval,
    RoomResponse, RoomSnapshot, Seat, TimeControl,
};
use futures_util::{SinkExt, StreamExt};
use rand::random;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    sync::{Mutex as AsyncMutex, OwnedMutexGuard, mpsc},
    time::{self, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tower_http::compression::CompressionLayer;
use url::Url;

use crate::compat::CompatState;
use crate::storage::{ReplaySummary, StorageError};
use crate::{
    AdminAuthenticator, AdminSecrets, BotTokenAuthority, BotTokenService, CharacterAsset,
    CharacterRegistry, CharacterRegistryError, CredentialError, RuntimeConfig, Storage,
};

const HTTP_JSON_LIMIT: usize = 64 * 1024;
const HUMAN_WS_MESSAGE_LIMIT: usize = 64 * 1024;
const ADMIN_SESSION_COOKIE: &str = "driichi_admin";
const GUEST_COOKIE_PREFIX: &str = "driichi_guest_";
const ADMIN_SESSION_MAX_AGE: u64 = 12 * 60 * 60;
const GUEST_SESSION_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
const GUEST_SESSION_MAX_ENTRIES: usize = 8_192;
const HUMAN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const HUMAN_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(60);
const HUMAN_OUTBOUND_CAPACITY: usize = 64;
const SERVER_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'";
const OPENAPI_YAML: &str = include_str!("../../../spec/openapi.yaml");

#[derive(Clone, Debug)]
pub struct ServerLimits {
    pub max_connections: usize,
    pub room_participant_limit: usize,
    pub code_lookup_limit: usize,
    pub participant_creation_limit: usize,
    pub admin_login_failure_limit: usize,
    pub agent_auth_failure_limit: usize,
    pub max_compat_matches: usize,
    pub max_ranked_queue: usize,
    pub rate_window: Duration,
    pub admin_login_window: Duration,
    pub trusted_proxy_cidrs: Vec<IpCidr>,
    pub http_json_limit: usize,
    pub human_ws_message_limit: usize,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            max_connections: 256,
            room_participant_limit: 32,
            code_lookup_limit: 20,
            participant_creation_limit: 10,
            admin_login_failure_limit: 5,
            agent_auth_failure_limit: 20,
            max_compat_matches: 32,
            max_ranked_queue: 128,
            rate_window: Duration::from_secs(60),
            admin_login_window: Duration::from_secs(15 * 60),
            trusted_proxy_cidrs: Vec::new(),
            http_json_limit: HTTP_JSON_LIMIT,
            human_ws_message_limit: HUMAN_WS_MESSAGE_LIMIT,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpCidr {
    network: IpAddr,
    prefix: u8,
}

impl IpCidr {
    pub fn new(network: IpAddr, prefix: u8) -> Result<Self, &'static str> {
        let max = match network {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > max {
            return Err("CIDR prefix is outside the address width");
        }
        Ok(Self {
            network: mask_ip(network, prefix),
            prefix,
        })
    }

    pub fn contains(&self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                prefix_matches_v4(u32::from(network), u32::from(address), self.prefix)
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                prefix_matches_v6(u128::from(network), u128::from(address), self.prefix)
            }
            _ => false,
        }
    }
}

impl FromStr for IpCidr {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = value.split_once('/').ok_or("CIDR must contain a slash")?;
        let address = address
            .parse::<IpAddr>()
            .map_err(|_| "CIDR address is invalid")?;
        let prefix = prefix.parse::<u8>().map_err(|_| "CIDR prefix is invalid")?;
        Self::new(address, prefix)
    }
}

fn mask_ip(address: IpAddr, prefix: u8) -> IpAddr {
    match address {
        IpAddr::V4(value) => IpAddr::V4(std::net::Ipv4Addr::from(
            u32::from(value) & prefix_mask_v4(prefix),
        )),
        IpAddr::V6(value) => IpAddr::V6(std::net::Ipv6Addr::from(
            u128::from(value) & prefix_mask_v6(prefix),
        )),
    }
}

fn prefix_mask_v4(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    }
}

fn prefix_mask_v6(prefix: u8) -> u128 {
    if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    }
}

fn prefix_matches_v4(network: u32, address: u32, prefix: u8) -> bool {
    (network & prefix_mask_v4(prefix)) == (address & prefix_mask_v4(prefix))
}

fn prefix_matches_v6(network: u128, address: u128, prefix: u8) -> bool {
    (network & prefix_mask_v6(prefix)) == (address & prefix_mask_v6(prefix))
}

#[derive(Debug, Error)]
pub enum ServerInitError {
    #[error("admin secrets could not be loaded")]
    Secrets(#[source] crate::SecretsError),
    #[error("admin authentication could not be initialized")]
    Auth(#[source] CredentialError),
    #[error("storage could not be initialized")]
    Storage(#[source] crate::StorageError),
    #[error("Character registry could not be initialized")]
    Characters(#[source] CharacterRegistryError),
    #[error("trusted proxy CIDR is invalid")]
    TrustedProxy,
    #[error("ChatGPT OAuth client could not be initialized")]
    ChatgptOAuth(#[source] crate::oauth::CimdError),
}

#[derive(Clone)]
pub struct ServerState {
    public_origin: String,
    public_origin_url: Url,
    secure_cookies: bool,
    admin: Arc<AdminAuthenticator>,
    rooms: RoomRegistry,
    registry: Option<Arc<CharacterRegistry>>,
    character_catalog: CharacterCatalog,
    guest_sessions: Arc<GuestSessionStore>,
    connections: Arc<HumanConnections>,
    rate_limiter: Arc<RateLimiter>,
    connection_count: Arc<AtomicUsize>,
    limits: ServerLimits,
    api_docs_enabled: bool,
    shutdown_seconds: u64,
    started_at: Instant,
    admission_open: Arc<AtomicBool>,
    shutdown_token: CancellationToken,
    admin_mutation_lock: Arc<AsyncMutex<()>>,
    replay_probe_ok: Arc<AtomicBool>,
    storage_maintenance_started: Arc<AtomicBool>,
    storage: Option<Arc<Storage>>,
    bot_tokens: Option<Arc<BotTokenService>>,
    pub(crate) chatgpt_oauth: Option<Arc<crate::oauth::OAuthGatewayState>>,
    pub(crate) compat: Arc<CompatState>,
    mcp_session_idle_seconds: u64,
    mcp_character: String,
    mcp_provider_characters: std::collections::BTreeMap<String, String>,
}

impl ServerState {
    pub fn for_tests(
        public_origin: impl Into<String>,
        admin: Arc<AdminAuthenticator>,
        rooms: RoomRegistry,
    ) -> Self {
        let mut state = Self::new_inner(
            public_origin.into(),
            admin,
            rooms,
            None,
            CharacterCatalog::starter(),
            ServerLimits::default(),
        );
        state.storage = None;
        state.bot_tokens = None;
        state
    }

    pub fn with_bot_token_service(mut self, service: Arc<BotTokenService>) -> Self {
        self.compat.watch_revocations(
            service.subscribe_revocations(),
            service.clone(),
            self.rooms.clone(),
        );
        if self.storage.is_none() {
            self.storage = Some(service.storage());
            self.start_storage_maintenance();
        }
        self.bot_tokens = Some(service);
        self
    }

    pub fn with_registry(
        public_origin: impl Into<String>,
        admin: Arc<AdminAuthenticator>,
        rooms: RoomRegistry,
        registry: Arc<CharacterRegistry>,
    ) -> Self {
        let catalog = catalog_from_registry(&registry);
        Self::new_inner(
            public_origin.into(),
            admin,
            rooms,
            Some(registry),
            catalog,
            ServerLimits::default(),
        )
    }

    pub fn with_api_docs_enabled(mut self, enabled: bool) -> Self {
        self.api_docs_enabled = enabled;
        self
    }

    pub fn with_limits(mut self, limits: ServerLimits) -> Self {
        self.rate_limiter = Arc::new(RateLimiter::new());
        self.compat
            .set_limits(limits.max_compat_matches, limits.max_ranked_queue);
        self.limits = limits;
        self
    }

    pub fn public_origin(&self) -> &str {
        &self.public_origin
    }

    pub fn rooms(&self) -> &RoomRegistry {
        &self.rooms
    }

    pub fn limits(&self) -> &ServerLimits {
        &self.limits
    }

    pub(crate) fn public_origin_url(&self) -> &Url {
        &self.public_origin_url
    }

    pub(crate) fn bot_token_active(&self, token_id: &str) -> bool {
        self.bot_tokens
            .as_ref()
            .is_some_and(|service| service.is_active_token_id(token_id))
    }

    pub(crate) fn mcp_session_idle_seconds(&self) -> u64 {
        self.mcp_session_idle_seconds
    }

    pub(crate) fn mcp_character_for_provider(&self, provider: &str) -> String {
        self.mcp_provider_characters
            .get(provider)
            .cloned()
            .unwrap_or_else(|| self.mcp_character.clone())
    }

    pub(crate) fn subscribe_bot_revocations(
        &self,
    ) -> Option<tokio::sync::broadcast::Receiver<crate::TokenRevoked>> {
        self.bot_tokens
            .as_ref()
            .map(|service| service.subscribe_revocations())
    }

    pub(crate) fn authenticate_bot(
        &self,
        headers: &HeaderMap,
        direct_peer: Option<SocketAddr>,
    ) -> Result<crate::BotTokenRecord, ()> {
        let ip = request_ip(headers, direct_peer, &self.limits.trusted_proxy_cidrs);
        if !self.rate_limiter.available(
            RateKind::AgentAuthFailure,
            ip,
            self.limits.agent_auth_failure_limit,
            self.limits.rate_window,
        ) {
            return Err(());
        }
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|value| !value.is_empty() && !value.contains(char::is_whitespace));
        let Some(token) = token else {
            self.rate_limiter
                .record(RateKind::AgentAuthFailure, ip, self.limits.rate_window);
            return Err(());
        };
        let Some(service) = &self.bot_tokens else {
            self.rate_limiter
                .record(RateKind::AgentAuthFailure, ip, self.limits.rate_window);
            return Err(());
        };
        match service.authenticate(token) {
            Ok(record) => Ok(record),
            Err(_) => {
                self.rate_limiter
                    .record(RateKind::AgentAuthFailure, ip, self.limits.rate_window);
                Err(())
            }
        }
    }

    pub fn begin_shutdown(&self) {
        self.admission_open.store(false, Ordering::Release);
        self.shutdown_token.cancel();
        self.compat.begin_shutdown();
    }

    pub(crate) fn admission_open(&self) -> bool {
        self.admission_open.load(Ordering::Acquire)
    }

    pub(crate) fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    pub async fn shutdown(&self) {
        self.begin_shutdown();
        self.compat.shutdown().await;
        self.rooms
            .shutdown(double_riichi_core::ShutdownMode::Graceful)
            .await;
        if let Some(storage) = &self.storage {
            if let Err(error) = storage.startup_cleanup().await {
                tracing::warn!(
                    error_kind = error.replay_failure_kind(),
                    "incomplete storage cleanup failed during shutdown"
                );
            }
            storage.close().await;
        }
    }

    pub async fn from_config(config: RuntimeConfig) -> Result<Self, ServerInitError> {
        let secrets = AdminSecrets::load(config.data_root()).map_err(ServerInitError::Secrets)?;
        let admin = Arc::new(
            AdminAuthenticator::new(secrets.username(), secrets.password_hash())
                .map_err(ServerInitError::Auth)?,
        );
        let storage = Arc::new(
            Storage::connect(config.data_root())
                .await
                .map_err(ServerInitError::Storage)?,
        );
        let token_authority = Arc::new(BotTokenAuthority::from_records(
            storage
                .load_bot_tokens()
                .await
                .map_err(ServerInitError::Storage)?,
        ));
        let token_service = Arc::new(BotTokenService::new(storage.clone(), token_authority));
        let chatgpt_oauth_requested = config.chatgpt_oauth.is_some();
        let chatgpt_oauth_config = config.chatgpt_oauth.clone().filter(|_| {
            std::env::var("DRIICHI_CHATGPT_BOT_TOKEN")
                .ok()
                .is_some_and(|token| token_service.authenticate(&token).is_ok())
        });
        let trusted_proxy_cidrs = config
            .network
            .trusted_proxy_cidrs
            .iter()
            .map(|value| value.parse().map_err(|_| ServerInitError::TrustedProxy))
            .collect::<Result<Vec<IpCidr>, _>>()?;
        let limits = ServerLimits {
            max_connections: config.network.max_connections,
            room_participant_limit: config.network.room_participant_limit,
            code_lookup_limit: config.network.code_lookup_per_minute,
            participant_creation_limit: config.network.participant_creation_per_minute,
            admin_login_failure_limit: config.network.admin_login_failures_per_15_minutes,
            agent_auth_failure_limit: config.network.agent_auth_failures_per_minute,
            trusted_proxy_cidrs,
            max_compat_matches: config.network.max_compat_matches,
            max_ranked_queue: config.network.max_ranked_queue,
            ..ServerLimits::default()
        };
        let registry = Arc::new(
            config
                .load_character_registry()
                .map_err(ServerInitError::Characters)?,
        );
        let worker_storage = Arc::clone(&storage);
        let mut state = Self::with_registry(
            config.public_origin.clone(),
            admin,
            RoomRegistry::new().with_effect_spawner(move |effects| {
                crate::storage::spawn_room_effect_worker(Arc::clone(&worker_storage), effects)
            }),
            registry,
        );
        state
            .compat
            .set_limits(limits.max_compat_matches, limits.max_ranked_queue);
        state.limits = limits;
        state.api_docs_enabled = config.api_docs;
        state.shutdown_seconds = config.shutdown_seconds;
        state.mcp_session_idle_seconds = config.mcp_session_idle_seconds;
        state.mcp_character = config.characters.mcp.clone();
        state.mcp_provider_characters = config.characters.mcp_providers.clone();
        state.storage = Some(Arc::clone(&storage));
        state.compat.watch_revocations(
            token_service.subscribe_revocations(),
            token_service.clone(),
            state.rooms.clone(),
        );
        state.bot_tokens = Some(token_service);
        state.chatgpt_oauth = chatgpt_oauth_config
            .map(crate::oauth::OAuthGatewayState::new)
            .transpose()
            .map_err(ServerInitError::ChatgptOAuth)?
            .map(Arc::new);
        if chatgpt_oauth_requested && state.chatgpt_oauth.is_none() {
            tracing::warn!(
                "ChatGPT OAuth discovery is disabled because DRIICHI_CHATGPT_BOT_TOKEN is missing or inactive"
            );
        }
        state.start_storage_maintenance();
        Ok(state)
    }

    fn new_inner(
        public_origin: String,
        admin: Arc<AdminAuthenticator>,
        rooms: RoomRegistry,
        registry: Option<Arc<CharacterRegistry>>,
        character_catalog: CharacterCatalog,
        limits: ServerLimits,
    ) -> Self {
        let (public_origin, public_origin_url) = canonical_origin(&public_origin);
        let secure_cookies = public_origin_url.scheme() == "https";
        let mcp_character = character_catalog
            .default_for(ParticipantKind::MCP)
            .unwrap_or_else(|| "mcp-agent".to_owned());
        let compat = Arc::new(CompatState::new(
            limits.max_compat_matches,
            limits.max_ranked_queue,
        ));
        let guest_sessions = Arc::new(GuestSessionStore::default());
        let state = Self {
            public_origin,
            public_origin_url,
            secure_cookies,
            admin,
            rooms,
            registry,
            character_catalog,
            guest_sessions,
            connections: Arc::new(HumanConnections::default()),
            rate_limiter: Arc::new(RateLimiter::new()),
            connection_count: Arc::new(AtomicUsize::new(0)),
            limits,
            api_docs_enabled: false,
            shutdown_token: CancellationToken::new(),
            admin_mutation_lock: Arc::new(AsyncMutex::new(())),
            replay_probe_ok: Arc::new(AtomicBool::new(true)),
            storage_maintenance_started: Arc::new(AtomicBool::new(false)),
            shutdown_seconds: 10,
            started_at: Instant::now(),
            admission_open: Arc::new(AtomicBool::new(true)),
            storage: None,
            bot_tokens: None,
            chatgpt_oauth: None,
            compat,
            mcp_session_idle_seconds: 30 * 60,
            mcp_character,
            mcp_provider_characters: std::collections::BTreeMap::new(),
        };
        state.start_guest_session_cleanup();
        state
    }

    fn start_guest_session_cleanup(&self) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let cleanup_sessions = self.guest_sessions.clone();
        let cleanup_rooms = self.rooms.clone();
        let shutdown = self.shutdown_token.clone();
        handle.spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = time::sleep(Duration::from_secs(60)) => {
                        cleanup_sessions.prune_for_rooms(&cleanup_rooms).await;
                    }
                }
            }
        });
    }

    fn start_storage_maintenance(&self) {
        let Some(storage) = self.storage.clone() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        if self
            .storage_maintenance_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.replay_probe_ok.store(
            !storage.replay_degraded() && storage.probe_replay().is_ok(),
            Ordering::Release,
        );
        let replay_probe_ok = self.replay_probe_ok.clone();
        let shutdown = self.shutdown_token.clone();
        handle.spawn(async move {
            let mut replay_tick = time::interval(Duration::from_secs(60));
            replay_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            replay_tick.tick().await;
            let mut audit_tick = time::interval(Duration::from_secs(24 * 60 * 60));
            audit_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            audit_tick.tick().await;
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = replay_tick.tick() => {
                        replay_probe_ok.store(storage.probe_replay().is_ok(), Ordering::Release);
                    }
                    _ = audit_tick.tick() => {
                        if let Err(error) = storage.retry_pending_audits().await {
                            tracing::warn!(error = ?error, "pending Admin audit recovery failed");
                        }
                        if let Err(error) = storage.cleanup_audit(audit_now()).await {
                            tracing::warn!(error = ?error, "audit retention cleanup failed");
                        }
                    }
                }
            }
        });
    }

    pub fn shutdown_seconds(&self) -> u64 {
        self.shutdown_seconds
    }

    fn room_config(&self, room_name: String, mode: GameMode) -> RoomConfig {
        let mut config = RoomConfig::new(room_name, mode, self.character_catalog.clone());
        config.max_participants = self.limits.room_participant_limit;
        config
    }

    pub(crate) fn participant_creation_allowed(
        &self,
        headers: &HeaderMap,
        direct_peer: Option<SocketAddr>,
    ) -> bool {
        let ip = request_ip(headers, direct_peer, &self.limits.trusted_proxy_cidrs);
        self.rate_limiter.allowed(
            RateKind::ParticipantCreation,
            ip,
            self.limits.participant_creation_limit,
            self.limits.rate_window,
        )
    }

    pub(crate) fn replay_root(&self) -> Option<std::path::PathBuf> {
        self.storage
            .as_ref()
            .map(|storage| storage.replay_root().to_path_buf())
    }

    pub(crate) fn replay_storage(&self) -> Option<Arc<Storage>> {
        self.storage.clone()
    }

    pub(crate) fn connection_permit(&self) -> Option<ConnectionPermit> {
        let mut current = self.connection_count.load(Ordering::Relaxed);
        loop {
            if current >= self.limits.max_connections {
                return None;
            }
            match self.connection_count.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(ConnectionPermit(self.connection_count.clone())),
                Err(observed) => current = observed,
            }
        }
    }
}

fn catalog_from_registry(registry: &CharacterRegistry) -> CharacterCatalog {
    let mut catalog = CharacterCatalog::starter();
    for character in registry.characters() {
        let usage = match character.usage() {
            crate::CharacterUsage::Human => CoreCharacterUsage::Human,
            crate::CharacterUsage::Mjai => CoreCharacterUsage::Mjai,
            crate::CharacterUsage::Mcp => CoreCharacterUsage::Mcp,
            crate::CharacterUsage::Builtin => CoreCharacterUsage::BuiltInBot,
        };
        catalog.insert(character.id(), usage);
    }
    catalog
}

#[derive(Clone, Debug)]
pub(crate) struct RequestId(String);

#[derive(Debug, Clone)]
struct ApiError {
    status: StatusCode,
    title: &'static str,
    detail: &'static str,
    code: &'static str,
}

impl ApiError {
    const fn new(
        status: StatusCode,
        title: &'static str,
        detail: &'static str,
        code: &'static str,
    ) -> Self {
        Self {
            status,
            title,
            detail,
            code,
        }
    }

    fn response(&self, request_id: &RequestId) -> Response {
        problem_response(
            self.status,
            self.title,
            self.detail,
            self.code,
            &request_id.0,
        )
    }
}

fn problem_response(
    status: StatusCode,
    title: &'static str,
    detail: &'static str,
    code: &'static str,
    request_id: &str,
) -> Response {
    let body = json!({
        "type": "about:blank",
        "title": title,
        "status": status.as_u16(),
        "detail": detail,
        "code": code,
        "request_id": request_id,
    });
    let mut response = json_response(status, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

pub(crate) fn json_response(status: StatusCode, body: Value) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn parse_json<T>(
    headers: &HeaderMap,
    body: Result<Bytes, BytesRejection>,
    request_id: &RequestId,
) -> Result<T, Response>
where
    T: DeserializeOwned,
{
    let body = body.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Request too large",
            "The request body exceeds the allowed limit.",
            "request_too_large",
        )
        .response(request_id)
    })?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported media type",
            "The request must use application/json.",
            "unsupported_media_type",
        )
        .response(request_id));
    }
    serde_json::from_slice(&body).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "Invalid request",
            "The request body is invalid.",
            "invalid_request",
        )
        .response(request_id)
    })
}

async fn request_context(
    Extension(state): Extension<Arc<ServerState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = RequestId(generate_ulid());
    let path = request.uri().path().to_owned();
    let method = request.method().as_str().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());
    let peer_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|peer| peer.0.ip().to_string());
    let auth_record_id = authenticated_record_id(&state, request.headers());
    let request_ids = request_log_ids(&route, &path);
    let started_at = Instant::now();
    request.extensions_mut().insert(request_id.clone());
    let mut response = if state.admission_open() {
        next.run(request).await
    } else {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Server shutting down",
            "The server is not accepting new requests.",
            "server_shutting_down",
        )
        .response(&request_id)
    };
    if (path.starts_with("/api/v1/") || path.starts_with("/ws/v1/"))
        && response.status().is_client_error()
        && !response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/problem+json"))
    {
        let (title, detail, code) = if response.status() == StatusCode::METHOD_NOT_ALLOWED {
            (
                "Method not allowed",
                "The HTTP method is not allowed.",
                "method_not_allowed",
            )
        } else {
            (
                "Invalid request",
                "The request could not be processed.",
                "invalid_request",
            )
        };
        response = problem_response(response.status(), title, detail, code, &request_id.0);
    }
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&request_id.0).expect("ULID is a valid header"),
    );
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        ),
        ("content-security-policy", SERVER_CSP),
    ] {
        response
            .headers_mut()
            .entry(name)
            .or_insert(HeaderValue::from_static(value));
    }
    if path.starts_with("/api/") || path.starts_with("/ws/") {
        response
            .headers_mut()
            .entry(header::CACHE_CONTROL)
            .or_insert(HeaderValue::from_static("no-store"));
    }
    tracing::info!(
        request_id = %request_id.0,
        route = %route,
        method = %method,
        status = response.status().as_u16(),
        latency_ms = started_at.elapsed().as_millis() as u64,
        peer_ip = peer_ip.as_deref().unwrap_or("unknown"),
        auth_record_id = auth_record_id.as_deref().unwrap_or("anonymous"),
        room_id = request_ids.room_id.as_deref().unwrap_or("none"),
        match_id = request_ids.match_id.as_deref().unwrap_or("none"),
        participant_id = request_ids.participant_id.as_deref().unwrap_or("none"),
        "http request"
    );
    response
}

#[derive(Default)]
struct RequestLogIds {
    room_id: Option<String>,
    match_id: Option<String>,
    participant_id: Option<String>,
}

fn request_log_ids(route: &str, path: &str) -> RequestLogIds {
    const MAX_LOG_ID_CHARS: usize = 128;

    let bounded = |value: &str| value.chars().take(MAX_LOG_ID_CHARS).collect::<String>();
    let segments: Vec<_> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut ids = RequestLogIds::default();
    if route.contains("{join_code}") {
        ids.room_id = segments
            .windows(2)
            .find(|window| window[0] == "rooms")
            .map(|window| bounded(window[1]));
    }
    if route.contains("{match_id}") {
        ids.match_id = segments
            .windows(2)
            .find(|window| window[0] == "replays")
            .map(|window| bounded(window[1]));
    }
    if route.contains("{participant_id}") {
        ids.participant_id = segments
            .windows(2)
            .find(|window| window[0] == "participants")
            .map(|window| bounded(window[1]));
    }
    ids
}

fn authenticated_record_id(state: &ServerState, headers: &HeaderMap) -> Option<String> {
    if let Some(value) = cookie_value(headers, ADMIN_SESSION_COOKIE)
        && let Ok(credential) = URL_SAFE_NO_PAD.decode(value.as_bytes())
        && state
            .admin
            .sessions()
            .validate(&credential, SystemTime::now())
    {
        return Some("admin".to_owned());
    }
    let raw_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty() && !value.contains(char::is_whitespace));
    state
        .bot_tokens
        .as_ref()
        .and_then(|service| raw_token.and_then(|token| service.authenticate(token).ok()))
        .map(|record| record.token_id().to_owned())
}

fn not_found_response(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "Not found",
        "The requested resource does not exist.",
        "not_found",
    )
    .response(request_id)
}

#[cfg(frontend_dist)]
fn frontend_asset_mime(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1;
    Some(match extension {
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "webmanifest" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => return None,
    })
}

#[cfg(frontend_dist)]
fn frontend_hashed_asset(path: &str) -> bool {
    let Some(file_name) = path.rsplit('/').next() else {
        return false;
    };
    let Some(stem) = file_name.rsplit_once('.').map(|(stem, _)| stem) else {
        return false;
    };
    let Some((_, hash)) = stem.rsplit_once('-') else {
        return false;
    };
    (8..=64).contains(&hash.len())
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(frontend_dist)]
fn frontend_path_is_safe(path: &str) -> bool {
    if !path.starts_with('/')
        || !path.is_ascii()
        || path.contains('%')
        || path.contains('\\')
        || path.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        || path.contains("//")
    {
        return false;
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    path.split('/').skip(1).all(|segment| {
        !segment.is_empty() && segment != "." && segment != ".." && !segment.starts_with('.')
    })
}

#[cfg(frontend_dist)]
fn frontend_spa_route(path: &str) -> bool {
    let segments = path.split('/').skip(1).collect::<Vec<_>>();
    let valid_room_code =
        |value: &str| value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit());
    let valid_match_id = |value: &str| {
        (1..=128).contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    match segments.as_slice() {
        ["admin"] | ["admin", "login"] | ["admin", "replays"] => true,
        ["admin", "rooms", room] if valid_room_code(room) => true,
        ["admin", "replays", match_id] if valid_match_id(match_id) => true,
        ["room", room] | ["room", room, "lobby"] if valid_room_code(room) => true,
        _ => false,
    }
}

#[cfg(frontend_dist)]
fn frontend_asset_key(path: &str) -> Option<&str> {
    if !frontend_path_is_safe(path) {
        return None;
    }
    let route_path = if path.len() > 1 {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    };
    if route_path == "/" || route_path == "/index.html" || frontend_spa_route(route_path) {
        return Some("index.html");
    }
    if path.ends_with('/') {
        return None;
    }
    let asset = route_path.strip_prefix('/')?;
    let asset_name = asset.strip_prefix("assets/")?;
    (!asset_name.is_empty() && frontend_asset_mime(asset_name).is_some()).then_some(asset)
}

#[cfg(frontend_dist)]
fn frontend_response(path: &str, request_id: &RequestId) -> Response {
    let Some(asset_key) = frontend_asset_key(path) else {
        return not_found_response(request_id);
    };
    let Some(asset) = crate::FrontendAssets::get(asset_key) else {
        return not_found_response(request_id);
    };
    let (content_type, cache_control) = if asset_key == "index.html" {
        ("text/html; charset=utf-8", "no-cache")
    } else {
        (
            frontend_asset_mime(asset_key).expect("frontend asset MIME was validated"),
            if frontend_hashed_asset(asset_key) {
                "public, max-age=31536000, immutable"
            } else {
                "no-store"
            },
        )
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(asset.data.into_owned()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[cfg(frontend_dist)]
async fn frontend_fallback(
    Extension(request_id): Extension<RequestId>,
    request: Request<Body>,
) -> Response {
    if request.method() != axum::http::Method::GET && request.method() != axum::http::Method::HEAD {
        return not_found_response(&request_id);
    }
    frontend_response(request.uri().path(), &request_id)
}

#[cfg(not(frontend_dist))]
async fn frontend_fallback(
    Extension(request_id): Extension<RequestId>,
    _request: Request<Body>,
) -> Response {
    not_found_response(&request_id)
}

pub fn server_router(state: Arc<ServerState>) -> Router {
    let mcp_runtime = crate::mcp::McpRuntime::new(state.clone());
    let mut router = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/admin/openapi.yaml", get(openapi_document))
        .route("/api/v1/characters/human", get(human_characters))
        .route(
            "/assets/characters/{id}/portrait.webp",
            get(character_portrait),
        )
        .route("/assets/characters/{id}/icon.webp", get(character_icon))
        .route(
            "/assets/characters/{id}/voices/{voice}",
            get(character_voice),
        )
        .route("/api/v1/admin/login", post(admin_login))
        .route("/api/v1/admin/logout", post(admin_logout))
        .route(
            "/api/v1/admin/tokens",
            get(admin_list_tokens).post(admin_create_token),
        )
        .route(
            "/api/v1/admin/tokens/{token_id}/revoke",
            post(admin_revoke_token),
        )
        .route(
            "/api/v1/admin/rooms",
            get(admin_list_rooms).post(admin_create_room),
        )
        .route("/api/v1/admin/replays", get(admin_list_replays))
        .route(
            "/api/v1/admin/replays/{match_id}",
            get(admin_view_replay)
                .layer(
                    CompressionLayer::new()
                        .gzip(true)
                        .no_deflate()
                        .no_br()
                        .no_zstd(),
                )
                .delete(admin_delete_replay),
        )
        .route(
            "/api/v1/admin/rooms/{join_code}",
            get(admin_room_detail)
                .patch(admin_patch_room)
                .delete(admin_delete_room),
        )
        .route(
            "/api/v1/admin/rooms/{join_code}/participants/{participant_id}/select",
            post(admin_select),
        )
        .route(
            "/api/v1/admin/rooms/{join_code}/participants/{participant_id}/deselect",
            post(admin_deselect),
        )
        .route(
            "/api/v1/admin/rooms/{join_code}/participants/{participant_id}/kick",
            post(admin_kick),
        )
        .route(
            "/api/v1/admin/rooms/{join_code}/fill-with-bots",
            post(admin_fill),
        )
        .route("/api/v1/admin/rooms/{join_code}/start", post(admin_start))
        .route(
            "/api/v1/admin/rooms/{join_code}/rematch",
            post(admin_rematch),
        )
        .route(
            "/api/v1/admin/rooms/{join_code}/back-to-lobby",
            post(admin_back_to_lobby),
        )
        .route("/api/v1/rooms/{join_code}", get(public_room_lookup))
        .route("/api/v1/rooms/{join_code}/join", post(public_join))
        .route(
            "/api/v1/rooms/{join_code}/agents/join",
            post(crate::compat::agent_join),
        )
        .route("/ws/v1/rooms/{join_code}/human", any(human_upgrade))
        .route(
            "/ws/v1/rooms/{join_code}/mjai",
            any(crate::compat::room_mjai_upgrade),
        )
        .route("/ws/ranked", any(crate::compat::ranked_upgrade))
        .route("/ws/validate", any(crate::compat::validate_upgrade))
        .route("/status", get(crate::compat::status))
        .route("/mcp", any(crate::mcp::mcp_endpoint));

    if state.chatgpt_oauth.is_some() {
        router = router
            .route(
                "/.well-known/oauth-protected-resource/chatgpt/mcp",
                get(crate::oauth::get_protected_resource_metadata),
            )
            .route(
                "/.well-known/oauth-authorization-server",
                get(crate::oauth::get_authorization_server_metadata),
            );
    }

    router
        .fallback(frontend_fallback)
        .layer(DefaultBodyLimit::max(state.limits.http_json_limit))
        .layer(middleware::from_fn(request_context))
        .layer(Extension(mcp_runtime))
        .layer(Extension(state.clone()))
        .with_state(state)
}

pub(crate) fn generate_ulid() -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let millis = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let value = ((millis & ((1u128 << 48) - 1)) << 80) | (random::<u128>() & ((1u128 << 80) - 1));
    let mut result = String::with_capacity(26);
    for shift in (0..26).rev().map(|index| index * 5) {
        result.push(ALPHABET[((value >> shift) & 31) as usize] as char);
    }
    result
}

fn public_origin_allowed(headers: &HeaderMap, state: &ServerState) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|origin| Url::parse(origin).ok())
        .is_some_and(|origin| same_origin(&origin, &state.public_origin_url))
}

fn unsafe_admin_origin_allowed(headers: &HeaderMap, state: &ServerState) -> bool {
    if let Some(origin) = headers.get(header::ORIGIN) {
        return origin
            .to_str()
            .ok()
            .and_then(|origin| Url::parse(origin).ok())
            .is_some_and(|origin| same_origin(&origin, &state.public_origin_url));
    }
    headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Url::parse(value).ok())
        .is_some_and(|referer| same_origin(&referer, &state.public_origin_url))
}

pub(crate) fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && effective_port(left) == effective_port(right)
        && left.username().is_empty()
        && left.password().is_none()
}

fn effective_port(url: &Url) -> Option<u16> {
    url.port_or_known_default()
}

fn canonical_origin(value: &str) -> (String, Url) {
    let parsed = Url::parse(value).expect("validated public origin");
    let host = parsed.host_str().expect("validated public origin");
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let mut origin = format!("{}://{host}", parsed.scheme());
    if let Some(port) = parsed.port() {
        let default_port = match parsed.scheme() {
            "http" => 80,
            "https" => 443,
            _ => 0,
        };
        if port != default_port {
            origin.push(':');
            origin.push_str(&port.to_string());
        }
    }
    let url = Url::parse(&origin).expect("canonical origin is valid");
    (origin, url)
}

fn authentication_required(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "Unauthorized",
        "Authentication is required.",
        "authentication_required",
    )
    .response(request_id)
}

fn require_admin(
    state: &ServerState,
    headers: &HeaderMap,
    request_id: &RequestId,
) -> Result<Vec<u8>, Response> {
    let Some(value) = cookie_value(headers, ADMIN_SESSION_COOKIE) else {
        return Err(authentication_required(request_id));
    };
    let Ok(credential) = URL_SAFE_NO_PAD.decode(value.as_bytes()) else {
        return Err(authentication_required(request_id));
    };
    if !state
        .admin
        .sessions()
        .validate(&credential, SystemTime::now())
    {
        return Err(authentication_required(request_id));
    }
    Ok(credential)
}

fn revalidate_admin(
    state: &ServerState,
    credential: &[u8],
    request_id: &RequestId,
) -> Result<(), Response> {
    state
        .admin
        .sessions()
        .validate(credential, SystemTime::now())
        .then_some(())
        .ok_or_else(|| authentication_required(request_id))
}

fn audit_now() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn prepare_admin_audit(
    state: &ServerState,
    request_id: &RequestId,
    action: &'static str,
    target_type: &'static str,
    target_id: &str,
    summary: &Value,
) -> Result<(), StorageError> {
    let Some(storage) = state.replay_storage() else {
        return Ok(());
    };
    storage
        .prepare_admin_audit(
            audit_now(),
            &request_id.0,
            action,
            target_type,
            target_id,
            summary,
        )
        .await
}

async fn complete_admin_audit(
    state: &ServerState,
    request_id: &RequestId,
) -> Result<(), StorageError> {
    let Some(storage) = state.replay_storage() else {
        return Ok(());
    };
    storage.complete_admin_audit(&request_id.0).await
}

async fn cancel_admin_audit(
    state: &ServerState,
    request_id: &RequestId,
) -> Result<(), StorageError> {
    let Some(storage) = state.replay_storage() else {
        return Ok(());
    };
    storage.cancel_admin_audit(&request_id.0).await
}

async fn rollback_admin_audit(
    state: &ServerState,
    request_id: &RequestId,
) -> Result<(), StorageError> {
    let Some(storage) = state.replay_storage() else {
        return Ok(());
    };
    storage.rollback_admin_audit(&request_id.0).await
}

fn audit_failure(
    request_id: &RequestId,
    action: &str,
    target_type: &str,
    target_id: &str,
    error: &StorageError,
) -> Response {
    tracing::error!(
        request_id = %request_id.0,
        action,
        target_type,
        target_id = %redact_audit_target_id(target_type, target_id),
        error_kind = error.replay_failure_kind(),
        "admin mutation audit failed; success is not reported"
    );
    internal_error(request_id)
}

fn redact_audit_target_id(_target_type: &str, target_id: &str) -> String {
    crate::storage::redact_audit_target_id(target_id)
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then_some(value)
    })
}

fn secure_cookie_suffix(state: &ServerState) -> &'static str {
    if state.secure_cookies { "; Secure" } else { "" }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn admin_login(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let ip = request_ip(
        &headers,
        peer.as_ref().map(|value| value.0.0),
        &state.limits.trusted_proxy_cidrs,
    );
    if !state.rate_limiter.available(
        RateKind::AdminLoginFailure,
        ip,
        state.limits.admin_login_failure_limit,
        state.limits.admin_login_window,
    ) {
        return rate_limited(&request_id, state.limits.admin_login_window);
    }
    let payload: LoginRequest = match parse_json(&headers, body, &request_id) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let _operation = state.admin_mutation_lock.lock().await;
    let now = SystemTime::now();
    let session = match state.admin.login(&payload.username, &payload.password, now) {
        Ok(session) => session,
        Err(CredentialError::InvalidCredentials) => {
            state.rate_limiter.record(
                RateKind::AdminLoginFailure,
                ip,
                state.limits.admin_login_window,
            );
            return ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                "The username or password is invalid.",
                "invalid_credentials",
            )
            .response(&request_id);
        }
        Err(_) => {
            return ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
                "The server could not complete the request.",
                "internal_error",
            )
            .response(&request_id);
        }
    };
    let summary = json!({});
    if let Err(error) =
        prepare_admin_audit(&state, &request_id, "login", "admin", "admin", &summary).await
    {
        state.admin.sessions().revoke(session.credential());
        return audit_failure(&request_id, "login", "admin", "admin", &error);
    }
    if let Err(error) = complete_admin_audit(&state, &request_id).await {
        if rollback_admin_audit(&state, &request_id).await.is_ok() {
            state.admin.sessions().revoke(session.credential());
        } else {
            tracing::error!(
                request_id = %request_id.0,
                "failed to durably mark the login audit rolled back"
            );
        }
        return audit_failure(&request_id, "login", "admin", "admin", &error);
    }
    let value = URL_SAFE_NO_PAD.encode(session.credential().as_bytes());
    let expires_at = system_time_rfc3339(session.expires_at());
    let cookie = format!(
        "{ADMIN_SESSION_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/api/v1/admin; Max-Age={ADMIN_SESSION_MAX_AGE}{}",
        secure_cookie_suffix(&state)
    );
    let mut response = json_response(StatusCode::OK, json!({"expires_at": expires_at}));
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("session cookie is valid"),
    );
    response
}

async fn admin_logout(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let _operation = state.admin_mutation_lock.lock().await;
    let revoked = cookie_value(&headers, ADMIN_SESSION_COOKIE)
        .and_then(|value| URL_SAFE_NO_PAD.decode(value.as_bytes()).ok())
        .and_then(|credential| {
            state
                .admin
                .sessions()
                .validate(&credential, SystemTime::now())
                .then(|| state.admin.sessions().revoke(&credential))
                .flatten()
                .map(|expires_at| (credential, expires_at))
        });
    if let Some((credential, expires_at)) = revoked {
        let summary = json!({});
        if let Err(error) =
            prepare_admin_audit(&state, &request_id, "logout", "admin", "admin", &summary).await
        {
            state.admin.sessions().restore(&credential, expires_at);
            return audit_failure(&request_id, "logout", "admin", "admin", &error);
        }
        if let Err(error) = complete_admin_audit(&state, &request_id).await {
            if rollback_admin_audit(&state, &request_id).await.is_ok() {
                state.admin.sessions().restore(&credential, expires_at);
            } else {
                tracing::error!(
                    request_id = %request_id.0,
                    "failed to durably mark the logout audit rolled back"
                );
            }
            return audit_failure(&request_id, "logout", "admin", "admin", &error);
        }
    }
    let cookie = format!(
        "{ADMIN_SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/api/v1/admin; Max-Age=0{}",
        secure_cookie_suffix(&state)
    );
    let mut response = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("session cookie is valid"),
    );
    response
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTokenRequest {
    name: String,
}

#[derive(Debug, Serialize)]
struct TokenView {
    token_id: String,
    name: String,
    state: String,
    created_at: String,
    revoked_at: Option<String>,
}

fn token_view(record: &crate::BotTokenRecord) -> TokenView {
    TokenView {
        token_id: record.token_id().to_owned(),
        name: record.name().to_owned(),
        state: record.state().as_str().to_owned(),
        created_at: unix_seconds_rfc3339(record.created_at()),
        revoked_at: record.revoked_at().map(unix_seconds_rfc3339),
    }
}

async fn admin_list_tokens(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    let Some(service) = &state.bot_tokens else {
        return internal_error(&request_id);
    };
    match service.list().await {
        Ok(records) => json_response(
            StatusCode::OK,
            json!(records.iter().map(token_view).collect::<Vec<_>>()),
        ),
        Err(_) => internal_error(&request_id),
    }
}

async fn admin_create_token(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let credential = match require_admin(&state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let payload: CreateTokenRequest = match parse_json(&headers, body, &request_id) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let _operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(&state, &credential, &request_id) {
        return response;
    }
    let Some(service) = &state.bot_tokens else {
        return internal_error(&request_id);
    };
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    match service.create(&payload.name, now, &request_id.0).await {
        Ok(created) => json_response(
            StatusCode::CREATED,
            json!({
                "token_id": created.record().token_id(),
                "name": created.record().name(),
                "state": created.record().state().as_str(),
                "created_at": unix_seconds_rfc3339(created.record().created_at()),
                "revoked_at": Value::Null,
                "token": created.secret().expose(),
            }),
        ),
        Err(CredentialError::InvalidTokenName) => ApiError::new(
            StatusCode::BAD_REQUEST,
            "Invalid Token name",
            "The Bot Token name is invalid.",
            "invalid_token_name",
        )
        .response(&request_id),
        Err(_) => internal_error(&request_id),
    }
}

async fn admin_revoke_token(
    State(state): State<Arc<ServerState>>,
    Path(token_id): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    let credential = match require_admin(&state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let _operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(&state, &credential, &request_id) {
        return response;
    }
    let Some(service) = &state.bot_tokens else {
        return internal_error(&request_id);
    };
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    match service.revoke(&token_id, now, &request_id.0).await {
        Ok(()) | Err(CredentialError::AlreadyRevoked) => {
            // SQLite and the authority are durable; the supervised revocation
            // worker delivers the same TokenRevoked transition to every Room.
            match service.list().await {
                Ok(records) => records
                    .iter()
                    .find(|record| record.token_id() == token_id)
                    .map(|record| json_response(StatusCode::OK, json!(token_view(record))))
                    .unwrap_or_else(|| invalid_credentials(&request_id)),
                Err(_) => internal_error(&request_id),
            }
        }
        Err(CredentialError::InvalidCredentials) => invalid_credentials(&request_id),
        Err(_) => internal_error(&request_id),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRoomRequest {
    room_name: String,
    game_mode: String,
    #[serde(default)]
    time_control: Option<String>,
    #[serde(default)]
    replay_save: Option<bool>,
    #[serde(default)]
    participant_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchRoomRequest {
    #[serde(default)]
    room_name: Option<String>,
    #[serde(default)]
    game_mode: Option<String>,
    #[serde(default)]
    time_control: Option<String>,
    #[serde(default)]
    replay_save: Option<bool>,
    #[serde(default)]
    participant_limit: Option<usize>,
}

fn parse_mode(value: &str) -> Option<GameMode> {
    value.parse().ok()
}

fn parse_time_control(value: &str) -> Option<TimeControl> {
    match value {
        "riichi_dev" | "riichi-dev" => Some(TimeControl::RiichiDev),
        "casual" => Some(TimeControl::Casual),
        "unlimited" => Some(TimeControl::Unlimited),
        _ => None,
    }
}

fn room_configuration_changed(
    before: &RoomSnapshot,
    after: &RoomSnapshot,
    request: &PatchRoomRequest,
) -> bool {
    (request.room_name.is_some() && before.room_name != after.room_name)
        || (request.game_mode.is_some() && before.mode != after.mode)
        || (request.time_control.is_some() && before.time_control != after.time_control)
        || (request.replay_save.is_some() && before.replay_save != after.replay_save)
        || (request.participant_limit.is_some()
            && before.participant_limit != after.participant_limit)
}

fn participant_command_changed(
    before: &RoomSnapshot,
    after: &RoomSnapshot,
    participant_id: &ParticipantId,
) -> bool {
    before
        .participants
        .iter()
        .find(|participant| participant.id == *participant_id)
        != after
            .participants
            .iter()
            .find(|participant| participant.id == *participant_id)
}

fn room_command_changed(action: &str, before: &RoomSnapshot, after: &RoomSnapshot) -> bool {
    match action {
        "fill_with_bots" => {
            before
                .participants
                .iter()
                .filter(|participant| participant.kind == ParticipantKind::BuiltInBot)
                .cloned()
                .collect::<Vec<_>>()
                != after
                    .participants
                    .iter()
                    .filter(|participant| participant.kind == ParticipantKind::BuiltInBot)
                    .cloned()
                    .collect::<Vec<_>>()
        }
        "match_start" | "rematch" | "back_to_lobby" => before.phase != after.phase,
        _ => before.revision != after.revision,
    }
}

async fn admin_create_room(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let credential = match require_admin(&state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let payload: CreateRoomRequest = match parse_json(&headers, body, &request_id) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let Some(mode) = parse_mode(&payload.game_mode) else {
        return invalid_request(&request_id);
    };
    let mut config = state.room_config(payload.room_name, mode);
    if let Some(value) = payload.time_control.as_deref() {
        let Some(time_control) = parse_time_control(value) else {
            return invalid_request(&request_id);
        };
        config.time_control = time_control;
    }
    if let Some(replay_save) = payload.replay_save {
        config.replay_save = replay_save;
    }
    if let Some(limit) = payload.participant_limit {
        if !(mode.seat_count()..=state.limits.room_participant_limit).contains(&limit) {
            return invalid_request(&request_id);
        }
        config.max_participants = limit;
    }
    let _operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(&state, &credential, &request_id) {
        return response;
    }
    let summary = json!({"room_name": config.room_name});
    let (target_id, handle) = loop {
        let target = RoomJoinCode::generate();
        if let Err(error) = prepare_admin_audit(
            &state,
            &request_id,
            "room_create",
            "room",
            target.as_str(),
            &summary,
        )
        .await
        {
            return audit_failure(&request_id, "room_create", "room", target.as_str(), &error);
        }
        match state
            .rooms
            .create_with_join_code(config.clone(), target.clone())
            .await
        {
            Ok(handle) => break (target.to_string(), handle),
            Err(RoomRegistryError::CodeUnavailable) => {
                if let Err(error) = cancel_admin_audit(&state, &request_id).await {
                    return audit_failure(
                        &request_id,
                        "room_create",
                        "room",
                        target.as_str(),
                        &error,
                    );
                }
            }
            Err(error) => {
                if let Err(audit_error) = cancel_admin_audit(&state, &request_id).await {
                    return audit_failure(
                        &request_id,
                        "room_create",
                        "room",
                        target.as_str(),
                        &audit_error,
                    );
                }
                return registry_error_response(error, &request_id);
            }
        }
    };
    if let Err(error) = complete_admin_audit(&state, &request_id).await {
        return audit_failure(&request_id, "room_create", "room", &target_id, &error);
    }
    let snapshot = match handle.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return internal_error(&request_id),
    };
    room_detail_response(StatusCode::CREATED, &snapshot)
}

#[derive(Debug, Default)]
struct ReplayListQuery {
    offset: Option<u64>,
    limit: Option<u64>,
}

fn parse_replay_list_query(raw_query: Option<&str>) -> Result<ReplayListQuery, ()> {
    let mut query = ReplayListQuery::default();
    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "offset"
                if query
                    .offset
                    .replace(value.parse().map_err(|_| ())?)
                    .is_some() =>
            {
                return Err(());
            }
            "limit"
                if query
                    .limit
                    .replace(value.parse().map_err(|_| ())?)
                    .is_some() =>
            {
                return Err(());
            }
            _ => {}
        }
    }
    Ok(query)
}

async fn admin_list_replays(
    State(state): State<Arc<ServerState>>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    let query = match parse_replay_list_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(()) => {
            return ApiError::new(
                StatusCode::BAD_REQUEST,
                "Invalid pagination",
                "Replay pagination is outside the allowed range.",
                "invalid_pagination",
            )
            .response(&request_id);
        }
    };
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    if limit == 0 || limit > 100 || offset > i64::MAX as u64 {
        return ApiError::new(
            StatusCode::BAD_REQUEST,
            "Invalid pagination",
            "Replay pagination is outside the allowed range.",
            "invalid_pagination",
        )
        .response(&request_id);
    }
    let Some(storage) = state.replay_storage() else {
        return internal_error(&request_id);
    };
    match storage.list_replays(offset, limit).await {
        Ok((replays, total)) => json_response(
            StatusCode::OK,
            json!({
                "replays": replays.iter().map(replay_summary_view).collect::<Vec<_>>(),
                "offset": offset,
                "limit": limit,
                "total": total,
                "has_more": offset.saturating_add(replays.len() as u64) < total,
            }),
        ),
        Err(_) => internal_error(&request_id),
    }
}

async fn admin_view_replay(
    State(state): State<Arc<ServerState>>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    let Some(storage) = state.replay_storage() else {
        return internal_error(&request_id);
    };
    match storage
        .load_replay(&match_id)
        .await
        .and_then(|replay| storage.encode_replay_view(&replay))
    {
        Ok(payload) => replay_view_response(payload, &request_id),
        Err(error) => {
            if matches!(
                error,
                crate::storage::StorageError::ReplayUnavailable
                    | crate::storage::StorageError::ReplayTooLarge
                    | crate::storage::StorageError::ReplayCorrupt
                    | crate::storage::StorageError::UnsafeReplayPath
            ) {
                let safe_match_id = redact_audit_target_id("replay", &match_id);
                tracing::warn!(
                    match_id = %safe_match_id,
                    error_kind = error.replay_failure_kind(),
                    "admin replay view unavailable"
                );
            }
            replay_error_response(error, &request_id)
        }
    }
}

async fn admin_delete_replay(
    State(state): State<Arc<ServerState>>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    let credential = match require_admin(&state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return origin_not_allowed(&request_id);
    }
    let _operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(&state, &credential, &request_id) {
        return response;
    }
    let Some(storage) = state.replay_storage() else {
        return internal_error(&request_id);
    };
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    match storage.delete_replay(&match_id, now, &request_id.0).await {
        Ok(()) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap_or_else(|_| Response::new(Body::empty())),
        Err(error) => replay_error_response(error, &request_id),
    }
}

fn replay_summary_view(summary: &ReplaySummary) -> Value {
    json!({
        "match_id": summary.match_id,
        "source": summary.source,
        "room_name": summary.room_name,
        "game_mode": summary.game_mode,
        "started_at": unix_seconds_rfc3339(summary.started_at),
        "completed_at": unix_seconds_rfc3339(summary.completed_at),
        "file_size": summary.file_size,
        "availability": summary.availability,
        "replay_available": summary.availability == "available",
    })
}

fn replay_view_response(payload: Vec<u8>, _request_id: &RequestId) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn replay_error_response(error: crate::storage::StorageError, request_id: &RequestId) -> Response {
    match error {
        crate::storage::StorageError::ReplayNotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "Replay not found",
            "The requested Replay does not exist.",
            "replay_not_found",
        )
        .response(request_id),
        crate::storage::StorageError::ReplayTooLarge => ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Replay too large",
            "The Replay timeline exceeds the maximum size.",
            "replay_too_large",
        )
        .response(request_id),
        crate::storage::StorageError::ReplayUnavailable
        | crate::storage::StorageError::ReplayCorrupt
        | crate::storage::StorageError::UnsafeReplayPath => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Replay unavailable",
            "The Replay cannot be viewed, but it remains available for deletion.",
            "replay_unavailable",
        )
        .response(request_id),
        _ => internal_error(request_id),
    }
}

async fn admin_list_rooms(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    let mut views = Vec::new();
    for handle in state.rooms.list().await {
        if let Ok(snapshot) = handle.snapshot().await {
            views.push(room_list_view(&snapshot));
        }
    }
    json_response(StatusCode::OK, json!(views))
}

async fn admin_room_detail(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    match handle.snapshot().await {
        Ok(snapshot) => room_detail_response(StatusCode::OK, &snapshot),
        Err(_) => room_not_found(&request_id),
    }
}

async fn admin_patch_room(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let credential = match require_admin(&state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let payload: PatchRoomRequest = match parse_json(&headers, body, &request_id) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    if payload.room_name.is_none()
        && payload.game_mode.is_none()
        && payload.time_control.is_none()
        && payload.replay_save.is_none()
        && payload.participant_limit.is_none()
    {
        return invalid_request(&request_id);
    }
    let _operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(&state, &credential, &request_id) {
        return response;
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let before = match handle.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return room_not_found(&request_id),
    };
    let mode = match payload.game_mode.as_deref() {
        Some(value) => match parse_mode(value) {
            Some(mode) => Some(mode),
            None => return invalid_request(&request_id),
        },
        None => None,
    };
    let time_control = match payload.time_control.as_deref() {
        Some(value) => match parse_time_control(value) {
            Some(time_control) => Some(time_control),
            None => return invalid_request(&request_id),
        },
        None => None,
    };
    if payload
        .participant_limit
        .is_some_and(|limit| limit > state.limits.room_participant_limit)
    {
        return invalid_request(&request_id);
    }
    let mut requested_fields = Vec::new();
    if payload.room_name.is_some() {
        requested_fields.push("room_name");
    }
    if payload.game_mode.is_some() {
        requested_fields.push("game_mode");
    }
    if payload.time_control.is_some() {
        requested_fields.push("time_control");
    }
    if payload.replay_save.is_some() {
        requested_fields.push("replay_save");
    }
    if payload.participant_limit.is_some() {
        requested_fields.push("participant_limit");
    }
    let summary = json!({"changed_fields": requested_fields});
    if let Err(error) = prepare_admin_audit(
        &state,
        &request_id,
        "room_configure",
        "room",
        &join_code,
        &summary,
    )
    .await
    {
        return audit_failure(&request_id, "room_configure", "room", &join_code, &error);
    }
    let command = RoomCommand::configure(
        payload.room_name.clone(),
        mode,
        time_control,
        payload.replay_save,
        payload.participant_limit,
    );
    let snapshot = match handle.send(command).await {
        Ok(RoomResponse::Accepted(snapshot)) => snapshot,
        Ok(_) => {
            if let Err(error) = cancel_admin_audit(&state, &request_id).await {
                return audit_failure(&request_id, "room_configure", "room", &join_code, &error);
            }
            return internal_error(&request_id);
        }
        Err(error) => {
            if let Err(audit_error) = cancel_admin_audit(&state, &request_id).await {
                return audit_failure(
                    &request_id,
                    "room_configure",
                    "room",
                    &join_code,
                    &audit_error,
                );
            }
            return room_error_response(error, &request_id);
        }
    };
    if !room_configuration_changed(&before, &snapshot, &payload) {
        if let Err(error) = cancel_admin_audit(&state, &request_id).await {
            return audit_failure(&request_id, "room_configure", "room", &join_code, &error);
        }
        return room_detail_response(StatusCode::OK, &snapshot);
    }
    if let Err(error) = complete_admin_audit(&state, &request_id).await {
        return audit_failure(&request_id, "room_configure", "room", &join_code, &error);
    }
    room_detail_response(StatusCode::OK, &snapshot)
}

async fn admin_delete_room(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    let credential = match require_admin(&state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let _operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(&state, &credential, &request_id) {
        return response;
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let room_name = match handle.snapshot().await {
        Ok(snapshot) => snapshot.room_name,
        Err(_) => return room_not_found(&request_id),
    };
    let summary = json!({"room_name": room_name});
    if let Err(error) = prepare_admin_audit(
        &state,
        &request_id,
        "room_delete",
        "room",
        &join_code,
        &summary,
    )
    .await
    {
        return audit_failure(&request_id, "room_delete", "room", &join_code, &error);
    }
    match state.rooms.remove_with_outcome(&join_code).await {
        Ok(RoomRemoval::Removed) => {
            state.guest_sessions.invalidate_room(&join_code);
            if let Err(error) = complete_admin_audit(&state, &request_id).await {
                return audit_failure(&request_id, "room_delete", "room", &join_code, &error);
            }
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(Body::empty())
                .unwrap()
        }
        Ok(RoomRemoval::AlreadyGone) => {
            if let Err(error) = cancel_admin_audit(&state, &request_id).await {
                return audit_failure(&request_id, "room_delete", "room", &join_code, &error);
            }
            room_not_found(&request_id)
        }
        Err(error) => {
            if let Err(audit_error) = cancel_admin_audit(&state, &request_id).await {
                return audit_failure(&request_id, "room_delete", "room", &join_code, &audit_error);
            }
            registry_error_response(error, &request_id)
        }
    }
}

async fn admin_select(
    State(state): State<Arc<ServerState>>,
    Path((join_code, participant_id)): Path<(String, String)>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_participant_command(
        &state,
        join_code,
        participant_id,
        headers,
        request_id,
        "participant_select",
        RoomCommand::select(ParticipantId::new("placeholder")),
    )
    .await
}

async fn admin_deselect(
    State(state): State<Arc<ServerState>>,
    Path((join_code, participant_id)): Path<(String, String)>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_participant_command(
        &state,
        join_code,
        participant_id,
        headers,
        request_id,
        "participant_deselect",
        RoomCommand::deselect(ParticipantId::new("placeholder")),
    )
    .await
}

async fn admin_kick(
    State(state): State<Arc<ServerState>>,
    Path((join_code, participant_id)): Path<(String, String)>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_participant_command(
        &state,
        join_code,
        participant_id,
        headers,
        request_id,
        "participant_kick",
        RoomCommand::kick(ParticipantId::new("placeholder")),
    )
    .await
}

async fn admin_participant_command(
    state: &ServerState,
    join_code: String,
    participant_id: String,
    headers: HeaderMap,
    request_id: RequestId,
    action: &'static str,
    command: RoomCommand,
) -> Response {
    let credential = match require_admin(state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let _admin_operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(state, &credential, &request_id) {
        return response;
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let before = match handle.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return room_not_found(&request_id),
    };
    let participant_id = ParticipantId::new(participant_id);
    let is_leave = matches!(
        command,
        RoomCommand::Leave { .. } | RoomCommand::Kick { .. }
    );
    let _operation = if is_leave {
        Some(state.connections.operation_lock(&participant_id).await)
    } else {
        None
    };
    let summary = json!({"participant_id": participant_id});
    if let Err(error) = prepare_admin_audit(
        state,
        &request_id,
        action,
        "participant",
        participant_id.as_str(),
        &summary,
    )
    .await
    {
        return audit_failure(
            &request_id,
            action,
            "participant",
            participant_id.as_str(),
            &error,
        );
    }
    let command = match command {
        RoomCommand::Select { .. } => RoomCommand::select(participant_id.clone()),
        RoomCommand::Deselect { .. } => RoomCommand::deselect(participant_id.clone()),
        RoomCommand::Leave { .. } => RoomCommand::leave(participant_id.clone()),
        RoomCommand::Kick { .. } => RoomCommand::kick(participant_id.clone()),
        _ => unreachable!(),
    };
    let snapshot = match handle.send(command).await {
        Ok(RoomResponse::Accepted(snapshot)) => snapshot,
        Ok(_) => {
            if let Err(error) = cancel_admin_audit(state, &request_id).await {
                return audit_failure(
                    &request_id,
                    action,
                    "participant",
                    participant_id.as_str(),
                    &error,
                );
            }
            return internal_error(&request_id);
        }
        Err(error) => {
            if let Err(audit_error) = cancel_admin_audit(state, &request_id).await {
                return audit_failure(
                    &request_id,
                    action,
                    "participant",
                    participant_id.as_str(),
                    &audit_error,
                );
            }
            return room_error_response(error, &request_id);
        }
    };
    if !participant_command_changed(&before, &snapshot, &participant_id) {
        if let Err(error) = cancel_admin_audit(state, &request_id).await {
            return audit_failure(
                &request_id,
                action,
                "participant",
                participant_id.as_str(),
                &error,
            );
        }
        return room_detail_response(StatusCode::OK, &snapshot);
    }
    if is_leave {
        state
            .guest_sessions
            .invalidate_participant(&join_code, &participant_id);
        state
            .connections
            .close(&participant_id, 4006, "session_expired");
    }
    if let Err(error) = complete_admin_audit(state, &request_id).await {
        return audit_failure(
            &request_id,
            action,
            "participant",
            participant_id.as_str(),
            &error,
        );
    }
    room_detail_response(StatusCode::OK, &snapshot)
}

async fn admin_fill(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_room_command(
        &state,
        &join_code,
        headers,
        request_id,
        "fill_with_bots",
        RoomCommand::fill_with_bots(),
    )
    .await
}

async fn admin_start(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_room_command(
        &state,
        &join_code,
        headers,
        request_id,
        "match_start",
        RoomCommand::start(),
    )
    .await
}

async fn admin_rematch(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_room_command(
        &state,
        &join_code,
        headers,
        request_id,
        "rematch",
        RoomCommand::rematch(),
    )
    .await
}

async fn admin_back_to_lobby(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    admin_room_command(
        &state,
        &join_code,
        headers,
        request_id,
        "back_to_lobby",
        RoomCommand::back_to_lobby(),
    )
    .await
}

async fn admin_room_command(
    state: &ServerState,
    join_code: &str,
    headers: HeaderMap,
    request_id: RequestId,
    action: &'static str,
    command: RoomCommand,
) -> Response {
    let credential = match require_admin(state, &headers, &request_id) {
        Ok(credential) => credential,
        Err(response) => return response,
    };
    if !unsafe_admin_origin_allowed(&headers, state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let _admin_operation = state.admin_mutation_lock.lock().await;
    if let Err(response) = revalidate_admin(state, &credential, &request_id) {
        return response;
    }
    let Some(handle) = state.rooms.get(join_code).await else {
        return room_not_found(&request_id);
    };
    let before = match handle.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return room_not_found(&request_id),
    };
    let summary = json!({});
    if let Err(error) =
        prepare_admin_audit(state, &request_id, action, "room", join_code, &summary).await
    {
        return audit_failure(&request_id, action, "room", join_code, &error);
    }
    let response = match handle.send(command).await {
        Ok(response @ RoomResponse::Started(_)) => response,
        Ok(RoomResponse::Accepted(snapshot)) => {
            if !room_command_changed(action, &before, &snapshot) {
                if let Err(error) = cancel_admin_audit(state, &request_id).await {
                    return audit_failure(&request_id, action, "room", join_code, &error);
                }
                return room_detail_response(StatusCode::OK, &snapshot);
            }
            RoomResponse::Accepted(snapshot)
        }
        Ok(_) => {
            if let Err(error) = cancel_admin_audit(state, &request_id).await {
                return audit_failure(&request_id, action, "room", join_code, &error);
            }
            return internal_error(&request_id);
        }
        Err(error) => {
            if let Err(audit_error) = cancel_admin_audit(state, &request_id).await {
                return audit_failure(&request_id, action, "room", join_code, &audit_error);
            }
            return room_error_response(error, &request_id);
        }
    };
    if let Err(error) = complete_admin_audit(state, &request_id).await {
        return audit_failure(&request_id, action, "room", join_code, &error);
    }
    let snapshot = match response {
        RoomResponse::Accepted(snapshot) => snapshot,
        RoomResponse::Started(_) => match handle.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(_) => return internal_error(&request_id),
        },
        _ => unreachable!(),
    };
    room_detail_response(StatusCode::OK, &snapshot)
}

async fn public_room_lookup(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
) -> Response {
    let ip = request_ip(
        &headers,
        peer.as_ref().map(|value| value.0.0),
        &state.limits.trusted_proxy_cidrs,
    );
    if !state.rate_limiter.allowed(
        RateKind::CodeLookup,
        ip,
        state.limits.code_lookup_limit,
        state.limits.rate_window,
    ) {
        return rate_limited(&request_id, state.limits.rate_window);
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let Ok(snapshot) = handle.snapshot().await else {
        return room_not_found(&request_id);
    };
    let join_allowed = snapshot.participants.len() < snapshot.participant_limit;
    json_response(
        StatusCode::OK,
        json!({
            "room_name": snapshot.room_name,
            "game_mode": snapshot.mode.as_str(),
            "phase": phase_name(&snapshot.phase),
            "join_allowed": join_allowed,
            "participant_count": snapshot.participants.len(),
            "participant_limit": snapshot.participant_limit,
        }),
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanJoinRequest {
    nickname: String,
    character_id: String,
}

async fn public_join(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    state.guest_sessions.prune_for_rooms(&state.rooms).await;
    let ip = request_ip(
        &headers,
        peer.as_ref().map(|value| value.0.0),
        &state.limits.trusted_proxy_cidrs,
    );
    if !state.rate_limiter.allowed(
        RateKind::ParticipantCreation,
        ip,
        state.limits.participant_creation_limit,
        state.limits.rate_window,
    ) {
        return rate_limited(&request_id, state.limits.rate_window);
    }
    let payload: HumanJoinRequest = match parse_json(&headers, body, &request_id) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let Some(nickname) = normalize_display_text(&payload.nickname) else {
        return invalid_request(&request_id);
    };
    if !crate::is_safe_character_id(&payload.character_id) {
        return invalid_request(&request_id);
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let participant_id = ParticipantId::new(generate_ulid());
    let response = handle
        .send(RoomCommand::Join {
            participant: Participant::new(participant_id.clone(), nickname, ParticipantKind::Human),
            character_id: Some(payload.character_id),
            token_id: None,
        })
        .await;
    if let Err(error) = response {
        return room_error_response(error, &request_id);
    }
    let cookie_value = state.guest_sessions.issue(&join_code, &participant_id);
    let cookie_name = format!("{GUEST_COOKIE_PREFIX}{join_code}");
    let cookie = format!(
        "{cookie_name}={cookie_value}; HttpOnly; SameSite=Strict; Path=/ws/v1/rooms/{join_code}{}",
        secure_cookie_suffix(&state)
    );
    let mut response = json_response(
        StatusCode::CREATED,
        json!({
            "participant_id": participant_id.as_str(),
            "websocket_url": format!("/ws/v1/rooms/{join_code}/human"),
        }),
    );
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("guest cookie is valid"),
    );
    response
}

async fn human_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    state.guest_sessions.prune_for_rooms(&state.rooms).await;
    if !public_origin_allowed(&headers, &state) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Origin not allowed",
            "The request origin is not allowed.",
            "origin_not_allowed",
        )
        .response(&request_id);
    }
    let Some(handle) = state.rooms.get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let cookie_name = format!("{GUEST_COOKIE_PREFIX}{join_code}");
    let Some(cookie) = cookie_value(&headers, &cookie_name) else {
        return invalid_credentials(&request_id);
    };
    let Some(participant_id) = state.guest_sessions.authenticate(&join_code, cookie) else {
        return invalid_credentials(&request_id);
    };
    let guest_cookie = cookie.to_owned();
    let snapshot = match handle.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return room_not_found(&request_id),
    };
    if !snapshot
        .participants
        .iter()
        .any(|participant| participant.id == participant_id)
    {
        state
            .guest_sessions
            .invalidate_participant(&join_code, &participant_id);
        return invalid_credentials(&request_id);
    }
    let state_for_upgrade = state.clone();
    let handle_for_upgrade = handle.clone();
    let join_code_for_upgrade = join_code.clone();
    let participant_for_upgrade = participant_id.clone();
    let ws_limit = state.limits.human_ws_message_limit;
    ws.max_message_size(ws_limit)
        .max_frame_size(ws_limit)
        .on_upgrade(move |mut socket| async move {
            let Some(permit) = state_for_upgrade.connection_permit() else {
                let _ = socket.send(close_message(1013, "server_busy")).await;
                return;
            };
            let operation = state_for_upgrade
                .connections
                .operation_lock(&participant_for_upgrade)
                .await;
            if state_for_upgrade
                .guest_sessions
                .authenticate(&join_code_for_upgrade, &guest_cookie)
                .is_none()
            {
                drop(operation);
                let _ = socket.send(close_message(4006, "session_expired")).await;
                drop(permit);
                return;
            }
            if handle_for_upgrade
                .send(RoomCommand::reconnect(participant_for_upgrade.clone()))
                .await
                .is_err()
            {
                drop(operation);
                let _ = socket.send(close_message(4006, "session_expired")).await;
                drop(permit);
                return;
            }
            let (generation, control) = state_for_upgrade
                .connections
                .register(participant_for_upgrade.clone())
                .await;
            drop(operation);
            run_human(
                socket,
                state_for_upgrade,
                handle_for_upgrade,
                join_code_for_upgrade,
                participant_for_upgrade,
                generation,
                control,
                permit,
            )
            .await;
        })
}

#[derive(Debug)]
enum Control {
    Close { code: u16, reason: &'static str },
}

pub(crate) struct ConnectionPermit(Arc<AtomicUsize>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Default)]
struct HumanConnections {
    next_generation: AtomicU64,
    entries: Mutex<HashMap<ParticipantId, ActiveConnection>>,
    participant_locks: Mutex<HashMap<ParticipantId, Arc<AsyncMutex<()>>>>,
}

struct ParticipantOperationGuard {
    connections: Arc<HumanConnections>,
    participant_id: ParticipantId,
    lock: Option<OwnedMutexGuard<()>>,
}

impl Drop for ParticipantOperationGuard {
    fn drop(&mut self) {
        self.lock.take();
        let mut locks = self
            .connections
            .participant_locks
            .lock()
            .expect("connection lock poisoned");
        if locks
            .get(&self.participant_id)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            locks.remove(&self.participant_id);
        }
    }
}

struct ActiveConnection {
    generation: u64,
    control: mpsc::Sender<Control>,
}

impl HumanConnections {
    async fn register(&self, participant_id: ParticipantId) -> (u64, mpsc::Receiver<Control>) {
        let (control, receiver) = mpsc::channel(2);
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let old = self
            .entries
            .lock()
            .expect("connection lock poisoned")
            .insert(
                participant_id,
                ActiveConnection {
                    generation,
                    control,
                },
            );
        if let Some(old) = old {
            let _ = old.control.try_send(Control::Close {
                code: 4001,
                reason: "connected_elsewhere",
            });
        }
        (generation, receiver)
    }

    async fn operation_lock(
        self: &Arc<Self>,
        participant_id: &ParticipantId,
    ) -> ParticipantOperationGuard {
        let lock = self
            .participant_locks
            .lock()
            .expect("connection lock poisoned")
            .entry(participant_id.clone())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone();
        ParticipantOperationGuard {
            connections: self.clone(),
            participant_id: participant_id.clone(),
            lock: Some(lock.lock_owned().await),
        }
    }

    fn close(&self, participant_id: &ParticipantId, code: u16, reason: &'static str) {
        if let Some(entry) = self
            .entries
            .lock()
            .expect("connection lock poisoned")
            .get(participant_id)
        {
            let _ = entry.control.try_send(Control::Close { code, reason });
        }
    }

    fn is_current(&self, participant_id: &ParticipantId, generation: u64) -> bool {
        self.entries
            .lock()
            .expect("connection lock poisoned")
            .get(participant_id)
            .is_some_and(|entry| entry.generation == generation)
    }

    fn remove(&self, participant_id: &ParticipantId, generation: u64) {
        let mut entries = self.entries.lock().expect("connection lock poisoned");
        if entries
            .get(participant_id)
            .is_some_and(|entry| entry.generation == generation)
        {
            entries.remove(participant_id);
        }
    }
}

async fn run_human(
    socket: WebSocket,
    state: Arc<ServerState>,
    room: RoomHandle,
    join_code: String,
    participant_id: ParticipantId,
    generation: u64,
    mut control: mpsc::Receiver<Control>,
    permit: ConnectionPermit,
) {
    let (mut sender, mut receiver) = socket.split();
    let (outbound, mut outbound_receiver) = mpsc::channel::<Message>(HUMAN_OUTBOUND_CAPACITY + 1);
    let mut writer = tokio::spawn(async move {
        while let Some(message) = outbound_receiver.recv().await {
            let close = matches!(message, Message::Close(_));
            if sender.send(message).await.is_err() {
                break;
            }
            if close {
                break;
            }
        }
    });
    let mut connection = match room.subscribe().await {
        Ok(connection) => connection,
        Err(_) => {
            let _ = enqueue(&outbound, close_message(4002, "room_deleted"));
            drop(outbound);
            if time::timeout(Duration::from_secs(1), &mut writer)
                .await
                .is_err()
            {
                writer.abort();
            }
            drop(permit);
            return;
        }
    };
    // RoomActor subscriptions begin with their own snapshot. The protocol sends
    // one audience-projected snapshot after reconnect, not the actor's raw event.
    let _ = connection.recv().await;
    let _ = send_snapshot(&outbound, &room, &participant_id).await;

    let mut heartbeat = time::interval(HUMAN_HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_pong = Instant::now();
    loop {
        tokio::select! {
            command = control.recv() => {
                match command {
                    Some(Control::Close { code, reason }) => {
                        let _ = enqueue(&outbound, close_message(code, reason));
                    }
                    None => break,
                }
                break;
            }
            event = connection.recv() => {
                let Some(event) = event else {
                    let _ = enqueue(&outbound, close_message(4005, "slow_consumer"));
                    break;
                };
                if matches!(event, RoomEvent::RoomDeleted) {
                    let _ = enqueue(&outbound, close_message(4002, "room_deleted"));
                    break;
                }
                if matches!(event, RoomEvent::ServerShutdown) {
                    let _ = enqueue(&outbound, close_message(4003, "server_shutdown"));
                    break;
                }
                if !queue_room_event(&outbound, &room, &participant_id, event).await {
                    let _ = enqueue(&outbound, close_message(4005, "slow_consumer"));
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if text.len() > state.limits.human_ws_message_limit {
                            let _ = enqueue(&outbound, close_message(1009, "message_too_large"));
                            break;
                        }
                        match handle_human_message(
                            &outbound,
                            &state,
                            &room,
                            &join_code,
                            &participant_id,
                            text.as_str(),
                        )
                        .await
                        {
                            HumanMessageOutcome::Continue => {}
                            HumanMessageOutcome::Close(reason) => {
                                let _ = enqueue(&outbound, close_message(4006, reason));
                                break;
                            }
                            HumanMessageOutcome::Invalid => {
                                let _ = enqueue(&outbound, close_message(1008, "invalid_message"));
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        if bytes.len() > state.limits.human_ws_message_limit {
                            let _ = enqueue(&outbound, close_message(1009, "message_too_large"));
                        } else {
                            let _ = enqueue(&outbound, close_message(1008, "invalid_message"));
                        }
                        break;
                    }
                    Some(Ok(Message::Pong(_))) => last_pong = Instant::now(),
                    Some(Ok(Message::Ping(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => {
                        let _ = enqueue(&outbound, close_message(1009, "message_too_large"));
                        break;
                    }
                }
            }
            _ = heartbeat.tick() => {
                if last_pong.elapsed() >= HUMAN_HEARTBEAT_TIMEOUT {
                    let _ = enqueue(&outbound, close_message(4006, "session_expired"));
                    break;
                }
                if !enqueue(&outbound, Message::Ping(Bytes::new())) {
                    let _ = enqueue(&outbound, close_message(4005, "slow_consumer"));
                    break;
                }
            }
        }
    }
    let operation = state.connections.operation_lock(&participant_id).await;
    if state.connections.is_current(&participant_id, generation) {
        let _ = room
            .send(RoomCommand::disconnect(participant_id.clone()))
            .await;
    }
    drop(operation);
    state.connections.remove(&participant_id, generation);
    drop(outbound);
    if time::timeout(Duration::from_secs(1), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
    }
    drop(permit);
    let _ = join_code;
}

fn close_message(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }))
}

fn enqueue(outbound: &mpsc::Sender<Message>, message: Message) -> bool {
    if !matches!(message, Message::Close(_)) && outbound.capacity() <= 1 {
        let _ = outbound.try_send(close_message(4005, "slow_consumer"));
        return false;
    }
    outbound.try_send(message).is_ok()
}

async fn send_snapshot(
    outbound: &mpsc::Sender<Message>,
    room: &RoomHandle,
    participant_id: &ParticipantId,
) -> bool {
    let Ok(snapshot) = room.snapshot().await else {
        return enqueue(outbound, close_message(4002, "room_deleted"));
    };
    let projection = match room.projection(participant_id.clone()).await {
        Ok(projection) => projection,
        Err(_) => return enqueue(outbound, close_message(4006, "session_expired")),
    };
    let value = json!({
        "type": "snapshot",
        "room": room_snapshot_value(&snapshot),
        "state": projection_value(projection),
    });
    enqueue(outbound, Message::text(value.to_string()))
}

async fn queue_room_event(
    outbound: &mpsc::Sender<Message>,
    room: &RoomHandle,
    participant_id: &ParticipantId,
    event: RoomEvent,
) -> bool {
    let Ok(snapshot) = room.snapshot().await else {
        return false;
    };
    if let RoomEvent::ParticipantLeft(left_id) = &event
        && left_id == participant_id
    {
        return enqueue(outbound, close_message(4006, "session_expired"));
    }
    let projection = match room.projection(participant_id.clone()).await {
        Ok(projection) => projection,
        Err(_) => return enqueue(outbound, close_message(4006, "session_expired")),
    };
    let viewer_seat = projection.as_ref().and_then(projection_viewer_seat);
    let value = match event {
        RoomEvent::ActionResolved { result, .. } => json!({
            "type": "game_update",
            "event": {
                "type": "action_resolved",
                "decision_id": decision_result_id(&result),
                "events": result.events().iter().map(|event| visible_game_event(event, viewer_seat)).collect::<Vec<_>>(),
            },
            "state": projection_value(projection),
        }),
        RoomEvent::DecisionOpened { decision, .. } => json!({
            "type": "game_update",
            "event": {"type": "decision_opened", "decision_id": decision.id().as_str()},
            "state": projection_value(projection),
        }),
        other => json!({
            "type": "room_update",
            "event": room_event_value(&other),
            "room": room_snapshot_value(&snapshot),
            "state": projection_value(projection),
        }),
    };
    enqueue(outbound, Message::text(value.to_string()))
}

fn decision_result_id(result: &double_riichi_core::DecisionResult) -> &str {
    match result {
        double_riichi_core::DecisionResult::Waiting { decision_id }
        | double_riichi_core::DecisionResult::Resolved { decision_id, .. } => decision_id.as_str(),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HumanMessageOutcome {
    Continue,
    Close(&'static str),
    Invalid,
}

async fn handle_human_message(
    outbound: &mpsc::Sender<Message>,
    state: &ServerState,
    room: &RoomHandle,
    join_code: &str,
    participant_id: &ParticipantId,
    text: &str,
) -> HumanMessageOutcome {
    let input = match serde_json::from_str::<HumanInput>(text) {
        Ok(input) => input,
        Err(_) => {
            let _ = enqueue(
                outbound,
                Message::text(json!({"type":"error","code":"invalid_message"}).to_string()),
            );
            return HumanMessageOutcome::Invalid;
        }
    };
    match input {
        HumanInput::SetReady {
            preloaded_characters,
        } => match room
            .send(RoomCommand::set_ready(
                participant_id.clone(),
                preloaded_characters,
            ))
            .await
        {
            Ok(_) => {
                let _ = send_snapshot(outbound, room, participant_id).await;
                HumanMessageOutcome::Continue
            }
            Err(_) => {
                let _ = enqueue(
                    outbound,
                    Message::text(json!({"type":"error","code":"not_ready"}).to_string()),
                );
                HumanMessageOutcome::Continue
            }
        },
        HumanInput::SubmitAction {
            decision_id,
            action_id,
        } => {
            let submitted_action_id = action_id.clone();
            match room
                .send(RoomCommand::submit_action(
                    participant_id.clone(),
                    decision_id.clone(),
                    action_id,
                ))
                .await
            {
                Ok(RoomResponse::Action(result)) => {
                    let response = json!({
                        "type": "action_result",
                        "decision_id": decision_result_id(&result),
                        "action_id": submitted_action_id,
                        "status": "accepted",
                    });
                    let _ = enqueue(outbound, Message::text(response.to_string()));
                    let _ = send_snapshot(outbound, room, participant_id).await;
                    HumanMessageOutcome::Continue
                }
                Err(error) => {
                    let code = action_error_code(&error);
                    let response = json!({
                        "type": "action_result",
                        "decision_id": decision_id,
                        "action_id": submitted_action_id,
                        "status": "rejected",
                        "code": code,
                    });
                    let _ = enqueue(outbound, Message::text(response.to_string()));
                    let _ = send_snapshot(outbound, room, participant_id).await;
                    HumanMessageOutcome::Continue
                }
                Ok(_) => HumanMessageOutcome::Invalid,
            }
        }
        HumanInput::Leave => {
            let operation = state.connections.operation_lock(participant_id).await;
            let result = room.send(RoomCommand::leave(participant_id.clone())).await;
            state
                .guest_sessions
                .invalidate_participant(join_code, participant_id);
            drop(operation);
            let _ = result;
            HumanMessageOutcome::Close("session_expired")
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum HumanInput {
    SetReady {
        preloaded_characters: Vec<String>,
    },
    SubmitAction {
        decision_id: String,
        action_id: String,
    },
    Leave,
}

fn action_error_code(error: &RoomError) -> &'static str {
    let value = error.to_string().to_ascii_lowercase();
    if value.contains("stale decision") || value.contains("decision is already closed") {
        "stale_decision"
    } else if value.contains("foreign action") || value.contains("illegal action") {
        "illegal_action"
    } else if value.contains("not eligible") {
        "not_eligible"
    } else if value.contains("disconnected") {
        "disconnected"
    } else {
        "action_rejected"
    }
}

fn room_event_value(event: &RoomEvent) -> Value {
    match event {
        RoomEvent::Snapshot(_) => json!({"type": "snapshot"}),
        RoomEvent::ParticipantJoined(participant) => json!({
            "type": "participant_joined",
            "participant_id": participant.id.as_str(),
        }),
        RoomEvent::ParticipantLeft(participant_id) => json!({
            "type": "participant_left",
            "participant_id": participant_id.as_str(),
        }),
        RoomEvent::SelectionChanged => json!({"type": "selection_changed"}),
        RoomEvent::PhaseChanged(phase) => {
            json!({"type": "phase_changed", "phase": phase_name(phase)})
        }
        RoomEvent::MatchStarted(match_id) => {
            json!({"type": "match_started", "match_id": match_id.as_str()})
        }
        RoomEvent::ActionResolved { .. } => json!({"type": "action_resolved"}),
        RoomEvent::MatchCompleted { .. } => json!({"type": "match_completed"}),
        RoomEvent::MatchAborted { .. } => json!({"type": "match_aborted"}),
        RoomEvent::StorageDegraded => json!({"type": "storage_degraded"}),
        RoomEvent::RoomDeleted => json!({"type": "room_deleted"}),
        RoomEvent::ServerShutdown => json!({"type": "server_shutdown"}),
        RoomEvent::DecisionOpened { .. } => json!({"type": "decision_opened"}),
    }
}

fn projection_viewer_seat(projection: &AudienceProjection) -> Option<Seat> {
    match projection {
        AudienceProjection::Player(player) => Some(player.viewer_seat),
        AudienceProjection::Public(_) | AudienceProjection::ReplayAdmin(_) => None,
    }
}

fn visible_game_event(event: &GameEvent, viewer_seat: Option<Seat>) -> Value {
    let mut value = serde_json::to_value(event).unwrap_or(Value::Null);
    normalize_protocol_value(&mut value);
    if let Value::Object(object) = &mut value {
        if let Some(Value::Object(start)) = object.get_mut("start_kyoku")
            && let Some(tehais) = start.get_mut("tehais")
        {
            if let Some(seat) = viewer_seat {
                if let Value::Array(all) = tehais {
                    let own = all
                        .get(seat.index() as usize)
                        .cloned()
                        .unwrap_or(Value::Null);
                    *tehais = json!([own]);
                }
            } else {
                start.remove("tehais");
            }
        }
        if let Some(Value::Object(tsumo)) = object.get_mut("tsumo") {
            let actor = tsumo
                .get("actor")
                .and_then(Value::as_u64)
                .map(|value| value as u8);
            if viewer_seat.map(|seat| seat.index()) != actor {
                tsumo.remove("tile");
            }
        }
        if let Some(Value::Object(ankan)) = object.get_mut("ankan") {
            let actor = ankan
                .get("actor")
                .and_then(Value::as_u64)
                .map(|value| value as u8);
            if viewer_seat.map(|seat| seat.index()) != actor {
                ankan.remove("consumed");
            }
        }
        if let Some(Value::Object(ryukyoku)) = object.get_mut("ryukyoku") {
            ryukyoku.remove("tehais");
        }
    }
    value
}

fn projection_value(projection: Option<AudienceProjection>) -> Value {
    let Some(projection) = projection else {
        return Value::Null;
    };
    let mut value = serde_json::to_value(projection).unwrap_or(Value::Null);
    normalize_protocol_value(&mut value);
    value
}

fn normalize_protocol_value(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                let Some(mut value) = object.remove(&key) else {
                    continue;
                };
                let key = protocol_key(&key).to_owned();
                match key.as_str() {
                    "mode" => {
                        if let Some(mode) = value.as_str().and_then(protocol_mode_name) {
                            value = Value::String(mode.to_owned());
                        }
                    }
                    "kind" => {
                        if let Some(kind) = value.as_str().and_then(|kind| {
                            protocol_kind_name(kind).or_else(|| protocol_decision_kind_name(kind))
                        }) {
                            value = Value::String(kind.to_owned());
                        }
                    }
                    "controller" | "presence" | "role" | "time_control" | "permanent_auto" => {
                        if let Value::String(name) = &value
                            && let Some(name) = protocol_value_name(name)
                        {
                            value = Value::String(name.to_owned());
                        }
                    }
                    _ => {}
                }
                normalize_protocol_value(&mut value);
                object.insert(key, value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(normalize_protocol_value),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn protocol_value_name(value: &str) -> Option<&'static str> {
    Some(match value {
        "Human" => "human",
        "MJAI" => "mjai",
        "MCP" => "mcp",
        "BuiltInBot" => "built_in_bot",
        "Interactive" => "interactive",
        "TemporaryAuto" => "temporary_auto",
        "PermanentAuto" => "permanent_auto",
        "LeftDuringMatch" => "left_during_match",
        "AgentLeft" => "agent_left",
        "TokenRevoked" => "token_revoked",
        "ConnectionLost" => "connection_lost",
        "Disconnect" => "disconnect",
        "Connected" => "connected",
        "Disconnected" => "disconnected",
        "None" => "none",
        "Player" => "player",
        "Spectator" => "spectator",
        "Turn" => "turn",
        "Response" => "response",
        "RiichiDev" => "riichi_dev",
        "Casual" => "casual",
        "East" => "east",
        "South" => "south",
        "West" => "west",
        "North" => "north",
        _ => return None,
    })
}

fn protocol_key(value: &str) -> &str {
    match value {
        "Player" => "player",
        "Public" => "public",
        "ReplayAdmin" => "replay_admin",
        "Interactive" => "interactive",
        "TemporaryAuto" => "temporary_auto",
        "PermanentAuto" => "permanent_auto",
        "LeftDuringMatch" => "left_during_match",
        "AgentLeft" => "agent_left",
        "TokenRevoked" => "token_revoked",
        "ConnectionLost" => "connection_lost",
        "StartGame" => "start_game",
        "StartKyoku" => "start_kyoku",
        "Tsumo" => "tsumo",
        "Dahai" => "dahai",
        "Pon" => "pon",
        "Chi" => "chi",
        "Daiminkan" => "daiminkan",
        "Kakan" => "kakan",
        "Ankan" => "ankan",
        "Dora" => "dora",
        "Reach" => "reach",
        "ReachAccepted" => "reach_accepted",
        "Hora" => "hora",
        "Ryukyoku" => "ryukyoku",
        "Kita" => "kita",
        "EndKyoku" => "end_kyoku",
        "EndGame" => "end_game",
        "Discard" => "discard",
        "RiichiDiscard" => "riichi_discard",
        "AbortiveDraw" => "abortive_draw",
        "BuiltInBot" => "built_in_bot",
        other => other,
    }
}

fn protocol_decision_kind_name(value: &str) -> Option<&'static str> {
    Some(match value {
        "Turn" => "turn",
        "Response" => "response",
        _ => return None,
    })
}

fn protocol_kind_name(value: &str) -> Option<&'static str> {
    Some(match value {
        "Human" => "human",
        "MJAI" => "mjai",
        "MCP" => "mcp",
        "BuiltInBot" => "built_in_bot",
        _ => return None,
    })
}

fn protocol_mode_name(value: &str) -> Option<&'static str> {
    Some(match value {
        "FourPlayerRedEast" => "4p-red-east",
        "FourPlayerRedHalf" => "4p-red-half",
        "ThreePlayerRedEast" => "3p-red-east",
        "ThreePlayerRedHalf" => "3p-red-half",
        _ => return None,
    })
}

#[derive(Serialize)]
struct RoomListView {
    join_code: String,
    room_name: String,
    game_mode: String,
    phase: String,
    connected_count: usize,
    participant_count: usize,
    selected_count: usize,
    created_at: String,
}

#[derive(Serialize)]
struct ParticipantView {
    participant_id: String,
    display_name: String,
    kind: String,
    presence: String,
    selected: bool,
    ready: bool,
    character_id: String,
    role: String,
    controller: String,
}

#[derive(Serialize)]
struct MatchPlayerView {
    participant_id: String,
    display_name: String,
    kind: String,
    seat: u8,
    character_id: Option<String>,
    controller: String,
}

#[derive(Serialize)]
struct RoomDetailView {
    #[serde(flatten)]
    summary: RoomListView,
    time_control: String,
    replay_save: bool,
    participant_limit: usize,
    participants: Vec<ParticipantView>,
    match_players: Vec<MatchPlayerView>,
    roster: Vec<MatchPlayerView>,
    result: Option<Value>,
    revision: u64,
    persistence_degraded: bool,
    replay_available: bool,
}

fn room_list_view(snapshot: &RoomSnapshot) -> RoomListView {
    RoomListView {
        join_code: snapshot.join_code.as_str().to_owned(),
        room_name: snapshot.room_name.clone(),
        game_mode: snapshot.mode.as_str().to_owned(),
        phase: phase_name(&snapshot.phase).to_owned(),
        connected_count: snapshot
            .participants
            .iter()
            .filter(|participant| participant.presence == Presence::Connected)
            .count(),
        participant_count: snapshot.participants.len(),
        selected_count: snapshot
            .participants
            .iter()
            .filter(|participant| participant.selected)
            .count(),
        created_at: unix_seconds_rfc3339(snapshot.created_at),
    }
}

fn room_detail_view(snapshot: &RoomSnapshot) -> RoomDetailView {
    RoomDetailView {
        summary: room_list_view(snapshot),
        time_control: match snapshot.time_control {
            TimeControl::Casual => "casual",
            TimeControl::RiichiDev => "riichi_dev",
            TimeControl::Unlimited => "unlimited",
        }
        .to_owned(),
        replay_save: snapshot.replay_save,
        participant_limit: snapshot.participant_limit,
        participants: snapshot.participants.iter().map(participant_view).collect(),
        match_players: snapshot
            .match_players
            .iter()
            .map(match_player_view)
            .collect(),
        roster: snapshot.roster.iter().map(match_player_view).collect(),
        result: snapshot
            .result
            .as_ref()
            .and_then(|result| serde_json::to_value(result).ok()),
        revision: snapshot.revision,
        persistence_degraded: snapshot.persistence_degraded,
        replay_available: snapshot.replay_available,
    }
}

fn participant_view(participant: &double_riichi_core::RoomParticipantSnapshot) -> ParticipantView {
    ParticipantView {
        participant_id: participant.id.as_str().to_owned(),
        display_name: participant.display_name.clone(),
        kind: participant_kind(participant.kind).to_owned(),
        presence: match participant.presence {
            Presence::Connected => "connected",
            Presence::Disconnected => "disconnected",
        }
        .to_owned(),
        selected: participant.selected,
        ready: participant.ready,
        character_id: participant.character_id.clone(),
        role: match participant.role {
            MatchRole::None => "none".to_owned(),
            MatchRole::Player(seat) => format!("player_{}", seat.index()),
            MatchRole::Spectator => "spectator".to_owned(),
        },
        controller: controller_name(participant.controller),
    }
}

fn match_player_view(player: &MatchPlayerSnapshot) -> MatchPlayerView {
    MatchPlayerView {
        participant_id: player.participant_id.as_str().to_owned(),
        display_name: player.display_name.clone(),
        kind: participant_kind(player.kind).to_owned(),
        seat: player.seat.index(),
        character_id: player.character_id.clone(),
        controller: controller_name(player.controller),
    }
}

fn participant_kind(kind: ParticipantKind) -> &'static str {
    match kind {
        ParticipantKind::Human => "human",
        ParticipantKind::MJAI => "mjai",
        ParticipantKind::MCP => "mcp",
        ParticipantKind::BuiltInBot => "built_in_bot",
    }
}

fn controller_name(controller: RoomController) -> String {
    match controller {
        RoomController::Interactive => "interactive".to_owned(),
        RoomController::TemporaryAuto => "temporary_auto".to_owned(),
        RoomController::PermanentAuto(reason) => {
            format!(
                "permanent_auto_{}",
                format!("{reason:?}").to_ascii_lowercase()
            )
        }
    }
}

fn room_detail_response(status: StatusCode, snapshot: &RoomSnapshot) -> Response {
    json_response(
        status,
        serde_json::to_value(room_detail_view(snapshot)).unwrap_or(Value::Null),
    )
}

fn room_snapshot_value(snapshot: &RoomSnapshot) -> Value {
    json!({
        "join_code": snapshot.join_code.as_str(),
        "room_name": snapshot.room_name,
        "game_mode": snapshot.mode.as_str(),
        "phase": phase_name(&snapshot.phase),
        "revision": snapshot.revision,
        "participants": snapshot.participants.iter().map(participant_view).collect::<Vec<_>>(),
        "match_players": snapshot.match_players.iter().map(match_player_view).collect::<Vec<_>>(),
        "roster": snapshot.roster.iter().map(match_player_view).collect::<Vec<_>>(),
        "result": snapshot.result.clone(),
    })
}

fn phase_name(phase: &RoomPhase) -> &'static str {
    match phase {
        RoomPhase::Lobby => "lobby",
        RoomPhase::Playing(_) => "playing",
        RoomPhase::PostMatch(_) => "post_match",
    }
}

pub(crate) fn invalid_request(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "Invalid request",
        "The request body is invalid.",
        "invalid_request",
    )
    .response(request_id)
}

pub(crate) fn internal_error(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal server error",
        "The server could not complete the request.",
        "internal_error",
    )
    .response(request_id)
}

pub(crate) fn room_not_found(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "Room not found",
        "The room does not exist.",
        "room_not_found",
    )
    .response(request_id)
}

pub(crate) fn invalid_credentials(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "Unauthorized",
        "The supplied credentials are invalid.",
        "invalid_credentials",
    )
    .response(request_id)
}

pub(crate) fn origin_not_allowed(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "Origin not allowed",
        "The request origin is not allowed.",
        "origin_not_allowed",
    )
    .response(request_id)
}

pub(crate) fn server_busy(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Server busy",
        "The server cannot accept another connection.",
        "server_busy",
    )
    .response(request_id)
}

pub(crate) fn already_connected(request_id: &RequestId) -> Response {
    ApiError::new(
        StatusCode::CONFLICT,
        "Already connected",
        "This bot is already queued or playing.",
        "already_connected",
    )
    .response(request_id)
}

pub(crate) fn rate_limited(request_id: &RequestId, window: Duration) -> Response {
    let mut response = ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "Too many requests",
        "The request rate limit has been exceeded.",
        "rate_limited",
    )
    .response(request_id);
    response.headers_mut().insert(
        header::RETRY_AFTER,
        HeaderValue::from_str(&window.as_secs().to_string()).expect("retry value is valid"),
    );
    response
}

fn registry_error_response(error: RoomRegistryError, request_id: &RequestId) -> Response {
    match error {
        RoomRegistryError::NotFound => room_not_found(request_id),
        RoomRegistryError::Full => ApiError::new(
            StatusCode::CONFLICT,
            "Room capacity reached",
            "The server cannot create another room.",
            "room_capacity_reached",
        )
        .response(request_id),
        RoomRegistryError::CodeUnavailable => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Room code unavailable",
            "The server could not allocate a room code.",
            "room_code_unavailable",
        )
        .response(request_id),
        RoomRegistryError::Room(error) => room_error_response(error, request_id),
    }
}

pub(crate) fn room_error_response(error: RoomError, request_id: &RequestId) -> Response {
    let (status, title, detail, code) = match error {
        RoomError::Busy => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Room busy",
            "The room is temporarily busy.",
            "room_busy",
        ),
        RoomError::RoomFull => (
            StatusCode::CONFLICT,
            "Room full",
            "The room cannot accept another participant.",
            "room_full",
        ),
        RoomError::NotLobby => (
            StatusCode::CONFLICT,
            "Room is not in the Lobby",
            "That command is unavailable in the current Room phase.",
            "not_lobby",
        ),
        RoomError::NotEnoughPlayers | RoomError::NotReady => (
            StatusCode::CONFLICT,
            "Room is not ready",
            "The Room does not have an eligible ready roster.",
            "room_not_ready",
        ),
        RoomError::NotSelected => (
            StatusCode::CONFLICT,
            "Participant is not selected",
            "The Participant is not selected for this Match.",
            "not_selected",
        ),
        RoomError::ParticipantNotFound(_) => (
            StatusCode::NOT_FOUND,
            "Participant not found",
            "The Participant does not exist in this Room.",
            "participant_not_found",
        ),
        RoomError::InvalidCharacter | RoomError::PreloadIncomplete => (
            StatusCode::BAD_REQUEST,
            "Invalid Character selection",
            "The Character selection or preload is invalid.",
            "invalid_character",
        ),
        RoomError::AlreadySelected => (
            StatusCode::CONFLICT,
            "Participant already selected",
            "The Participant is already selected.",
            "already_selected",
        ),
        RoomError::Disconnected => (
            StatusCode::CONFLICT,
            "Participant disconnected",
            "The Participant is disconnected.",
            "participant_disconnected",
        ),
        RoomError::DeleteWhilePlaying => (
            StatusCode::CONFLICT,
            "Room is playing",
            "The Room cannot be deleted while a Match is active.",
            "delete_while_playing",
        ),
        RoomError::Persistence => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Replay persistence unavailable",
            "The Room could not persist the Match start or replay.",
            "persistence_failure",
        ),
        RoomError::Deleted | RoomError::Closed => (
            StatusCode::NOT_FOUND,
            "Room not found",
            "The room does not exist.",
            "room_not_found",
        ),
        _ => (
            StatusCode::CONFLICT,
            "Room command rejected",
            "The Room command is not available.",
            "room_command_rejected",
        ),
    };
    ApiError::new(status, title, detail, code).response(request_id)
}

async fn health(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    let room_handles = state.rooms.list().await;
    let rooms = room_handles.len();
    let mut active_matches = 0usize;
    for handle in room_handles {
        if handle
            .snapshot()
            .await
            .ok()
            .is_some_and(|snapshot| matches!(snapshot.phase, RoomPhase::Playing(_)))
        {
            active_matches += 1;
        }
    }
    let uptime = state.started_at.elapsed().as_secs();
    let active_compat_matches = state.compat.active_count().await;
    let (database, replay_storage) = match &state.storage {
        Some(storage) => {
            let database_ok = time::timeout(Duration::from_secs(1), storage.scalar_i64("SELECT 1"))
                .await
                .is_ok_and(|result| result.is_ok());
            let replay_ok = !state.compat.replay_degraded()
                && !storage.replay_degraded()
                && state.replay_probe_ok.load(Ordering::Acquire)
                && storage.replay_root().is_dir();
            (
                if database_ok { "ok" } else { "degraded" },
                if replay_ok { "ok" } else { "degraded" },
            )
        }
        None => ("not_configured", "not_configured"),
    };
    let healthy = database == "ok" && replay_storage == "ok";
    json_response(
        if healthy {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        json!({
            "version": crate::BUILD_VERSION,
            "commit": crate::BUILD_COMMIT,
            "uptime_seconds": uptime,
            "database": database,
            "replay_storage": replay_storage,
            "active_rooms": rooms,
            "active_room_matches": active_matches,
            "active_compat_matches": active_compat_matches,
        }),
    )
}

async fn openapi_document(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if !state.api_docs_enabled {
        return ApiError::new(
            StatusCode::NOT_FOUND,
            "Not found",
            "The requested resource does not exist.",
            "not_found",
        )
        .response(&request_id);
    }
    if let Err(response) = require_admin(&state, &headers, &request_id) {
        return response;
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/yaml")
        .body(Body::from(OPENAPI_YAML))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

async fn human_characters(State(state): State<Arc<ServerState>>) -> Response {
    let Some(registry) = &state.registry else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    };
    json_response(
        StatusCode::OK,
        serde_json::to_value(registry.human_characters()).unwrap_or(Value::Null),
    )
}

async fn character_portrait(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    character_asset(&state, &id, CharacterAsset::Portrait, &headers)
}

async fn character_icon(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    character_asset(&state, &id, CharacterAsset::Icon, &headers)
}

async fn character_voice(
    State(state): State<Arc<ServerState>>,
    Path((id, voice)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some(voice) = voice.strip_suffix(".ogg").and_then(|voice| match voice {
        "chi" => Some(crate::VoiceLine::Chi),
        "pon" => Some(crate::VoiceLine::Pon),
        "kan" => Some(crate::VoiceLine::Kan),
        "riichi" => Some(crate::VoiceLine::Riichi),
        "ron" => Some(crate::VoiceLine::Ron),
        "tsumo" => Some(crate::VoiceLine::Tsumo),
        _ => None,
    }) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    };
    character_asset(&state, &id, CharacterAsset::Voice(voice), &headers)
}

fn character_asset(
    state: &ServerState,
    id: &str,
    asset: CharacterAsset,
    headers: &HeaderMap,
) -> Response {
    let Some(registry) = &state.registry else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    };
    let Some(asset) = registry.asset(id, asset) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    };
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == asset.etag());
    let mut response = Response::builder()
        .status(if not_modified {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, asset.content_type())
        .header(header::CACHE_CONTROL, "public, no-cache")
        .header(header::ETAG, asset.etag());
    if !not_modified {
        response = response.header(header::CONTENT_LENGTH, asset.bytes().len());
    }
    response
        .body(if not_modified {
            Body::empty()
        } else {
            Body::from(asset.bytes().to_vec())
        })
        .unwrap()
}

pub(crate) fn normalize_display_text(value: &str) -> Option<String> {
    let value = value.trim_matches(char::is_whitespace);
    if !(1..=64).contains(&value.chars().count()) || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.to_owned())
}

fn unix_seconds_rfc3339(seconds: i64) -> String {
    system_time_rfc3339(std::time::UNIX_EPOCH + Duration::from_secs(seconds.max(0) as u64))
}

fn system_time_rfc3339(time: SystemTime) -> String {
    let seconds = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        day_seconds / 3_600,
        (day_seconds / 60) % 60,
        day_seconds % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u8, day as u8)
}

fn request_ip(
    headers: &HeaderMap,
    direct_peer: Option<SocketAddr>,
    trusted_proxies: &[IpCidr],
) -> IpAddr {
    let direct = direct_peer
        .map(|peer| peer.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    if !trusted_proxies.iter().any(|cidr| cidr.contains(direct)) {
        return direct;
    }
    let Some(header) = headers.get("x-forwarded-for") else {
        return direct;
    };
    let Ok(header) = header.to_str() else {
        return direct;
    };
    let mut addresses = Vec::new();
    for value in header.split(',') {
        let Ok(address) = value.trim().parse::<IpAddr>() else {
            return direct;
        };
        addresses.push(address);
    }
    if addresses.is_empty() {
        return direct;
    }
    addresses
        .iter()
        .rev()
        .copied()
        .find(|address| !trusted_proxies.iter().any(|cidr| cidr.contains(*address)))
        .unwrap_or(addresses[0])
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum RateKind {
    CodeLookup,
    ParticipantCreation,
    AdminLoginFailure,
    AgentAuthFailure,
}

struct RateBucket {
    window: Duration,
    hits: VecDeque<Instant>,
}

#[derive(Default)]
struct RateLimiter {
    entries: Mutex<HashMap<(RateKind, IpAddr), RateBucket>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self::default()
    }

    fn available(&self, kind: RateKind, ip: IpAddr, limit: usize, _window: Duration) -> bool {
        let mut entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        prune_rate_entries(&mut entries, now);
        entries
            .get(&(kind, ip))
            .is_none_or(|entry| entry.hits.len() < limit)
    }

    fn allowed(&self, kind: RateKind, ip: IpAddr, limit: usize, window: Duration) -> bool {
        let mut entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        prune_rate_entries(&mut entries, now);
        let entry = entries.entry((kind, ip)).or_insert_with(|| RateBucket {
            window,
            hits: VecDeque::new(),
        });
        if entry.hits.len() >= limit {
            return false;
        }
        entry.hits.push_back(now);
        enforce_rate_bound(&mut entries);
        true
    }

    fn record(&self, kind: RateKind, ip: IpAddr, window: Duration) {
        let mut entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        prune_rate_entries(&mut entries, now);
        entries
            .entry((kind, ip))
            .or_insert_with(|| RateBucket {
                window,
                hits: VecDeque::new(),
            })
            .hits
            .push_back(now);
        enforce_rate_bound(&mut entries);
    }
}

fn prune_rate_entries(entries: &mut HashMap<(RateKind, IpAddr), RateBucket>, now: Instant) {
    for bucket in entries.values_mut() {
        while bucket
            .hits
            .front()
            .is_some_and(|started| now.duration_since(*started) >= bucket.window)
        {
            bucket.hits.pop_front();
        }
    }
    entries.retain(|_, bucket| !bucket.hits.is_empty());
    enforce_rate_bound(entries);
}

fn enforce_rate_bound(entries: &mut HashMap<(RateKind, IpAddr), RateBucket>) {
    while entries.len() > 8_192 {
        let Some(key) = entries.keys().next().copied() else {
            break;
        };
        entries.remove(&key);
    }
}

#[derive(Default)]
struct GuestSessionStore {
    sessions: Mutex<HashMap<[u8; 32], GuestSession>>,
}

struct GuestSession {
    join_code: String,
    participant_id: ParticipantId,
    issued_at: Instant,
}

impl GuestSessionStore {
    fn issue(&self, join_code: &str, participant_id: &ParticipantId) -> String {
        let bytes: [u8; 32] = random();
        let value = URL_SAFE_NO_PAD.encode(bytes);
        let mut sessions = self.sessions.lock().expect("guest session lock poisoned");
        let now = Instant::now();
        prune_guest_sessions(&mut sessions, now);
        while sessions.len() >= GUEST_SESSION_MAX_ENTRIES {
            let Some(key) = sessions
                .iter()
                .min_by_key(|(_, session)| session.issued_at)
                .map(|(key, _)| *key)
            else {
                break;
            };
            sessions.remove(&key);
        }
        sessions.insert(
            crate::hash_token(&value),
            GuestSession {
                join_code: join_code.to_owned(),
                participant_id: participant_id.clone(),
                issued_at: now,
            },
        );
        value
    }

    fn authenticate(&self, join_code: &str, value: &str) -> Option<ParticipantId> {
        let mut sessions = self.sessions.lock().expect("guest session lock poisoned");
        prune_guest_sessions(&mut sessions, Instant::now());
        sessions
            .get(&crate::hash_token(value))
            .filter(|session| session.join_code == join_code)
            .map(|session| session.participant_id.clone())
    }

    fn invalidate_room(&self, join_code: &str) {
        self.sessions
            .lock()
            .expect("guest session lock poisoned")
            .retain(|_, session| session.join_code != join_code);
    }

    fn prune(&self) {
        let mut sessions = self.sessions.lock().expect("guest session lock poisoned");
        prune_guest_sessions(&mut sessions, Instant::now());
    }

    async fn prune_for_rooms(&self, rooms: &RoomRegistry) {
        self.prune();
        let candidates: Vec<_> = self
            .sessions
            .lock()
            .expect("guest session lock poisoned")
            .iter()
            .map(|(key, session)| {
                (
                    *key,
                    session.join_code.clone(),
                    session.participant_id.clone(),
                )
            })
            .collect();
        let mut invalid = Vec::new();
        for (key, join_code, participant_id) in candidates {
            let valid = if let Some(handle) = rooms.get(&join_code).await {
                match handle.snapshot().await {
                    Ok(snapshot) => snapshot
                        .participants
                        .iter()
                        .any(|participant| participant.id == participant_id),
                    Err(RoomError::Busy) => true,
                    Err(RoomError::Closed | RoomError::Deleted) => false,
                    Err(_) => true,
                }
            } else {
                false
            };
            if !valid {
                invalid.push(key);
            }
        }
        let mut sessions = self.sessions.lock().expect("guest session lock poisoned");
        for key in invalid {
            sessions.remove(&key);
        }
    }

    fn invalidate_participant(&self, join_code: &str, participant_id: &ParticipantId) {
        self.sessions
            .lock()
            .expect("guest session lock poisoned")
            .retain(|_, session| {
                session.join_code != join_code || &session.participant_id != participant_id
            });
    }
}

fn prune_guest_sessions(sessions: &mut HashMap<[u8; 32], GuestSession>, now: Instant) {
    sessions.retain(|_, session| now.duration_since(session.issued_at) < GUEST_SESSION_LIFETIME);
    while sessions.len() > GUEST_SESSION_MAX_ENTRIES {
        let Some(key) = sessions
            .iter()
            .min_by_key(|(_, session)| session.issued_at)
            .map(|(key, _)| *key)
        else {
            break;
        };
        sessions.remove(&key);
    }
}

fn _unused_types(_: &GameAction, _: &RoomJoinCode) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_pruning_keeps_each_kind_window() {
        let limiter = RateLimiter::new();
        let ip = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
        let now = Instant::now();
        limiter.entries.lock().unwrap().insert(
            (RateKind::AdminLoginFailure, ip),
            RateBucket {
                window: Duration::from_secs(15 * 60),
                hits: VecDeque::from([now - Duration::from_secs(70)]),
            },
        );
        assert!(limiter.available(RateKind::CodeLookup, ip, 1, Duration::from_secs(60)));
        assert_eq!(
            limiter
                .entries
                .lock()
                .unwrap()
                .get(&(RateKind::AdminLoginFailure, ip))
                .unwrap()
                .hits
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn participant_operation_locks_are_reclaimed() {
        let connections = Arc::new(HumanConnections::default());
        let participant = ParticipantId::new("participant");
        {
            let _guard = connections.operation_lock(&participant).await;
            assert_eq!(connections.participant_locks.lock().unwrap().len(), 1);
        }
        assert!(connections.participant_locks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn guest_sessions_drop_participants_removed_by_room_cleanup() {
        let sessions = GuestSessionStore::default();
        let participant = ParticipantId::new("participant");
        let credential = sessions.issue("MISSING", &participant);
        let rooms = RoomRegistry::with_max_rooms(1);
        sessions.prune_for_rooms(&rooms).await;
        assert!(sessions.authenticate("MISSING", &credential).is_none());
    }

    #[test]
    fn request_log_ids_bound_untrusted_path_values() {
        let participant_id = "A".repeat(512);
        let path = format!("/api/v1/admin/rooms/123456/participants/{participant_id}/kick");
        let ids = request_log_ids(
            "/api/v1/admin/rooms/{join_code}/participants/{participant_id}/kick",
            &path,
        );
        assert_eq!(ids.room_id.as_deref(), Some("123456"));
        assert_eq!(ids.participant_id.unwrap().len(), 128);
    }

    #[test]
    fn protocol_normalization_snake_cases_enum_values() {
        let mut value = json!({"controller":{"PermanentAuto":"ConnectionLost"},"role":{"Player":2},"kind":"Turn","display_name":"East"});
        normalize_protocol_value(&mut value);
        assert_eq!(value["controller"]["permanent_auto"], "connection_lost");
        assert_eq!(value["role"]["player"], 2);
        assert_eq!(value["kind"], "turn");
        assert_eq!(value["display_name"], "East");
    }

    #[test]
    fn outbound_queue_has_a_hard_capacity_and_slow_close() {
        let (sender, mut receiver) = mpsc::channel(HUMAN_OUTBOUND_CAPACITY + 1);
        for _ in 0..HUMAN_OUTBOUND_CAPACITY {
            assert!(enqueue(&sender, Message::text("x")));
        }
        assert!(!enqueue(&sender, Message::text("overflow")));
        for _ in 0..HUMAN_OUTBOUND_CAPACITY {
            assert!(matches!(receiver.try_recv(), Ok(Message::Text(_))));
        }
        match receiver.try_recv().unwrap() {
            Message::Close(Some(frame)) => {
                assert_eq!(frame.code, 4005);
                assert_eq!(frame.reason, "slow_consumer");
            }
            other => panic!("expected slow-consumer close, got {other:?}"),
        }
    }

    #[test]
    fn audit_failure_target_ids_redact_raw_tokens_but_keep_safe_ids() {
        assert_eq!(redact_audit_target_id("room", "ROOM-123"), "ROOM-123");
        assert_eq!(
            redact_audit_target_id(
                "bot_token",
                "driichi_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            ),
            "[REDACTED]"
        );
        assert_eq!(redact_audit_target_id("replay", "MATCH15"), "MATCH15");
        assert_eq!(
            redact_audit_target_id(
                "replay",
                "replay-driichi_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            ),
            "[REDACTED]"
        );
    }

    #[tokio::test]
    async fn revalidation_rejects_a_credential_revoked_while_waiting_for_mutation_lock() {
        let admin = Arc::new(
            AdminAuthenticator::new(
                "admin",
                crate::hash_password("correct horse battery staple").unwrap(),
            )
            .unwrap(),
        );
        let state = ServerState::for_tests(
            "http://127.0.0.1:3000",
            admin.clone(),
            RoomRegistry::with_max_rooms(1),
        );
        let session = admin
            .login("admin", "correct horse battery staple", SystemTime::now())
            .unwrap();
        let credential = session.credential().as_bytes().to_vec();
        let _lock = state.admin_mutation_lock.lock().await;
        admin.sessions().revoke(&credential);
        assert!(revalidate_admin(&state, &credential, &RequestId("REQ".into())).is_err());
    }

    #[tokio::test]
    async fn command_change_detection_ignores_unrelated_public_join() {
        let rooms = RoomRegistry::with_max_rooms(1);
        let handle = rooms
            .create(RoomConfig::new(
                "Race",
                GameMode::FourPlayerRedEast,
                CharacterCatalog::starter(),
            ))
            .await
            .unwrap();
        let before = handle.snapshot().await.unwrap();
        handle
            .send(RoomCommand::join(Participant::new(
                "public-join",
                "Public",
                ParticipantKind::Human,
            )))
            .await
            .unwrap();
        let after = handle.snapshot().await.unwrap();
        assert!(!room_command_changed("fill_with_bots", &before, &after));
        let request = PatchRoomRequest {
            room_name: Some(before.room_name.clone()),
            game_mode: None,
            time_control: None,
            replay_save: None,
            participant_limit: None,
        };
        assert!(!room_configuration_changed(&before, &after, &request));
    }

    #[test]
    fn trusted_proxy_ip_uses_first_untrusted_forwarded_address() {
        let proxies = vec![
            IpCidr::new("10.0.0.0".parse().unwrap(), 8).unwrap(),
            IpCidr::new("192.168.0.0".parse().unwrap(), 16).unwrap(),
        ];
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.7, 10.1.2.3".parse().unwrap());
        assert_eq!(
            request_ip(&headers, Some("10.1.2.3:3000".parse().unwrap()), &proxies,),
            "198.51.100.7".parse::<IpAddr>().unwrap()
        );
    }

    #[tokio::test]
    async fn oauth_metadata_routes_publish_matching_issuer_and_resource() {
        let issuer = Url::parse("https://driichi.example/").unwrap();
        let mut resource = issuer.clone();
        resource.set_path("/chatgpt/mcp");
        let oauth = crate::config::ChatgptOAuthConfig {
            issuer,
            resource,
            client_id: Url::parse("https://chatgpt.com/oauth/client.json").unwrap(),
            redirect_uri: Url::parse(
                "https://chatgpt.com/connector_platform_oauth_redirect",
            )
            .unwrap(),
            allowed_origins: vec![Url::parse("https://chatgpt.com/").unwrap()],
        };
        let admin = Arc::new(
            AdminAuthenticator::new(
                "admin",
                crate::hash_password("a sufficiently long test password").unwrap(),
            )
            .unwrap(),
        );
        let mut state = ServerState::for_tests(
            "https://driichi.example",
            admin,
            RoomRegistry::new(),
        );
        state.chatgpt_oauth = Some(Arc::new(
            crate::oauth::OAuthGatewayState::new(oauth).unwrap(),
        ));
        let app = server_router(Arc::new(state));

        use tower::ServiceExt;
        let resource_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/.well-known/oauth-protected-resource/chatgpt/mcp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resource_response.status(), StatusCode::OK);
        let resource_body = axum::body::to_bytes(resource_response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let resource_metadata: Value = serde_json::from_slice(&resource_body).unwrap();
        assert_eq!(
            resource_metadata["resource"],
            "https://driichi.example/chatgpt/mcp"
        );
        assert_eq!(
            resource_metadata["authorization_servers"][0],
            "https://driichi.example"
        );

        let authorization_response = app
            .oneshot(
                Request::builder()
                    .uri("/.well-known/oauth-authorization-server")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorization_response.status(), StatusCode::OK);
        let authorization_body =
            axum::body::to_bytes(authorization_response.into_body(), 64 * 1024)
                .await
                .unwrap();
        let authorization_metadata: Value = serde_json::from_slice(&authorization_body).unwrap();
        assert_eq!(
            authorization_metadata["issuer"],
            resource_metadata["authorization_servers"][0]
        );
        assert_eq!(
            authorization_metadata["code_challenge_methods_supported"],
            json!(["S256"])
        );
        assert_eq!(
            authorization_metadata["token_endpoint_auth_methods_supported"],
            json!(["none"])
        );
        assert_eq!(
            authorization_metadata["client_id_metadata_document_supported"],
            json!(true)
        );
    }

}
