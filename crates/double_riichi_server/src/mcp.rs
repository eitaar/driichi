use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::Response,
};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        tool::Extension,
        wrapper::{Json, Parameters},
    },
    model::{
        CallToolResult, ErrorCode, ListResourceTemplatesResult, ListResourcesResult,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig,
        SubscribeRequestParams,
    },
    schemars::JsonSchema,
    service::{RequestContext, RoleServer, SubscriptionContext},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::SessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    sync::{Notify, OwnedSemaphorePermit, Semaphore},
    time,
};
use tokio_util::sync::CancellationToken;

use double_riichi_core::{
    DecisionId, Participant, ParticipantId, ParticipantKind, RoomCommand, RoomError, RoomEvent,
    RoomHandle, RoomPhase, RoomResponse,
};

use crate::http::ServerState;

pub(crate) const MCP_MAX_BODY_BYTES: usize = 1024 * 1024;
pub(crate) const MCP_MAX_WAIT_SECONDS: u64 = 330;
const MCP_DEFAULT_IDLE: Duration = Duration::from_secs(30 * 60);
const MCP_MAX_SESSIONS: usize = 1_024;
const MCP_MAX_WAIT_TASKS: usize = 1_024;
const MCP_MAX_SUBSCRIPTIONS: usize = 4_096;

const STATE_TEMPLATE: &str = "riichi://rooms/{code}/participants/{participant_id}/state";
const PUBLIC_STATE_TEMPLATE: &str = "riichi://rooms/{code}/public-state";
const HISTORY_TEMPLATE: &str = "riichi://rooms/{code}/history";

#[derive(Clone, Debug)]
pub(crate) struct McpAuth(pub(crate) String);

#[derive(Clone)]
struct SessionEntry {
    session_id: String,
    token_id: String,
    room_code: String,
    room: RoomHandle,
    participant_id: ParticipantId,
    display_name: String,
    character_id: String,
    last_seen: Instant,
    wake: Arc<RevisionWake>,
    cancel: CancellationToken,
}

#[derive(Clone)]
struct TransportSession {
    token_id: String,
    last_seen: Instant,
}

#[derive(Default)]
struct RegistryState {
    sessions: HashMap<String, SessionEntry>,
    transport_sessions: HashMap<String, TransportSession>,
    participants: HashMap<(String, String), ParticipantId>,
    retired_participants: HashSet<(String, String)>,
    bound_rooms: HashMap<String, (String, String)>,
}

#[derive(Clone)]
struct RevisionWake {
    state: Arc<Mutex<WakeState>>,
    notify: Arc<Notify>,
}

#[derive(Clone, Debug)]
struct WakeState {
    revision: u64,
    reason: String,
}

