use std::sync::Arc;

use axum::{body::Body, http::Request};
use double_riichi_core::RoomRegistry;
use double_riichi_server::{AdminAuthenticator, ServerState, hash_password, server_router};
use serde_json::{Value, json};
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
