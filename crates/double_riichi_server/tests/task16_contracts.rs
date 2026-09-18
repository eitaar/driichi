use std::{
    fs,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{body::Body, http::Request};
use double_riichi_core::{CharacterCatalog, GameMode, RoomConfig, RoomRegistry};
use double_riichi_server::{
    AdminAuthenticator, BUILD_COMMIT, BUILD_VERSION, RuntimeConfig, ServerState, TracingFormat,
    hash_password, server_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const OPENAPI_YAML: &str = include_str!("../../../spec/openapi.yaml");

fn app(api_docs: bool) -> Arc<ServerState> {
    let password_hash = hash_password("correct horse battery staple").unwrap();
    let admin = Arc::new(AdminAuthenticator::new("admin", password_hash).unwrap());
    let state = ServerState::for_tests(
        "http://127.0.0.1:3000",
        admin,
        RoomRegistry::with_max_rooms(8),
    )
    .with_api_docs_enabled(api_docs);
    Arc::new(state)
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 128 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn admin_cookie(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/login")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "password": "correct horse battery staple"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn raw_openapi_is_disabled_and_admin_authenticated() {
    let disabled = server_router(app(false));
    let response = disabled
        .oneshot(
            Request::get("/api/v1/openapi.yaml")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    let enabled_state = app(true);
    let enabled = server_router(enabled_state);
    let response = enabled
        .clone()
        .oneshot(
            Request::get("/api/v1/openapi.yaml")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);

    let cookie = admin_cookie(&enabled).await;
    let response = enabled
        .oneshot(
            Request::get("/api/v1/openapi.yaml")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "text/yaml");
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), OPENAPI_YAML.as_bytes());
    assert!(!String::from_utf8_lossy(&body).contains("swagger-ui"));
}

#[tokio::test]
async fn health_requires_admin_and_status_matches_the_pinned_fixture() {
    let state = app(false);
    let router = server_router(state.clone());

    let response = router
        .clone()
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 401);

    let cookie = admin_cookie(&router).await;
    let health = router
        .clone()
        .oneshot(
            Request::get("/api/v1/health")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), 503);
    let health = response_json(health).await;
    assert_eq!(health["version"], BUILD_VERSION);
    assert_eq!(health["commit"], BUILD_COMMIT);
    assert_eq!(health["database"], "not_configured");
    assert_eq!(health["replay_storage"], "not_configured");
    assert!(
        health["uptime_seconds"]
            .as_u64()
            .is_some_and(|value| value < 60)
    );
    for key in [
        "database",
        "replay_storage",
        "active_rooms",
        "active_room_matches",
        "active_compat_matches",
    ] {
        assert!(health.get(key).is_some(), "missing health field {key}");
    }
    assert!(!health.to_string().contains("double-riichi.db"));
    assert!(!health.to_string().contains("replays/"));

    let status = router
        .oneshot(Request::get("/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    let status = response_json(status).await;
    let expected: Value =
        serde_json::from_str(include_str!("../../../spec/fixtures/public-status.json")).unwrap();
    assert_eq!(status, expected);
}

#[tokio::test]
async fn public_room_lookup_matches_the_pinned_fixture() {
    let state = app(false);
    let mut config = RoomConfig::new(
        "Contract Room",
        GameMode::FourPlayerRedEast,
        CharacterCatalog::starter(),
    );
    config.max_participants = 4;
    let room = state.rooms().create(config).await.unwrap();
    let join_code = room.join_code().to_owned();
    let router = server_router(state.clone());
    let response = router
        .oneshot(
            Request::get(format!("/api/v1/rooms/{join_code}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response_json(response).await;
    let expected: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/public-room-lookup.json"
    ))
    .unwrap();
    assert_eq!(body, expected);
    state.shutdown().await;
}

#[tokio::test]
async fn shutdown_closes_admission_before_waiting_for_state_cleanup() {
    let state = app(false);
    let router = server_router(state.clone());
    state.begin_shutdown();

    let response = router
        .oneshot(Request::get("/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(
        response_json(response).await["code"],
        "server_shutting_down"
    );

    state.shutdown().await;
}

#[test]
fn build_metadata_is_compile_time_semver_and_short_sha() {
    assert_eq!(BUILD_VERSION, "0.1.0");
    assert!(BUILD_COMMIT.len() == 12);
    assert!(
        BUILD_COMMIT.chars().all(|value| value.is_ascii_hexdigit())
            || (BUILD_COMMIT.starts_with("dev")
                && BUILD_COMMIT[3..].chars().all(|value| value == '0'))
    );
}

#[test]
fn tracing_format_and_docs_are_configurable_without_ambient_defaults() {
    let root = std::env::temp_dir().join(format!(
        "double-riichi-task16-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    fs::write(
        &path,
        "public_origin = \"http://127.0.0.1:3000\"\ntracing_format = \"json\"\napi_docs = true\n",
    )
    .unwrap();
    let config = RuntimeConfig::from_path(&path).unwrap();
    assert_eq!(config.tracing_format, TracingFormat::Json);
    assert!(config.api_docs);
    fs::remove_dir_all(root).unwrap();

    let root = std::env::temp_dir().join(format!(
        "double-riichi-task16-default-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    fs::write(&path, "public_origin = \"http://127.0.0.1:3000\"\n").unwrap();
    let config = RuntimeConfig::from_path(&path).unwrap();
    assert_eq!(config.tracing_format, TracingFormat::Text);
    assert!(!config.api_docs);
    fs::remove_dir_all(root).unwrap();
}
