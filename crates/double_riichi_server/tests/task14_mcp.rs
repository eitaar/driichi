use std::{
    future::Future,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use double_riichi_core::{
    GameMode, ParticipantId, Presence, RoomCommand, RoomConfig, RoomRegistry, TimeControl,
};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerState, Storage, hash_password,
    server_router,
};
use rmcp::{
    ClientHandler,
    model::{
        CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation,
        ReadResourceRequestParams, SubscribeRequestParams, UnsubscribeRequestParams,
    },
};
use serde_json::{Value, json};
use tokio::{
    net::TcpListener,
    sync::oneshot,
    time::{Instant, timeout},
};
use tower::ServiceExt;
use url::Url;

const ORIGIN: &str = "http://127.0.0.1:3000";
const PROTOCOL_VERSION: &str = "2025-06-18";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

fn root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "double-riichi-task14-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

async fn fixture(label: &str) -> (Arc<ServerState>, Arc<BotTokenService>, Arc<Storage>, String) {
    fixture_with_origin(label, ORIGIN).await
}

async fn fixture_with_origin(
    label: &str,
    origin: &str,
) -> (Arc<ServerState>, Arc<BotTokenService>, Arc<Storage>, String) {
    let storage = Arc::new(Storage::connect(&root(label)).await.unwrap());
    let service = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::empty()),
    ));
    let token = service.create("runner", 1, "task14").await.unwrap();
    let raw = token.secret().expose().to_owned();
    let state = Arc::new(
        ServerState::for_tests(
            origin,
            Arc::new(
                AdminAuthenticator::new(
                    "admin",
                    hash_password("correct horse battery staple").unwrap(),
                )
                .unwrap(),
            ),
            RoomRegistry::with_max_rooms(8),
        )
        .with_bot_token_service(service.clone()),
    );
    (state, service, storage, raw)
}

fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": {"name": "task14", "version": "1"}
    })
}

async fn mcp(
    app: &Router,
    token: Option<&str>,
    origin: Option<&str>,
    session_id: Option<&str>,
    id: Option<u64>,
    method: &str,
    params: Value,
) -> Response {
    let mut message = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    if let Some(id) = id {
        message["id"] = json!(id);
    }
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("host", "127.0.0.1:3000")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    if let Some(session_id) = session_id {
        builder = builder.header("mcp-session-id", session_id);
        builder = builder.header("mcp-protocol-version", PROTOCOL_VERSION);
    }
    let request = builder.body(Body::from(message.to_string())).unwrap();
    timeout(REQUEST_TIMEOUT, app.clone().oneshot(request))
        .await
        .expect("MCP request timed out")
        .unwrap()
}

async fn mcp_delete(app: &Router, token: &str, session_id: &str) -> Response {
    let request = Request::builder()
        .method("DELETE")
        .uri("/mcp")
        .header("host", "127.0.0.1:3000")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("Bearer {token}"))
        .header("mcp-session-id", session_id)
        .header("mcp-protocol-version", PROTOCOL_VERSION)
        .body(Body::empty())
        .unwrap();
    timeout(REQUEST_TIMEOUT, app.clone().oneshot(request))
        .await
        .expect("MCP DELETE timed out")
        .unwrap()
}

async fn body_json(response: Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    if body.is_empty() {
        return Value::Null;
    }
    if let Ok(value) = serde_json::from_slice(&body) {
        return value;
    }
    let text = String::from_utf8_lossy(&body);
    text.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|line| !line.is_empty())
        .last()
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| panic!("MCP response was not JSON or SSE: body={body:?}"))
}

fn assert_rpc_success(body: &Value) {
    assert_eq!(
        body["jsonrpc"], "2.0",
        "unexpected JSON-RPC response: {body}"
    );
    assert!(body["error"].is_null(), "JSON-RPC error: {body}");
    assert!(!body["result"].is_null(), "missing JSON-RPC result: {body}");
}

fn tool_value(body: &Value) -> Value {
    assert_rpc_success(body);
    let result = &body["result"];
    if let Some(value) = result
        .get("structuredContent")
        .filter(|value| !value.is_null())
    {
        return value.clone();
    }
    let text = result["content"]
        .as_array()
        .and_then(|content| content.first())
        .and_then(|content| content["text"].as_str())
        .unwrap_or_else(|| panic!("tool result has no structured content: {body}"));
    serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("tool result text was not JSON: {error}; body={body}"))
}

