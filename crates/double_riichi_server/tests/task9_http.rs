use std::sync::Arc;

use axum::{body::Body, http::Request};
use double_riichi_core::{GameMode, RoomConfig, RoomRegistry};
use double_riichi_server::{AdminAuthenticator, ServerState, hash_password, server_router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as WsMessage, client::IntoClientRequest},
};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    let password_hash = hash_password("correct horse battery staple").unwrap();
    let admin = Arc::new(AdminAuthenticator::new("admin", password_hash).unwrap());
    let state = Arc::new(ServerState::for_tests(
        "http://127.0.0.1:3000",
        admin,
        RoomRegistry::with_max_rooms(8),
    ));
    server_router(state)
}

async fn body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 128 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn missing_public_room_is_rfc_problem_with_server_ulid_request_id() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/rooms/123456")
                .header("x-request-id", "client-supplied")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    let request_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(request_id.len(), 26);
    assert!(
        request_id
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
    );
    assert_ne!(request_id, "client-supplied");
    let problem = body(response).await;
    assert_eq!(problem["type"], "about:blank");
    assert_eq!(problem["title"], "Room not found");
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["code"], "room_not_found");
    assert_eq!(problem["request_id"], request_id);
    assert!(!problem.to_string().contains("/api/v1/rooms/123456"));
}

#[tokio::test]
async fn admin_login_issues_strict_cookie_and_room_create_is_visible_only_to_admin() {
    let app = test_app();
    let response = app
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
    assert_eq!(response.status(), 200);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Path=/api/v1/admin"));
    assert!(!cookie.contains("Secure"));
    assert!(!body(response).await.to_string().contains("password"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Task 9","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let room = body(response).await;
    let join_code = room["join_code"].as_str().unwrap().to_owned();
    assert_eq!(join_code.len(), 6);
    assert!(
        join_code
            .chars()
            .all(|character| character.is_ascii_digit())
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/rooms/{join_code}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let public = body(response).await;
    assert_eq!(public["room_name"], "Task 9");
    assert!(public.get("participants").is_none());
    assert!(public.get("participant_ids").is_none());
}

#[tokio::test]
async fn malformed_admin_json_is_rejected_without_mutating_a_room() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/login")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username":"admin","password":"correct horse battery staple","extra":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    let problem = body(response).await;
    assert_eq!(problem["code"], "invalid_request");
}

#[tokio::test]
async fn custom_method_rejections_are_problem_details() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/rooms/123456")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 405);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(body(response).await["code"], "method_not_allowed");
}

#[tokio::test]
async fn trailing_slash_public_origin_accepts_browser_origin_without_slash() {
    let password_hash = hash_password("correct horse battery staple").unwrap();
    let admin = Arc::new(AdminAuthenticator::new("admin", password_hash).unwrap());
    let state = Arc::new(ServerState::for_tests(
        "http://127.0.0.1:3000/",
        admin,
        RoomRegistry::with_max_rooms(8),
    ));
    let response = server_router(state)
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
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn patch_mode_and_participant_limit_is_atomic() {
    let app = test_app();
    let response = app
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
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Atomic","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let room = body(response).await;
    let join_code = room["join_code"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"game_mode":"3p-red-east","participant_limit":3}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let patched = body(response).await;
    assert_eq!(patched["game_mode"], "3p-red-east");
}

#[tokio::test]
async fn room_projection_rejects_permanent_auto_after_leave() {
    let rooms = RoomRegistry::with_max_rooms(1);
    let handle = rooms
        .create(RoomConfig::new(
            "Projection",
            GameMode::FourPlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    let participant =
        double_riichi_core::Participant::new("h", "h", double_riichi_core::ParticipantKind::Human);
    handle
        .send(double_riichi_core::RoomCommand::join(participant))
        .await
        .unwrap();
    handle
        .send(double_riichi_core::RoomCommand::select_with_character(
            "h",
            "player-red",
        ))
        .await
        .unwrap();
    handle
        .send(double_riichi_core::RoomCommand::fill_with_bots())
        .await
        .unwrap();
    handle
        .send(double_riichi_core::RoomCommand::set_ready(
            "h",
            vec!["player-red".into(), "tsumogiri-bot".into()],
        ))
        .await
        .unwrap();
    handle
        .send(double_riichi_core::RoomCommand::start())
        .await
        .unwrap();
    handle
        .send(double_riichi_core::RoomCommand::leave("h"))
        .await
        .unwrap();
    assert!(handle.projection("h").await.is_err());
}

#[tokio::test]
async fn live_human_upgrade_authenticates_cookie_sends_snapshot_and_replaces_connection() {
    let admin = Arc::new(
        AdminAuthenticator::new(
            "admin",
            hash_password("correct horse battery staple").unwrap(),
        )
        .unwrap(),
    );
    let state = Arc::new(ServerState::for_tests(
        "http://127.0.0.1:3000",
        admin,
        RoomRegistry::with_max_rooms(1),
    ));
    let room = state
        .rooms()
        .create(RoomConfig::new(
            "Live",
            GameMode::FourPlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    let join_code = room.join_code().to_string();
    let app = server_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{join_code}/join"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"nickname":"Live","character_id":"player-red"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned();

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
    let uri = format!("ws://{address}/ws/v1/rooms/{join_code}/human");
    let mut request = uri.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", "http://127.0.0.1:3000".parse().unwrap());
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut first, _) = connect_async(request).await.unwrap();
    let first_snapshot = tokio::time::timeout(std::time::Duration::from_secs(2), first.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let WsMessage::Text(first_snapshot) = first_snapshot else {
        panic!("expected snapshot")
    };
    assert_eq!(
        serde_json::from_str::<Value>(&first_snapshot).unwrap()["type"],
        "snapshot"
    );

    let mut replacement_request = uri.into_client_request().unwrap();
    replacement_request
        .headers_mut()
        .insert("origin", "http://127.0.0.1:3000".parse().unwrap());
    replacement_request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut second, _) = connect_async(replacement_request).await.unwrap();
    second
        .send(WsMessage::Text(r#"{"type":"leave"}"#.into()))
        .await
        .unwrap();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match first.next().await {
                Some(Ok(WsMessage::Close(Some(frame)))) => break frame,
                Some(Ok(_)) => continue,
                other => panic!("expected replacement close, got {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        frame.code,
        tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(4001)
    ));
    assert_eq!(frame.reason, "connected_elsewhere");
    let leave_frame = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match second.next().await {
                Some(Ok(WsMessage::Close(Some(frame)))) => break frame,
                Some(Ok(_)) => continue,
                other => panic!("expected leave close, got {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        leave_frame.code,
        tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(4006)
    ));
    assert_eq!(leave_frame.reason, "session_expired");
    server.abort();
}
