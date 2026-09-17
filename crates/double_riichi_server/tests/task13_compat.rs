use std::{
    fs,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use double_riichi_core::{GameMode, RoomConfig, RoomRegistry};
use double_riichi_replay::parse_mjson;
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerLimits, ServerState, Storage,
    hash_password, server_router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sqlx::Row;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as WsMessage, client::IntoClientRequest},
};
use tower::ServiceExt;

fn root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "double-riichi-task13-{label}-{}",
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
    let token = service.create("runner", 1, "task13").await.unwrap();
    let raw = token.secret().expose().to_owned();
    let state = Arc::new(
        ServerState::for_tests(
            "http://127.0.0.1:3000",
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

async fn serve(state: Arc<ServerState>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            server_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (format!("ws://{address}"), task)
}

async fn ws(
    base: &str,
    path: &str,
    token: Option<&str>,
    origin: Option<&str>,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Error,
> {
    let mut request = format!("{base}{path}").into_client_request().unwrap();
    if let Some(token) = token {
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
    }
    Ok(connect_async(request).await?.0)
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

async fn play_bot(
    mut socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    illegal_first: bool,
) -> (bool, bool, Option<String>) {
    let mut saw_end = false;
    let mut saw_ack = false;
    let mut validation = None;
    let mut sent_illegal = false;
    let mut retry = None;
    loop {
        let message = match tokio::time::timeout(Duration::from_secs(45), socket.next()).await {
            Ok(Some(Ok(message))) => message,
            _ => break,
        };
        let WsMessage::Text(text) = message else {
            if let WsMessage::Close(_frame) = message {
                break;
            }
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        match value["type"].as_str() {
            Some("request_action") => {
                let request_id = value["request_id"].clone();
                let actions = value["possible_actions"].as_array().unwrap();
                let mut action = actions.first().cloned().unwrap();
                if illegal_first && !sent_illegal {
                    let mut legal_retry = action.clone();
                    if let Some(object) = legal_retry.as_object_mut() {
                        object.insert("request_id".into(), request_id.clone());
                    }
                    retry = Some(legal_retry);
                    action = json!({"type":"none"});
                    sent_illegal = true;
                }
                if let Some(object) = action.as_object_mut() {
                    object.insert("request_id".into(), request_id);
                }
                socket
                    .send(WsMessage::Text(action.to_string().into()))
                    .await
                    .unwrap();
            }
            Some("action_ack") => {
                if value["status"] == "accepted" || value["status"] == "defaulted" {
                    saw_ack = true;
                }
                if value["status"] == "rejected"
                    && let Some(action) = retry.take()
                {
                    socket
                        .send(WsMessage::Text(action.to_string().into()))
                        .await
                        .unwrap();
                }
            }
            Some("end_game") => saw_end = true,
            Some("validation_result") => {
                validation = value["passed"].as_bool().map(|passed| {
                    if passed {
                        "passed".to_owned()
                    } else {
                        value["reason"].as_str().unwrap_or("failed").to_owned()
                    }
                });
            }
            _ => {}
        }
        if saw_end && validation.is_some() {
            break;
        }
    }
    (saw_end, saw_ack, validation)
}

#[tokio::test]
async fn live_compat_upgrade_checks_auth_origin_and_revocation() {
    let (state, service, storage, raw) = fixture("auth").await;
    let (base, server) = serve(state.clone()).await;
    assert!(ws(&base, "/ws/ranked", None, None).await.is_err());
    assert!(
        ws(&base, "/ws/ranked", Some("driichi_invalid"), None)
            .await
            .is_err()
    );
    assert!(
        ws(&base, "/ws/ranked", Some(&raw), Some("http://evil.invalid"),)
            .await
            .is_err()
    );

    let mut waiting = ws(&base, "/ws/ranked", Some(&raw), None).await.unwrap();
    service
        .revoke("tok_missing", 2, "task13-revoke-missing")
        .await
        .err();
    let record = service.list().await.unwrap().pop().unwrap();
    service
        .revoke(record.token_id(), 2, "task13-revoke")
        .await
        .unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(2), waiting.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match closed {
        WsMessage::Close(Some(frame)) => {
            assert_eq!(
                frame.code,
                tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(4006)
            );
            assert_eq!(frame.reason, "token_revoked");
        }
        other => panic!("expected token_revoked close, got {other:?}"),
    }
    server.abort();
    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_room_join_is_strict_for_unknown_fields_rooms_and_rate_limits() {
    let (base_state, service, storage, raw) = fixture("join-boundaries").await;
    let mut limits = ServerLimits::default();
    limits.participant_creation_limit = 2;
    let state = Arc::new((*base_state).clone().with_limits(limits));
    let room = state
        .rooms()
        .create(RoomConfig::new(
            "Room",
            GameMode::FourPlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    let app = server_router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{}/agents/join", room.join_code()))
                .header("authorization", format!("Bearer {raw}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"display_name":"agent", "unexpected":true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response_json(response).await["code"], "invalid_request");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/rooms/MISSING/agents/join")
                .header("authorization", format!("Bearer {raw}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"display_name":"agent"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response_json(response).await["code"], "room_not_found");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{}/agents/join", room.join_code()))
                .header("authorization", format!("Bearer {raw}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"display_name":"agent"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response_json(response).await["code"], "rate_limited");

    state.shutdown().await;
    storage.close().await;
    drop(service);
}

#[tokio::test]
async fn live_room_mjai_enforces_token_ownership_replacement_and_three_player_limit() {
    let (state, service, storage, raw) = fixture("room").await;
    let other = service.create("other", 1, "task13-other").await.unwrap();
    let other_raw = other.secret().expose().to_owned();
    let room = state
        .rooms()
        .create(RoomConfig::new(
            "Room",
            GameMode::FourPlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    let join_code = room.join_code().to_string();
    let response = server_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{join_code}/agents/join"))
                .header("authorization", format!("Bearer {raw}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"display_name":"agent"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let participant_id = serde_json::from_slice::<Value>(&body).unwrap()["participant_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("/ws/v1/rooms/{join_code}/mjai?participant_id={participant_id}");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let serve_state = state.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            server_router(serve_state)
                .into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let base = format!("ws://{address}");
    let mut first = ws(&base, &path, Some(&raw), None).await.unwrap();
    first
        .send(WsMessage::Text(r#"{"type":"none"}"#.into()))
        .await
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), first.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        WsMessage::Text(_)
    ));
    let mut wrong = ws(&base, &path, Some(&other_raw), None).await.unwrap();
    let close = tokio::time::timeout(Duration::from_secs(2), wrong.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(close, WsMessage::Close(Some(frame)) if frame.reason == "session_expired"));
    first
        .send(WsMessage::Text(r#"{"type":"none"}"#.into()))
        .await
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), first.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        WsMessage::Text(_)
    ));
    let mut replacement = ws(&base, &path, Some(&raw), None).await.unwrap();
    replacement
        .send(WsMessage::Text(r#"{"type":"none"}"#.into()))
        .await
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), replacement.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        WsMessage::Text(_)
    ));
    let old = tokio::time::timeout(Duration::from_secs(2), first.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(old, WsMessage::Close(Some(frame)) if frame.reason == "connected_elsewhere"));
    replacement.close(None).await.unwrap();
    server.abort();

    let three = state
        .rooms()
        .create(RoomConfig::new(
            "Three",
            GameMode::ThreePlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    let response = server_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{}/agents/join", three.join_code()))
                .header("authorization", format!("Bearer {raw}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"display_name":"agent"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_ranked_bot_completes_and_persists_mjson_metadata() {
    let (state, _service, storage, raw) = fixture("ranked").await;
    let (base, server) = serve(state.clone()).await;
    let socket = ws(&base, "/ws/ranked", Some(&raw), None).await.unwrap();
    let (saw_end, saw_ack, validation) = play_bot(socket, false).await;
    assert!(saw_end, "production-style bot did not receive end_game");
    assert!(
        saw_ack,
        "production-style bot did not receive an accepted ack"
    );
    assert!(validation.is_none());
    let row = sqlx::query("SELECT status, source, game_mode, replay_path, file_size FROM matches")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("status"), "completed");
    assert_eq!(row.get::<String, _>("source"), "ranked");
    assert_eq!(row.get::<String, _>("game_mode"), "4p-red-half");
    let relative = row.get::<String, _>("replay_path");
    let path = storage.replay_root().join(&relative);
    assert!(path.is_file());
    assert!(row.get::<i64, _>("file_size") > 0);
    let events = parse_mjson(fs::read_to_string(path).unwrap()).unwrap();
    assert!(matches!(
        events.last(),
        Some(double_riichi_core::GameEvent::EndGame)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM match_players")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        4
    );
    server.abort();
    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn live_validate_reports_illegal_action_but_completes_match() {
    let (state, _service, storage, raw) = fixture("validate").await;
    let (base, server) = serve(state.clone()).await;
    let socket = ws(&base, "/ws/validate", Some(&raw), None).await.unwrap();
    let (saw_end, saw_ack, validation) = play_bot(socket, true).await;
    assert!(saw_end);
    assert!(saw_ack);
    assert_eq!(validation.as_deref(), Some("illegal_action"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    server.abort();
    state.shutdown().await;
    storage.close().await;
}

#[tokio::test]
async fn shutdown_closes_waiters_and_rejects_new_compat_admission() {
    let (state, _service, storage, raw) = fixture("shutdown-admission").await;
    let (base, server) = serve(state.clone()).await;
    let mut waiting = ws(&base, "/ws/ranked", Some(&raw), None).await.unwrap();

    state.begin_shutdown();
    let response = server_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response_json(response).await["code"],
        "server_shutting_down"
    );
    assert!(ws(&base, "/ws/ranked", Some(&raw), None).await.is_err());

    state.shutdown().await;
    let close = tokio::time::timeout(Duration::from_secs(2), waiting.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(close, WsMessage::Close(Some(frame))
        if frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(4006)
            && frame.reason == "server_shutdown"));
    server.abort();
    storage.close().await;
}

#[tokio::test]
async fn health_reports_active_compat_count_while_ranked_match_waits() {
    let (state, _service, storage, raw) = fixture("health-count").await;
    let app = server_router(state.clone());
    let login = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/login")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin","password":"correct horse battery staple"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login.headers()["set-cookie"].to_str().unwrap().to_owned();
    let (base, server) = serve(state.clone()).await;
    let mut sockets = Vec::new();
    for _ in 0..4 {
        sockets.push(ws(&base, "/ws/ranked", Some(&raw), None).await.unwrap());
    }

    let (status, body) = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let response = server_router(state.clone())
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/health")
                        .header("cookie", &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let body = response_json(response).await;
            if body["active_compat_matches"].as_u64() == Some(1) {
                break (status, body);
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active_rooms"], 0);
    assert_eq!(body["active_room_matches"], 0);
    assert_eq!(body["active_compat_matches"], 1);

    state.shutdown().await;
    drop(sockets);
    server.abort();
    storage.close().await;
}

#[tokio::test]
async fn health_probes_database_and_replay_storage() {
    let (state, _service, storage, _raw) = fixture("health").await;
    let app = server_router(state.clone());
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/login")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin","password":"correct horse battery staple"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login.headers()["set-cookie"].to_str().unwrap().to_owned();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let healthy = serde_json::from_slice::<Value>(
        &axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(healthy["database"], "ok");
    assert_eq!(healthy["replay_storage"], "ok");

    fs::remove_dir_all(storage.replay_root()).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let degraded = serde_json::from_slice::<Value>(
        &axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(degraded["database"], "ok");
    assert_eq!(degraded["replay_storage"], "degraded");

    state.begin_shutdown();
    let response = server_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    state.shutdown().await;
    storage.close().await;
}