async fn initialize(app: &Router, token: &str, id: u64) -> (String, Value) {
    let response = mcp(
        app,
        Some(token),
        None,
        None,
        Some(id),
        "initialize",
        initialize_params(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let session_id = response
        .headers()
        .get("mcp-session-id")
        .expect("initialize response has MCP session ID")
        .to_str()
        .unwrap()
        .to_owned();
    let body = body_json(response).await;
    assert_rpc_success(&body);

    let response = mcp(
        app,
        Some(token),
        None,
        Some(&session_id),
        None,
        "notifications/initialized",
        json!({}),
    )
    .await;
    assert!(
        response.status().is_success(),
        "initialized notification failed: {}",
        response.status()
    );
    (session_id, body)
}

async fn tool_call(
    app: &Router,
    token: &str,
    session_id: &str,
    id: u64,
    name: &str,
    arguments: Value,
) -> Value {
    let response = mcp(
        app,
        Some(token),
        None,
        Some(session_id),
        Some(id),
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

async fn read_resource(app: &Router, token: &str, session_id: &str, id: u64, uri: &str) -> Value {
    let response = mcp(
        app,
        Some(token),
        None,
        Some(session_id),
        Some(id),
        "resources/read",
        json!({"uri": uri}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_rpc_success(&body);
    let text = body["result"]["contents"]
        .as_array()
        .and_then(|contents| contents.first())
        .and_then(|content| content["text"].as_str())
        .unwrap_or_else(|| panic!("resource result has no text: {body}"));
    serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("resource text was not JSON: {error}; body={body}"))
}

async fn wait_for_presence(room: &double_riichi_core::RoomHandle, participant_id: &str) {
    timeout(Duration::from_secs(2), async {
        loop {
            let snapshot = room.snapshot().await.unwrap();
            if snapshot
                .participants
                .iter()
                .find(|participant| participant.id.as_str() == participant_id)
                .is_some_and(|participant| participant.presence == Presence::Disconnected)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("MCP disconnect was not observed");
}

async fn room_with_mode(
    state: &ServerState,
    name: &str,
    mode: GameMode,
) -> double_riichi_core::RoomHandle {
    state
        .rooms()
        .create(RoomConfig::new(
            name,
            mode,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap()
}

async fn room_with_code(state: &ServerState, name: &str) -> double_riichi_core::RoomHandle {
    room_with_mode(state, name, GameMode::FourPlayerRedEast).await
}

#[tokio::test]
async fn live_mcp_auth_origin_and_session_ownership() {
    let (state, service, storage, token) = fixture("auth").await;
    let other = service.create("other", 1, "task14-other").await.unwrap();
    let other_token = other.secret().expose().to_owned();
    let app = server_router(state.clone());

    for (presented, origin, expected_status) in [
        (None, None, StatusCode::UNAUTHORIZED),
        (Some("driichi_invalid"), None, StatusCode::UNAUTHORIZED),
        (
            Some(token.as_str()),
            Some("http://evil.invalid"),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let response = mcp(
            &app,
            presented,
            origin,
            None,
            Some(1),
            "initialize",
            initialize_params(),
        )
        .await;
        assert_eq!(response.status(), expected_status);
    }

    let (session_id, _) = initialize(&app, &token, 2).await;
    let response = mcp(
        &app,
        Some(&other_token),
        None,
        Some(&session_id),
        Some(3),
        "tools/list",
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    service
        .revoke(
            service.authenticate(&token).unwrap().token_id(),
            2,
            "task14-revoke",
        )
        .await
        .unwrap();
    let response = mcp(
        &app,
        Some(&token),
        None,
        Some(&session_id),
        Some(4),
        "tools/list",
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_mcp_discovers_joins_reads_and_keeps_room_binding_permanent() {
    let (state, _service, storage, token) = fixture("surface").await;
    let first_room = room_with_code(&state, "MCP surface").await;
    let second_room = room_with_code(&state, "MCP other").await;
    let first_code = first_room.join_code().to_string();
    let second_code = second_room.join_code().to_string();
    let app = server_router(state.clone());
    let (session_id, _) = initialize(&app, &token, 10).await;

    let response = mcp(
        &app,
        Some(&token),
        None,
        Some(&session_id),
        Some(11),
        "tools/list",
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_rpc_success(&body);
    let mut tools: Vec<_> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    tools.sort();
    assert_eq!(
        tools,
        ["join_room", "leave_room", "submit_action", "wait_for_turn"]
    );

    let join = tool_call(
        &app,
        &token,
        &session_id,
        12,
        "join_room",
        json!({
            "room_code": first_code,
            "provider": "runner",
            "display_name": "MCP Runner"
        }),
    )
    .await;
    let join_value = tool_value(&join);
    let participant_id = join_value["participant_id"].as_str().unwrap().to_owned();
    let state_uri = join_value["state_uri"].as_str().unwrap().to_owned();
    let public_uri = join_value["public_state_uri"].as_str().unwrap().to_owned();
    let history_uri = join_value["history_uri"].as_str().unwrap().to_owned();
    assert_eq!(join_value["resumed"], false);

    let response = mcp(
        &app,
        Some(&token),
        None,
        Some(&session_id),
        Some(13),
        "resources/list",
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_rpc_success(&body);
    let resources = body["result"]["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 3);
    let mut resource_uris: Vec<_> = resources
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap().to_owned())
        .collect();
    resource_uris.sort();
    let mut expected_uris = vec![state_uri.clone(), public_uri.clone(), history_uri.clone()];
    expected_uris.sort();
    assert_eq!(resource_uris, expected_uris);

    let response = mcp(
        &app,
        Some(&token),
        None,
        Some(&session_id),
        Some(14),
        "resources/templates/list",
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_rpc_success(&body);
    assert_eq!(
        body["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let private = read_resource(&app, &token, &session_id, 15, &state_uri).await;
    assert_eq!(private["room_code"], first_code);
    assert_eq!(private["participant_id"], participant_id);
    assert_eq!(private["state_uri"], state_uri);
    let public = read_resource(&app, &token, &session_id, 16, &public_uri).await;
    assert_eq!(public["room_code"], first_code);
    assert_eq!(public["state_uri"], public_uri);
    let history = read_resource(&app, &token, &session_id, 17, &history_uri).await;
    assert_eq!(history["history_uri"], history_uri);
    assert_eq!(history["phase"], "Lobby");

    let wrong_room = tool_call(
        &app,
        &token,
        &session_id,
        18,
        "join_room",
        json!({
            "room_code": second_code,
            "provider": "runner",
            "display_name": "MCP Runner"
        }),
    )
    .await;
    assert_eq!(tool_value(&wrong_room)["code"], "session_already_bound");

    let left = tool_call(&app, &token, &session_id, 19, "leave_room", json!({})).await;
    assert_eq!(tool_value(&left)["left"], true);
    let after_leave = tool_call(
        &app,
        &token,
        &session_id,
        20,
        "join_room",
        json!({
            "room_code": second_code,
            "provider": "runner",
            "display_name": "MCP Runner"
        }),
    )
    .await;
    assert_eq!(tool_value(&after_leave)["code"], "session_already_bound");

    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_mcp_delete_and_shutdown_close_sessions_and_disconnect_participants() {
    let (state, _service, storage, token) = fixture("close").await;
    let room = room_with_code(&state, "MCP delete").await;
    let app = server_router(state.clone());
    let (session_id, _) = initialize(&app, &token, 30).await;
    let join = tool_call(
        &app,
        &token,
        &session_id,
        31,
        "join_room",
        json!({
            "room_code": room.join_code(),
            "provider": "runner",
            "display_name": "MCP Delete"
        }),
    )
    .await;
    let participant_id = tool_value(&join)["participant_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let response = mcp_delete(&app, &token, &session_id).await;
    assert!(
        response.status().is_success() || response.status() == StatusCode::NOT_FOUND,
        "unexpected MCP DELETE status: {}",
        response.status()
    );
    wait_for_presence(&room, &participant_id).await;

    let shutdown_room = room_with_code(&state, "MCP shutdown").await;
    let (shutdown_session, _) = initialize(&app, &token, 32).await;
    let join = tool_call(
        &app,
        &token,
        &shutdown_session,
        33,
        "join_room",
        json!({
            "room_code": shutdown_room.join_code(),
            "provider": "runner",
            "display_name": "MCP Shutdown"
        }),
    )
    .await;
    let shutdown_participant = tool_value(&join)["participant_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let waiting_app = app.clone();
    let waiting_token = token.clone();
    let waiting_session = shutdown_session.clone();
    let waiting = tokio::spawn(async move {
        mcp(
            &waiting_app,
            Some(&waiting_token),
            None,
            Some(&waiting_session),
            Some(34),
            "tools/call",
            json!({
                "name": "wait_for_turn",
                "arguments": {"after_revision": 0, "timeout_seconds": 30}
            }),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    state.begin_shutdown();
    let response = timeout(Duration::from_secs(2), waiting)
        .await
        .expect("MCP wait did not close on shutdown")
        .unwrap();
    assert!(
        response.status().is_success() || response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "unexpected shutdown response: {}",
        response.status()
    );
    wait_for_presence(&shutdown_room, &shutdown_participant).await;
    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_mcp_wait_observes_selection_when_event_wins_the_wait_race() {
    let (state, _service, storage, token) = fixture("wait-race").await;
    let room = room_with_code(&state, "MCP wait").await;
    let app = server_router(state.clone());
    let (session_id, _) = initialize(&app, &token, 40).await;
    let join = tool_call(
        &app,
        &token,
        &session_id,
        41,
        "join_room",
        json!({
            "room_code": room.join_code(),
            "provider": "runner",
            "display_name": "MCP Wait"
        }),
    )
    .await;
    let participant_id = tool_value(&join)["participant_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = room.snapshot().await.unwrap().revision;

    let waiting_app = app.clone();
    let waiting_token = token.clone();
    let waiting_session = session_id.clone();
    let waiting = tokio::spawn(async move {
        mcp(
            &waiting_app,
            Some(&waiting_token),
            None,
            Some(&waiting_session),
            Some(42),
            "tools/call",
            json!({
                "name": "wait_for_turn",
                "arguments": {"after_revision": before, "timeout_seconds": 2}
            }),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    room.send(RoomCommand::select(ParticipantId::new(participant_id)))
        .await
        .unwrap();
    let response = timeout(Duration::from_secs(2), waiting)
        .await
        .expect("MCP wait race timed out")
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let value = tool_value(&body);
    assert_eq!(value["reason"], "selected");
    assert!(value["revision"].as_u64().unwrap() > before);

    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_mcp_join_wait_uses_snapshot_barrier() {
    let (state, _service, storage, token) = fixture("join-barrier").await;
    let room = room_with_code(&state, "MCP join barrier").await;
    let app = server_router(state.clone());
    let (session_id, _) = initialize(&app, &token, 50).await;
    let joined = tool_call(
        &app,
        &token,
        &session_id,
        51,
        "join_room",
        json!({
            "room_code": room.join_code(),
            "provider": "runner",
            "display_name": "MCP Barrier"
        }),
    )
    .await;
    assert!(!tool_value(&joined)["participant_id"].is_null());

    let waited = tool_call(
        &app,
        &token,
        &session_id,
        52,
        "wait_for_turn",
        json!({"after_revision": 0, "timeout_seconds": 1}),
    )
    .await;
    let value = tool_value(&waited);
    assert_eq!(value["reason"], "deselected");
    assert!(
        value["revision"]
            .as_u64()
            .is_some_and(|revision| revision > 0)
    );

    state.shutdown().await;
    storage.close().await;
}

#[derive(Clone)]
struct ProtocolBot {
    updates: Arc<std::sync::Mutex<Vec<String>>>,
}

#[allow(clippy::manual_async_fn)]
impl ClientHandler for ProtocolBot {
    fn on_resource_updated(
        &self,
        params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        let updates = self.updates.clone();
        async move {
            updates
                .lock()
                .expect("protocol bot update lock poisoned")
                .push(params.uri);
        }
    }

    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("task14-protocol-bot", "1"),
        )
    }
}

async fn protocol_tool(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    name: &'static str,
    arguments: Value,
) -> Value {
    let arguments = arguments
        .as_object()
        .cloned()
        .expect("MCP tool arguments are an object");
    let result = timeout(
        REQUEST_TIMEOUT,
        peer.call_tool(CallToolRequestParams::new(name).with_arguments(arguments)),
    )
    .await
    .expect("protocol MCP tool timed out")
    .expect("protocol MCP tool failed");
    assert_ne!(result.is_error, Some(true), "MCP tool returned an error");
    if let Some(value) = result.structured_content {
        return value;
    }
    let encoded = serde_json::to_value(result).expect("tool result serializes");
    let text = encoded["content"]
        .as_array()
        .and_then(|content| content.first())
        .and_then(|content| content["text"].as_str())
        .expect("tool result has JSON content");
    serde_json::from_str(text).expect("tool content is JSON")
}

async fn protocol_resource(peer: &rmcp::Peer<rmcp::RoleClient>, uri: &str) -> Value {
    let result = timeout(
        REQUEST_TIMEOUT,
        peer.read_resource(ReadResourceRequestParams::new(uri)),
    )
    .await
    .expect("protocol MCP resource read timed out")
    .expect("protocol MCP resource read failed");
    let encoded = serde_json::to_value(result).expect("resource result serializes");
    let text = encoded["contents"]
        .as_array()
        .and_then(|contents| contents.first())
        .and_then(|content| content["text"].as_str())
        .expect("resource result has JSON text");
    serde_json::from_str(text).expect("resource text is JSON")
}

fn assert_no_private_data(value: &Value, participant_id: &str) {
    fn assert_no_redacted_keys(value: &Value) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    assert!(
                        !matches!(
                            key.as_str(),
                            "tehais" | "hands" | "wall" | "private_state" | "raw_state"
                        ),
                        "private history field leaked through MCP: {key}"
                    );
                    assert_no_redacted_keys(value);
                }
            }
            Value::Array(values) => values.iter().for_each(assert_no_redacted_keys),
            _ => {}
        }
    }

    if let Some(players) = value["players"].as_array() {
        for player in players {
            if player["participant_id"] == participant_id {
                continue;
            }
            assert!(
                player.get("hand").is_none_or(Value::is_null),
                "opponent concealed hand leaked through MCP: {player}"
            );
        }
    }
    assert_no_redacted_keys(value);
}

fn is_post_match(value: &Value) -> bool {
    value["phase"].as_str() == Some("PostMatch") || value["phase"].get("PostMatch").is_some()
}

#[tokio::test]
async fn live_mcp_bridge_protocol_bot_completes_resource_driven_match() {
    const WAIT_SECONDS: u64 = 1;
    const MAX_AGENT_STEPS: usize = 512;
    const MATCH_TIMEOUT: Duration = Duration::from_secs(60);

    timeout(MATCH_TIMEOUT, async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (state, _service, storage, token) = fixture_with_origin("bridge-e2e", &origin).await;
        let room = room_with_mode(&state, "MCP bridge E2E", GameMode::FourPlayerRedHalf).await;
        let app = server_router(state.clone());
        let (server_stop, server_stop_rx) = oneshot::channel();
        let server_task = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(async {
                    let _ = server_stop_rx.await;
                })
                .await
                .unwrap();
        });

        let (bridge_io, bot_io) = tokio::io::duplex(64 * 1024);
        let bridge = tokio::spawn(double_riichi_mcp::run_with_token_and_transport(
            double_riichi_mcp::BridgeConfig {
                server: Url::parse(&format!("{origin}/mcp")).unwrap(),
            },
            token,
            bridge_io,
        ));
        let updates = Arc::new(std::sync::Mutex::new(Vec::new()));
        let bot = timeout(
            REQUEST_TIMEOUT,
            rmcp::serve_client(
                ProtocolBot {
                    updates: updates.clone(),
                },
                bot_io,
            ),
        )
        .await
        .expect("stdio protocol bot initialization timed out")
        .expect("stdio protocol bot initialization failed");
        let peer = bot.peer().clone();

        let tools = timeout(REQUEST_TIMEOUT, peer.list_tools(None))
            .await
            .expect("tools/list timed out")
            .unwrap();
        let mut tool_names: Vec<_> = tools
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        tool_names.sort();
        assert_eq!(
            tool_names,
            ["join_room", "leave_room", "submit_action", "wait_for_turn"]
        );

        let templates = timeout(REQUEST_TIMEOUT, peer.list_resource_templates(None))
            .await
            .expect("resources/templates/list timed out")
            .unwrap();
        let mut template_uris: Vec<_> = templates
            .resource_templates
            .iter()
            .map(|template| template.uri_template.clone())
            .collect();
        template_uris.sort();
        assert_eq!(
            template_uris,
            [
                "riichi://rooms/{code}/history",
                "riichi://rooms/{code}/participants/{participant_id}/state",
                "riichi://rooms/{code}/public-state",
            ]
        );

        let joined = protocol_tool(
            &peer,
            "join_room",
            json!({
                "room_code": room.join_code(),
                "provider": "runner",
                "display_name": "MCP Bridge Runner"
            }),
        )
        .await;
        let participant_id = joined["participant_id"].as_str().unwrap().to_owned();
        let state_uri = joined["state_uri"].as_str().unwrap().to_owned();
        let public_uri = joined["public_state_uri"].as_str().unwrap().to_owned();
        let history_uri = joined["history_uri"].as_str().unwrap().to_owned();

        let resources = timeout(REQUEST_TIMEOUT, peer.list_resources(None))
            .await
            .expect("resources/list timed out")
            .unwrap();
        let mut resource_uris: Vec<_> = resources
            .resources
            .iter()
            .map(|resource| resource.uri.clone())
            .collect();
        resource_uris.sort();
        let mut expected_uris = vec![state_uri.clone(), public_uri.clone(), history_uri.clone()];
        expected_uris.sort();
        assert_eq!(resource_uris, expected_uris);

        #[allow(deprecated)]
        for _ in 0..4 {
            timeout(
                REQUEST_TIMEOUT,
                peer.subscribe(SubscribeRequestParams::new(state_uri.clone())),
            )
            .await
            .expect("repeated subscribe timed out")
            .unwrap();
            timeout(
                REQUEST_TIMEOUT,
                peer.unsubscribe(UnsubscribeRequestParams::new(state_uri.clone())),
            )
            .await
            .expect("repeated unsubscribe timed out")
            .unwrap();
        }

        #[allow(deprecated)]
        let _subscriptions = vec![
            timeout(
                REQUEST_TIMEOUT,
                peer.subscribe(SubscribeRequestParams::new(state_uri.clone())),
            )
            .await
            .expect("private resource subscription timed out")
            .unwrap(),
            timeout(
                REQUEST_TIMEOUT,
                peer.subscribe(SubscribeRequestParams::new(public_uri.clone())),
            )
            .await
            .expect("public resource subscription timed out")
            .unwrap(),
            timeout(
                REQUEST_TIMEOUT,
                peer.subscribe(SubscribeRequestParams::new(history_uri.clone())),
            )
            .await
            .expect("history resource subscription timed out")
            .unwrap(),
        ];

        let initial_private = protocol_resource(&peer, &state_uri).await;
        let initial_public = protocol_resource(&peer, &public_uri).await;
        let initial_history = protocol_resource(&peer, &history_uri).await;
        assert_eq!(initial_private["room_code"], room.join_code().to_string());
        assert_eq!(initial_public["room_code"], room.join_code().to_string());
        assert_eq!(initial_history["history_uri"], history_uri);
        assert_no_private_data(&initial_private, &participant_id);
        assert_no_private_data(&initial_public, &participant_id);
        assert_no_private_data(&initial_history, &participant_id);
        let mut last_revision = initial_private["revision"].as_u64().unwrap();
        let mut wait_after_revision = last_revision;
        assert_eq!(initial_public["revision"].as_u64(), Some(last_revision));
        assert_eq!(initial_history["revision"].as_u64(), Some(last_revision));

        // Setup is the only direct Room control in this test; all play below is MCP.
        room.send(RoomCommand::set_time_control(TimeControl::Unlimited))
            .await
            .unwrap();
        room.send(RoomCommand::select(ParticipantId::new(
            participant_id.clone(),
        )))
        .await
        .unwrap();
        room.send(RoomCommand::fill_with_bots()).await.unwrap();
        room.send(RoomCommand::start()).await.unwrap();

        let mut saw_playing = false;
        let mut saw_game_ended = false;
        for step in 0..MAX_AGENT_STEPS {
            let after_revision = wait_after_revision;
            let wait_started = Instant::now();
            let waited = protocol_tool(
                &peer,
                "wait_for_turn",
                json!({
                    "after_revision": after_revision,
                    "timeout_seconds": WAIT_SECONDS
                }),
            )
            .await;
            assert!(
                wait_started.elapsed() <= REQUEST_TIMEOUT,
                "wait_for_turn exceeded bounded request timeout at step {step}"
            );
            let wait_revision = waited["revision"].as_u64().unwrap();
            let reason = waited["reason"].as_str().unwrap();
            assert!(
                wait_revision >= after_revision,
                "wait_for_turn revision regressed from {after_revision} to {wait_revision}"
            );
            if reason != "timeout" {
                assert!(
                    wait_revision > after_revision,
                    "non-timeout wait did not advance revision: {waited}"
                );
            }
            assert_eq!(waited["state_uri"], state_uri);

            tokio::task::yield_now().await;
            let private = protocol_resource(&peer, &state_uri).await;
            let public = protocol_resource(&peer, &public_uri).await;
            let history = protocol_resource(&peer, &history_uri).await;
            let private_revision = private["revision"].as_u64().unwrap();
            let public_revision = public["revision"].as_u64().unwrap();
            let history_revision = history["revision"].as_u64().unwrap();
            assert!(
                private_revision >= last_revision,
                "private revision regressed at step {step}"
            );
            assert!(
                public_revision >= private_revision,
                "public revision regressed at step {step}"
            );
            assert!(
                history_revision >= public_revision,
                "history revision regressed at step {step}"
            );
            last_revision = history_revision;
            assert_no_private_data(&private, &participant_id);
            assert_no_private_data(&public, &participant_id);
            assert_no_private_data(&history, &participant_id);

            if is_post_match(&private) {
                assert_eq!(reason, "game_ended");
                assert!(!history["result"].is_null(), "Post-Match has no result");
                let projection = &history["history"];
                let current_events = projection["current_kyoku"]["events"]
                    .as_array()
                    .expect("Post-Match keeps current Kyoku events");
                assert!(!current_events.is_empty());
                assert!(current_events.len() < 1_000);
                let summaries = projection["previous_kyoku"]
                    .as_array()
                    .expect("Post-Match keeps prior Kyoku summaries");
                assert!(!summaries.is_empty(), "Match should retain a prior summary");
                assert!(summaries.len() < 1_000);
                assert_eq!(&history["events"], &projection["current_kyoku"]["events"]);
                assert_eq!(&history["summaries"], &projection["previous_kyoku"]);
                assert_no_private_data(&history, &participant_id);
                saw_game_ended = true;
                break;
            }
            if private["phase"].get("Playing").is_some() {
                saw_playing = true;
            }
            let actions = private["legal_actions"]
                .as_array()
                .expect("private state has legal_actions");
            if actions.is_empty() {
                wait_after_revision = history_revision;
                continue;
            }
            assert_eq!(private["is_my_turn"], true);
            let action_id = actions[0]["action_id"]
                .as_str()
                .expect("legal action has action_id")
                .to_owned();
            let submit_arguments = json!({"action_id": action_id});
            assert_eq!(submit_arguments.as_object().unwrap().len(), 1);
            let submitted = protocol_tool(&peer, "submit_action", submit_arguments).await;
            assert_eq!(submitted["accepted"], true);
            let submitted_revision = submitted["revision"].as_u64().unwrap();
            assert!(
                submitted_revision >= last_revision,
                "submit_action revision regressed at step {step}"
            );
            last_revision = submitted_revision;
            wait_after_revision = submitted_revision.saturating_sub(1);
        }
        assert!(saw_playing, "protocol bot never observed a Playing state");
        assert!(
            saw_game_ended,
            "protocol bot did not reach game_ended/PostMatch within bound"
        );
        assert!(
            !updates.lock().unwrap().is_empty(),
            "stdio protocol bot received no resource update notifications"
        );
        let before_shutdown = updates.lock().unwrap().len();
        state.begin_shutdown();
        timeout(Duration::from_secs(2), async {
            loop {
                let updates = updates.lock().unwrap();
                let shutdown_updates = &updates[before_shutdown..];
                if [
                    state_uri.as_str(),
                    public_uri.as_str(),
                    history_uri.as_str(),
                ]
                .iter()
                .all(|uri| shutdown_updates.iter().any(|update| update == uri))
                {
                    break;
                }
                drop(updates);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("server shutdown resource notifications were not delivered");

        let final_snapshot = room.snapshot().await.unwrap();
        assert!(matches!(
            final_snapshot.phase,
            double_riichi_core::RoomPhase::PostMatch(_)
        ));
        assert!(final_snapshot.result.is_some());

        timeout(REQUEST_TIMEOUT, bot.cancel())
            .await
            .expect("stdio protocol bot shutdown timed out")
            .unwrap();
        let bridge_result = timeout(REQUEST_TIMEOUT, bridge)
            .await
            .expect("bridge shutdown timed out")
            .unwrap();
        assert!(bridge_result.is_ok(), "bridge failed: {bridge_result:?}");
        state.shutdown().await;
        server_stop.send(()).unwrap();
        timeout(REQUEST_TIMEOUT, server_task)
            .await
            .expect("Axum test server shutdown timed out")
            .unwrap();
        storage.close().await;
    })
    .await
    .expect("bounded MCP bridge match timed out");
}
