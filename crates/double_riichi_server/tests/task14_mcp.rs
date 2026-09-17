use std::{
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
    GameMode, ParticipantId, Presence, RoomCommand, RoomConfig, RoomRegistry,
};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerState, Storage, hash_password,
    server_router,
};
use serde_json::{Value, json};
use tokio::time::timeout;
use tower::ServiceExt;

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
    let storage = Arc::new(Storage::connect(&root(label)).await.unwrap());
    let service = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::empty()),
    ));
    let token = service.create("runner", 1, "task14").await.unwrap();
    let raw = token.secret().expose().to_owned();
    let state = Arc::new(
        ServerState::for_tests(
            ORIGIN,
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

async fn room_with_code(state: &ServerState, name: &str) -> double_riichi_core::RoomHandle {
    state
        .rooms()
        .create(RoomConfig::new(
            name,
            GameMode::FourPlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap()
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