impl Default for RevisionWake {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(WakeState {
                revision: 0,
                reason: "timeout".to_owned(),
            })),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl RevisionWake {
    fn record(&self, revision: u64, reason: impl Into<String>) -> bool {
        let mut state = self.state.lock().expect("MCP wake lock poisoned");
        if revision <= state.revision {
            return false;
        }
        state.revision = revision;
        state.reason = reason.into();
        drop(state);
        self.notify.notify_waiters();
        true
    }

    fn current(&self) -> WakeState {
        self.state.lock().expect("MCP wake lock poisoned").clone()
    }

    #[cfg(test)]
    async fn wait(&self, after_revision: u64, timeout: Duration) -> WakeState {
        self.wait_until(after_revision, timeout, &CancellationToken::new())
            .await
            .unwrap_or_else(|| self.current())
    }

    async fn wait_until(
        &self,
        after_revision: u64,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Option<WakeState> {
        let deadline = Instant::now() + timeout;
        loop {
            let current = self.current();
            if current.revision > after_revision {
                return Some(current);
            }
            let notified = self.notify.notified();
            let current = self.current();
            if current.revision > after_revision {
                return Some(current);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Some(self.current());
            }
            tokio::pin!(notified);
            tokio::select! {
                _ = cancel.cancelled() => return None,
                _ = &mut notified => {}
                _ = time::sleep(remaining) => return Some(self.current()),
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct McpRuntime {
    registry: Arc<McpSessionRegistry>,
    manager: Arc<rmcp::transport::streamable_http_server::session::local::LocalSessionManager>,
    service: StreamableHttpService<
        McpHandler,
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
    >,
}

impl McpRuntime {
    pub(crate) fn new(state: Arc<ServerState>) -> Arc<Self> {
        let idle = Duration::from_secs(state.mcp_session_idle_seconds());
        let registry = Arc::new(McpSessionRegistry::new(idle));
        let host = state
            .public_origin_url()
            .host_str()
            .map(str::to_owned)
            .unwrap_or_else(|| "127.0.0.1".to_owned());
        let authority = state
            .public_origin_url()
            .port()
            .map(|port| format!("{host}:{port}"))
            .unwrap_or_else(|| host.clone());
        let config = StreamableHttpServerConfig::default()
            .with_allowed_hosts([host, authority])
            .with_allowed_origins([state.public_origin().to_owned()])
            .with_max_request_body_bytes(MCP_MAX_BODY_BYTES)
            .with_json_response(true)
            .with_cancellation_token(state.shutdown_token());
        let factory_state = state.clone();
        let factory_registry = registry.clone();
        let manager = Arc::new(
            rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
        );
        let factory_manager = manager.clone();
        let service = StreamableHttpService::new(
            move || {
                Ok(McpHandler::new(
                    factory_state.clone(),
                    factory_registry.clone(),
                    factory_manager.clone(),
                ))
            },
            manager.clone(),
            config,
        );
        let runtime = Arc::new(Self {
            registry,
            manager,
            service,
        });
        if let Some(mut revocations) = state.subscribe_bot_revocations() {
            let runtime = runtime.clone();
            tokio::spawn(async move {
                while let Ok(revocation) = revocations.recv().await {
                    let entries = runtime.registry.revoke_token(revocation.token_id()).await;
                    disconnect_entries(entries.clone()).await;
                    runtime.close_sessions(&entries).await;
                }
            });
        }
        let runtime_reaper = runtime.clone();
        tokio::spawn(async move {
            let interval = idle.min(Duration::from_secs(60));
            loop {
                time::sleep(interval).await;
                let entries = runtime_reaper.registry.expire_idle(Instant::now()).await;
                disconnect_entries(entries.clone()).await;
                runtime_reaper.close_sessions(&entries).await;
            }
        });
        let runtime_shutdown = runtime.clone();
        let shutdown = state.shutdown_token();
        tokio::spawn(async move {
            shutdown.cancelled().await;
            let entries = runtime_shutdown.registry.terminate_all().await;
            disconnect_entries(entries.clone()).await;
            runtime_shutdown.close_sessions(&entries).await;
        });
        runtime
    }

    async fn close_sessions(&self, entries: &[SessionEntry]) {
        for entry in entries {
            let _ = self
                .manager
                .close_session(&entry.session_id.clone().into())
                .await;
        }
    }

    pub(crate) async fn handle(
        &self,
        state: Arc<ServerState>,
        mut request: Request<Body>,
    ) -> Response {
        let headers = request.headers().clone();
        let Some(record) = state.authenticate_bot(&headers, None).ok() else {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"code":"invalid_credentials"}"#))
                .expect("MCP auth response is valid");
        };
        if !mcp_origin_allowed(&headers, &state) {
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"code":"origin_not_allowed"}"#))
                .expect("MCP origin response is valid");
        }
        let session_id = header_session_id(&headers);
        let expired = self.registry.expire_idle(Instant::now()).await;
        disconnect_entries(expired.clone()).await;
        self.close_sessions(&expired).await;
        if let Some(session_id) = session_id.as_deref()
            && self
                .registry
                .token_for(session_id)
                .await
                .is_some_and(|token| token != record.token_id())
        {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"code":"invalid_credentials"}"#))
                .expect("MCP auth response is valid");
        }
        if let Some(session_id) = session_id.as_deref() {
            self.registry
                .touch_transport(session_id, record.token_id())
                .await;
        }
        request
            .extensions_mut()
            .insert(McpAuth(record.token_id().to_owned()));
        let is_delete = request.method() == Method::DELETE;
        let response = self.service.clone().handle(request).await;
        let response_session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .or(session_id);
        if let Some(session_id) = response_session {
            if is_delete {
                if let Some(entry) = self.registry.remove(&session_id).await {
                    disconnect_entry(entry).await;
                }
            } else if response.status().is_success() {
                let _ = self
                    .registry
                    .register_transport(session_id.clone(), record.token_id().to_owned())
                    .await;
                self.registry.touch(&session_id, record.token_id()).await;
            }
        }
        let (parts, body) = response.into_parts();
        Response::from_parts(parts, Body::new(body))
    }
}

fn mcp_origin_allowed(headers: &HeaderMap, state: &ServerState) -> bool {
    let Some(value) = headers.get(header::ORIGIN) else {
        return true;
    };
    value
        .to_str()
        .ok()
        .and_then(|origin| url::Url::parse(origin).ok())
        .is_some_and(|origin| crate::http::same_origin(&origin, state.public_origin_url()))
}

fn header_session_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn normalize_room_code(value: &str) -> Option<String> {
    (value.len() == 6 && value != "000000" && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.to_owned())
}

fn normalize_provider(value: &str) -> Option<String> {
    let value = value.trim_matches(char::is_whitespace);
    if !(1..=64).contains(&value.chars().count()) || value.chars().any(char::is_control) {
        return None;
    }
    value
        .bytes()
        .all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
        .then(|| value.to_owned())
}

async fn disconnect_entries(entries: Vec<SessionEntry>) {
    for entry in entries {
        disconnect_entry(entry).await;
    }
}

async fn disconnect_entry(entry: SessionEntry) {
    entry.cancel.cancel();
    let _ = entry
        .room
        .send(RoomCommand::disconnect(entry.participant_id.clone()))
        .await;
}

pub(crate) struct McpSessionRegistry {
    state: tokio::sync::Mutex<RegistryState>,
    idle: Duration,
    join_lock: tokio::sync::Mutex<()>,
    waiters: Arc<Semaphore>,
    subscriptions: Arc<Semaphore>,
}

impl McpSessionRegistry {
    pub(crate) fn new(idle: Duration) -> Self {
        Self {
            state: tokio::sync::Mutex::new(RegistryState::default()),
            idle: if idle.is_zero() {
                MCP_DEFAULT_IDLE
            } else {
                idle
            },
            join_lock: tokio::sync::Mutex::new(()),
            waiters: Arc::new(Semaphore::new(MCP_MAX_WAIT_TASKS)),
            subscriptions: Arc::new(Semaphore::new(MCP_MAX_SUBSCRIPTIONS)),
        }
    }

    async fn token_for(&self, session_id: &str) -> Option<String> {
        self.state
            .lock()
            .await
            .transport_sessions
            .get(session_id)
            .map(|session| session.token_id.clone())
    }

    async fn register_transport(
        &self,
        session_id: String,
        token_id: String,
    ) -> Result<(), McpFailure> {
        let mut state = self.state.lock().await;
        if let Some(session) = state.transport_sessions.get(&session_id)
            && session.token_id != token_id
        {
            return Err(McpFailure::InvalidCredentials);
        }
        state
            .transport_sessions
            .entry(session_id)
            .or_insert(TransportSession {
                token_id,
                last_seen: Instant::now(),
            });
        Ok(())
    }

    async fn touch_transport(&self, session_id: &str, token_id: &str) {
        let mut state = self.state.lock().await;
        if let Some(session) = state.transport_sessions.get_mut(session_id)
            && session.token_id == token_id
        {
            session.last_seen = Instant::now();
        }
    }

    async fn touch(&self, session_id: &str, token_id: &str) {
        let mut state = self.state.lock().await;
        if let Some(session) = state.transport_sessions.get_mut(session_id)
            && session.token_id == token_id
        {
            session.last_seen = Instant::now();
        }
        if let Some(entry) = state.sessions.get_mut(session_id)
            && entry.token_id == token_id
        {
            entry.last_seen = Instant::now();
        }
    }

    async fn expire_idle(&self, now: Instant) -> Vec<SessionEntry> {
        let mut state = self.state.lock().await;
        let expired_ids: Vec<_> = state
            .transport_sessions
            .iter()
            .filter_map(|(id, session)| {
                (now.duration_since(session.last_seen) >= self.idle).then_some(id.clone())
            })
            .collect();
        expired_ids
            .into_iter()
            .filter_map(|id| {
                state.transport_sessions.remove(&id);
                state.bound_rooms.remove(&id);
                state.sessions.remove(&id)
            })
            .collect()
    }

    async fn terminate_all(&self) -> Vec<SessionEntry> {
        let mut state = self.state.lock().await;
        state.transport_sessions.clear();
        state.bound_rooms.clear();
        state.participants.clear();
        state.retired_participants.clear();
        state.sessions.drain().map(|(_, entry)| entry).collect()
    }

    async fn remove(&self, session_id: &str) -> Option<SessionEntry> {
        let mut state = self.state.lock().await;
        state.transport_sessions.remove(session_id);
        state.bound_rooms.remove(session_id);
        state.sessions.remove(session_id)
    }

    async fn revoke_token(&self, token_id: &str) -> Vec<SessionEntry> {
        let mut state = self.state.lock().await;
        let ids: Vec<_> = state
            .transport_sessions
            .iter()
            .filter_map(|(id, session)| (session.token_id == token_id).then_some(id.clone()))
            .collect();
        for id in &ids {
            state.transport_sessions.remove(id);
            state.bound_rooms.remove(id);
        }
        state
            .participants
            .retain(|(bound_token, _), _| bound_token != token_id);
        state
            .retired_participants
            .retain(|(bound_token, _)| bound_token != token_id);
        ids.into_iter()
            .filter_map(|id| state.sessions.remove(&id))
            .collect()
    }

    async fn binding(&self, session_id: &str, token_id: &str) -> Result<SessionEntry, McpFailure> {
        let mut state = self.state.lock().await;
        let entry = state
            .sessions
            .get_mut(session_id)
            .ok_or(McpFailure::SessionExpired)?;
        if entry.token_id != token_id {
            return Err(McpFailure::InvalidCredentials);
        }
        entry.last_seen = Instant::now();
        Ok(entry.clone())
    }

    async fn was_bound(&self, session_id: &str, token_id: &str) -> Result<bool, McpFailure> {
        let state = self.state.lock().await;
        let Some((bound_token, _)) = state.bound_rooms.get(session_id) else {
            return Ok(false);
        };
        if bound_token != token_id {
            return Err(McpFailure::InvalidCredentials);
        }
        Ok(true)
    }

    async fn bind(
        &self,
        session_id: String,
        token_id: String,
        room_code: String,
        room: RoomHandle,
        participant_id: ParticipantId,
        display_name: String,
        character_id: String,
    ) -> Result<(SessionEntry, Option<SessionEntry>), McpFailure> {
        let mut state = self.state.lock().await;
        if state.sessions.len() >= MCP_MAX_SESSIONS && !state.sessions.contains_key(&session_id) {
            return Err(McpFailure::Busy);
        }
        if let Some(current) = state.transport_sessions.get(&session_id)
            && current.token_id != token_id
        {
            return Err(McpFailure::InvalidCredentials);
        }
        state
            .transport_sessions
            .entry(session_id.clone())
            .or_insert(TransportSession {
                token_id: token_id.clone(),
                last_seen: Instant::now(),
            });
        if let Some(current) = state.sessions.get(&session_id)
            && current.token_id != token_id
        {
            return Err(McpFailure::InvalidCredentials);
        }
        if let Some((bound_token, _)) = state.bound_rooms.get(&session_id) {
            if bound_token != &token_id {
                return Err(McpFailure::InvalidCredentials);
            }
            return Err(McpFailure::SessionBound);
        }
        let old_id = state.sessions.iter().find_map(|(id, entry)| {
            (id != &session_id
                && entry.token_id == token_id
                && entry.room_code == room_code
                && entry.participant_id == participant_id)
                .then_some(id.clone())
        });
        let old = old_id.and_then(|id| {
            state.transport_sessions.remove(&id);
            state.bound_rooms.remove(&id);
            state.sessions.remove(&id)
        });
        let entry = SessionEntry {
            session_id: session_id.clone(),
            token_id: token_id.clone(),
            room_code: room_code.clone(),
            room,
            participant_id: participant_id.clone(),
            display_name,
            character_id,
            last_seen: Instant::now(),
            wake: Arc::new(RevisionWake::default()),
            cancel: CancellationToken::new(),
        };
        state
            .participants
            .insert((token_id.clone(), room_code.clone()), participant_id);
        state
            .bound_rooms
            .insert(session_id.clone(), (token_id, room_code));
        state.sessions.insert(session_id, entry.clone());
        Ok((entry, old))
    }

    async fn acquire_waiter(&self) -> Option<OwnedSemaphorePermit> {
        self.waiters.clone().try_acquire_owned().ok()
    }

    async fn acquire_subscription(&self) -> Option<OwnedSemaphorePermit> {
        self.subscriptions.clone().try_acquire_owned().ok()
    }

    async fn participant_for(&self, token_id: &str, room_code: &str) -> Option<ParticipantId> {
        self.state
            .lock()
            .await
            .participants
            .get(&(token_id.to_owned(), room_code.to_owned()))
            .cloned()
    }

    async fn retired(&self, token_id: &str, room_code: &str) -> bool {
        self.state
            .lock()
            .await
            .retired_participants
            .contains(&(token_id.to_owned(), room_code.to_owned()))
    }

    async fn remove_binding(&self, session_id: &str) -> Option<SessionEntry> {
        let mut state = self.state.lock().await;
        let entry = state.sessions.remove(session_id)?;
        state
            .retired_participants
            .insert((entry.token_id.clone(), entry.room_code.clone()));
        Some(entry)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum McpFailure {
    InvalidCredentials,
    SessionExpired,
    SessionBound,
    RoomNotFound,
    WrongParticipant,
    StaleAction,
    IllegalAction,
    RoomFull,
    Busy,
    LeaveUnavailable,
    InvalidInput,
    Internal,
}

impl McpFailure {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "invalid_credentials",
            Self::SessionExpired => "session_expired",
            Self::SessionBound => "session_already_bound",
            Self::RoomNotFound => "room_not_found",
            Self::WrongParticipant => "wrong_participant",
            Self::StaleAction => "stale_action",
            Self::IllegalAction => "illegal_action",
            Self::RoomFull => "room_full",
            Self::Busy => "busy",
            Self::LeaveUnavailable => "leave_unavailable",
            Self::InvalidInput => "invalid_input",
            Self::Internal => "internal_error",
        }
    }

    fn result(&self) -> CallToolResult {
        CallToolResult::structured_error(json!({"code": self.code()}))
    }

    fn error_data(&self) -> ErrorData {
        ErrorData::new(ErrorCode::INVALID_PARAMS, self.code(), None)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JoinRoomInput {
    pub room_code: String,
    pub provider: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct JoinRoomOutput {
    pub participant_id: String,
    pub resumed: bool,
    pub display_name: String,
    pub character_id: String,
    pub state_uri: String,
    pub public_state_uri: String,
    pub history_uri: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitActionInput {
    pub action_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SubmitActionOutput {
    pub accepted: bool,
    pub revision: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitForTurnInput {
    pub after_revision: u64,
    #[serde(default = "default_wait_seconds")]
    pub timeout_seconds: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WaitForTurnOutput {
    pub reason: String,
    pub revision: u64,
    pub state_uri: String,
}

fn default_wait_seconds() -> u64 {
    MCP_MAX_WAIT_SECONDS
}

#[derive(Clone)]
pub struct McpHandler {
    state: Arc<ServerState>,
    registry: Arc<McpSessionRegistry>,
    manager: Arc<rmcp::transport::streamable_http_server::session::local::LocalSessionManager>,
    tool_router: ToolRouter<Self>,
}

impl McpHandler {
    fn new(
        state: Arc<ServerState>,
        registry: Arc<McpSessionRegistry>,
        manager: Arc<rmcp::transport::streamable_http_server::session::local::LocalSessionManager>,
    ) -> Self {
        Self {
            state,
            registry,
            manager,
            tool_router: Self::tool_router(),
        }
    }

    fn session_id(parts: &axum::http::request::Parts) -> Result<String, McpFailure> {
        header_session_id(&parts.headers).ok_or(McpFailure::SessionExpired)
    }

    fn auth(parts: &axum::http::request::Parts) -> Result<String, McpFailure> {
        parts
            .extensions
            .get::<McpAuth>()
            .map(|auth| auth.0.clone())
            .ok_or(McpFailure::InvalidCredentials)
    }

    async fn binding(
        &self,
        parts: &axum::http::request::Parts,
    ) -> Result<(String, SessionEntry), McpFailure> {
        let session_id = Self::session_id(parts)?;
        let token_id = Self::auth(parts)?;
        let entry = self.registry.binding(&session_id, &token_id).await?;
        Ok((session_id, entry))
    }

    fn uri(code: &str, participant_id: &str, kind: &str) -> String {
        match kind {
            "state" => format!("riichi://rooms/{code}/participants/{participant_id}/state"),
            "public" => format!("riichi://rooms/{code}/public-state"),
            _ => format!("riichi://rooms/{code}/history"),
        }
    }

    fn join_output(entry: &SessionEntry, resumed: bool) -> JoinRoomOutput {
        JoinRoomOutput {
            participant_id: entry.participant_id.to_string(),
            resumed,
            display_name: entry.display_name.clone(),
            character_id: entry.character_id.clone(),
            state_uri: Self::uri(&entry.room_code, entry.participant_id.as_str(), "state"),
            public_state_uri: Self::uri(&entry.room_code, "", "public"),
            history_uri: Self::uri(&entry.room_code, "", "history"),
        }
    }

    async fn start_watcher(&self, session_id: String, entry: SessionEntry) {
        let registry = self.registry.clone();
        tokio::spawn(async move {
            let Some(_subscription) = registry.acquire_subscription().await else {
                return;
            };
            let Ok(mut connection) = entry.room.subscribe().await else {
                return;
            };
            loop {
                tokio::select! {
                    _ = entry.cancel.cancelled() => break,
                    event = connection.recv() => {
                        let Some(event) = event else { break; };
                        let Some((revision, reason)) = watcher_reason(&entry, event).await else { continue; };
                        if entry.wake.record(revision, reason) {
                            registry.touch(&session_id, &entry.token_id).await;
                        }
                    }
                }
            }
        });
    }

    async fn read_state(&self, entry: &SessionEntry, uri: &str) -> Result<Value, McpFailure> {
        let snapshot = entry
            .room
            .snapshot()
            .await
            .map_err(|_| McpFailure::Internal)?;
        let projection = entry
            .room
            .projection(entry.participant_id.clone())
            .await
            .map_err(|_| McpFailure::Internal)?;
        let mut value = json!({
            "revision": snapshot.revision,
            "phase": snapshot.phase,
            "room_code": entry.room_code,
            "participant_id": entry.participant_id,
        });
        if let Some(projection) = projection {
            let projection = serde_json::to_value(projection).map_err(|_| McpFailure::Internal)?;
            if let (Some(target), Some(object)) = (value.as_object_mut(), projection.as_object()) {
                for (key, value) in object {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        add_decision_timing(&mut value);
        let actions = value
            .get("decision")
            .and_then(|decision| decision.get("actions"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        value["is_my_turn"] = Value::Bool(!actions.is_empty());
        value["legal_actions"] = Value::Array(actions);
        value["state_uri"] = Value::String(uri.to_owned());
        Ok(value)
    }

    async fn read_public(&self, entry: &SessionEntry, uri: &str) -> Result<Value, McpFailure> {
        let snapshot = entry
            .room
            .snapshot()
            .await
            .map_err(|_| McpFailure::Internal)?;
        let projection = entry
            .room
            .public_projection()
            .await
            .map_err(|_| McpFailure::Internal)?;
        let mut value = json!({"revision": snapshot.revision, "phase": snapshot.phase, "room_code": entry.room_code});
        if let Some(projection) = projection
            && let (Some(target), Some(object)) = (
                value.as_object_mut(),
                serde_json::to_value(projection)
                    .map_err(|_| McpFailure::Internal)?
                    .as_object(),
            )
        {
            for (key, value) in object {
                target.insert(key.clone(), value.clone());
            }
        }
        add_decision_timing(&mut value);
        value["state_uri"] = Value::String(uri.to_owned());
        Ok(value)
    }

    async fn read_history(&self, entry: &SessionEntry, uri: &str) -> Result<Value, McpFailure> {
        let snapshot = entry
            .room
            .snapshot()
            .await
            .map_err(|_| McpFailure::Internal)?;
        let events = entry
            .room
            .match_events()
            .await
            .map_err(|_| McpFailure::Internal)?;
        let mut events = serde_json::to_value(events).map_err(|_| McpFailure::Internal)?;
        redact_private_history(&mut events);
        Ok(json!({
            "revision": snapshot.revision,
            "phase": snapshot.phase,
            "result": snapshot.result,
            "events": events,
            "history_uri": uri,
        }))
    }
}

#[tool_router]
impl McpHandler {
    #[tool(description = "Join or resume one MCP Participant in a Room.")]
    async fn join_room(
        &self,
        Extension(parts): Extension<axum::http::request::Parts>,
        Parameters(input): Parameters<JoinRoomInput>,
    ) -> Result<Json<JoinRoomOutput>, CallToolResult> {
        let session_id = match Self::session_id(&parts) {
            Ok(value) => value,
            Err(error) => return Err(error.result()),
        };
        let token_id = match Self::auth(&parts) {
            Ok(value) => value,
            Err(error) => return Err(error.result()),
        };
        let Some(room_code) = normalize_room_code(&input.room_code) else {
            return Err(McpFailure::InvalidInput.result());
        };
        let Some(provider) = normalize_provider(&input.provider) else {
            return Err(McpFailure::InvalidInput.result());
        };
        let Some(display_name) = crate::http::normalize_display_text(&input.display_name) else {
            return Err(McpFailure::InvalidInput.result());
        };
        let _join_guard = self.registry.join_lock.lock().await;
        let existing = self.registry.binding(&session_id, &token_id).await;
        if let Ok(entry) = &existing {
            if entry.room_code != room_code {
                return Err(McpFailure::SessionBound.result());
            }
            return Ok(Json(Self::join_output(entry, true)));
        }
        if !matches!(existing, Err(McpFailure::SessionExpired)) {
            return Err(McpFailure::InvalidCredentials.result());
        }
        if self
            .registry
            .was_bound(&session_id, &token_id)
            .await
            .map_err(|error| error.result())?
        {
            return Err(McpFailure::SessionBound.result());
        }
        let room = self
            .state
            .rooms()
            .get(&room_code)
            .await
            .ok_or(McpFailure::RoomNotFound)
            .map_err(|e| e.result())?;
        let pair = self.registry.participant_for(&token_id, &room_code).await;
        let default_character = self.state.mcp_character_for_provider(&provider);
        let (participant_id, resumed, character_id, joined_display_name) =
            if let Some(participant_id) = pair {
                let snapshot = room
                    .snapshot()
                    .await
                    .map_err(|_| McpFailure::Internal)
                    .map_err(|e| e.result())?;
                if let Some(participant) = snapshot
                    .participants
                    .iter()
                    .find(|value| value.id == participant_id)
                {
                    room.send(RoomCommand::reconnect(participant_id.clone()))
                        .await
                        .map_err(room_failure)
                        .map_err(|e| e.result())?;
                    (
                        participant_id,
                        true,
                        participant.character_id.clone(),
                        participant.display_name.clone(),
                    )
                } else {
                    if self.registry.retired(&token_id, &room_code).await {
                        return Err(McpFailure::LeaveUnavailable.result());
                    }
                    let participant_id = ParticipantId::new(crate::http::generate_ulid());
                    let participant = Participant::new(
                        participant_id.clone(),
                        display_name.clone(),
                        ParticipantKind::MCP,
                    );
                    let joined = room
                        .send(RoomCommand::Join {
                            participant,
                            character_id: Some(default_character.clone()),
                            token_id: Some(token_id.clone()),
                        })
                        .await
                        .map_err(room_failure)
                        .map_err(|e| e.result())?;
                    let RoomResponse::Joined(joined) = joined else {
                        return Err(McpFailure::Internal.result());
                    };
                    (joined.id, false, joined.character_id, joined.display_name)
                }
            } else {
                let participant_id = ParticipantId::new(crate::http::generate_ulid());
                let participant = Participant::new(
                    participant_id.clone(),
                    display_name.clone(),
                    ParticipantKind::MCP,
                );
                let joined = room
                    .send(RoomCommand::Join {
                        participant,
                        character_id: Some(default_character),
                        token_id: Some(token_id.clone()),
                    })
                    .await
                    .map_err(room_failure)
                    .map_err(|e| e.result())?;
                let RoomResponse::Joined(joined) = joined else {
                    return Err(McpFailure::Internal.result());
                };
                (joined.id, false, joined.character_id, joined.display_name)
            };
        let (entry, old) = self
            .registry
            .bind(
                session_id.clone(),
                token_id,
                input.room_code.clone(),
                room.clone(),
                participant_id.clone(),
                joined_display_name,
                character_id.clone(),
            )
            .await
            .map_err(|e| e.result())?;
        if let Some(old) = old {
            old.cancel.cancel();
            let _ = self
                .manager
                .close_session(&old.session_id.clone().into())
                .await;
        }
        self.start_watcher(session_id, entry.clone()).await;
        Ok(Json(Self::join_output(&entry, resumed)))
    }

    #[tool(description = "Leave the bound Room and permanently end this MCP Participant.")]
    async fn leave_room(
        &self,
        Extension(parts): Extension<axum::http::request::Parts>,
    ) -> Result<Json<Value>, CallToolResult> {
        let (session_id, entry) = self.binding(&parts).await.map_err(|e| e.result())?;
        entry
            .room
            .send(RoomCommand::leave(entry.participant_id.clone()))
            .await
            .map_err(room_failure)
            .map_err(|e| e.result())?;
        let _ = self.registry.remove_binding(&session_id).await;
        entry.cancel.cancel();
        Ok(Json(json!({"left": true})))
    }

    #[tool(description = "Submit the current legal action identifier for this MCP Participant.")]
    async fn submit_action(
        &self,
        Extension(parts): Extension<axum::http::request::Parts>,
        Parameters(input): Parameters<SubmitActionInput>,
    ) -> Result<Json<SubmitActionOutput>, CallToolResult> {
        if input.action_id.is_empty() || input.action_id.len() > 128 {
            return Err(McpFailure::InvalidInput.result());
        }
        let (_session_id, entry) = self.binding(&parts).await.map_err(|e| e.result())?;
        let projection = entry
            .room
            .projection(entry.participant_id.clone())
            .await
            .map_err(|_| McpFailure::Internal)
            .map_err(|e| e.result())?;
        let Some(projection) = projection else {
            return Err(McpFailure::StaleAction.result());
        };
        let value = serde_json::to_value(projection)
            .map_err(|_| McpFailure::Internal)
            .map_err(|e| e.result())?;
        if !contains_action_id(&value, &input.action_id) {
            return Err(McpFailure::IllegalAction.result());
        }
        let Some(decision_id) =
            find_string(&value, "decision_id").or_else(|| find_string(&value, "decisionId"))
        else {
            return Err(McpFailure::StaleAction.result());
        };
        let result = entry
            .room
            .send(RoomCommand::submit_action(
                entry.participant_id.clone(),
                DecisionId::new(decision_id),
                input.action_id,
            ))
            .await
            .map_err(room_failure)
            .map_err(|e| e.result())?;
        let RoomResponse::Action(action) = result else {
            return Err(McpFailure::Internal.result());
        };
        let snapshot = entry
            .room
            .snapshot()
            .await
            .map_err(|_| McpFailure::Internal)
            .map_err(|e| e.result())?;
        Ok(Json(SubmitActionOutput {
            accepted: matches!(
                action,
                double_riichi_core::DecisionResult::Waiting { .. }
                    | double_riichi_core::DecisionResult::Resolved { .. }
            ),
            revision: snapshot.revision,
        }))
    }

    #[tool(description = "Wait for a relevant Room revision without returning state.")]
    async fn wait_for_turn(
        &self,
        Extension(parts): Extension<axum::http::request::Parts>,
        Parameters(input): Parameters<WaitForTurnInput>,
    ) -> Result<Json<WaitForTurnOutput>, CallToolResult> {
        if input.timeout_seconds > MCP_MAX_WAIT_SECONDS {
            return Err(McpFailure::InvalidInput.result());
        }
        let (_session_id, entry) = self.binding(&parts).await.map_err(|e| e.result())?;
        let Some(_waiter) = self.registry.acquire_waiter().await else {
            return Err(McpFailure::Busy.result());
        };
        let timeout = Duration::from_secs(if input.timeout_seconds == 0 {
            MCP_MAX_WAIT_SECONDS
        } else {
            input.timeout_seconds
        });
        let Some(wake) = entry
            .wake
            .wait_until(input.after_revision, timeout, &entry.cancel)
            .await
        else {
            return Err(McpFailure::SessionExpired.result());
        };
        let reason = if wake.revision > input.after_revision {
            wake.reason
        } else {
            "timeout".to_owned()
        };
        Ok(Json(WaitForTurnOutput {
            reason,
            revision: wake.revision,
            state_uri: Self::uri(&entry.room_code, entry.participant_id.as_str(), "state"),
        }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_subscribe()
                .build(),
        )
        .with_instructions("Double Riichi MCP; authenticate with a private Bot Token.")
    }

    fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        async move {
            let Some(parts) = context.extensions.get::<axum::http::request::Parts>() else {
                return Ok(ListResourcesResult::with_all_items(Vec::new()));
            };
            let Ok((_session_id, entry)) = self.binding(parts).await else {
                return Ok(ListResourcesResult::with_all_items(Vec::new()));
            };
            Ok(ListResourcesResult::with_all_items(resources(&entry)))
        }
    }

    fn list_resource_templates(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(
            resource_templates(),
        )))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResponse, ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        async move {
            let parts = context
                .extensions
                .get::<axum::http::request::Parts>()
                .ok_or_else(|| ErrorData::invalid_request("missing request context", None))?;
            let (_session_id, entry) = self
                .binding(parts)
                .await
                .map_err(|error| error.error_data())?;
            let uri = request.uri;
            let value =
                if uri == Self::uri(&entry.room_code, entry.participant_id.as_str(), "state") {
                    self.read_state(&entry, &uri)
                        .await
                        .map_err(|error| error.error_data())?
                } else if uri == Self::uri(&entry.room_code, "", "public") {
                    self.read_public(&entry, &uri)
                        .await
                        .map_err(|error| error.error_data())?
                } else if uri == Self::uri(&entry.room_code, "", "history") {
                    self.read_history(&entry, &uri)
                        .await
                        .map_err(|error| error.error_data())?
                } else {
                    return Err(ErrorData::resource_not_found(uri, None));
                };
            Ok(
                ReadResourceResult::new(vec![ResourceContents::text(value.to_string(), uri)])
                    .into(),
            )
        }
    }

    fn accepted_subscription_filter(
        &self,
        requested: &rmcp::model::SubscriptionFilter,
    ) -> Option<rmcp::model::SubscriptionFilter> {
        Some(requested.clone())
    }

    #[allow(deprecated)]
    fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + rmcp::service::MaybeSendFuture + '_
    {
        async move {
            let parts = context
                .extensions
                .get::<axum::http::request::Parts>()
                .ok_or_else(|| ErrorData::invalid_request("missing request context", None))?;
            let (_session_id, entry) = self
                .binding(parts)
                .await
                .map_err(|error| error.error_data())?;
            let valid_uri = resources(&entry)
                .iter()
                .any(|resource| resource.uri == request.uri);
            if !valid_uri {
                return Err(ErrorData::resource_not_found(request.uri, None));
            }
            let Some(subscription) = self.registry.acquire_subscription().await else {
                return Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, "busy", None));
            };
            let mut connection = entry
                .room
                .subscribe()
                .await
                .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
            let uri = request.uri;
            let peer = context.peer.clone();
            tokio::spawn(async move {
                let _subscription = subscription;
                loop {
                    tokio::select! {
                        _ = entry.cancel.cancelled() => break,
                        event = connection.recv() => {
                            let Some(event) = event else { break; };
                            let Some((_revision, reason)) = watcher_reason(&entry, event).await else {
                                continue;
                            };
                            if notification_uris(&entry, reason).iter().any(|candidate| candidate == &uri)
                                && peer
                                    .notify_resource_updated(
                                        rmcp::model::ResourceUpdatedNotificationParam::new(
                                            uri.clone(),
                                        ),
                                    )
                                    .await
                                    .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            });
            Ok(())
        }
    }

    fn listen(
        &self,
        context: SubscriptionContext,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + rmcp::service::MaybeSendFuture + '_
    {
        async move {
            let parts = context
                .request_context()
                .extensions
                .get::<axum::http::request::Parts>()
                .ok_or_else(|| ErrorData::invalid_request("missing request context", None))?;
            let session_id = Self::session_id(parts).map_err(|error| error.error_data())?;
            let token_id = Self::auth(parts).map_err(|error| error.error_data())?;
            let entry = self
                .registry
                .binding(&session_id, &token_id)
                .await
                .map_err(|error| error.error_data())?;
            let Some(_subscription) = self.registry.acquire_subscription().await else {
                return Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, "busy", None));
            };
            let mut connection = entry
                .room
                .subscribe()
                .await
                .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
            let accepted = context
                .accepted()
                .resource_subscriptions
                .clone()
                .unwrap_or_default();
            let sink = context.sink().clone();
            loop {
                tokio::select! {
                    _ = context.cancelled() => break,
                    event = connection.recv() => {
                        let Some(event) = event else { break; };
                        let Some((revision, reason)) = watcher_reason(&entry, event).await else {
                            continue;
                        };
                        if !entry.wake.record(revision, reason) {
                            continue;
                        }
                        self.registry.touch(&session_id, &entry.token_id).await;
                        for uri in notification_uris(&entry, reason) {
                            if accepted.iter().any(|candidate| candidate == &uri)
                                && sink.notify_resource_updated(uri).await.is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

async fn watcher_reason(entry: &SessionEntry, event: RoomEvent) -> Option<(u64, &'static str)> {
    if let RoomEvent::RoomDeleted | RoomEvent::ServerShutdown = event {
        return Some((
            entry.wake.current().revision.saturating_add(1),
            match event {
                RoomEvent::RoomDeleted => "room_deleted",
                RoomEvent::ServerShutdown => "server_shutdown",
                _ => unreachable!(),
            },
        ));
    }
    let snapshot = entry.room.snapshot().await.ok()?;
    let revision = snapshot.revision;
    match event {
        RoomEvent::DecisionOpened { decision, .. } => {
            let participant = snapshot
                .participants
                .iter()
                .find(|value| value.id == entry.participant_id)?;
            let double_riichi_core::MatchRole::Player(seat) = participant.role else {
                return None;
            };
            decision
                .eligible()
                .any(|candidate| candidate == seat)
                .then_some((revision, "my_decision"))
        }
        RoomEvent::SelectionChanged => snapshot
            .participants
            .iter()
            .find(|value| value.id == entry.participant_id)
            .map(|participant| {
                (
                    revision,
                    if participant.selected {
                        "selected"
                    } else {
                        "deselected"
                    },
                )
            }),
        RoomEvent::PhaseChanged(RoomPhase::Playing(_)) | RoomEvent::MatchStarted(_) => {
            Some((revision, "match_started"))
        }
        RoomEvent::MatchCompleted { .. } | RoomEvent::PhaseChanged(RoomPhase::PostMatch(_)) => {
            Some((revision, "game_ended"))
        }
        RoomEvent::MatchAborted { .. } | RoomEvent::PhaseChanged(RoomPhase::Lobby) => {
            Some((revision, "round_ended"))
        }
        RoomEvent::ActionResolved { result, .. } => {
            if matches!(snapshot.phase, RoomPhase::PostMatch(_)) {
                Some((revision, "game_ended"))
            } else if result
                .events()
                .iter()
                .any(|event| matches!(event, double_riichi_core::GameEvent::StartKyoku { .. }))
            {
                Some((revision, "round_started"))
            } else if result
                .events()
                .iter()
                .any(|event| matches!(event, double_riichi_core::GameEvent::EndKyoku))
            {
                Some((revision, "round_ended"))
            } else {
                None
            }
        }
        RoomEvent::ParticipantLeft(participant_id) if participant_id == entry.participant_id => {
            snapshot
                .participants
                .iter()
                .find(|value| value.id == entry.participant_id)
                .and_then(|participant| {
                    matches!(
                        participant.controller,
                        double_riichi_core::RoomController::PermanentAuto(_)
                    )
                    .then_some((revision, "permanent_auto"))
                })
        }
        RoomEvent::Snapshot(_) => snapshot
            .participants
            .iter()
            .find(|value| value.id == entry.participant_id)
            .and_then(|participant| {
                matches!(
                    participant.controller,
                    double_riichi_core::RoomController::PermanentAuto(_)
                )
                .then_some((revision, "permanent_auto"))
            }),
        RoomEvent::RoomDeleted => Some((revision, "room_deleted")),
        RoomEvent::ServerShutdown => Some((revision, "server_shutdown")),
        RoomEvent::StorageDegraded
        | RoomEvent::ParticipantJoined(_)
        | RoomEvent::ParticipantLeft(_) => None,
    }
}

fn resources(entry: &SessionEntry) -> Vec<Resource> {
    vec![
        Resource::new(
            McpHandler::uri(&entry.room_code, entry.participant_id.as_str(), "state"),
            "private-state",
        )
        .with_mime_type("application/json"),
        Resource::new(
            McpHandler::uri(&entry.room_code, "", "public"),
            "public-state",
        )
        .with_mime_type("application/json"),
        Resource::new(McpHandler::uri(&entry.room_code, "", "history"), "history")
            .with_mime_type("application/json"),
    ]
}

fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(STATE_TEMPLATE, "private-state").with_mime_type("application/json"),
        ResourceTemplate::new(PUBLIC_STATE_TEMPLATE, "public-state")
            .with_mime_type("application/json"),
        ResourceTemplate::new(HISTORY_TEMPLATE, "history").with_mime_type("application/json"),
    ]
}

fn add_decision_timing(value: &mut Value) {
    let remaining = value
        .get("decision")
        .and_then(|decision| decision.get("remaining_ms"))
        .and_then(Value::as_u64);
    value["remaining_ms_at_read"] = remaining.map_or(Value::Null, Value::from);
    value["decision_expires_at"] = remaining.map_or(Value::Null, |value| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Value::from(now.saturating_add(u128::from(value)) as u64)
    });
}

fn notification_uris(entry: &SessionEntry, reason: &str) -> Vec<String> {
    let mut uris = vec![
        McpHandler::uri(&entry.room_code, entry.participant_id.as_str(), "state"),
        McpHandler::uri(&entry.room_code, "", "public"),
    ];
    if matches!(
        reason,
        "round_started" | "round_ended" | "game_ended" | "room_deleted" | "server_shutdown"
    ) {
        uris.push(McpHandler::uri(&entry.room_code, "", "history"));
    }
    uris
}

fn room_failure(error: RoomError) -> McpFailure {
    match error {
        RoomError::RoomFull => McpFailure::RoomFull,
        RoomError::ParticipantNotFound(_) => McpFailure::WrongParticipant,
        RoomError::Disconnected | RoomError::ControllerNotInteractive => McpFailure::SessionExpired,
        RoomError::NotPlaying | RoomError::Playing | RoomError::Match(_) => McpFailure::StaleAction,
        RoomError::Busy => McpFailure::Busy,
        RoomError::Deleted | RoomError::Closed => McpFailure::RoomNotFound,
        RoomError::NotLobby | RoomError::RematchUnavailable => McpFailure::LeaveUnavailable,
        _ => McpFailure::Internal,
    }
}

fn find_string(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(object) => object.iter().find_map(|(candidate, value)| {
            if candidate == key {
                value.as_str().map(str::to_owned)
            } else {
                find_string(value, key)
            }
        }),
        Value::Array(values) => values.iter().find_map(|value| find_string(value, key)),
        _ => None,
    }
}

fn contains_action_id(value: &Value, action_id: &str) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (key == "action_id" || key == "actionId") && value.as_str() == Some(action_id)
                || contains_action_id(value, action_id)
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| contains_action_id(value, action_id)),
        _ => false,
    }
}

fn redact_private_history(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| {
                !matches!(
                    key.as_str(),
                    "tehais" | "hands" | "wall" | "private_state" | "raw_state"
                )
            });
            for value in object.values_mut() {
                redact_private_history(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_private_history(value);
            }
        }
        _ => {}
    }
}

pub(crate) async fn mcp_endpoint(
    State(state): State<Arc<ServerState>>,
    axum::extract::Extension(runtime): axum::extract::Extension<Arc<McpRuntime>>,
    request: Request<Body>,
) -> Response {
    runtime.handle(state, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use double_riichi_core::{
        CharacterCatalog, GameMode, ParticipantKind, PermanentAutoReason, Presence, RoomActor,
        RoomCommand, RoomConfig, RoomController, RoomResponse, ShutdownMode,
    };

    fn room(mode: GameMode) -> RoomHandle {
        RoomActor::spawn(RoomConfig::new(
            "MCP tests",
            mode,
            CharacterCatalog::starter(),
        ))
    }

    fn entry(
        room: RoomHandle,
        session_id: &str,
        token_id: &str,
        room_code: &str,
        participant_id: &str,
    ) -> SessionEntry {
        SessionEntry {
            session_id: session_id.to_owned(),
            token_id: token_id.to_owned(),
            room_code: room_code.to_owned(),
            room,
            participant_id: ParticipantId::new(participant_id),
            display_name: "Agent".to_owned(),
            character_id: "mcp-bot".to_owned(),
            last_seen: Instant::now(),
            wake: Arc::new(RevisionWake::default()),
            cancel: CancellationToken::new(),
        }
    }

    #[test]
    fn tool_and_resource_surfaces_are_exact() {
        let mut names: Vec<_> = McpHandler::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["join_room", "leave_room", "submit_action", "wait_for_turn"]
        );

        let templates = resource_templates();
        assert_eq!(templates.len(), 3);
        let encoded = serde_json::to_string(&templates).expect("resource templates serialize");
        for uri in [STATE_TEMPLATE, PUBLIC_STATE_TEMPLATE, HISTORY_TEMPLATE] {
            assert!(encoded.contains(uri), "missing resource template {uri}");
        }
    }

    #[tokio::test]
    async fn session_binding_survives_leave_and_cannot_change_rooms() {
        let registry = McpSessionRegistry::new(Duration::from_secs(60));
        let first_room = room(GameMode::FourPlayerRedEast);
        registry
            .bind(
                "session".to_owned(),
                "token".to_owned(),
                "123456".to_owned(),
                first_room,
                ParticipantId::new("agent"),
                "Agent".to_owned(),
                "mcp-bot".to_owned(),
            )
            .await
            .expect("first bind");

        let removed = registry.remove_binding("session").await;
        assert!(removed.is_some());
        assert!(matches!(
            registry.binding("session", "token").await,
            Err(McpFailure::SessionExpired)
        ));
        assert!(registry.was_bound("session", "token").await.unwrap());
        let result = registry
            .bind(
                "session".to_owned(),
                "token".to_owned(),
                "654321".to_owned(),
                room(GameMode::FourPlayerRedEast),
                ParticipantId::new("other"),
                "Other".to_owned(),
                "mcp-bot".to_owned(),
            )
            .await;
        assert!(matches!(result, Err(McpFailure::SessionBound)));
    }

    #[tokio::test]
    async fn same_token_room_resumes_and_new_session_wins_generation() {
        let registry = McpSessionRegistry::new(Duration::from_secs(60));
        let room = room(GameMode::FourPlayerRedEast);
        let participant_id = ParticipantId::new("agent");
        registry
            .bind(
                "old".to_owned(),
                "token".to_owned(),
                "123456".to_owned(),
                room.clone(),
                participant_id.clone(),
                "Agent".to_owned(),
                "mcp-bot".to_owned(),
            )
            .await
            .expect("old generation");
        assert_eq!(
            registry.participant_for("token", "123456").await,
            Some(participant_id.clone())
        );

        let (current, replaced) = registry
            .bind(
                "new".to_owned(),
                "token".to_owned(),
                "123456".to_owned(),
                room,
                participant_id.clone(),
                "Changed name is ignored by join resume".to_owned(),
                "other-character".to_owned(),
            )
            .await
            .expect("replacement generation");
        assert_eq!(replaced.expect("old generation returned").session_id, "old");
        assert_eq!(current.session_id, "new");
        assert!(matches!(
            registry.binding("old", "token").await,
            Err(McpFailure::SessionExpired)
        ));
        assert_eq!(
            registry
                .binding("new", "token")
                .await
                .unwrap()
                .participant_id,
            participant_id
        );
    }

    #[tokio::test]
    async fn idle_ttl_expires_session_and_disconnects_participant() {
        let registry = McpSessionRegistry::new(Duration::from_secs(1));
        let room = room(GameMode::FourPlayerRedEast);
        room.send(RoomCommand::join(Participant::new(
            "agent",
            "Agent",
            ParticipantKind::MCP,
        )))
        .await
        .expect("join participant");
        registry
            .bind(
                "session".to_owned(),
                "token".to_owned(),
                "123456".to_owned(),
                room.clone(),
                ParticipantId::new("agent"),
                "Agent".to_owned(),
                "mcp-bot".to_owned(),
            )
            .await
            .expect("bind session");

        let expired = registry
            .expire_idle(Instant::now() + Duration::from_secs(2))
            .await;
        assert_eq!(expired.len(), 1);
        disconnect_entries(expired).await;
        assert!(matches!(
            registry.binding("session", "token").await,
            Err(McpFailure::SessionExpired)
        ));
        assert_eq!(
            room.snapshot()
                .await
                .unwrap()
                .participants
                .first()
                .unwrap()
                .presence,
            Presence::Disconnected
        );
        assert_eq!(
            registry.participant_for("token", "123456").await,
            Some(ParticipantId::new("agent"))
        );
    }

    #[tokio::test]
    async fn relevant_wake_filtering_and_notification_uris_are_revision_safe() {
        let room = room(GameMode::FourPlayerRedEast);
        let joined = match room
            .send(RoomCommand::join(Participant::new(
                "agent",
                "Agent",
                ParticipantKind::MCP,
            )))
            .await
            .unwrap()
        {
            RoomResponse::Joined(value) => value,
            other => panic!("unexpected join response: {other:?}"),
        };
        let watcher = entry(room.clone(), "session", "token", "123456", "agent");
        assert_eq!(
            watcher_reason(&watcher, RoomEvent::ParticipantJoined(joined)).await,
            None
        );
        assert_eq!(
            watcher_reason(&watcher, RoomEvent::StorageDegraded).await,
            None
        );
        assert_eq!(
            watcher_reason(
                &watcher,
                RoomEvent::ParticipantLeft(ParticipantId::new("other")),
            )
            .await,
            None
        );

        room.send(RoomCommand::select("agent")).await.unwrap();
        let revision = room.snapshot().await.unwrap().revision;
        assert_eq!(
            watcher_reason(&watcher, RoomEvent::SelectionChanged).await,
            Some((revision, "selected"))
        );
        for (reason, includes_history) in [
            ("selected", false),
            ("deselected", false),
            ("my_decision", false),
            ("round_started", true),
            ("round_ended", true),
            ("game_ended", true),
            ("permanent_auto", false),
            ("room_deleted", true),
            ("server_shutdown", true),
        ] {
            let uris = notification_uris(&watcher, reason);
            assert_eq!(uris.len(), if includes_history { 3 } else { 2 }, "{reason}");
            assert!(uris.iter().any(|uri| uri.contains("/public-state")));
            assert_eq!(
                uris.iter().filter(|uri| uri.contains("/history")).count(),
                usize::from(includes_history)
            );
        }
    }

    #[tokio::test]
    async fn notification_before_wait_and_old_revisions_do_not_race() {
        let wake = RevisionWake::default();
        assert!(wake.record(4, "my_decision"));
        let result = wake.wait(3, Duration::from_millis(1)).await;
        assert_eq!(
            (result.revision, result.reason.as_str()),
            (4, "my_decision")
        );

        let old = RevisionWake::default();
        assert!(!old.record(0, "old"));
        let result = old.wait(0, Duration::from_millis(1)).await;
        assert_eq!(result.reason, "timeout");
    }

    #[test]
    fn submit_action_errors_and_nested_action_ids_are_stable() {
        for (failure, code) in [
            (McpFailure::StaleAction, "stale_action"),
            (McpFailure::IllegalAction, "illegal_action"),
            (McpFailure::WrongParticipant, "wrong_participant"),
        ] {
            assert_eq!(failure.code(), code);
        }
        for (error, code) in [
            (
                RoomError::ParticipantNotFound(ParticipantId::new("other")),
                "wrong_participant",
            ),
            (RoomError::NotPlaying, "stale_action"),
            (RoomError::Playing, "stale_action"),
            (RoomError::Match("expired".to_owned()), "stale_action"),
        ] {
            assert_eq!(room_failure(error).code(), code);
        }
        let projection = json!({"decision": {"actions": [{"actionId": "a1"}]} });
        assert!(contains_action_id(&projection, "a1"));
        assert!(!contains_action_id(&projection, "a2"));
    }

    #[tokio::test]
    async fn private_and_public_room_projections_keep_concealed_tiles_private() {
        let room = room(GameMode::FourPlayerRedEast);
        room.send(RoomCommand::join(Participant::new(
            "agent",
            "Agent",
            ParticipantKind::MCP,
        )))
        .await
        .unwrap();
        room.send(RoomCommand::select("agent")).await.unwrap();
        room.send(RoomCommand::fill_with_bots()).await.unwrap();
        assert!(matches!(
            room.send(RoomCommand::start()).await.unwrap(),
            RoomResponse::Started(_)
        ));

        let private = serde_json::to_value(
            room.projection(ParticipantId::new("agent"))
                .await
                .unwrap()
                .expect("private projection"),
        )
        .unwrap();
        let public =
            serde_json::to_value(room.public_projection().await.unwrap().unwrap()).unwrap();
        assert_eq!(private["audience"], "player");
        assert_eq!(public["audience"], "public");
        assert!(private.to_string().contains("\"hand\""));
        assert!(!public.to_string().contains("\"hand\""));

        let mut history = json!({"tehais": [1], "nested": {"hands": [2], "visible": true}});
        redact_private_history(&mut history);
        assert_eq!(history, json!({"nested": {"visible": true}}));
    }

    #[tokio::test]
    async fn leave_disconnect_and_revocation_update_room_state() {
        let active = room(GameMode::ThreePlayerRedEast);
        active
            .send(RoomCommand::join(Participant::new(
                "agent",
                "Agent",
                ParticipantKind::MCP,
            )))
            .await
            .unwrap();
        active.send(RoomCommand::select("agent")).await.unwrap();
        active.send(RoomCommand::fill_with_bots()).await.unwrap();
        active.send(RoomCommand::start()).await.unwrap();
        active.send(RoomCommand::leave("agent")).await.unwrap();
        let participant = active
            .snapshot()
            .await
            .unwrap()
            .participants
            .into_iter()
            .find(|value| value.id.as_str() == "agent")
            .expect("active player retained until match end");
        assert_eq!(
            participant.controller,
            RoomController::PermanentAuto(PermanentAutoReason::LeftDuringMatch)
        );

        let watchdog = room(GameMode::ThreePlayerRedEast);
        watchdog
            .send(RoomCommand::join(Participant::new(
                "agent",
                "Agent",
                ParticipantKind::MCP,
            )))
            .await
            .unwrap();
        watchdog.send(RoomCommand::select("agent")).await.unwrap();
        watchdog.send(RoomCommand::fill_with_bots()).await.unwrap();
        watchdog.send(RoomCommand::start()).await.unwrap();
        let disconnected = entry(watchdog.clone(), "session", "token", "123456", "agent");
        disconnect_entry(disconnected.clone()).await;
        assert!(disconnected.cancel.is_cancelled());
        assert_eq!(
            watchdog
                .snapshot()
                .await
                .unwrap()
                .participants
                .into_iter()
                .find(|value| value.id.as_str() == "agent")
                .unwrap()
                .presence,
            Presence::Disconnected
        );

        let revoked = room(GameMode::FourPlayerRedEast);
        revoked
            .send(RoomCommand::join_with_token(
                Participant::new("agent", "Agent", ParticipantKind::MCP),
                "token",
            ))
            .await
            .unwrap();
        revoked
            .send(RoomCommand::revoke_token("token"))
            .await
            .unwrap();
        assert!(revoked.snapshot().await.unwrap().participants.is_empty());
    }

    #[tokio::test]
    async fn room_delete_and_shutdown_publish_terminal_events() {
        let deleted = room(GameMode::FourPlayerRedEast);
        let mut deleted_events = deleted.subscribe().await.unwrap();
        assert!(matches!(
            deleted_events.recv().await,
            Some(RoomEvent::Snapshot(_))
        ));
        assert!(matches!(
            deleted.send(RoomCommand::Delete).await,
            Ok(RoomResponse::Deleted)
        ));
        assert!(matches!(
            deleted_events.recv().await,
            Some(RoomEvent::RoomDeleted)
        ));
        assert!(deleted.snapshot().await.is_err());

        let shutting_down = room(GameMode::FourPlayerRedEast);
        let mut shutdown_events = shutting_down.subscribe().await.unwrap();
        assert!(matches!(
            shutdown_events.recv().await,
            Some(RoomEvent::Snapshot(_))
        ));
        assert!(matches!(
            shutting_down
                .send(RoomCommand::shutdown(ShutdownMode::Forced))
                .await,
            Ok(RoomResponse::Shutdown)
        ));
        assert!(matches!(
            shutdown_events.recv().await,
            Some(RoomEvent::ServerShutdown)
        ));
        assert!(shutting_down.snapshot().await.is_err());
    }
}
