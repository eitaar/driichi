use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use double_riichi_core::{CharacterCatalog, GameMode, RoomConfig, RoomJoinCode, RoomRegistry};
use double_riichi_server::{
    AdminAuthenticator, BUILD_COMMIT, BUILD_VERSION, BotTokenAuthority, BotTokenService,
    RuntimeConfig, ServerState, Storage, TracingFormat, hash_password, server_router,
};
use futures_util::{SinkExt, StreamExt};
use reqwest::cookie::{CookieStore, Jar};
use serde_json::{Value, json};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tower::ServiceExt;
use url::Url;

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

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "double-riichi-task16-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

async fn storage_app(label: &str) -> (Arc<ServerState>, Arc<Storage>, PathBuf) {
    let root = temp_root(label);
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let service = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::empty()),
    ));
    let state = Arc::new((*app(false)).clone().with_bot_token_service(service));
    (state, storage, root)
}

fn store_cookies(jar: &Jar, headers: &axum::http::HeaderMap, url: &Url) {
    let values = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| reqwest::header::HeaderValue::from_str(value.to_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    let mut values = values.iter();
    jar.set_cookies(&mut values, url);
}

fn cookie_header(jar: &Jar, url: &Url) -> String {
    jar.cookies(url)
        .expect("cookie jar should match the request path")
        .to_str()
        .unwrap()
        .to_owned()
}

async fn admin_cookie_jar(app: &axum::Router) -> Arc<Jar> {
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
    let jar = Arc::new(Jar::default());
    store_cookies(
        &jar,
        response.headers(),
        &Url::parse("http://127.0.0.1:3000/api/v1/admin/login").unwrap(),
    );
    let body = response_json(response).await;
    assert_eq!(
        body.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["expires_at"]
    );
    assert!(body["expires_at"].as_str().is_some());
    jar
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 128 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn response_bytes(
    response: axum::response::Response,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let status = response.status();
    let headers = response.headers().clone();
    // These are finite, in-process static assets, not untrusted API response bodies.
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, body.to_vec())
}

#[tokio::test]
async fn response_reader_preserves_frontend_assets_larger_than_one_mib() {
    let asset = vec![b'x'; 1024 * 1024 + 1];
    let response = axum::response::Response::new(Body::from(asset.clone()));
    let (status, _, body) = response_bytes(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, asset);
}

fn referenced_asset(index: &[u8]) -> String {
    let index = String::from_utf8(index.to_vec()).unwrap();
    let marker = "/assets/";
    let start = index
        .find(marker)
        .expect("index should reference a static asset");
    let value = &index[start..];
    let end = value
        .find(|character: char| character == char::from(34) || character == char::from(39))
        .expect("static asset reference should be quoted");
    value[..end].to_owned()
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
async fn embedded_frontend_serves_root_and_referenced_static_asset() {
    let router = server_router(app(false));
    let root = router
        .clone()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, headers, body) = response_bytes(root).await;
    if cfg!(debug_assertions) {
        assert_eq!(status, StatusCode::NOT_FOUND);
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/html; charset=utf-8");
    assert_eq!(headers[header::CACHE_CONTROL], "no-cache");
    assert!(String::from_utf8_lossy(&body).contains("<html"));
    let asset_path = referenced_asset(&body);
    let asset = router
        .oneshot(Request::get(&asset_path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, headers, body) = response_bytes(asset).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
    assert!(
        headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/javascript")
    );
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn embedded_frontend_serves_only_required_spa_routes() {
    let router = server_router(app(false));
    for path in [
        "/",
        "/room/123456",
        "/room/123456/lobby",
        "/admin",
        "/admin/login",
        "/admin/rooms/123456",
        "/admin/replays",
        "/admin/replays/01MATCH",
    ] {
        let response = router
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if cfg!(debug_assertions) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::OK
            },
            "SPA route {path}"
        );
        if !cfg!(debug_assertions) {
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "no-cache",
                "SPA route {path} cache policy"
            );
        }
    }
    for path in [
        "/missing",
        "/room/12345",
        "/room/123456/unknown",
        "/admin/private",
        "/admin/replays/01MATCH/extra",
    ] {
        let response = router
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "path {path}");
    }
}

#[tokio::test]
async fn frontend_rejects_missing_traversal_and_encoded_paths() {
    let router = server_router(app(false));
    for path in [
        "/assets/missing.js",
        "/assets/../index.html",
        "/assets/%2e%2e/index.html",
        "/assets/%252e%252e/index.html",
        "/assets\\\\index.js",
        "/.env",
        "/.vite/manifest.json",
    ] {
        let response = router
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "path {path}");
    }
}

#[tokio::test]
async fn api_routes_keep_precedence_over_frontend_fallback() {
    let router = server_router(app(false));
    let health = router
        .clone()
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::UNAUTHORIZED);
    let missing = router
        .clone()
        .oneshot(
            Request::get("/api/v1/not-a-frontend-route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );
    let ws = router
        .oneshot(
            Request::get("/ws/not-a-frontend-route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ws.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        ws.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );
}

#[tokio::test]
async fn frontend_responses_include_security_headers() {
    let router = server_router(app(false));
    let response = router
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        ),
    ] {
        assert_eq!(response.headers()[name], value, "header {name}");
    }
    assert!(
        response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'self'")
    );
}

#[tokio::test]
async fn raw_openapi_is_disabled_and_admin_authenticated() {
    let disabled = server_router(app(false));
    let response = disabled
        .oneshot(
            Request::get("/api/v1/admin/openapi.yaml")
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
            Request::get("/api/v1/admin/openapi.yaml")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);

    let jar = admin_cookie_jar(&enabled).await;
    let docs_url = Url::parse("http://127.0.0.1:3000/api/v1/admin/openapi.yaml").unwrap();
    let response = enabled
        .oneshot(
            Request::get("/api/v1/admin/openapi.yaml")
                .header(header::COOKIE, cookie_header(&jar, &docs_url))
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
async fn storage_health_covers_ok_and_degraded_components() {
    let (state, storage, root) = storage_app("health").await;
    let router = server_router(state.clone());
    let jar = admin_cookie_jar(&router).await;
    let cookie = cookie_header(
        &jar,
        &Url::parse("http://127.0.0.1:3000/api/v1/admin/health").unwrap(),
    );

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/v1/health")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let health = response_json(response).await;
    assert_eq!(health["database"], "ok");
    assert_eq!(health["replay_storage"], "ok");

    storage.close().await;
    let response = router
        .oneshot(
            Request::get("/api/v1/health")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let health = response_json(response).await;
    assert_eq!(health["database"], "degraded");
    state.shutdown().await;
    drop(state);
    storage.close().await;
    drop(storage);
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn admin_room_contract_fields_are_accepted_by_runtime() {
    let state = app(false);
    let router = server_router(state.clone());
    let cookie = admin_cookie(&router).await;
    let created = router
        .clone()
        .oneshot(
            Request::post("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .header(header::COOKIE, &cookie)
                .body(Body::from(
                    json!({
                        "room_name": "Contract Fields",
                        "game_mode": "4p-red-east",
                        "time_control": "unlimited",
                        "replay_save": true,
                        "participant_limit": 4
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let created = response_json(created).await;
    assert_eq!(created["time_control"], "unlimited");
    assert_eq!(created["participant_limit"], 4);
    assert!(created["persistence_degraded"].is_boolean());
    assert!(created["replay_available"].is_boolean());
    let join_code = created["join_code"].as_str().unwrap().to_owned();

    let patched = router
        .oneshot(
            Request::patch(format!("/api/v1/admin/rooms/{join_code}"))
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(
                    json!({"time_control":"riichi-dev","participant_limit":4}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patched.status(), 200);
    let patched = response_json(patched).await;
    assert_eq!(patched["time_control"], "riichi_dev");
    assert_eq!(patched["participant_limit"], 4);
    assert!(patched["persistence_degraded"].is_boolean());
    assert!(patched["replay_available"].is_boolean());
    state.shutdown().await;
}

#[tokio::test]
async fn shutdown_cleans_storage_after_closing_admission() {
    let (state, storage, root) = storage_app("shutdown").await;
    let partial = storage
        .replay_root()
        .join(".incomplete")
        .join("task16-shutdown.mjson.part");
    fs::write(&partial, b"partial").unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status, replay_path) VALUES (?, 'ranked', NULL, ?, ?, 'writing', NULL)",
    )
    .bind("task16-shutdown")
    .bind("4p-red-east")
    .bind(0_i64)
    .execute(storage.pool())
    .await
    .unwrap();

    let router = server_router(state.clone());
    state.begin_shutdown();
    let response = router
        .oneshot(Request::get("/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    state.shutdown().await;

    assert!(!partial.exists());
    assert!(storage.pool().is_closed());
    drop(state);
    drop(storage);
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn human_snapshot_matches_the_retained_fixture_shape() {
    let state = app(false);
    state
        .rooms()
        .create_with_join_code(
            RoomConfig::new(
                "Contract Room",
                GameMode::FourPlayerRedEast,
                CharacterCatalog::starter(),
            ),
            RoomJoinCode::new("123456").unwrap(),
        )
        .await
        .unwrap();
    let router = server_router(state.clone());
    let join = router
        .clone()
        .oneshot(
            Request::post("/api/v1/rooms/123456/join")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"nickname":"Contract Human","character_id":"player-red"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(join.status(), 201);
    let jar = Arc::new(Jar::default());
    store_cookies(
        &jar,
        join.headers(),
        &Url::parse("http://127.0.0.1:3000/api/v1/rooms/123456/join").unwrap(),
    );

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
    let ws_url = Url::parse(&format!("ws://{address}/ws/v1/rooms/123456/human")).unwrap();
    let cookie_url = Url::parse(&format!("http://{address}/ws/v1/rooms/123456/human")).unwrap();
    let mut request = ws_url.as_str().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", "http://127.0.0.1:3000".parse().unwrap());
    request.headers_mut().insert(
        header::COOKIE,
        cookie_header(&jar, &cookie_url).parse().unwrap(),
    );
    let (mut socket, _) = connect_async(request).await.unwrap();
    let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Text(message) = message else {
        panic!("expected Human snapshot")
    };
    let actual: Value = serde_json::from_str(&message).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../spec/fixtures/human-snapshot.json")).unwrap();
    assert_eq!(actual["type"], fixture["type"]);
    for key in ["room_name", "game_mode", "phase"] {
        assert_eq!(
            actual["room"][key], fixture["room"][key],
            "room field {key}"
        );
    }
    for key in ["participants", "match_players", "roster", "result"] {
        assert!(
            actual["room"].get(key).is_some(),
            "missing room field {key}"
        );
        assert!(fixture["room"].get(key).is_some(), "fixture field {key}");
    }
    assert!(actual["room"]["revision"].as_u64().is_some());
    assert!(actual["state"].is_object() || actual["state"].is_null());
    assert!(actual["room"].get("kyoku_end").is_some());
    socket
        .send(Message::Text(
            json!({
                "type": "confirm_result",
                "step_id": {"match_id": "unrelated", "kyoku_sequence": 999, "step_index": 0}
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let response = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    break serde_json::from_str::<Value>(&text).unwrap();
                }
                Some(Ok(_)) => continue,
                other => panic!("expected current snapshot for stale confirmation, got {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(response["type"], "snapshot");
    assert_eq!(response["room"]["revision"], actual["room"]["revision"]);
    assert_eq!(response["room"]["kyoku_end"], Value::Null);
    server.abort();
    state.shutdown().await;
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
fn build_metadata_matches_git_head_probe() {
    let expected = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "--short=12", "HEAD"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    assert_eq!(BUILD_COMMIT, expected);
}

#[test]
fn build_script_watches_linked_worktree_git_metadata() {
    let build_script = include_str!("../build.rs");
    assert!(build_script.contains("commondir"));
    assert!(build_script.contains("packed-refs"));
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

#[cfg(feature = "task8-test-support")]
mod task8_http_human {
    use super::*;
    use std::time::Duration;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

    // Fixture selection is process-global; isolate only tests that require a deal.
    fn run_in_fixture_process(fixture: &str) -> bool {
        if std::env::var("DRIICHI_TASK8_FIXTURE").as_deref() == Ok(fixture) {
            return false;
        }
        let name = std::thread::current()
            .name()
            .expect("named test thread")
            .to_owned();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--test-threads=1"])
            .env("DRIICHI_TASK8_FIXTURE", fixture)
            .output()
            .expect("fixture test process");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            output.status.success(),
            "fixture {fixture} failed: {}
{stdout}
{stderr}",
            output.status
        );
        assert!(
            stdout
                .lines()
                .any(|line| line == format!("test {name} ... ok")),
            "fixture test was not executed: {stdout}"
        );
        true
    }

    struct ReceiveDiagnostic {
        started: std::time::Instant,
        path: PathBuf,
        records: std::sync::Mutex<Vec<Value>>,
    }

    impl ReceiveDiagnostic {
        fn new(label: &str) -> Arc<Self> {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/sdd/2026-10-02-authoritative-kyoku-results/task-8-native1/automatic-tests-continuation1");
            assert!(
                root.is_dir(),
                "private diagnostic evidence directory must exist"
            );
            Arc::new(Self {
                started: std::time::Instant::now(),
                path: root.join(format!(
                    "receive-{label}-{}-{}.json",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                )),
                records: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn record(&self, peer: usize, kind: &str, value: Value) {
            let mut records = self.records.lock().unwrap();
            assert!(records.len() < 4096, "finite diagnostic frame bound");
            let order = records.len();
            records.push(json!({"order":order,"peer":peer,"kind":kind,
                "elapsed_us":self.started.elapsed().as_micros(),
                "epoch_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
                "value":value}));
        }
    }

    impl Drop for ReceiveDiagnostic {
        fn drop(&mut self) {
            // No per-frame disk I/O: flush only after timeout unwinding/teardown.
            let mut save = || -> Result<(), Box<dyn std::error::Error>> {
                use std::io::Write;
                let bytes = serde_json::to_vec_pretty(&json!({
                    "panicking":std::thread::panicking(),"records":self.records.get_mut().unwrap()
                }))?;
                assert!(!bytes.contains(&0));
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.path)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                assert_eq!(fs::read(&self.path)?, bytes);
                Ok(())
            };
            match save() {
                Ok(()) => eprintln!("Task8 receive diagnostic retained: {}", self.path.display()),
                Err(error) => eprintln!(
                    "Task8 diagnostic preservation FAILED (primary test outcome unchanged): {error}"
                ),
            }
        }
    }

    struct HumanSocket {
        wire: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
        peer: usize,
        participant_id: String,
        diagnostic: Arc<ReceiveDiagnostic>,
    }

    impl HumanSocket {
        async fn send(
            &mut self,
            message: Message,
        ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
            let raw = message.to_string();
            self.wire.send(message).await?;
            self.diagnostic
                .record(self.peer, "sent", json!({"raw":raw}));
            Ok(())
        }
        async fn close(
            &mut self,
            frame: Option<tokio_tungstenite::tungstenite::protocol::CloseFrame>,
        ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
            self.wire.close(frame).await
        }
    }

    fn assert_boundary(value: &Value, winners: u64, viewer: Option<u64>) {
        match value {
            Value::Object(fields) => {
                for hidden in [
                    "queued_steps",
                    "prepared_outcomes",
                    "authoritative_result",
                    "engine_state",
                ] {
                    assert!(!fields.contains_key(hidden), "private field {hidden}");
                }
                if fields.contains_key("winning_tile_index") {
                    assert!(
                        fields["seat"].as_u64().unwrap() <= winners,
                        "future WinResult"
                    );
                }
                for name in ["hora", "Hora"] {
                    if let Some(hora) = fields.get(name) {
                        assert!(
                            hora["actor"].as_u64().unwrap() <= winners,
                            "future canonical Hora"
                        );
                    }
                }
                if fields.get("type").and_then(Value::as_str) == Some("hora") {
                    assert!(fields["actor"].as_u64().unwrap() <= winners);
                }
                for child in fields.values() {
                    assert_boundary(child, winners, viewer);
                }
            }
            Value::Array(values) => {
                // Frozen physical seat2 hand; legitimate own information is allowed.
                if winners < 2 && viewer != Some(2) {
                    assert_ne!(
                        value,
                        &json!([37, 41, 45, 49, 54, 57, 61, 65, 69, 78, 80, 81, 82]),
                        "future opponent hand"
                    );
                }
                for child in values {
                    assert_boundary(child, winners, viewer);
                }
            }
            _ => {}
        }
    }

    async fn http(
        client: &reqwest::Client,
        base: &str,
        method: reqwest::Method,
        path: &str,
        payload: Value,
        status: u16,
    ) -> Value {
        http_at_origin(
            client,
            base,
            "http://127.0.0.1:3000",
            method,
            path,
            payload,
            status,
        )
        .await
    }

    async fn http_at_origin(
        client: &reqwest::Client,
        base: &str,
        origin: &str,
        method: reqwest::Method,
        path: &str,
        payload: Value,
        status: u16,
    ) -> Value {
        let response = client
            .request(method, format!("{base}{path}"))
            .header("origin", origin)
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "HTTP {path}");
        let bytes = response.bytes().await.unwrap();
        assert!(!bytes.contains(&0));
        let value: Value = serde_json::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert_boundary(&value, 2, None);
        value
    }

    async fn receive(
        socket: &mut HumanSocket,
        transcript: &mut Vec<Value>,
        expected: Value,
        predicate: impl Fn(&Value) -> bool,
    ) -> Value {
        socket
            .diagnostic
            .record(socket.peer, "receive_begin", expected.clone());
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match socket.wire.next().await {
                    Some(Ok(Message::Text(text))) => {
                        assert!(!text.as_bytes().contains(&0));
                        socket.diagnostic.record(
                            socket.peer,
                            "received",
                            json!({"raw":text.as_str()}),
                        );
                        let value: Value = serde_json::from_str(&text).unwrap();
                        let matches = predicate(&value);
                        transcript.push(value.clone());
                        if matches {
                            break value;
                        }
                    }
                    Some(Ok(Message::Ping(bytes))) => {
                        socket.send(Message::Pong(bytes)).await.unwrap()
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    other => panic!("Human wire ended: {other:?}"),
                }
            }
        })
        .await;
        socket.diagnostic.record(
            socket.peer,
            if result.is_ok() {
                "receive_matched"
            } else {
                "receive_timeout"
            },
            expected,
        );
        result.expect("actual Human response within protocol timeout")
    }

    async fn choose(socket: &mut HumanSocket, transcript: &mut Vec<Value>, kind: &str) {
        let offered = |a: &Value| {
            if kind == "discard" {
                a["action"]["discard"]["tile"] == 77
            } else if kind == "tsumogiri" {
                a["action"]["discard"]["tsumogiri"] == true
            } else {
                a["action"].get(kind).is_some() || a["action"] == kind
            }
        };
        let current = transcript
            .iter()
            .rev()
            .find(|v| v["state"].is_object())
            .cloned();
        let usable = current.as_ref().is_some_and(|v| {
            let decision = &v["state"]["decision"];
            let already_sent = socket.diagnostic.records.lock().unwrap().iter().any(|r| {
                if r["peer"] != socket.peer || r["kind"] != "sent" {
                    return false;
                }
                let message = serde_json::from_str::<Value>(r["value"]["raw"].as_str().unwrap())
                    .unwrap_or(Value::Null);
                message["type"] == "submit_action"
                    && message["decision_id"] == decision["decision_id"]
            });
            !already_sent
                && decision["decision_id"].is_string()
                && decision["actions"]
                    .as_array()
                    .is_some_and(|actions| actions.iter().any(offered))
        });
        let view = if usable {
            current.unwrap()
        } else {
            receive(
                socket,
                transcript,
                json!({"phase":"offered_action","kind":kind}),
                |v| {
                    v["state"]["decision"]["actions"]
                        .as_array()
                        .is_some_and(|actions| actions.iter().any(offered))
                },
            )
            .await
        };
        let decision = &view["state"]["decision"];
        let action = decision["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| offered(a))
            .unwrap();
        socket.send(Message::Text(json!({"type":"submit_action", "decision_id":decision["decision_id"], "action_id":action["action_id"]}).to_string().into())).await.unwrap();
        let accepted = receive(socket, transcript, json!({"phase":"action_result","decision_id":decision["decision_id"],"action_id":action["action_id"]}), |v| v["type"] == "action_result" && v["decision_id"] == decision["decision_id"]).await;
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["action_id"], action["action_id"]);
    }

    async fn join_humans(
        admin: &reqwest::Client,
        base: &str,
        address: std::net::SocketAddr,
        code: &str,
        count: usize,
    ) -> (Vec<HumanSocket>, Vec<Vec<Value>>) {
        join_humans_at_origin(admin, base, "http://127.0.0.1:3000", address, code, count).await
    }

    async fn join_humans_at_origin(
        admin: &reqwest::Client,
        base: &str,
        origin: &str,
        address: std::net::SocketAddr,
        code: &str,
        count: usize,
    ) -> (Vec<HumanSocket>, Vec<Vec<Value>>) {
        join_humans_with_preloads_at_origin(
            admin,
            base,
            origin,
            address,
            code,
            count,
            json!(["player-red"]),
        )
        .await
    }

    async fn join_humans_with_preloads_at_origin(
        admin: &reqwest::Client,
        base: &str,
        origin: &str,
        address: std::net::SocketAddr,
        code: &str,
        count: usize,
        preloaded_characters: Value,
    ) -> (Vec<HumanSocket>, Vec<Vec<Value>>) {
        let mut sockets = Vec::new();
        let mut transcripts = vec![Vec::new(); count];
        let mut ids = Vec::new();
        let diagnostic = ReceiveDiagnostic::new(&format!("{count}p"));
        for seat in 0..count {
            let jar = Arc::new(Jar::default());
            let client = reqwest::Client::builder()
                .cookie_provider(jar.clone())
                .build()
                .unwrap();
            let joined = http_at_origin(
                &client,
                base,
                origin,
                reqwest::Method::POST,
                &format!("/api/v1/rooms/{code}/join"),
                json!({"nickname":format!("Human{seat}"),"character_id":"player-red"}),
                201,
            )
            .await;
            let id = joined["participant_id"].as_str().unwrap().to_owned();
            http_at_origin(
                admin,
                base,
                origin,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/participants/{id}/select"),
                Value::Null,
                200,
            )
            .await;
            ids.push(id);
            let mut request = format!("ws://{address}/ws/v1/rooms/{code}/human")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
            request.headers_mut().insert(
                header::COOKIE,
                cookie_header(
                    &jar,
                    &Url::parse(&format!("{base}/ws/v1/rooms/{code}/human")).unwrap(),
                )
                .parse()
                .unwrap(),
            );
            let (socket, _) = connect_async(request).await.unwrap();
            sockets.push(HumanSocket {
                wire: socket,
                peer: seat,
                participant_id: ids[seat].clone(),
                diagnostic: diagnostic.clone(),
            });
        }
        if preloaded_characters != json!(["player-red"]) {
            let detail = http_at_origin(
                admin,
                base,
                origin,
                reqwest::Method::GET,
                &format!("/api/v1/admin/rooms/{code}"),
                Value::Null,
                200,
            )
            .await;
            diagnostic.record(
                4,
                "mixed_selected_roster_before_Human_Ready",
                detail.clone(),
            );
            let selected = detail["participants"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["selected"] == true)
                .collect::<Vec<_>>();
            assert_eq!(selected.len(), 4);
            assert_eq!(
                selected
                    .iter()
                    .filter(|p| p["kind"] == "mjai" && p["character_id"] == "mjai-bot")
                    .count(),
                1
            );
            for participant in selected {
                assert!(
                    preloaded_characters
                        .as_array()
                        .unwrap()
                        .contains(&participant["character_id"])
                );
            }
        }
        for socket in &mut sockets {
            socket
                .send(Message::Text(
                    json!({"type":"set_ready","preloaded_characters":preloaded_characters})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
        for seat in 0..count {
            receive(
                &mut sockets[seat],
                &mut transcripts[seat],
                json!({"phase":"Ready","participant_id":ids[seat]}),
                |v| {
                    v["room"]["participants"].as_array().is_some_and(|ps| {
                        ps.iter()
                            .any(|p| p["participant_id"] == ids[seat] && p["ready"] == true)
                    })
                },
            )
            .await;
        }
        (sockets, transcripts)
    }

    fn assert_public_human_state(state: &Value) {
        assert!(state.is_object());
        for key in ["viewer_seat", "own_tehai", "legal_actions"] {
            assert!(
                state.get(key).is_none(),
                "public canonical Human DTO has no {key}"
            );
        }
        for player in state["players"].as_array().unwrap() {
            assert!(player.get("hand").is_none(), "public Human opponent hand");
        }
        if let Some(decision) = state.get("decision").filter(|d| d.is_object()) {
            for key in ["actions", "default_action_id", "legal_actions"] {
                assert!(
                    decision.get(key).is_none(),
                    "public Human action offer {key}"
                );
            }
        }
    }

    async fn join_human_watcher(
        base: &str,
        origin: &str,
        address: std::net::SocketAddr,
        code: &str,
        diagnostic: Arc<ReceiveDiagnostic>,
    ) -> HumanSocket {
        let jar = Arc::new(Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .unwrap();
        let joined = http_at_origin(
            &client,
            base,
            origin,
            reqwest::Method::POST,
            &format!("/api/v1/rooms/{code}/join"),
            json!({"nickname":"Human watcher","character_id":"player-red"}),
            201,
        )
        .await;
        let mut request = format!("ws://{address}/ws/v1/rooms/{code}/human")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        request.headers_mut().insert(
            header::COOKIE,
            cookie_header(
                &jar,
                &Url::parse(&format!("{base}/ws/v1/rooms/{code}/human")).unwrap(),
            )
            .parse()
            .unwrap(),
        );
        let (socket, _) = connect_async(request).await.unwrap();
        HumanSocket {
            wire: socket,
            peer: 4,
            participant_id: joined["participant_id"].as_str().unwrap().to_owned(),
            diagnostic,
        }
    }

    async fn observe_outer_response(
        axum::extract::State(diagnostic): axum::extract::State<Arc<ReceiveDiagnostic>>,
        request: Request<Body>,
        next: axum::middleware::Next,
    ) -> axum::response::Response {
        let method = request.method().as_str().to_owned();
        let path = request.uri().path().to_owned();
        let observed = path == "/mcp"
            || (method == "GET"
                && (path.starts_with("/api/v1/rooms/") || path.starts_with("/api/v1/admin/rooms")));
        let mut response = next.run(request).await;
        if !observed {
            return response;
        }
        // Preserve the finite Content-Length Hyper would derive from the original body size hint.
        if let Some(length) = axum::body::HttpBody::size_hint(response.body())
            .exact()
            .filter(|n| *n > 0)
        {
            response
                .headers_mut()
                .entry(header::CONTENT_LENGTH)
                .or_insert(length.to_string().parse().unwrap());
        }
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let headers: serde_json::Map<String, Value> =
            ["content-type", "content-length", "content-encoding"]
                .into_iter()
                .filter_map(|name| {
                    response
                        .headers()
                        .get(name)
                        .map(|value| (name.to_owned(), json!(value.to_str().unwrap())))
                })
                .collect();
        diagnostic.record(usize::MAX, "http_response", json!({"request_id":request_id,"method":method,"path":path,"status":response.status().as_u16(),"headers":headers,"scope":"server-polled response DATA bytes; no packets/transfer-coding/trailers; auth/cookie/session headers excluded"}));
        let (parts, body) = response.into_parts();
        // shortcut: these pinned JSON/SSE producers have no trailers; revisit if target routes add trailers.
        let stream = body.into_data_stream().map(move |chunk| {
            match &chunk {
                Ok(bytes) => {
                    assert!(
                        bytes.len() <= 256 * 1024,
                        "bounded outer HTTP diagnostic chunk"
                    );
                    diagnostic.record(
                        usize::MAX,
                        "http_body_chunk",
                        json!({"request_id":request_id,"bytes":bytes.to_vec()}),
                    );
                }
                Err(error) => diagnostic.record(
                    usize::MAX,
                    "http_body_error",
                    json!({"request_id":request_id,"error":error.to_string()}),
                ),
            }
            chunk
        });
        axum::response::Response::from_parts(parts, Body::from_stream(stream))
    }

    fn decode_outer_body(content_type: &str, bytes: &[u8]) -> (Vec<Value>, usize) {
        if bytes.is_empty() {
            return (Vec::new(), 0);
        }
        if content_type.starts_with("application/json") {
            return (
                vec![serde_json::from_slice(bytes).expect("complete outer JSON response")],
                0,
            );
        }
        assert!(
            content_type.starts_with("text/event-stream"),
            "uninspected outer encoding: {content_type}"
        );
        // shortcut: the pinned SSE writer emits LF or CRLF; revisit if its framing changes.
        let end = bytes
            .windows(2)
            .rposition(|w| w == b"\n\n")
            .map(|p| p + 2)
            .into_iter()
            .chain(
                bytes
                    .windows(4)
                    .rposition(|w| w == b"\r\n\r\n")
                    .map(|p| p + 4),
            )
            .max()
            .unwrap_or(0);
        let text = std::str::from_utf8(&bytes[..end])
            .expect("complete SSE event must be UTF8")
            .replace("\r\n", "\n");
        let mut values = Vec::new();
        for event in text.split("\n\n") {
            let data = event
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("data:")
                        .map(|v| v.strip_prefix(' ').unwrap_or(v))
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                values
                    .push(serde_json::from_str(&data).expect("complete outer JSON-RPC SSE event"));
            }
        }
        (values, bytes.len() - end)
    }

    fn assert_outer_http_boundary(
        diagnostic: &ReceiveDiagnostic,
        winners: u64,
        step: u64,
    ) -> usize {
        let records = diagnostic.records.lock().unwrap().clone();
        let mut responses = std::collections::BTreeMap::<String, (Value, Vec<u8>)>::new();
        let mut chunks = 0;
        let mut total_bytes = 0;
        for record in &records {
            let value = &record["value"];
            match record["kind"].as_str() {
                Some("http_response") => {
                    assert!(
                        value["status"] == 200 || value["status"] == 202 || value["status"] == 204
                    );
                    assert!(
                        value["headers"]["content-encoding"].is_null(),
                        "encoded body must not escape inspection"
                    );
                    assert!(
                        responses
                            .insert(
                                value["request_id"].as_str().unwrap().to_owned(),
                                (value.clone(), Vec::new())
                            )
                            .is_none()
                    );
                }
                Some("http_body_chunk") => {
                    let (_, bytes) = responses
                        .get_mut(value["request_id"].as_str().unwrap())
                        .expect("chunk has actual response metadata");
                    let chunk = value["bytes"].as_array().unwrap();
                    total_bytes += chunk.len();
                    assert!(
                        total_bytes <= 10 * 1024 * 1024,
                        "bounded outer response observation"
                    );
                    bytes.extend(
                        chunk
                            .iter()
                            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap()),
                    );
                    chunks += 1;
                }
                Some("http_body_error") => panic!("outer body stream error retained: {value}"),
                _ => {}
            }
        }
        assert!(
            chunks > 0,
            "actual outer HTTP/SSE body chunks must be retained before confirmation"
        );
        let mut envelope_count = 0;
        let mut partial_suffixes = Vec::new();
        for (id, (metadata, bytes)) in responses {
            let (envelopes, partial) = decode_outer_body(
                metadata["headers"]["content-type"].as_str().unwrap_or(""),
                &bytes,
            );
            for envelope in &envelopes {
                assert_mcp_typed_boundary(envelope, winners, step);
            }
            envelope_count += envelopes.len();
            if partial != 0 {
                partial_suffixes.push(json!({"request_id":id,"bytes":partial,"status":"UNKNOWN: retained incomplete SSE suffix, not discarded/no-leak proof"}));
            }
        }
        assert!(envelope_count > 0, "real complete outer envelopes required");
        diagnostic.record(usize::MAX, "outer_boundary_proved", json!({"winners":winners,"step_index":step,"through_record_count":records.len(),"chunk_count":chunks,"complete_envelopes":envelope_count,"body_bytes":total_bytes,"partial_suffixes":partial_suffixes,"scope":"sampled polled DATA bytes and decoded outer JSON/JSON-RPC plus nested content; no packet/drain/trailer claim"}));
        chunks
    }

    #[test]
    fn outer_response_decoder_retains_partial_utf8_and_inspects_encoded_future_win() {
        let mut raw =
            b"event:message\r\ndata: {\"left\":1,\r\ndata: \"right\":2}\r\n\r\n:heartbeat\n\n"
                .to_vec();
        raw.extend_from_slice(b"data: \"");
        raw.extend_from_slice(&[0xe6, 0x9d]);
        let (decoded, partial) = decode_outer_body("text/event-stream", &raw);
        assert_eq!(decoded, vec![json!({"left":1,"right":2})]);
        assert_eq!(partial, 9);
        assert_eq!(
            decode_outer_body("application/json", b"{\"ok\":true}"),
            (vec![json!({"ok":true})], 0)
        );
        let future = json!({"jsonrpc":"2.0","result":{"content":[{"text":"{\"seat\":2,\"winning_tile_index\":13}"}]}});
        assert!(std::panic::catch_unwind(|| assert_mcp_typed_boundary(&future, 1, 0)).is_err());
        assert_mcp_typed_boundary(&future, 2, 1);
    }

    async fn assert_held_step(
        admin: &reqwest::Client,
        base: &str,
        code: &str,
        step: u64,
        sockets: &mut [HumanSocket],
        transcripts: &mut [Vec<Value>],
        outer: &ReceiveDiagnostic,
        watcher: &mut HumanSocket,
        watcher_transcript: &mut Vec<Value>,
    ) {
        let mut published = Vec::new();
        for seat in 0..sockets.len() {
            let prior_id = transcripts[seat].iter().rev().find_map(|v| {
                v["room"]["kyoku_end"]["id"]
                    .as_object()
                    .map(|_| v["room"]["kyoku_end"]["id"].clone())
            });
            let view = receive(
                &mut sockets[seat],
                &mut transcripts[seat],
                json!({"phase":"held_publication","step_index":step,"prior_actual_id":prior_id}),
                |v| v["room"]["kyoku_end"]["id"]["step_index"] == step,
            )
            .await;
            assert_eq!(view["state"]["viewer_seat"], seat);
            assert_eq!(view["state"]["decision"], Value::Null);
            assert_eq!(view["room"]["result"], Value::Null);
            let wins = view["state"]["kyoku_results"]["wins"].as_array().unwrap();
            assert_eq!(
                wins.iter()
                    .map(|w| w["seat"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                if step == 0 { vec![1] } else { vec![1, 2] }
            );
            assert_eq!(wins[0]["winning_tile"], 77);
            assert_eq!(wins[0]["win_points"], 2600);
            assert_boundary(&view, if step == 0 { 1 } else { 2 }, Some(seat as u64));
            published.push(view);
        }
        let watched = receive(
            watcher,
            watcher_transcript,
            json!({"phase":"public_Human_watcher_held","step":step}),
            |v| v["room"]["kyoku_end"]["id"] == published[0]["room"]["kyoku_end"]["id"],
        )
        .await;
        assert_public_human_state(&watched["state"]);
        assert_eq!(watched["state"]["decision"], Value::Null);
        assert_eq!(
            watched["state"]["kyoku_results"],
            published[0]["state"]["kyoku_results"]
        );
        assert_boundary(&watched, if step == 0 { 1 } else { 2 }, None);
        assert!(
            !watched["room"]["kyoku_end"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!(watcher.participant_id))
        );
        let public = reqwest::Client::new();
        let lookup = http(
            &public,
            base,
            reqwest::Method::GET,
            &format!("/api/v1/rooms/{code}"),
            Value::Null,
            200,
        )
        .await;
        assert_eq!(lookup["room_name"], "Task8 transport");
        assert_eq!(lookup["phase"], "playing");
        assert_eq!(lookup["participant_count"], sockets.len() + 1);
        assert_eq!(lookup["join_allowed"], false);
        assert_boundary(&lookup, if step == 0 { 1 } else { 2 }, None);
        let detail = http(
            admin,
            base,
            reqwest::Method::GET,
            &format!("/api/v1/admin/rooms/{code}"),
            Value::Null,
            200,
        )
        .await;
        assert!(detail.get("kyoku_end").is_none());
        assert_eq!(detail["join_code"], code);
        assert_eq!(detail["game_mode"], published[0]["room"]["game_mode"]);
        assert_eq!(detail["phase"], "playing");
        assert_eq!(detail["roster"], published[0]["room"]["roster"]);
        assert_eq!(
            detail["match_players"],
            published[0]["room"]["match_players"]
        );
        assert!(
            detail["revision"].as_u64().unwrap()
                >= published[0]["room"]["revision"].as_u64().unwrap()
        );
        assert_eq!(detail["result"], Value::Null);
        assert_boundary(&detail, if step == 0 { 1 } else { 2 }, None);
        let list = http(
            admin,
            base,
            reqwest::Method::GET,
            "/api/v1/admin/rooms",
            Value::Null,
            200,
        )
        .await;
        assert_boundary(&list, if step == 0 { 1 } else { 2 }, None);
        assert_outer_http_boundary(outer, if step == 0 { 1 } else { 2 }, step);
        outer.record(usize::MAX, "outer_held_id", json!({"actual_step_id":published[0]["room"]["kyoku_end"]["id"],"deadline_at_ms":published[0]["room"]["kyoku_end"]["deadline_at_ms"]}));
        for (socket, view) in sockets.iter_mut().zip(&published) {
            socket
                .send(Message::Text(
                    json!({"type":"confirm_result", "step_id":view["room"]["kyoku_end"]["id"]})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
    }

    async fn confirm_with_handler_barrier(
        socket: &mut HumanSocket,
        transcript: &mut Vec<Value>,
        id: &Value,
        counter: usize,
    ) {
        let nonce = format!("task8-final:{}:{counter}", id["match_id"].as_str().unwrap());
        assert!(nonce.len() <= 125);
        socket
            .send(Message::Text(
                json!({"type":"confirm_result","step_id":id})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Ping(nonce.clone().into_bytes().into()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            for _ in 0..4096 {
                match socket.wire.next().await {
                    Some(Ok(Message::Text(text))) => {
                        socket.diagnostic.record(
                            socket.peer,
                            "final_confirm_barrier_text",
                            json!({"raw":text.as_str(),"counter":counter}),
                        );
                        transcript.push(serde_json::from_str(&text).unwrap());
                    }
                    Some(Ok(Message::Ping(bytes))) => {
                        socket.send(Message::Pong(bytes)).await.unwrap();
                    }
                    Some(Ok(Message::Pong(bytes))) => {
                        socket.diagnostic.record(
                            socket.peer,
                            "final_confirm_barrier_pong",
                            json!({"bytes":bytes.to_vec(),"counter":counter}),
                        );
                        if bytes.as_ref() == nonce.as_bytes() {
                            return;
                        }
                    }
                    other => panic!("final confirmation wire ended: {other:?}"),
                }
            }
            panic!("finite final confirmation frame bound");
        })
        .await
        .expect("matching final confirmation Pong");
        // Handler ordering only: caller reads actor authority next, not outbound queue drain.
    }

    #[tokio::test]
    async fn actual_final_draw_tied_pot_holds_then_saves_once_and_readies_rematch_in_both_modes() {
        if run_in_fixture_process("final-draw") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "final-draw"
        );
        let root = temp_root("final-draw-lifecycle");
        let storage = Arc::new(Storage::connect(&root).await.unwrap());
        let worker_storage = storage.clone();
        let registry = RoomRegistry::with_max_rooms(8).with_effect_spawner(move |effects| {
            double_riichi_server::spawn_room_effect_worker(worker_storage.clone(), effects)
        });
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
                registry,
            )
            .with_bot_token_service(Arc::new(BotTokenService::new(
                storage.clone(),
                Arc::new(BotTokenAuthority::empty()),
            ))),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                server_router(serve_state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        let admin = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        http(
            &admin,
            &base,
            reqwest::Method::POST,
            "/api/v1/admin/login",
            json!({"username":"admin","password":"correct horse battery staple"}),
            200,
        )
        .await;
        for (mode, count) in [("3p-red-east", 3usize), ("4p-red-east", 4)] {
            let created=http(&admin,&base,reqwest::Method::POST,"/api/v1/admin/rooms",json!({"room_name":"Task8 actual final draw","game_mode":mode,"time_control":"unlimited","participant_limit":count,"replay_save":true}),201).await;
            let code = created["join_code"].as_str().unwrap();
            let (sockets, transcripts) = join_humans(&admin, &base, address, code, count).await;
            http(
                &admin,
                &base,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/start"),
                Value::Null,
                200,
            )
            .await;
            let mut mapped = Vec::new();
            for (mut socket, mut transcript) in sockets.into_iter().zip(transcripts) {
                let view = receive(
                    &mut socket,
                    &mut transcript,
                    json!({"phase":"final_draw_seat_mapping"}),
                    |v| v["state"].is_object(),
                )
                .await;
                let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
                assert!(seat < count);
                assert_eq!(
                    view["room"]["match_players"][seat]["participant_id"],
                    socket.participant_id
                );
                mapped.push((seat, socket, transcript));
            }
            mapped.sort_by_key(|(seat, _, _)| *seat);
            assert_eq!(
                mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
                (0..count).collect::<Vec<_>>()
            );
            let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) =
                mapped.into_iter().map(|(_, s, t)| (s, t)).unzip();
            let mut held = None;
            let mut submitted = 0;
            for _ in 0..=280 {
                let mut acted = false;
                for seat in 0..count {
                    let view = foreign_match_actor_view(&state, code, &sockets[seat]).await;
                    if view["room"]["kyoku_end"].is_object() {
                        held = Some(view);
                        break;
                    }
                    let decision = &view["state"]["decision"];
                    let Some(actions) = decision["actions"].as_array().filter(|a| !a.is_empty())
                    else {
                        continue;
                    };
                    let kind = if actions.iter().any(|a| a["action"] == "Pass") {
                        "Pass"
                    } else {
                        assert!(
                            actions
                                .iter()
                                .any(|a| a["action"]["Discard"]["tsumogiri"] == true)
                        );
                        "tsumogiri"
                    };
                    let id = decision["decision_id"].clone();
                    receive(
                        &mut sockets[seat],
                        &mut transcripts[seat],
                        json!({"phase":"actual_final_draw_offered_ID","decision_id":id}),
                        |v| v["state"]["decision"]["decision_id"] == id,
                    )
                    .await;
                    choose(&mut sockets[seat], &mut transcripts[seat], kind).await;
                    acted = true;
                    submitted += 1;
                    assert!(submitted <= 280);
                    break;
                }
                if held.is_some() {
                    break;
                }
                assert!(acted, "actual final draw action or terminal required");
            }
            let held = held.expect("actual full-wall exhaustive ending");
            let step = held["room"]["kyoku_end"].clone();
            let id = step["id"].clone();
            let match_id = id["match_id"].as_str().unwrap();
            let settlement = step["content"]["Draw"].clone();
            assert_eq!(id["kyoku_sequence"], 1);
            assert_eq!(id["step_index"], 0);
            assert_eq!(settlement["draw_reason"], "exhaustive_draw");
            assert_eq!(
                (
                    settlement["pot_before"].clone(),
                    settlement["pot_after"].clone()
                ),
                (json!(2), json!(0))
            );
            assert_eq!(
                step["deadline_at_ms"].as_u64().unwrap()
                    - step["published_at_ms"].as_u64().unwrap(),
                5000
            );
            let after = (0..count)
                .map(|i| if i == 0 { 102000 } else { 100000 })
                .collect::<Vec<i32>>();
            assert_eq!(
                settlement["changes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| (
                        c["before"].as_i64().unwrap(),
                        c["delta"].as_i64().unwrap(),
                        c["after"].as_i64().unwrap()
                    ))
                    .collect::<Vec<_>>(),
                (0..count)
                    .map(|i| (100000, if i == 0 { 2000 } else { 0 }, i64::from(after[i])))
                    .collect::<Vec<_>>()
            );
            for seat in 0..count {
                let wire = receive(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    json!({"phase":"actual_final_draw_held","id":id}),
                    |v| v["room"]["kyoku_end"]["id"] == id,
                )
                .await;
                assert_eq!(wire["room"]["result"], Value::Null);
                assert_eq!(wire["room"]["phase"], "playing");
                assert_eq!(wire["state"]["decision"], Value::Null);
                assert_eq!(wire["state"]["kyoku_results"]["wins"], json!([]));
                assert_eq!(wire["state"]["kyoku_results"]["settlement"], settlement);
            }
            let detail = http(
                &admin,
                &base,
                reqwest::Method::GET,
                &format!("/api/v1/admin/rooms/{code}"),
                Value::Null,
                200,
            )
            .await;
            assert_eq!(detail["result"], Value::Null);
            assert_eq!(detail["phase"], "playing");
            use sqlx::Row;
            let row = sqlx::query(
                "SELECT status,authoritative_result_json FROM matches WHERE match_id=?",
            )
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("status"), "writing");
            assert!(
                row.get::<Option<String>, _>("authoritative_result_json")
                    .is_none()
            );
            for seat in 0..count {
                confirm_with_handler_barrier(&mut sockets[seat], &mut transcripts[seat], &id, seat)
                    .await;
                let view = foreign_match_actor_view(&state, code, &sockets[seat]).await;
                if seat + 1 < count {
                    assert_eq!(view["room"]["kyoku_end"]["id"], id);
                    assert_eq!(
                        view["room"]["kyoku_end"]["published_at_ms"],
                        step["published_at_ms"]
                    );
                    assert_eq!(
                        view["room"]["kyoku_end"]["deadline_at_ms"],
                        step["deadline_at_ms"]
                    );
                    assert_eq!(view["room"]["result"], Value::Null);
                    assert_eq!(view["state"]["decision"], Value::Null);
                    assert_eq!(
                        view["room"]["kyoku_end"]["confirmed"]
                            .as_array()
                            .unwrap()
                            .len(),
                        seat + 1
                    );
                }
            }
            let final_view = receive(
                &mut sockets[0],
                &mut transcripts[0],
                json!({"phase":"actual_final_draw_PostMatch"}),
                |v| v["room"]["phase"] == "post_match",
            )
            .await;
            let result = final_view["room"]["result"].clone();
            assert_eq!(result["final_scores"], json!(after));
            assert_eq!(
                result["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["rank"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                (1..=count as u64).collect::<Vec<_>>()
            );
            assert_eq!(
                result["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["participant_id"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                sockets
                    .iter()
                    .map(|s| s.participant_id.as_str())
                    .collect::<Vec<_>>()
            );
            assert_eq!(final_view["state"], Value::Null);
            let replay = http(
                &admin,
                &base,
                reqwest::Method::GET,
                &format!("/api/v1/admin/replays/{match_id}"),
                Value::Null,
                200,
            )
            .await;
            assert_eq!(replay["result"], result);
            let frames = replay["frames"].as_array().unwrap();
            assert_eq!(
                frames
                    .iter()
                    .filter(|f| !f["settlement"].is_null())
                    .map(|f| f["settlement"].clone())
                    .collect::<Vec<_>>(),
                vec![settlement.clone()]
            );
            assert!(frames.iter().all(|f| f["result"].is_null()));
            let row = sqlx::query(
                "SELECT status,replay_path,authoritative_result_json FROM matches WHERE match_id=?",
            )
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("status"), "completed");
            assert_eq!(
                serde_json::from_str::<Value>(&row.get::<String, _>("authoritative_result_json"))
                    .unwrap(),
                result
            );
            let bytes = fs::read(
                storage
                    .replay_root()
                    .join(row.get::<String, _>("replay_path")),
            )
            .unwrap();
            assert!(!bytes.contains(&0));
            let canonical = std::str::from_utf8(&bytes)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                frames
                    .iter()
                    .map(|f| f["visible_event"].clone())
                    .collect::<Vec<_>>(),
                canonical
            );
            assert_eq!(
                frames
                    .iter()
                    .map(|f| f["event_index"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                (0..canonical.len() as u64).collect::<Vec<_>>()
            );
            assert_eq!(
                canonical.iter().filter(|e| e["type"] == "end_game").count(),
                1
            );
            assert_eq!(
                canonical
                    .iter()
                    .filter(|e| e["type"] == "end_kyoku")
                    .count(),
                1
            );
            assert_eq!(
                canonical.iter().filter(|e| e["type"] == "ryukyoku").count(),
                1
            );
            let payload: Value = serde_json::from_str(
                &sqlx::query_scalar::<_, String>(
                    "SELECT payload_json FROM match_kyoku_results WHERE match_id=?",
                )
                .bind(match_id)
                .fetch_one(storage.pool())
                .await
                .unwrap(),
            )
            .unwrap();
            assert_eq!(payload["settlement"], settlement);
            assert_eq!(payload["wins"], json!([]));
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE match_id=?")
                    .bind(match_id)
                    .fetch_one(storage.pool())
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM match_kyoku_results WHERE match_id=?"
                )
                .bind(match_id)
                .fetch_one(storage.pool())
                .await
                .unwrap(),
                1
            );
            confirm_with_handler_barrier(&mut sockets[0], &mut transcripts[0], &id, 100).await;
            let duplicate = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert_eq!(duplicate["room"]["result"], result);
            assert_eq!(duplicate["room"]["kyoku_end"], Value::Null);
            for seat in 0..count {
                sockets[seat]
                    .send(Message::Text(
                        json!({"type":"set_ready","preloaded_characters":["player-red"]})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                let participant = sockets[seat].participant_id.clone();
                receive(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    json!({"phase":"actual_PostMatch_ready","participant_id":participant}),
                    |v| {
                        v["room"]["phase"] == "post_match"
                            && v["room"]["participants"].as_array().is_some_and(|ps| {
                                ps.iter().any(|p| {
                                    p["participant_id"] == participant && p["ready"] == true
                                })
                            })
                    },
                )
                .await;
            }
            let cutoff = transcripts[0].len();
            http(
                &admin,
                &base,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/rematch"),
                Value::Null,
                200,
            )
            .await;
            let started = receive(
                &mut sockets[0],
                &mut transcripts[0],
                json!({"phase":"actual_fresh_MatchStarted","prior":match_id,"cutoff":cutoff}),
                |v| {
                    v["type"] == "room_update"
                        && v["event"]["type"] == "match_started"
                        && v["event"]["match_id"].is_string()
                        && v["event"]["match_id"] != match_id
                },
            )
            .await;
            assert!(!started["event"]["match_id"].as_str().unwrap().is_empty());
            let before = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert_eq!(before["room"]["result"], Value::Null);
            assert_eq!(before["room"]["kyoku_end"], Value::Null);
            confirm_with_handler_barrier(&mut sockets[0], &mut transcripts[0], &id, 101).await;
            let after_stale = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert_eq!(
                before, after_stale,
                "old final ID cannot modify fresh Match"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM match_kyoku_results WHERE match_id=?"
                )
                .bind(match_id)
                .fetch_one(storage.pool())
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                fs::read(
                    storage
                        .replay_root()
                        .join(row.get::<String, _>("replay_path"))
                )
                .unwrap(),
                bytes
            );
            sockets[0].diagnostic.record(0,"saved_final_draw_authority_NOT_wire",json!({"match_id":match_id,"submitted":submitted,"held_id":id,"settlement":settlement,"result":result,"record":payload,"canonical":canonical,"fresh_match_started":started}));
            for socket in &mut sockets {
                socket.close(None).await.unwrap();
            }
        }
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
        storage.pool().close().await;
    }

    #[tokio::test]
    async fn actual_missed_tick_republishes_each_final_double_ron_step_with_fresh_clock() {
        if run_in_fixture_process("double-ron-final") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "double-ron-final"
        );
        let state = app(false);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                server_router(serve_state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        let admin = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        http(
            &admin,
            &base,
            reqwest::Method::POST,
            "/api/v1/admin/login",
            json!({"username":"admin","password":"correct horse battery staple"}),
            200,
        )
        .await;
        let now_ms = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
        };
        for (mode, count) in [("3p-red-east", 3usize), ("4p-red-east", 4)] {
            let created=http(&admin,&base,reqwest::Method::POST,"/api/v1/admin/rooms",json!({"room_name":"Task8 missed native tick","game_mode":mode,"time_control":"unlimited","participant_limit":count,"replay_save":false}),201).await;
            let code = created["join_code"].as_str().unwrap();
            let (sockets, transcripts) = join_humans(&admin, &base, address, code, count).await;
            http(
                &admin,
                &base,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/start"),
                Value::Null,
                200,
            )
            .await;
            let mut mapped = Vec::new();
            for (mut socket, mut transcript) in sockets.into_iter().zip(transcripts) {
                let view = receive(
                    &mut socket,
                    &mut transcript,
                    json!({"phase":"missed_tick_seat_mapping"}),
                    |v| v["state"].is_object(),
                )
                .await;
                let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
                assert!(seat < count);
                mapped.push((seat, socket, transcript));
            }
            mapped.sort_by_key(|(seat, _, _)| *seat);
            assert_eq!(
                mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
                (0..count).collect::<Vec<_>>()
            );
            let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) =
                mapped.into_iter().map(|(_, s, t)| (s, t)).unzip();
            choose(&mut sockets[0], &mut transcripts[0], "discard").await;
            choose(&mut sockets[1], &mut transcripts[1], "Ron").await;
            choose(&mut sockets[2], &mut transcripts[2], "Ron").await;
            let first = receive(
                &mut sockets[0],
                &mut transcripts[0],
                json!({"phase":"actual_Win0_before_executor_starvation"}),
                |v| v["room"]["kyoku_end"]["id"]["step_index"] == 0,
            )
            .await;
            let first_step = first["room"]["kyoku_end"].clone();
            assert_eq!(
                first_step["deadline_at_ms"].as_u64().unwrap()
                    - first_step["published_at_ms"].as_u64().unwrap(),
                5000
            );
            assert_boundary(&first, 1, Some(0));
            assert_eq!(first["state"]["decision"], Value::Null);
            assert_eq!(first["room"]["result"], Value::Null);
            let before_stall = now_ms();
            sockets[0].diagnostic.record(0,"executor_starvation_begin",json!({"actual_first_step":first_step,"requested_block_ms":11000,"runtime":"current_thread"}));
            // Deliberately miss the actor's wake on this test runtime; production clocks stay unchanged.
            std::thread::sleep(Duration::from_millis(11000));
            let resumed = now_ms();
            assert!(resumed >= before_stall + 11000);
            sockets[0].diagnostic.record(0,"executor_starvation_end",json!({"before_ms":before_stall,"resumed_ms":resumed,"actual_gap_ms":resumed-before_stall}));
            let second = receive(
                &mut sockets[0],
                &mut transcripts[0],
                json!({"phase":"actual_Win1_after_missed_tick"}),
                |v| v["room"]["kyoku_end"]["id"]["step_index"] == 1,
            )
            .await;
            let second_step = second["room"]["kyoku_end"].clone();
            let second_id = second_step["id"].clone();
            assert_eq!(second_id["match_id"], first_step["id"]["match_id"]);
            assert_eq!(second_id["kyoku_sequence"], 1);
            assert!(second_step["published_at_ms"].as_u64().unwrap() >= resumed);
            assert_eq!(
                second_step["deadline_at_ms"].as_u64().unwrap()
                    - second_step["published_at_ms"].as_u64().unwrap(),
                5000
            );
            assert_eq!(second_step["confirmed"], json!([]));
            assert_eq!(second_step["required"], first_step["required"]);
            assert_eq!(second_step["required"].as_array().unwrap().len(), count);
            assert_eq!(
                second["state"]["kyoku_results"]["wins"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(second["state"]["kyoku_results"]["settlement"], Value::Null);
            assert_eq!(second["state"]["decision"], Value::Null);
            assert_eq!(second["room"]["result"], Value::Null);
            confirm_with_handler_barrier(
                &mut sockets[0],
                &mut transcripts[0],
                &first_step["id"],
                200,
            )
            .await;
            let old = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert_eq!(old["room"]["kyoku_end"]["id"], second_id);
            assert_eq!(old["room"]["kyoku_end"]["confirmed"], json!([]));
            assert_eq!(
                old["room"]["kyoku_end"]["published_at_ms"],
                second_step["published_at_ms"]
            );
            assert_eq!(
                old["room"]["kyoku_end"]["deadline_at_ms"],
                second_step["deadline_at_ms"]
            );
            for seat in 0..count {
                confirm_with_handler_barrier(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    &second_id,
                    201 + seat,
                )
                .await;
            }
            let settlement_view = receive(
                &mut sockets[0],
                &mut transcripts[0],
                json!({"phase":"actual_final_Settlement2"}),
                |v| v["room"]["kyoku_end"]["id"]["step_index"] == 2,
            )
            .await;
            let third = settlement_view["room"]["kyoku_end"].clone();
            let id = third["id"].clone();
            assert_eq!(id["match_id"], second_id["match_id"]);
            assert_eq!(id["kyoku_sequence"], 1);
            assert_eq!(
                third["deadline_at_ms"].as_u64().unwrap()
                    - third["published_at_ms"].as_u64().unwrap(),
                5000
            );
            assert_eq!(third["confirmed"], json!([]));
            assert_eq!(third["required"], first_step["required"]);
            assert_eq!(settlement_view["state"]["decision"], Value::Null);
            assert_eq!(settlement_view["room"]["result"], Value::Null);
            assert_eq!(settlement_view["room"]["phase"], "playing");
            let after = if count == 3 {
                json!([-4600, 105000, 102600])
            } else {
                json!([-4800, 105200, 102600, 100000])
            };
            assert_eq!(
                third["content"]["Settlement"]["changes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c["after"].clone())
                    .collect::<Vec<_>>(),
                after.as_array().unwrap().clone()
            );
            for seat in 0..count - 1 {
                confirm_with_handler_barrier(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    &id,
                    210 + seat,
                )
                .await;
            }
            let partial = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert_eq!(partial["room"]["kyoku_end"]["id"], id);
            assert_eq!(partial["room"]["result"], Value::Null);
            assert_eq!(partial["state"]["decision"], Value::Null);
            assert_eq!(
                partial["room"]["kyoku_end"]["confirmed"]
                    .as_array()
                    .unwrap()
                    .len(),
                count - 1
            );
            assert_eq!(
                partial["room"]["kyoku_end"]["deadline_at_ms"],
                third["deadline_at_ms"]
            );
            let deadline = third["deadline_at_ms"].as_u64().unwrap();
            tokio::time::sleep(Duration::from_millis(
                deadline.saturating_sub(now_ms()) + 25,
            ))
            .await;
            let final_view = receive(
                &mut sockets[0],
                &mut transcripts[0],
                json!({"phase":"actual_final_timeout_PostMatch","deadline_ms":deadline}),
                |v| v["room"]["phase"] == "post_match",
            )
            .await;
            assert!(now_ms() >= deadline);
            let final_result = final_view["room"]["result"].clone();
            assert_eq!(final_result["final_scores"], after);
            assert_eq!(
                final_result["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["rank"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                if count == 3 {
                    vec![3, 1, 2]
                } else {
                    vec![4, 1, 2, 3]
                }
            );
            confirm_with_handler_barrier(&mut sockets[0], &mut transcripts[0], &id, 220).await;
            let duplicate = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert_eq!(duplicate["room"]["result"], final_result);
            assert_eq!(duplicate["room"]["kyoku_end"], Value::Null);
            sockets[0].diagnostic.record(0,"missed_tick_final_authority_NOT_wire",json!({"first":first_step,"second":second_step,"third":third,"result":final_result,"starvation_gap_ms":resumed-before_stall}));
            for socket in &mut sockets {
                socket.close(None).await.unwrap();
            }
        }
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn actual_dealer_tenhou_first_release_keeps_old_step_out_of_natural_renchan_decision() {
        if run_in_fixture_process("dealer-tenhou") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "dealer-tenhou"
        );
        let state = app(false);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                server_router(serve_state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        let base = format!("http://{address}");
        let admin = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        http(
            &admin,
            &base,
            reqwest::Method::POST,
            "/api/v1/admin/login",
            json!({"username":"admin","password":"correct horse battery staple"}),
            200,
        )
        .await;
        let created=http(&admin,&base,reqwest::Method::POST,"/api/v1/admin/rooms",json!({"room_name":"Task8 Tenhou first renchan","game_mode":"4p-red-east","time_control":"unlimited","participant_limit":4,"replay_save":false}),201).await;
        let code = created["join_code"].as_str().unwrap();
        let (sockets, transcripts) = join_humans(&admin, &base, address, code, 4).await;
        http(
            &admin,
            &base,
            reqwest::Method::POST,
            &format!("/api/v1/admin/rooms/{code}/start"),
            Value::Null,
            200,
        )
        .await;
        let mut mapped = Vec::new();
        for (mut socket, mut transcript) in sockets.into_iter().zip(transcripts) {
            let view = receive(
                &mut socket,
                &mut transcript,
                json!({"phase":"tenhou_actual_seat_mapping"}),
                |v| v["state"].is_object(),
            )
            .await;
            let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
            assert!(seat < 4);
            let player = view["room"]["match_players"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["seat"] == seat)
                .unwrap();
            assert_eq!(player["participant_id"], socket.participant_id);
            mapped.push((seat, socket, transcript));
        }
        mapped.sort_by_key(|(seat, _, _)| *seat);
        assert_eq!(
            mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) =
            mapped.into_iter().map(|(_, s, t)| (s, t)).unzip();
        let initial = transcripts[0]
            .iter()
            .rev()
            .find(|v| v["state"].is_object())
            .unwrap()
            .clone();
        assert_eq!(
            initial["state"]["players"][0]["hand"],
            json!([36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76, 77])
        );
        assert_eq!(initial["state"]["honba"], 2);
        assert_eq!(initial["state"]["kyotaku"], 2);
        choose(&mut sockets[0], &mut transcripts[0], "Tsumo").await;
        let mut win_views = Vec::new();
        for seat in 0..4 {
            let view = receive(
                &mut sockets[seat],
                &mut transcripts[seat],
                json!({"phase":"actual_Tenhou_win0","seat":seat}),
                |v| v["room"]["kyoku_end"]["id"]["step_index"] == 0,
            )
            .await;
            assert_eq!(view["state"]["decision"], Value::Null);
            assert_eq!(view["room"]["result"], Value::Null);
            assert_eq!(view["state"]["kyoku_results"]["settlement"], Value::Null);
            assert_boundary(&view, 1, Some(seat as u64));
            let wins = view["state"]["kyoku_results"]["wins"].as_array().unwrap();
            assert_eq!(wins.len(), 1);
            assert_eq!(wins[0]["seat"], 0);
            assert_eq!(wins[0]["win_points"], 48000);
            assert_eq!(wins[0]["evaluation"], json!({"Yakuman":{"multiplier":1}}));
            assert_eq!(wins[0]["winning_tile"], 77);
            assert_eq!(wins[0]["winning_tile_index"], 13);
            assert_eq!(
                wins[0]["concealed"],
                json!([36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76, 77])
            );
            assert_eq!(wins[0]["yaku"].as_array().unwrap().len(), 1);
            assert_eq!(wins[0]["yaku"][0]["id"], 35);
            assert_eq!(wins[0]["yaku"][0]["han"], Value::Null);
            assert_eq!(wins[0]["yaku"][0]["yakuman_multiplier"], 1);
            win_views.push(view);
        }
        let first = win_views[0]["room"]["kyoku_end"].clone();
        let match_id = first["id"]["match_id"].as_str().unwrap().to_owned();
        let mut settlement_id = first["id"].clone();
        settlement_id["step_index"] = json!(1);
        assert_eq!(first["id"]["kyoku_sequence"], 1);
        assert_eq!(
            first["deadline_at_ms"].as_u64().unwrap() - first["published_at_ms"].as_u64().unwrap(),
            5000
        );
        let room = state.rooms().get(code).await.unwrap();
        let double_riichi_core::RoomResponse::MatchEvents(events) = room
            .send(double_riichi_core::RoomCommand::GetMatchEvents)
            .await
            .unwrap()
        else {
            panic!("held canonical")
        };
        sockets[0].diagnostic.record(
            0,
            "held_Tenhou_canonical_NOT_Human_wire",
            serde_json::to_value(&events).unwrap(),
        );
        assert_eq!(
            events
                .iter()
                .map(double_riichi_core::GameEvent::kind)
                .collect::<Vec<_>>(),
            ["start_game", "start_kyoku", "tsumo"]
        );
        for seat in 0..4 {
            assert!(
                first["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(sockets[seat].participant_id))
            );
            foreign_match_confirm_barrier(
                &mut sockets[seat],
                &mut transcripts[seat],
                &first["id"],
                100 + seat,
                1,
                if seat == 3 {
                    Some(&settlement_id)
                } else {
                    None
                },
            )
            .await;
        }
        let settlement = receive(
            &mut sockets[0],
            &mut transcripts[0],
            json!({"phase":"actual_Tenhou_settlement1"}),
            |v| v["room"]["kyoku_end"]["id"]["step_index"] == 1,
        )
        .await;
        let second = settlement["room"]["kyoku_end"].clone();
        assert_eq!(second["id"]["match_id"], match_id);
        assert_eq!(second["id"]["kyoku_sequence"], 1);
        assert_eq!(settlement["state"]["decision"], Value::Null);
        assert_eq!(settlement["room"]["result"], Value::Null);
        let changes = settlement["state"]["kyoku_results"]["settlement"]["changes"]
            .as_array()
            .unwrap();
        assert_eq!(
            changes
                .iter()
                .map(|c| c["delta"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![50600, -16200, -16200, -16200]
        );
        assert_eq!(
            changes
                .iter()
                .map(|c| c["after"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![150600, 83800, 83800, 83800]
        );
        assert_eq!(
            settlement["state"]["kyoku_results"]["settlement"]["pot_before"],
            2
        );
        assert_eq!(
            settlement["state"]["kyoku_results"]["settlement"]["pot_after"],
            0
        );
        for seat in 0..4 {
            foreign_match_confirm_barrier(
                &mut sockets[seat],
                &mut transcripts[seat],
                &second["id"],
                110 + seat,
                1,
                Some(&settlement_id),
            )
            .await;
        }
        let next = receive(
            &mut sockets[0],
            &mut transcripts[0],
            json!({"phase":"actual_first_natural_renchan_Decision"}),
            |v| v["state"]["decision"].is_object(),
        )
        .await;
        assert_eq!(next["room"]["kyoku_end"], Value::Null);
        assert_eq!(next["room"]["result"], Value::Null);
        assert_eq!(next["room"]["phase"], "playing");
        assert_eq!(next["state"]["round"], "East");
        assert_eq!(next["state"]["kyoku"], 1);
        assert_eq!(next["state"]["dealer"], 0);
        assert_eq!(next["state"]["honba"], 3);
        assert_eq!(next["state"]["kyotaku"], 0);
        assert_eq!(
            next["state"]["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["score"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![150600, 83800, 83800, 83800]
        );
        assert_ne!(
            next["state"]["players"][0]["hand"], initial["state"]["players"][0]["hand"],
            "real shuffled next hand, not fixture reapplication"
        );
        let before = foreign_match_actor_view(&state, code, &sockets[0]).await;
        foreign_match_confirm_barrier(
            &mut sockets[0],
            &mut transcripts[0],
            &first["id"],
            120,
            1,
            None,
        )
        .await;
        let after = foreign_match_actor_view(&state, code, &sockets[0]).await;
        assert_eq!(
            after, before,
            "genuine old K1 step does not modify a K2 Decision; NOT a K2-held-required proof"
        );
        let double_riichi_core::RoomResponse::MatchView {
            snapshot,
            match_id: actual_id,
            events,
            history,
            ..
        } = room
            .send(double_riichi_core::RoomCommand::GetMatchView {
                participant_id: sockets[0].participant_id.clone().into(),
            })
            .await
            .unwrap()
        else {
            panic!("renchan authority")
        };
        let authority =
            json!({"snapshot":snapshot,"match_id":actual_id,"events":events,"history":history});
        sockets[0].diagnostic.record(
            0,
            "first_natural_renchan_authority_NOT_Human_wire",
            authority.clone(),
        );
        assert_eq!(authority["match_id"], match_id);
        // Second ordinal is source-defined StartKyoku count, not a wire-exposed K2 ResultStepID.
        let current_history = authority["history"]["current_kyoku"]["events"]
            .as_array()
            .unwrap();
        assert_eq!(
            current_history
                .iter()
                .filter(|e| e.get("StartKyoku").is_some())
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .map(double_riichi_core::GameEvent::kind)
                .collect::<Vec<_>>(),
            [
                "start_game",
                "start_kyoku",
                "tsumo",
                "hora",
                "end_kyoku",
                "start_kyoku",
                "tsumo"
            ]
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, double_riichi_core::GameEvent::StartKyoku { .. }))
                .count(),
            2
        );
        let canonical = serde_json::to_value(events).unwrap();
        assert_eq!(
            canonical[3]["Hora"]["delta"],
            json!([50600, -16200, -16200, -16200])
        );
        assert_eq!(canonical[5]["StartKyoku"]["honba"], 3);
        assert_eq!(canonical[5]["StartKyoku"]["oya"], 0);
        assert_eq!(canonical[5]["StartKyoku"]["kyotaku"], 0);

        // Independent noten oracle: the fixed seed's unchanged 13-tile hands cannot
        // complete with one tile. Seats 0/2/3 have multiple isolated honor types;
        // seat 1 has an isolated West plus a White pair (neither can both be used).
        // None has six pairs or the orphan types; normal discards also rule out nagashi.
        let unchanged_hands = [
            json!([34, 37, 38, 50, 68, 90, 91, 96, 97, 98, 99, 119, 122]),
            json!([0, 6, 18, 22, 49, 53, 58, 70, 72, 104, 117, 124, 125]),
            json!([3, 10, 21, 23, 33, 55, 60, 69, 75, 86, 110, 131, 135]),
            json!([13, 16, 54, 64, 71, 76, 78, 95, 106, 112, 120, 121, 126]),
        ];
        // The natural second wall is never replaced: decline calls/wins and discard draws.
        // At most 70 discards plus three responses per discard; no blind unbounded play.
        let mut k2 = None;
        let mut submitted = 0;
        for _ in 0..=280 {
            let mut acted = false;
            for seat in 0..4 {
                let view = foreign_match_actor_view(&state, code, &sockets[seat]).await;
                let own_hand = view["state"]["players"][seat]["hand"].as_array().unwrap();
                assert_eq!(&own_hand[..13], unchanged_hands[seat].as_array().unwrap());
                if view["room"]["kyoku_end"].is_object() {
                    k2 = Some(view);
                    break;
                }
                let decision = &view["state"]["decision"];
                let Some(actions) = decision["actions"].as_array().filter(|a| !a.is_empty()) else {
                    continue;
                };
                let kind = if actions.iter().any(|a| a["action"] == "Pass") {
                    "Pass"
                } else {
                    // Actor serde enums are PascalCase; the actual Human wire is normalized.
                    assert!(
                        actions
                            .iter()
                            .any(|a| a["action"]["Discard"]["tsumogiri"] == true)
                    );
                    "tsumogiri"
                };
                let decision_id = decision["decision_id"].clone();
                receive(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    json!({"phase":"natural_K2_offered_Decision","decision_id":decision_id}),
                    |v| v["state"]["decision"]["decision_id"] == decision_id,
                )
                .await;
                choose(&mut sockets[seat], &mut transcripts[seat], kind).await;
                acted = true;
                submitted += 1;
                break;
            }
            if k2.is_some() {
                break;
            }
            assert!(acted, "natural K2 must offer an action or publish a result");
        }
        let k2 = k2.expect("natural K2 result within the physical live-wall bound");
        let held = k2["room"]["kyoku_end"].clone();
        assert_eq!(held["id"]["match_id"], match_id);
        assert_eq!(held["id"]["kyoku_sequence"], 2);
        assert_eq!(held["id"]["step_index"], 0);
        assert_eq!(held["content"]["Draw"]["draw_reason"], "exhaustive_draw");
        assert_eq!(k2["state"]["kyoku_results"]["wins"], json!([]));
        assert_eq!(k2["state"]["decision"], Value::Null);
        assert_eq!(k2["room"]["result"], Value::Null);
        assert_eq!(held["confirmed"], json!([]));
        assert!(
            held["required"]
                .as_array()
                .unwrap()
                .contains(&json!(sockets[0].participant_id))
        );
        let changes = held["content"]["Draw"]["changes"].as_array().unwrap();
        assert_eq!(
            changes
                .iter()
                .map(|c| c["delta"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![0, 0, 0, 0]
        );
        assert_eq!(
            changes
                .iter()
                .map(|c| c["after"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![150600, 83800, 83800, 83800]
        );
        for player in k2["state"]["players"].as_array().unwrap() {
            assert!(
                player["discards"].as_array().unwrap().iter().any(|tile| {
                    let kind = tile.as_u64().unwrap() / 4;
                    kind < 27 && kind % 9 != 0 && kind % 9 != 8
                }),
                "a simple discard prevents nagashi"
            );
        }
        sockets[0].diagnostic.record(
            0,
            "natural_K2_held_authority_NOT_Human_wire",
            json!({"submitted":submitted,"view":k2}),
        );
        for (counter, id, expected_confirmed) in [
            (130, &first["id"], json!([])),
            (131, &second["id"], json!([])),
            (132, &held["id"], json!([sockets[0].participant_id])),
            (133, &held["id"], json!([sockets[0].participant_id])),
        ] {
            foreign_match_confirm_barrier(
                &mut sockets[0],
                &mut transcripts[0],
                id,
                counter,
                1,
                Some(&held["id"]),
            )
            .await;
            let after = foreign_match_actor_view(&state, code, &sockets[0]).await;
            assert!(foreign_match_step_unchanged(
                &held,
                &after["room"]["kyoku_end"],
                &expected_confirmed
            ));
            assert_eq!(after["state"]["decision"], Value::Null);
            assert_eq!(after["room"]["result"], Value::Null);
            assert_eq!(
                after["state"]["kyoku_results"],
                k2["state"]["kyoku_results"]
            );
        }
        for (seat, transcript) in transcripts.iter().enumerate() {
            for value in transcript {
                assert_boundary(value, 1, Some(seat as u64));
            }
        }
        for socket in &mut sockets {
            socket.close(None).await.unwrap();
        }
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
    }

    fn foreign_match_step_unchanged(before: &Value, after: &Value, confirmed: &Value) -> bool {
        [
            "id",
            "content",
            "published_at_ms",
            "deadline_at_ms",
            "required",
        ]
        .iter()
        .all(|key| before[key] == after[key])
            && after["confirmed"] == *confirmed
            && after["deadline_at_ms"].as_u64().unwrap()
                - after["published_at_ms"].as_u64().unwrap()
                == 5000
            && after["remaining_ms"]
                .as_u64()
                .is_some_and(|ms| ms > 0 && ms <= 5000)
    }

    async fn foreign_match_actor_view(
        state: &ServerState,
        code: &str,
        socket: &HumanSocket,
    ) -> Value {
        let room = state.rooms().get(code).await.unwrap();
        let double_riichi_core::RoomResponse::MatchView {
            snapshot,
            projection,
            ..
        } = room
            .send(double_riichi_core::RoomCommand::GetMatchView {
                participant_id: socket.participant_id.clone().into(),
            })
            .await
            .unwrap()
        else {
            panic!("actor MatchView response")
        };
        let view = json!({"room":snapshot,"state":projection});
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        socket.diagnostic.record(
            socket.peer,
            "after_barrier_actor_view_NOT_Human_wire",
            json!({"room_code":code,"observed_at_ms":now,"view":view}),
        );
        if !view["room"]["kyoku_end"].is_null() {
            assert!(
                now < view["room"]["kyoku_end"]["deadline_at_ms"]
                    .as_u64()
                    .unwrap(),
                "actual current actor read before fixed publication deadline"
            );
        }
        view
    }

    fn confirmation_settlement_within_frontier(
        value: &Value,
        settlement_id: Option<&Value>,
    ) -> bool {
        let settlement = &value["state"]["kyoku_results"]["settlement"];
        settlement.is_null()
            || settlement_id.is_some_and(|id| {
                let kind = if id["step_index"] == 0 {
                    "Draw"
                } else {
                    "Settlement"
                };
                value["room"]["kyoku_end"]["id"] == *id
                    && value["room"]["kyoku_end"]["content"].get(kind) == Some(settlement)
            })
    }

    #[test]
    fn confirmation_wire_frontier_requires_the_allowed_settlement_identity_and_content() {
        // Detector controls only; actual gameplay/payment evidence comes from the Human tests.
        let id = json!({"match_id":"match-a","kyoku_sequence":1,"step_index":1});
        let settlement = json!({"pot_before":2,"pot_after":0});
        let view = json!({
            "room":{"kyoku_end":{"id":id,"content":{"Settlement":settlement}}},
            "state":{"kyoku_results":{"settlement":settlement}}
        });
        assert!(confirmation_settlement_within_frontier(&view, Some(&id)));
        let mut draw = view.clone();
        let draw_id = json!({"match_id":"match-a","kyoku_sequence":2,"step_index":0});
        draw["room"]["kyoku_end"]["id"] = draw_id.clone();
        draw["room"]["kyoku_end"]["content"] = json!({"Draw":settlement});
        assert!(confirmation_settlement_within_frontier(
            &draw,
            Some(&draw_id)
        ));
        assert!(
            !confirmation_settlement_within_frontier(&draw, Some(&id)),
            "old Kyoku ID"
        );
        assert!(
            !confirmation_settlement_within_frontier(&draw, None),
            "premature draw"
        );
        draw["room"]["kyoku_end"]["content"] = json!({"Settlement":settlement});
        assert!(
            !confirmation_settlement_within_frontier(&draw, Some(&draw_id)),
            "draw has one Draw step, not a Win settlement"
        );
        assert!(
            !confirmation_settlement_within_frontier(&view, None),
            "premature settlement"
        );
        for (field, wrong) in [
            ("match_id", json!("match-b")),
            ("kyoku_sequence", json!(2)),
            ("step_index", json!(0)),
        ] {
            let mut invalid = view.clone();
            invalid["room"]["kyoku_end"]["id"][field] = wrong;
            assert!(
                !confirmation_settlement_within_frontier(&invalid, Some(&id)),
                "wrong {field}"
            );
        }
        let mut invalid = view.clone();
        invalid["room"]["kyoku_end"]["content"] = json!({"Win":{}});
        assert!(!confirmation_settlement_within_frontier(
            &invalid,
            Some(&id)
        ));
        invalid["room"]["kyoku_end"]["content"] = json!({"Settlement":{"pot_before":99}});
        assert!(!confirmation_settlement_within_frontier(
            &invalid,
            Some(&id)
        ));
        invalid["state"]["kyoku_results"]["settlement"] = Value::Null;
        assert!(confirmation_settlement_within_frontier(&invalid, None));
    }

    async fn foreign_match_confirm_barrier(
        socket: &mut HumanSocket,
        transcript: &mut Vec<Value>,
        step_id: &Value,
        counter: usize,
        winners: u64,
        settlement_id: Option<&Value>,
    ) {
        let nonce = format!(
            "task8-confirm:{}:{}:{counter}",
            step_id["match_id"].as_str().unwrap(),
            step_id["step_index"]
        );
        assert!(nonce.is_ascii() && nonce.len() <= 125);
        socket.diagnostic.record(socket.peer, "confirm_barrier_begin", json!({"step_id":step_id,"counter":counter,"nonce":nonce,"transcript_start":transcript.len(),"epoch_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()}));
        socket
            .send(Message::Text(
                json!({"type":"confirm_result","step_id":step_id})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Ping(nonce.clone().into_bytes().into()))
            .await
            .unwrap();
        let viewer = transcript
            .iter()
            .rev()
            .find_map(|v| v["state"]["viewer_seat"].as_u64())
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match socket.wire.next().await {
                    Some(Ok(Message::Text(text))) => {
                        socket.diagnostic.record(socket.peer, "confirm_barrier_received_text", json!({"raw":text.as_str(),"counter":counter,"epoch_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()}));
                        let value: Value = serde_json::from_str(&text).unwrap();
                        transcript.push(value.clone());
                        assert_boundary(&value, winners, Some(viewer));
                        if value["state"].is_object() && !value["state"]["kyoku_results"].is_null() {
                            assert_eq!(value["state"]["decision"], Value::Null);
                            assert!(confirmation_settlement_within_frontier(&value, settlement_id), "settlement outside allowed publication frontier");
                        }
                        if value.get("room").is_some() { assert_eq!(value["room"]["result"], Value::Null); }
                    }
                    Some(Ok(Message::Ping(bytes))) => { socket.diagnostic.record(socket.peer, "confirm_barrier_heartbeat_ping", json!({"bytes":bytes.to_vec()})); socket.send(Message::Pong(bytes)).await.unwrap(); }
                    Some(Ok(Message::Pong(bytes))) => {
                        socket.diagnostic.record(socket.peer, "confirm_barrier_pong", json!({"bytes":bytes.to_vec(),"counter":counter,"epoch_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()}));
                        if bytes.as_ref() == nonce.as_bytes() { break; }
                    }
                    other => panic!("Human confirmation barrier wire ended: {other:?}"),
                }
            }
        }).await.expect("matching control Pong within existing protocol timeout");
        // The server awaits the confirm handler before reading the next Ping.
        // Pong does NOT flush the separate outbound Snapshot queue; the next actor read is authority.
    }

    #[tokio::test]
    async fn live_foreign_match_confirmation_keeps_double_ron_publication_bound_to_its_match() {
        if run_in_fixture_process("double-ron") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "double-ron"
        );
        let state = app(false);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                server_router(serve_state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        let base = format!("http://{address}");
        let admin = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        http(
            &admin,
            &base,
            reqwest::Method::POST,
            "/api/v1/admin/login",
            json!({"username":"admin","password":"correct horse battery staple"}),
            200,
        )
        .await;
        let mut rooms = Vec::new();
        // All eight real Humans are selected and Ready before either publication clock starts.
        for label in ["A", "B"] {
            let created = http(&admin, &base, reqwest::Method::POST, "/api/v1/admin/rooms", json!({"room_name":format!("Task8 foreign {label}"),"game_mode":"4p-red-east","time_control":"unlimited","participant_limit":4,"replay_save":false}), 201).await;
            let code = created["join_code"].as_str().unwrap().to_owned();
            let (sockets, transcripts) = join_humans(&admin, &base, address, &code, 4).await;
            rooms.push((code, sockets, transcripts));
        }
        let mut first_steps = Vec::new();
        for (code, sockets, transcripts) in &mut rooms {
            http(
                &admin,
                &base,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/start"),
                Value::Null,
                200,
            )
            .await;
            let mut mapped = Vec::new();
            for (mut socket, mut transcript) in sockets.drain(..).zip(transcripts.drain(..)) {
                let view = receive(
                    &mut socket,
                    &mut transcript,
                    json!({"phase":"foreign_test_actual_seat_mapping"}),
                    |v| v["state"].is_object(),
                )
                .await;
                let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
                assert!(seat < 4);
                let player = view["room"]["match_players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["seat"] == seat)
                    .unwrap();
                assert_eq!(player["participant_id"], socket.participant_id);
                mapped.push((seat, socket, transcript));
            }
            mapped.sort_by_key(|(seat, _, _)| *seat);
            assert_eq!(
                mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
                vec![0, 1, 2, 3]
            );
            for (_, socket, transcript) in mapped {
                sockets.push(socket);
                transcripts.push(transcript);
            }
            choose(&mut sockets[0], &mut transcripts[0], "discard").await;
            choose(&mut sockets[1], &mut transcripts[1], "Ron").await;
            choose(&mut sockets[2], &mut transcripts[2], "Ron").await;
            let view = receive(
                &mut sockets[1],
                &mut transcripts[1],
                json!({"phase":"actual_foreign_source_step0"}),
                |v| v["room"]["kyoku_end"]["id"]["step_index"] == 0,
            )
            .await;
            assert_boundary(&view, 1, Some(1));
            let step = view["room"]["kyoku_end"].clone();
            assert!(
                step["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(sockets[1].participant_id))
            );
            assert_eq!(step["id"]["kyoku_sequence"], 1);
            assert_eq!(step["confirmed"], json!([]));
            first_steps.push(step);
        }
        assert_ne!(
            first_steps[0]["id"]["match_id"],
            first_steps[1]["id"]["match_id"]
        );
        let (a_code, a_sockets, a_transcripts) = &mut rooms[0];
        let a_id = a_sockets[1].participant_id.clone();
        let first = &first_steps[0];
        let mut settlement_id = first["id"].clone();
        settlement_id["step_index"] = json!(2);
        let foreign_id = &first_steps[1]["id"];
        let initial = foreign_match_actor_view(&state, a_code, &a_sockets[1]).await;
        assert!(foreign_match_step_unchanged(
            first,
            &initial["room"]["kyoku_end"],
            &json!([])
        ));
        let mut poisoned = initial["room"]["kyoku_end"].clone();
        poisoned["confirmed"] = json!([a_id]);
        assert!(
            !foreign_match_step_unchanged(first, &poisoned, &json!([])),
            "accepting the foreign ID must fail the actual unchanged-state detector"
        );
        for (counter, id, expected_confirmed) in [
            (1, foreign_id, json!([])),
            (2, &first["id"], json!([a_id])),
            (3, &first["id"], json!([a_id])),
        ] {
            foreign_match_confirm_barrier(
                &mut a_sockets[1],
                &mut a_transcripts[1],
                id,
                counter,
                1,
                None,
            )
            .await;
            let authority = foreign_match_actor_view(&state, a_code, &a_sockets[1]).await;
            assert!(foreign_match_step_unchanged(
                first,
                &authority["room"]["kyoku_end"],
                &expected_confirmed
            ));
            assert_eq!(authority["state"]["decision"], Value::Null);
            assert_eq!(authority["room"]["result"], Value::Null);
            assert_eq!(
                authority["state"]["kyoku_results"]["settlement"],
                Value::Null
            );
            let wins = authority["state"]["kyoku_results"]["wins"]
                .as_array()
                .unwrap();
            assert_eq!(wins.len(), 1);
            assert_eq!(wins[0]["seat"], 1);
            assert_eq!(wins[0]["win_points"], 2600);
            assert_eq!(wins[0]["winning_tile"], 77);
            assert_eq!(wins[0]["winning_tile_index"], 13);
            assert_eq!(
                wins[0]["concealed"],
                json!([36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76, 77])
            );
            assert_boundary(&authority, 1, Some(1));
        }
        for seat in [0, 2, 3] {
            foreign_match_confirm_barrier(
                &mut a_sockets[seat],
                &mut a_transcripts[seat],
                &first["id"],
                10 + seat,
                if seat == 3 { 2 } else { 1 },
                None,
            )
            .await;
        }
        let second_view = receive(
            &mut a_sockets[1],
            &mut a_transcripts[1],
            json!({"phase":"actual_A_second_publication"}),
            |v| v["room"]["kyoku_end"]["id"]["step_index"] == 1,
        )
        .await;
        let second = second_view["room"]["kyoku_end"].clone();
        assert_eq!(second["id"]["match_id"], first["id"]["match_id"]);
        assert_eq!(second["confirmed"], json!([]));
        for (counter, id) in [(21, &first["id"]), (22, foreign_id)] {
            foreign_match_confirm_barrier(
                &mut a_sockets[1],
                &mut a_transcripts[1],
                id,
                counter,
                2,
                None,
            )
            .await;
            let authority = foreign_match_actor_view(&state, a_code, &a_sockets[1]).await;
            assert!(foreign_match_step_unchanged(
                &second,
                &authority["room"]["kyoku_end"],
                &json!([])
            ));
            assert_eq!(authority["state"]["decision"], Value::Null);
            assert_eq!(authority["room"]["result"], Value::Null);
            assert_eq!(
                authority["state"]["kyoku_results"]["settlement"],
                Value::Null
            );
            assert_eq!(
                authority["state"]["kyoku_results"]["wins"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_boundary(&authority, 2, Some(1));
        }
        for seat in 0..4 {
            foreign_match_confirm_barrier(
                &mut a_sockets[seat],
                &mut a_transcripts[seat],
                &second["id"],
                30 + seat,
                2,
                if seat == 3 {
                    Some(&settlement_id)
                } else {
                    None
                },
            )
            .await;
        }
        let settlement = receive(
            &mut a_sockets[1],
            &mut a_transcripts[1],
            json!({"phase":"actual_A_literal_settlement"}),
            |v| v["room"]["kyoku_end"]["id"]["step_index"] == 2,
        )
        .await;
        let wins = settlement["state"]["kyoku_results"]["wins"]
            .as_array()
            .unwrap();
        assert_eq!(
            wins.iter()
                .map(|w| w["seat"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(wins.iter().all(|w| w["winning_tile"] == 77
            && w["winning_tile_index"] == 13
            && w["win_points"] == 2600));
        let changes = settlement["state"]["kyoku_results"]["settlement"]["changes"]
            .as_array()
            .unwrap();
        assert_eq!(
            changes
                .iter()
                .map(|c| c["after"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![94200, 105200, 102600, 100000]
        );
        let room = state.rooms().get(a_code).await.unwrap();
        let double_riichi_core::RoomResponse::MatchEvents(canonical) = room
            .send(double_riichi_core::RoomCommand::GetMatchEvents)
            .await
            .unwrap()
        else {
            panic!("released canonical")
        };
        let canonical = serde_json::to_value(canonical).unwrap();
        a_sockets[1].diagnostic.record(
            1,
            "released_canonical_actor_read_NOT_Human_wire",
            canonical.clone(),
        );
        let horas = canonical
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.get("Hora"))
            .collect::<Vec<_>>();
        assert_eq!(horas.len(), 2);
        assert_eq!(horas[0]["delta"], json!([-3200, 5200, 0, 0]));
        assert_eq!(horas[1]["delta"], json!([-2600, 0, 2600, 0]));
        let (b_code, b_sockets, _) = &rooms[1];
        let b = foreign_match_actor_view(&state, b_code, &b_sockets[1]).await;
        assert!(
            foreign_match_step_unchanged(&first_steps[1], &b["room"]["kyoku_end"], &json!([])),
            "no A confirmation transfers into B"
        );
        for (_, sockets, transcripts) in &mut rooms {
            for (seat, transcript) in transcripts.iter().enumerate() {
                let mut frontier = 1;
                for value in transcript {
                    if value["room"]["kyoku_end"]["id"]["step_index"]
                        .as_u64()
                        .is_some_and(|step| step >= 1)
                    {
                        frontier = 2;
                    }
                    assert_boundary(value, frontier, Some(seat as u64));
                }
            }
            for socket in sockets {
                socket.close(None).await.unwrap();
            }
        }
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn actual_double_ron_http_and_human_complete_boundaries_keep_future_win_and_decision_private()
     {
        if run_in_fixture_process("double-ron") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "double-ron"
        );
        for (mode, count) in [("3p-red-east", 3_usize), ("4p-red-east", 4)] {
            let outer = ReceiveDiagnostic::new(&format!("http-outer-{count}p"));
            let state = app(false);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
            let serve_state = state.clone();
            let observed_outer = outer.clone();
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    server_router(serve_state)
                        .layer(axum::middleware::from_fn_with_state(
                            observed_outer,
                            observe_outer_response,
                        ))
                        .into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
            });
            let base = format!("http://{address}");
            let admin = reqwest::Client::builder()
                .cookie_store(true)
                .build()
                .unwrap();
            http(
                &admin,
                &base,
                reqwest::Method::POST,
                "/api/v1/admin/login",
                json!({"username":"admin","password":"correct horse battery staple"}),
                200,
            )
            .await;
            let created = http(&admin, &base, reqwest::Method::POST, "/api/v1/admin/rooms", json!({"room_name":"Task8 transport","game_mode":mode,"time_control":"unlimited","participant_limit":count+1,"replay_save":false}), 201).await;
            let code = created["join_code"].as_str().unwrap();
            let (mut sockets, mut transcripts) =
                join_humans(&admin, &base, address, code, count).await;
            let mut watcher = join_human_watcher(
                &base,
                "http://127.0.0.1:3000",
                address,
                code,
                sockets[0].diagnostic.clone(),
            )
            .await;
            let mut watcher_transcript = Vec::new();
            http(
                &admin,
                &base,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/start"),
                Value::Null,
                200,
            )
            .await;
            let mut mapped = Vec::new();
            let mut common_roster = None;
            for (mut socket, mut transcript) in sockets.into_iter().zip(transcripts) {
                let view = receive(
                    &mut socket,
                    &mut transcript,
                    json!({"phase":"initial_viewer_mapping"}),
                    |v| v["state"].is_object(),
                )
                .await;
                let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
                assert!(seat < count);
                assert_eq!(view["room"]["join_code"], code);
                assert_eq!(view["room"]["game_mode"], mode);
                assert_eq!(view["room"]["phase"], "playing");
                let roster = &view["room"]["match_players"];
                let player = roster
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["seat"] == seat)
                    .unwrap();
                assert_eq!(player["participant_id"], socket.participant_id);
                if let Some(previous) = &common_roster {
                    assert_eq!(previous, roster);
                } else {
                    common_roster = Some(roster.clone());
                }
                mapped.push((seat, socket, transcript));
            }
            mapped.sort_by_key(|(seat, _, _)| *seat);
            assert_eq!(
                mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
                (0..count).collect::<Vec<_>>()
            );
            let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) = mapped
                .into_iter()
                .map(|(_, socket, transcript)| (socket, transcript))
                .unzip();
            choose(&mut sockets[0], &mut transcripts[0], "discard").await;
            choose(&mut sockets[1], &mut transcripts[1], "Ron").await;
            choose(&mut sockets[2], &mut transcripts[2], "Ron").await;
            for step in 0..3 {
                assert_held_step(
                    &admin,
                    &base,
                    code,
                    step,
                    &mut sockets,
                    &mut transcripts,
                    &outer,
                    &mut watcher,
                    &mut watcher_transcript,
                )
                .await;
            }
            if count == 4 {
                let mut first_publication_seen = false;
                for value in &transcripts[3] {
                    if value["state"].is_object() {
                        assert_eq!(value["state"]["viewer_seat"], 3);
                        let decision = &value["state"]["decision"];
                        assert!(
                            decision.is_null() || decision["kind"] != "response",
                            "nonwinner received a response Decision"
                        );
                    }
                    if value["room"]["kyoku_end"]["id"]["step_index"] == 0 {
                        assert_eq!(value["state"]["decision"], Value::Null);
                        first_publication_seen = true;
                        break;
                    }
                }
                assert!(first_publication_seen);
                let original_peer = sockets[3].peer;
                let diagnostic = sockets[3].diagnostic.records.lock().unwrap();
                for record in diagnostic
                    .iter()
                    .filter(|r| r["peer"] == original_peer && r["kind"] == "sent")
                {
                    let message =
                        serde_json::from_str::<Value>(record["value"]["raw"].as_str().unwrap())
                            .unwrap_or(Value::Null);
                    assert_ne!(
                        message["type"], "submit_action",
                        "nonwinner sent an unoffered action"
                    );
                }
            }
            for (seat, transcript) in transcripts.iter().enumerate() {
                let mut allowed_winners = 1;
                for value in transcript {
                    if let Some(step) = value["room"]["kyoku_end"]["id"]["step_index"].as_u64() {
                        allowed_winners = if step == 0 { 1 } else { 2 };
                    }
                    if !value["state"]["kyoku_results"].is_null() {
                        assert_eq!(value["state"]["decision"], Value::Null);
                    }
                    assert_boundary(value, allowed_winners, Some(seat as u64));
                }
            }
            let released = receive(
                &mut watcher,
                &mut watcher_transcript,
                json!({"phase":"public_Human_watcher_actual_K2_release"}),
                |v| v["state"]["kyoku"] == 2,
            )
            .await;
            assert_public_human_state(&released["state"]);
            assert_eq!(released["room"]["kyoku_end"], Value::Null);
            let mut allowed = 1;
            for value in &watcher_transcript {
                if let Some(step) = value["room"]["kyoku_end"]["id"]["step_index"].as_u64() {
                    allowed = if step == 0 { 1 } else { 2 };
                }
                assert_boundary(value, allowed, None);
                if value["state"].is_object() {
                    assert_public_human_state(&value["state"]);
                }
            }
            watcher.close(None).await.unwrap();
            println!(
                "Task8 {mode}: complete public/Admin detail/list HTTP, {count} Human wire boundaries plus public Human watcher, all three held publications; frames={:?}",
                transcripts.iter().map(Vec::len).collect::<Vec<_>>()
            );
            for socket in &mut sockets {
                socket.close(None).await.unwrap();
            }
            state.shutdown().await;
            stop.send(()).unwrap();
            server.await.unwrap();
        }
    }

    #[derive(Clone)]
    struct McpObserver {
        diagnostic: Arc<ReceiveDiagnostic>,
    }

    #[allow(clippy::manual_async_fn)]
    impl rmcp::ClientHandler for McpObserver {
        fn on_resource_updated(
            &self,
            params: rmcp::model::ResourceUpdatedNotificationParam,
            _context: rmcp::service::NotificationContext<rmcp::RoleClient>,
        ) -> impl std::future::Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
            async move {
                self.diagnostic.record(
                    usize::MAX,
                    "mcp_notification",
                    serde_json::to_value(params).unwrap(),
                );
            }
        }
        fn get_info(&self) -> rmcp::model::ClientConfig {
            rmcp::model::ClientConfig::new(
                rmcp::model::ClientCapabilities::default(),
                rmcp::model::Implementation::new("task8-mcp-observer", "1"),
            )
        }
    }

    async fn mcp_tool(
        peer: &rmcp::Peer<rmcp::RoleClient>,
        diagnostic: &ReceiveDiagnostic,
        name: &'static str,
        args: Value,
    ) -> Value {
        diagnostic.record(
            usize::MAX,
            "mcp_tool_request",
            json!({"name":name,"arguments":args}),
        );
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            peer.call_tool(
                rmcp::model::CallToolRequestParams::new(name)
                    .with_arguments(args.as_object().unwrap().clone()),
            ),
        )
        .await
        .expect("MCP tool timeout")
        .expect("MCP tool failure");
        let envelope = serde_json::to_value(&result).unwrap();
        diagnostic.record(
            usize::MAX,
            "mcp_tool_reply",
            json!({"name":name,"result":envelope}),
        );
        assert_ne!(result.is_error, Some(true));
        if let Some(value) = result.structured_content {
            value
        } else {
            serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap()).unwrap()
        }
    }

    async fn mcp_resource(
        peer: &rmcp::Peer<rmcp::RoleClient>,
        diagnostic: &ReceiveDiagnostic,
        uri: &str,
    ) -> Value {
        diagnostic.record(usize::MAX, "mcp_resource_request", json!({"uri":uri}));
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            peer.read_resource(rmcp::model::ReadResourceRequestParams::new(uri)),
        )
        .await
        .expect("MCP resource timeout")
        .expect("MCP resource failure");
        let envelope = serde_json::to_value(result).unwrap();
        diagnostic.record(
            usize::MAX,
            "mcp_resource_reply",
            json!({"uri":uri,"result":envelope}),
        );
        serde_json::from_str(envelope["contents"][0]["text"].as_str().unwrap()).unwrap()
    }

    fn assert_mcp_typed_boundary(value: &Value, winners: u64, step: u64) {
        assert_boundary(value, winners, None);
        match value {
            Value::Object(object) => {
                assert!(!object.contains_key("own_tehai"));
                for hidden in ["decision", "decision_expires_at", "remaining_ms_at_read"] {
                    if let Some(value) = object.get(hidden) {
                        assert!(value.is_null(), "held spectator must not receive {hidden}");
                    }
                }
                if let Some(kyoku) = object.get("kyoku").and_then(Value::as_u64) {
                    assert!(kyoku <= 1, "future Kyoku in actual first-East1 fixture");
                }
                if let Some(draw) = object.get("Tsumo") {
                    assert!(draw.get("tile").is_none());
                }
                assert!(!object.contains_key("EndKyoku"));
                assert!(!object.contains_key("EndGame"));
                if step < 2 {
                    if let Some(settlement) = object.get("settlement") {
                        assert!(settlement.is_null());
                    }
                }
                for child in object.values() {
                    assert_mcp_typed_boundary(child, winners, step);
                }
            }
            Value::Array(array) => {
                for child in array {
                    assert_mcp_typed_boundary(child, winners, step);
                }
            }
            Value::String(text) => {
                assert_ne!(text, "EndKyoku");
                assert_ne!(text, "EndGame");
                if let Ok(decoded) = serde_json::from_str::<Value>(text) {
                    if decoded != *value {
                        assert_mcp_typed_boundary(&decoded, winners, step);
                    }
                }
            }
            _ => {}
        }
    }

    fn assert_mcp_state(value: &Value, step_id: &Value, revision: u64, winners: u64) {
        assert_eq!(value["phase"]["Playing"], step_id["match_id"]);
        assert_eq!(value["kyoku_end"]["id"], *step_id);
        assert_eq!(value["revision"].as_u64().unwrap(), revision);
        assert_eq!(value["decision"], Value::Null);
        assert_eq!(value["decision_expires_at"], Value::Null);
        assert_eq!(value["remaining_ms_at_read"], Value::Null);
        assert!(value.get("viewer_seat").is_none());
        let wins = value["kyoku_results"]["wins"].as_array().unwrap();
        assert_eq!(
            wins.iter()
                .map(|w| w["seat"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            if winners == 1 { vec![1] } else { vec![1, 2] }
        );
        assert_eq!(wins[0]["win_points"], 2600);
        assert_eq!(wins[0]["winning_tile"], 77);
        assert_eq!(
            wins[0]["concealed"],
            json!([36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76, 77])
        );
        assert_eq!(wins[0]["winning_tile_index"], 13);
        assert_eq!(wins[0]["event_index"], 4);
        if winners == 2 {
            assert_eq!(
                wins[1]["concealed"],
                json!([37, 41, 45, 49, 54, 57, 61, 65, 69, 78, 80, 81, 82, 77])
            );
            assert_eq!(wins[1]["win_points"], 2600);
            assert_eq!(wins[1]["winning_tile_index"], 13);
            assert_eq!(wins[1]["event_index"], 5);
        }
        for player in value["players"].as_array().unwrap() {
            assert!(player.get("hand").is_none_or(Value::is_null));
        }
        assert_boundary(value, winners, None);
    }

    #[tokio::test]
    async fn unpublished_double_ron_mcp_get_wait_resources_and_history_follow_publication() {
        if run_in_fixture_process("double-ron") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "double-ron"
        );
        for (mode, count) in [("3p-red-east", 3_usize), ("4p-red-east", 4)] {
            let outer = ReceiveDiagnostic::new(&format!("mcp-outer-{count}p"));
            let root = temp_root("task8-mcp");
            let storage = Arc::new(Storage::connect(&root).await.unwrap());
            let tokens = Arc::new(BotTokenService::new(
                storage.clone(),
                Arc::new(BotTokenAuthority::empty()),
            ));
            let token = tokens.create("runner", 1, "task8-private").await.unwrap();
            let raw_token = token.secret().expose().to_owned();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let base = format!("http://{address}");
            let state = Arc::new(
                ServerState::for_tests(
                    &base,
                    Arc::new(
                        AdminAuthenticator::new(
                            "admin",
                            hash_password("correct horse battery staple").unwrap(),
                        )
                        .unwrap(),
                    ),
                    RoomRegistry::with_max_rooms(8),
                )
                .with_bot_token_service(tokens),
            );
            let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
            let serve_state = state.clone();
            let observed_outer = outer.clone();
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    server_router(serve_state)
                        .layer(axum::middleware::from_fn_with_state(
                            observed_outer,
                            observe_outer_response,
                        ))
                        .into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
            });
            let admin = reqwest::Client::builder()
                .cookie_store(true)
                .build()
                .unwrap();
            http_at_origin(
                &admin,
                &base,
                &base,
                reqwest::Method::POST,
                "/api/v1/admin/login",
                json!({"username":"admin","password":"correct horse battery staple"}),
                200,
            )
            .await;
            let created = http_at_origin(&admin, &base, &base, reqwest::Method::POST, "/api/v1/admin/rooms", json!({"room_name":"Task8 MCP","game_mode":mode,"time_control":"unlimited","participant_limit":count+1,"replay_save":false}), 201).await;
            let code = created["join_code"].as_str().unwrap();
            let (sockets, transcripts) =
                join_humans_at_origin(&admin, &base, &base, address, code, count).await;
            let diagnostic = sockets[0].diagnostic.clone();
            let (bridge_io, bot_io) = tokio::io::duplex(64 * 1024);
            let bridge = tokio::spawn(double_riichi_mcp::run_with_token_and_transport(
                double_riichi_mcp::BridgeConfig {
                    server: Url::parse(&format!("{base}/mcp")).unwrap(),
                },
                raw_token,
                bridge_io,
            ));
            let observer = tokio::time::timeout(
                Duration::from_secs(3),
                rmcp::serve_client(
                    McpObserver {
                        diagnostic: diagnostic.clone(),
                    },
                    bot_io,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            let peer = observer.peer().clone();
            let joined = mcp_tool(
                &peer,
                &diagnostic,
                "join_room",
                json!({"room_code":code,"provider":"runner","display_name":"MCP spectator"}),
            )
            .await;
            let observer_id = joined["participant_id"].as_str().unwrap().to_owned();
            let uris: Vec<String> = ["state_uri", "public_state_uri", "history_uri"]
                .iter()
                .map(|key| joined[*key].as_str().unwrap().to_owned())
                .collect();
            #[allow(deprecated)]
            for uri in &uris {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    peer.subscribe(rmcp::model::SubscribeRequestParams::new(uri.clone())),
                )
                .await
                .unwrap()
                .unwrap();
            }
            let initial = mcp_tool(&peer, &diagnostic, "get_my_state", json!({})).await;
            let mut previous_revision = initial["revision"].as_u64().unwrap();
            http_at_origin(
                &admin,
                &base,
                &base,
                reqwest::Method::POST,
                &format!("/api/v1/admin/rooms/{code}/start"),
                Value::Null,
                200,
            )
            .await;
            let detail = http_at_origin(
                &admin,
                &base,
                &base,
                reqwest::Method::GET,
                &format!("/api/v1/admin/rooms/{code}"),
                Value::Null,
                200,
            )
            .await;
            diagnostic.record(usize::MAX, "mcp_admin_detail", detail.clone());
            assert_eq!(detail["match_players"].as_array().unwrap().len(), count);
            assert!(
                detail["match_players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| p["kind"] == "human" && p["participant_id"] != observer_id)
            );
            let participant = detail["participants"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["participant_id"] == observer_id)
                .unwrap();
            assert_eq!(participant["selected"], false);
            assert_eq!(participant["role"], "none");
            assert_eq!(participant["kind"], "mcp");
            let mut mapped = Vec::new();
            for (mut socket, mut transcript) in sockets.into_iter().zip(transcripts) {
                let view = receive(
                    &mut socket,
                    &mut transcript,
                    json!({"phase":"MCP initial Human mapping"}),
                    |v| v["state"].is_object(),
                )
                .await;
                let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
                assert!(seat < count);
                assert_eq!(view["room"]["join_code"], code);
                assert_eq!(view["room"]["match_players"], detail["match_players"]);
                let player = view["room"]["match_players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["seat"] == seat)
                    .unwrap();
                assert_eq!(player["participant_id"], socket.participant_id);
                mapped.push((seat, socket, transcript));
            }
            mapped.sort_by_key(|(seat, _, _)| *seat);
            assert_eq!(
                mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
                (0..count).collect::<Vec<_>>()
            );
            let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) =
                mapped.into_iter().map(|(_, s, t)| (s, t)).unzip();
            let opening = mcp_resource(&peer, &diagnostic, &uris[2]).await;
            let opening_events = opening["events"].as_array().unwrap().clone();
            assert_eq!(opening_events.len(), 2);
            assert_eq!(opening_events[0]["StartKyoku"]["oya"], 0);
            assert_eq!(
                opening_events[0]["StartKyoku"]["scores"],
                json!(vec![100000; count])
            );
            assert_eq!(opening_events[1]["Tsumo"]["actor"], 0);
            assert!(opening_events[1]["Tsumo"].get("tile").is_none());
            assert_mcp_typed_boundary(&opening, 0, 0);
            choose(&mut sockets[0], &mut transcripts[0], "discard").await;
            choose(&mut sockets[1], &mut transcripts[1], "Ron").await;
            choose(&mut sockets[2], &mut transcripts[2], "Ron").await;
            let match_id = opening["phase"]["Playing"].clone();
            assert!(match_id.is_string());
            let mut prior_step: Option<Value> = None;
            for step in 0..3_u64 {
                let mut publications = Vec::new();
                for seat in 0..count {
                    let current_boundary = |v: &Value| {
                        let id = &v["room"]["kyoku_end"]["id"];
                        v["state"].is_object()
                            && id["match_id"] == match_id
                            && id["step_index"] == step
                            && prior_step
                                .as_ref()
                                .is_none_or(|prior| id["kyoku_sequence"] == prior["kyoku_sequence"])
                            && id["kyoku_sequence"] == v["state"]["kyoku_results"]["kyoku_sequence"]
                    };
                    let latest = transcripts[seat]
                        .iter()
                        .rev()
                        .find(|v| v["state"].is_object())
                        .cloned();
                    let view = if latest.as_ref().is_some_and(current_boundary) {
                        latest.unwrap()
                    } else {
                        receive(&mut sockets[seat], &mut transcripts[seat], json!({"phase":"MCP held boundary","step_index":step,"actual_match_id":match_id,"prior_actual_step":prior_step}), current_boundary).await
                    };
                    publications.push(view);
                }
                let publication = &publications[0]["room"]["kyoku_end"];
                let step_id = &publication["id"];
                prior_step = Some(step_id.clone());
                let revision = publications[0]["room"]["revision"].as_u64().unwrap();
                assert_eq!(publication["required"].as_array().unwrap().len(), count);
                assert!(
                    !publication["required"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(observer_id))
                );
                for socket in &sockets {
                    assert!(
                        publication["required"]
                            .as_array()
                            .unwrap()
                            .contains(&json!(socket.participant_id))
                    );
                }
                for view in &publications {
                    assert_eq!(view["room"]["kyoku_end"]["id"], *step_id);
                    assert_eq!(view["state"]["decision"], Value::Null);
                }
                let winners = if step == 0 { 1 } else { 2 };
                let mcp_record_start = diagnostic.records.lock().unwrap().len();
                let mine = mcp_tool(&peer, &diagnostic, "get_my_state", json!({})).await;
                assert_mcp_state(&mine, step_id, revision, winners);
                assert_eq!(mine["participant_id"], observer_id);
                assert_eq!(mine["kyoku_end"]["required"], publication["required"]);
                assert_eq!(
                    mine["kyoku_end"]["published_at_ms"],
                    publication["published_at_ms"]
                );
                assert_eq!(
                    mine["kyoku_end"]["deadline_at_ms"],
                    publication["deadline_at_ms"]
                );
                assert_eq!(mine["is_my_turn"], false);
                assert_eq!(mine["legal_actions"], json!([]));
                let waited = mcp_tool(
                    &peer,
                    &diagnostic,
                    "wait_for_turn",
                    json!({"after_revision":previous_revision,"timeout_seconds":1}),
                )
                .await;
                assert_eq!(waited["revision"].as_u64().unwrap(), revision);
                assert_ne!(waited["reason"], "timeout");
                assert_eq!(waited["state_uri"], uris[0]);
                previous_revision = revision;
                let private = mcp_resource(&peer, &diagnostic, &uris[0]).await;
                let public = mcp_resource(&peer, &diagnostic, &uris[1]).await;
                for value in [&private, &public] {
                    assert_mcp_state(value, step_id, revision, winners);
                    assert_eq!(value["kyoku_end"]["required"], publication["required"]);
                    assert_eq!(
                        value["kyoku_end"]["published_at_ms"],
                        publication["published_at_ms"]
                    );
                    assert_eq!(
                        value["kyoku_end"]["deadline_at_ms"],
                        publication["deadline_at_ms"]
                    );
                    if step < 2 {
                        assert_eq!(value["kyoku_results"]["settlement"], Value::Null);
                    } else {
                        let scores: Vec<_> = value["kyoku_results"]["settlement"]["changes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v["after"].as_i64().unwrap())
                            .collect();
                        assert_eq!(
                            scores,
                            if count == 3 {
                                vec![94400, 105000, 102600]
                            } else {
                                vec![94200, 105200, 102600, 100000]
                            }
                        );
                    }
                }
                let history = mcp_resource(&peer, &diagnostic, &uris[2]).await;
                assert_eq!(history["revision"].as_u64().unwrap(), revision);
                assert_eq!(history["result"], Value::Null);
                assert_boundary(&history, winners, None);
                let events = history["events"].as_array().unwrap();
                assert_eq!(&events[..opening_events.len()], opening_events.as_slice());
                let names: Vec<_> = events
                    .iter()
                    .map(|v| v.as_object().unwrap().keys().next().unwrap().as_str())
                    .collect();
                assert_eq!(
                    names,
                    if step < 2 {
                        vec!["StartKyoku", "Tsumo", "Dahai"]
                    } else {
                        vec!["StartKyoku", "Tsumo", "Dahai", "Hora", "Hora"]
                    }
                );
                assert_eq!(
                    events[opening_events.len()]["Dahai"],
                    json!({"actor":0,"tile":76,"tsumogiri":true})
                );
                assert_eq!(
                    history["history"]["current_kyoku"]["authoritative_results"],
                    mine["kyoku_results"]
                );
                assert_eq!(history["summaries"], json!([]));
                let horas: Vec<_> = events.iter().filter_map(|v| v.get("Hora")).collect();
                assert_eq!(
                    horas
                        .iter()
                        .map(|v| v["actor"].as_u64().unwrap())
                        .collect::<Vec<_>>(),
                    if step < 2 { vec![] } else { vec![1, 2] }
                );
                if step == 2 {
                    assert_eq!(
                        horas[0]["delta"],
                        if count == 3 {
                            json!([-3000, 5000, 0])
                        } else {
                            json!([-3200, 5200, 0, 0])
                        }
                    );
                    assert_eq!(
                        horas[1]["delta"],
                        if count == 3 {
                            json!([-2600, 0, 2600])
                        } else {
                            json!([-2600, 0, 2600, 0])
                        }
                    );
                    assert!(horas.iter().all(|v| v["target"] == 0));
                }
                assert!(
                    !events
                        .iter()
                        .any(|v| v.get("EndKyoku").is_some() || v.as_str() == Some("EndKyoku"))
                );
                assert!(
                    !events
                        .iter()
                        .any(|v| v.get("EndGame").is_some() || v.as_str() == Some("EndGame"))
                );
                let complete_typed = {
                    let records = diagnostic.records.lock().unwrap();
                    records[mcp_record_start..].to_vec()
                };
                for record in &complete_typed {
                    if matches!(
                        record["kind"].as_str(),
                        Some("mcp_tool_reply" | "mcp_resource_reply" | "mcp_notification")
                    ) {
                        assert_mcp_typed_boundary(&record["value"], winners, step);
                    }
                }
                let outer_chunks = assert_outer_http_boundary(&outer, winners, step);
                outer.record(usize::MAX, "outer_held_id", json!({"actual_step_id":step_id,"deadline_at_ms":publication["deadline_at_ms"]}));
                diagnostic.record(usize::MAX, "mcp_boundary_proved", json!({"actual_human_step_id":step_id,"actual_revision":revision,"resources_read":uris,"notifications_are_URI_metadata_not_state_proof":true,"capture_scope":"complete typed CallToolResult/ReadResourceResult and ResourceUpdated params, including encoded contents","outer_body_chunk_count":outer_chunks,"physical_transfer_coding_packets_trailers":"NOT OBSERVED"}));
                for seat in 0..count {
                    sockets[seat]
                        .send(Message::Text(
                            json!({"type":"confirm_result","step_id":step_id})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    let pid = sockets[seat].participant_id.clone();
                    receive(&mut sockets[seat], &mut transcripts[seat], json!({"phase":"MCP scenario Human confirmation","sent_actual_id":step_id,"participant_id":pid}), |v| {
                        let end = &v["room"]["kyoku_end"];
                        if seat + 1 < count { end["id"] == *step_id && end["confirmed"].as_array().is_some_and(|cs| cs.contains(&json!(pid))) }
                        else if step < 2 { end["id"]["match_id"] == step_id["match_id"] && end["id"]["kyoku_sequence"] == step_id["kyoku_sequence"] && end["id"]["step_index"] == step+1 }
                        else { end.is_null() && v["room"]["phase"] == "playing" && v["room"]["join_code"] == code && v["room"]["match_players"] == detail["match_players"] && v["state"]["round"] == mine["round"] && v["state"]["kyoku"] == mine["kyoku"].as_u64().unwrap()+1 && v["state"]["dealer"] == 1 }
                    }).await;
                }
                if step == 2 {
                    let released = mcp_tool(&peer, &diagnostic, "get_my_state", json!({})).await;
                    assert_eq!(released["phase"]["Playing"], step_id["match_id"]);
                    assert!(released["revision"].as_u64().unwrap() > revision);
                    assert_eq!(released["kyoku_end"], Value::Null);
                    assert_eq!(released["kyoku"], mine["kyoku"].as_u64().unwrap() + 1);
                    assert_eq!(released["dealer"], 1);
                    assert_eq!(released["legal_actions"], json!([]));
                    assert_eq!(released["is_my_turn"], false);
                    let released_history = mcp_resource(&peer, &diagnostic, &uris[2]).await;
                    assert_eq!(released_history["phase"]["Playing"], step_id["match_id"]);
                    assert_eq!(released_history["summaries"].as_array().unwrap().len(), 1);
                    let previous = &released_history["summaries"][0];
                    assert_eq!(previous["authoritative_results"], mine["kyoku_results"]);
                    assert_eq!(previous["results"].as_array().unwrap().len(), 2);
                    for (seat, result) in previous["results"].as_array().unwrap().iter().enumerate()
                    {
                        assert_eq!(result["Hora"]["actor"], seat + 1);
                        assert_eq!(result["Hora"]["target"], 0);
                        assert_eq!(result["Hora"]["delta"], horas[seat]["delta"]);
                    }
                    let next = released_history["events"].as_array().unwrap();
                    assert_eq!(next.len(), 2);
                    assert_eq!(next[0]["StartKyoku"]["kyoku"], 2);
                    assert_eq!(next[0]["StartKyoku"]["oya"], 1);
                    assert_eq!(next[0]["StartKyoku"]["kyotaku"], 0);
                    assert_eq!(
                        next[0]["StartKyoku"]["scores"],
                        if count == 3 {
                            json!([94400, 105000, 102600])
                        } else {
                            json!([94200, 105200, 102600, 100000])
                        }
                    );
                    assert_eq!(next[1]["Tsumo"], json!({"actor":1}));
                    let room = state.rooms().get(code).await.unwrap();
                    let double_riichi_core::RoomResponse::MatchEvents(canonical) = room
                        .send(double_riichi_core::RoomCommand::GetMatchEvents)
                        .await
                        .unwrap()
                    else {
                        panic!("released canonical events response");
                    };
                    let canonical = serde_json::to_value(canonical).unwrap();
                    diagnostic.record(
                        usize::MAX,
                        "supplementary_released_Room_canonical_events_NOT_MCP_wire",
                        canonical.clone(),
                    );
                    let journal = canonical.as_array().unwrap();
                    let names: Vec<_> = journal
                        .iter()
                        .map(|v| {
                            v.as_str().unwrap_or_else(|| {
                                v.as_object().unwrap().keys().next().unwrap().as_str()
                            })
                        })
                        .collect();
                    assert_eq!(
                        names,
                        vec![
                            "StartGame",
                            "StartKyoku",
                            "Tsumo",
                            "Dahai",
                            "Hora",
                            "Hora",
                            "EndKyoku",
                            "StartKyoku",
                            "Tsumo"
                        ]
                    );
                    assert_eq!(journal[3]["Dahai"]["tile"], 76);
                    for seat in 0..2 {
                        assert_eq!(journal[seat + 4]["Hora"]["actor"], seat + 1);
                        assert_eq!(journal[seat + 4]["Hora"]["delta"], horas[seat]["delta"]);
                    }
                    assert_eq!(journal[7]["StartKyoku"]["kyoku"], 2);
                    assert_eq!(journal[7]["StartKyoku"]["oya"], 1);
                    assert_eq!(journal[8]["Tsumo"]["actor"], 1);
                }
            }
            let records = diagnostic.records.lock().unwrap().clone();
            for uri in &uris {
                assert!(
                    records
                        .iter()
                        .any(|r| r["kind"] == "mcp_notification" && r["value"]["uri"] == *uri)
                );
            }
            drop(records);
            println!(
                "Task8 {mode}: MCP typed get/wait, 3 resources/history, spectator, all held steps and Human release"
            );
            tokio::time::timeout(Duration::from_secs(3), observer.cancel())
                .await
                .unwrap()
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_secs(3), bridge)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_ok()
            );
            for socket in &mut sockets {
                socket.close(None).await.unwrap();
            }
            state.shutdown().await;
            stop.send(()).unwrap();
            server.await.unwrap();
            storage.close().await;
        }
    }
    fn assert_mjai_request_boundary(request: &Value, seat: usize) -> Value {
        use base64::Engine as _;
        // Offered Hora is a unit action DTO, never a canonical Hora event.
        // Exact roundtrip prevents the typed parser from hiding unknown fields.
        let parsed = double_riichi_mjai::parse_request_action(
            &serde_json::to_vec(request).unwrap(),
            GameMode::FourPlayerRedHalf,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            *request,
            "exact request/action schema"
        );
        assert!(request["request_id"].as_u64().unwrap() > 0);
        assert!(seat < 4);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(request["observation"].as_str().unwrap())
            .unwrap();
        assert!(!bytes.contains(&0));
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["player_id"], seat);
        for action in value["_legal_actions"].as_array().unwrap() {
            assert_eq!(
                action["actor"], seat,
                "legal action belongs to actual viewer"
            );
        }
        let hands = value["hands"].as_array().unwrap();
        assert_eq!(hands.len(), 4);
        for (other, hand) in hands.iter().enumerate() {
            if other == seat {
                assert!(!hand.as_array().unwrap().is_empty());
            } else {
                assert_eq!(hand, &json!([]));
            }
        }
        assert_boundary(&value, 0, Some(seat as u64));
        for event in value["events"].as_array().unwrap() {
            let event: Value = serde_json::from_str(event.as_str().unwrap()).unwrap();
            assert_boundary(&event, 0, Some(seat as u64));
        }
        value
    }

    fn assert_mjai_observation(request: &Value, seat: usize, diagnostic: &ReceiveDiagnostic) {
        let value = assert_mjai_request_boundary(request, seat);
        diagnostic.record(4, "decoded_MJAI_observation", value);
    }

    #[test]
    fn standalone_request_schema_keeps_offered_hora_distinct_from_canonical_hora() {
        use base64::Engine as _;
        use double_riichi_core::{
            Audience, AudienceProjection, GameAction, MatchMachine, Participant, ParticipantKind,
            Seat, TimeControl,
        };
        let mode = GameMode::FourPlayerRedHalf;
        let players = (0..4)
            .map(|i| Participant::new(format!("schema-{i}"), "schema", ParticipantKind::MJAI))
            .collect();
        let mut machine = MatchMachine::with_task8_fixture(
            mode,
            players,
            TimeControl::Unlimited,
            "double-ron-final",
        )
        .unwrap();
        let dealer = Seat::new(0).unwrap();
        let decision = machine.current_decision().unwrap().unwrap();
        let discard = decision.actions_for(dealer).iter().find(|offer| matches!(&offer.action, GameAction::Discard { tile, .. } if tile.id() == 77)).unwrap();
        machine
            .submit_action(dealer, decision.id(), &discard.id)
            .unwrap();
        let AudienceProjection::Player(projection) = machine
            .project(Audience::Player(Seat::new(1).unwrap()))
            .unwrap()
        else {
            panic!("Player projection");
        };
        let request = serde_json::to_value(
            double_riichi_mjai::build_request_action(
                1,
                double_riichi_mjai::RequestTime::default(),
                &projection,
                machine.events(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            request["possible_actions"],
            json!([{"type":"hora"},{"type":"none"}])
        );
        let observation = assert_mjai_request_boundary(&request, 1);
        let canonical = json!({"type":"hora","actor":1,"target":0,"uradora_markers":[],"deltas":[-3200,5200,0,0]});
        assert_boundary(&canonical, 1, None);
        let reject_canonical = |value: &Value| {
            assert!(
                std::panic::catch_unwind(|| assert_boundary(value, 1, None)).is_err(),
                "canonical poison accepted: {value}"
            )
        };
        reject_canonical(&json!({"type":"hora"}));
        reject_canonical(&json!({"type":"hora","actor":null}));
        reject_canonical(&json!({"type":"hora","actor":2,"target":0,"deltas":[-2600,0,2600,0]}));
        reject_canonical(&json!({"unrelated":{"type":"hora","actor":2}}));
        let reject_request = |value: &Value| {
            assert!(
                std::panic::catch_unwind(|| assert_mjai_request_boundary(value, 1)).is_err(),
                "request poison accepted"
            )
        };
        for field in ["actor", "target", "pai", "deltas", "engine_state"] {
            let mut poison = request.clone();
            poison["possible_actions"][0][field] = json!(0);
            reject_request(&poison);
        }
        for id in [Value::Null, json!(0)] {
            let mut poison = request.clone();
            poison["request_id"] = id;
            reject_request(&poison);
        }
        let mut poison = request.clone();
        poison.as_object_mut().unwrap().remove("request_id");
        reject_request(&poison);
        let mut poison = request.clone();
        poison["unknown_nested"] = json!({"engine_state":{}});
        reject_request(&poison);
        let mut poison = request.clone();
        poison["possible_actions"][0]["unknown_nested"] = json!({"type":"hora","actor":2});
        reject_request(&poison);
        let encode_poison = |value: Value| {
            let mut poison = request.clone();
            poison["observation"] = json!(
                base64::engine::general_purpose::STANDARD
                    .encode(serde_json::to_vec(&value).unwrap())
            );
            poison
        };
        let mut decoded = observation.clone();
        decoded["player_id"] = json!(2);
        reject_request(&encode_poison(decoded));
        let mut decoded = observation.clone();
        decoded["_legal_actions"][0]["actor"] = json!(2);
        reject_request(&encode_poison(decoded));
        let mut decoded = observation.clone();
        decoded["hands"][2] = json!([37]);
        reject_request(&encode_poison(decoded));
        let mut decoded = observation;
        decoded["events"] = json!([json!({"type":"hora","actor":2}).to_string()]);
        reject_request(&encode_poison(decoded));
    }

    async fn choose_mjai(
        socket: &mut HumanSocket,
        transcript: &mut Vec<Value>,
        seat: usize,
        kind: &str,
    ) {
        let offered = |a: &Value| a["type"] == kind && (kind != "dahai" || a["pai"] == "2s");
        let latest = transcript
            .iter()
            .rev()
            .find(|v| v["type"] == "request_action")
            .cloned();
        let usable = latest.as_ref().is_some_and(|v| {
            v["possible_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(offered)
        });
        let request = if usable {
            latest.unwrap()
        } else {
            receive(
                socket,
                transcript,
                json!({"phase":"MJAI actual offer","seat":seat,"kind":kind}),
                |v| {
                    v["type"] == "request_action"
                        && v["possible_actions"]
                            .as_array()
                            .is_some_and(|actions| actions.iter().any(offered))
                },
            )
            .await
        };
        assert_mjai_observation(&request, seat, &socket.diagnostic);
        let mut action = request["possible_actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| offered(a))
            .unwrap()
            .clone();
        action
            .as_object_mut()
            .unwrap()
            .insert("request_id".into(), request["request_id"].clone());
        socket
            .send(Message::Text(action.to_string().into()))
            .await
            .unwrap();
        let ack = receive(
            socket,
            transcript,
            json!({"phase":"MJAI actual ack","request_id":request["request_id"]}),
            |v| v["type"] == "action_ack" && v["request_id"] == request["request_id"],
        )
        .await;
        assert_eq!(ack["status"], "accepted");
    }

    async fn mjai_server(
        label: &str,
    ) -> (
        Arc<ServerState>,
        Arc<Storage>,
        String,
        std::net::SocketAddr,
        String,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let storage = Arc::new(Storage::connect(&temp_root(label)).await.unwrap());
        let tokens = Arc::new(BotTokenService::new(
            storage.clone(),
            Arc::new(BotTokenAuthority::empty()),
        ));
        let token = tokens.create("runner", 1, "task8-private").await.unwrap();
        let raw = token.secret().expose().to_owned();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let state = Arc::new(
            ServerState::for_tests(
                &base,
                Arc::new(
                    AdminAuthenticator::new(
                        "admin",
                        hash_password("correct horse battery staple").unwrap(),
                    )
                    .unwrap(),
                ),
                RoomRegistry::with_max_rooms(8),
            )
            .with_bot_token_service(tokens),
        );
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                server_router(serve_state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        (state, storage, raw, address, base, stop, server)
    }
    #[tokio::test]
    async fn unpublished_double_ron_room_mjai_releases_only_settlement_canonical_prefix() {
        actual_room_mjai_double_ron(false).await;
    }

    fn room_snapshot_steps_match_fixed_authority(left: &Value, right: &Value) -> bool {
        let (Some(left), Some(right)) = (left.as_object(), right.as_object()) else {
            return false;
        };
        let keys = [
            "id",
            "content",
            "published_at_ms",
            "deadline_at_ms",
            "remaining_ms",
            "required",
            "confirmed",
        ];
        for fields in [left, right] {
            if fields.len() != keys.len() || !keys.iter().all(|key| fields.contains_key(*key)) {
                return false;
            }
            let (Some(published), Some(deadline), Some(remaining)) = (
                fields["published_at_ms"].as_u64(),
                fields["deadline_at_ms"].as_u64(),
                fields["remaining_ms"].as_u64(),
            ) else {
                return false;
            };
            if deadline.checked_sub(published) != Some(5000) || remaining > 5000 {
                return false;
            }
        }
        keys.iter()
            .filter(|key| **key != "remaining_ms")
            .all(|key| left[*key] == right[*key])
    }

    #[test]
    fn room_snapshot_steps_match_fixed_authority_keeps_derived_clock_distinct() {
        // Typed DTO + deterministic projection-formula unit only, not a live producer timestamp.
        let t0 = tokio::time::Instant::now();
        let deadline = t0 + Duration::from_secs(5);
        let baseline = double_riichi_core::KyokuEndStep {
            id: double_riichi_core::ResultStepId {
                match_id: "clock-unit".into(),
                kyoku_sequence: 1,
                step_index: 0,
            },
            content: double_riichi_core::KyokuEndContent::Draw(
                double_riichi_core::KyokuSettlement {
                    changes: Vec::new(),
                    pot_before: 0,
                    pot_after: 0,
                    draw_reason: Some("clock-unit-only".into()),
                },
            ),
            published_at_ms: 1000,
            deadline_at_ms: 6000,
            remaining_ms: deadline.saturating_duration_since(t0).as_millis() as u64,
            required: vec!["clock-unit-human".into()],
            confirmed: Vec::new(),
        };
        let mut later = baseline.clone();
        later.remaining_ms = deadline
            .saturating_duration_since(t0 + Duration::from_millis(1))
            .as_millis() as u64;
        assert_eq!(baseline.remaining_ms, 5000);
        assert_eq!(later.remaining_ms, 4999);
        let first = serde_json::to_value(baseline).unwrap();
        let second = serde_json::to_value(later).unwrap();
        assert!(room_snapshot_steps_match_fixed_authority(&first, &second));
        for key in [
            "id",
            "content",
            "published_at_ms",
            "deadline_at_ms",
            "required",
            "confirmed",
        ] {
            let mut poisoned = second.clone();
            poisoned[key] = json!("poison");
            assert!(
                !room_snapshot_steps_match_fixed_authority(&first, &poisoned),
                "fixed authority poison: {key}"
            );
        }
        for key in [
            "id",
            "content",
            "published_at_ms",
            "deadline_at_ms",
            "remaining_ms",
            "required",
            "confirmed",
        ] {
            let mut poisoned = second.clone();
            poisoned.as_object_mut().unwrap().remove(key);
            assert!(!room_snapshot_steps_match_fixed_authority(
                &first, &poisoned
            ));
            poisoned = second.clone();
            poisoned[key] = Value::Null;
            assert!(!room_snapshot_steps_match_fixed_authority(
                &first, &poisoned
            ));
        }
        for value in [json!(5001), json!(-1), json!("4999"), json!(4999.5)] {
            let mut poisoned = second.clone();
            poisoned["remaining_ms"] = value;
            assert!(!room_snapshot_steps_match_fixed_authority(
                &first, &poisoned
            ));
        }
        let mut expired = second.clone();
        expired["remaining_ms"] = json!(
            deadline
                .saturating_duration_since(t0 + Duration::from_secs(6))
                .as_millis() as u64
        );
        assert_eq!(expired["remaining_ms"], 0);
        assert!(room_snapshot_steps_match_fixed_authority(&first, &expired));
    }

    fn admin_room_detail_keys_are_exact(value: &Value) -> bool {
        let Some(fields) = value.as_object() else {
            return false;
        };
        let mut expected = vec![
            "join_code",
            "room_name",
            "game_mode",
            "phase",
            "connected_count",
            "participant_count",
            "selected_count",
            "created_at",
            "time_control",
            "replay_save",
            "participant_limit",
            "participants",
            "match_players",
            "roster",
            "result",
            "revision",
            "persistence_degraded",
            "replay_available",
        ];
        if fields.contains_key("benchmark_mode") {
            if fields["benchmark_mode"] != true {
                return false;
            }
            expected.push("benchmark_mode");
        }
        fields.len() == expected.len() && expected.iter().all(|key| fields.contains_key(*key))
    }

    fn assert_actual_admin_room_detail_schema(value: &Value) {
        assert!(
            admin_room_detail_keys_are_exact(value),
            "HTTP Admin RoomDetailView exact schema"
        );
        assert_boundary(value, 2, None);
        for absent in [
            "kyoku_end",
            "match_view",
            "state",
            "unexpected_private_subtree",
        ] {
            let mut poisoned = value.clone();
            poisoned[absent] = json!({"id":"poison"});
            assert!(
                !admin_room_detail_keys_are_exact(&poisoned),
                "Admin DTO poison accepted: {absent}"
            );
        }
    }

    #[tokio::test]
    async fn unpublished_double_ron_room_mjai_snapshot_fifo_writer_and_peer_prefix() {
        actual_room_mjai_double_ron(true).await;
    }

    async fn actual_room_mjai_double_ron(probe_fifo: bool) {
        if run_in_fixture_process("double-ron") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "double-ron"
        );
        let (state, storage, raw, address, base, stop, server) = mjai_server("task8-mjai").await;
        let admin = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::POST,
            "/api/v1/admin/login",
            json!({"username":"admin","password":"correct horse battery staple"}),
            200,
        )
        .await;
        let created = http_at_origin(&admin, &base, &base, reqwest::Method::POST, "/api/v1/admin/rooms", json!({"room_name":"Task8 MJAI","game_mode":"4p-red-east","time_control":"unlimited","participant_limit":4,"replay_save":false}), 201).await;
        let code = created["join_code"].as_str().unwrap();
        let response = reqwest::Client::new()
            .post(format!("{base}/api/v1/rooms/{code}/agents/join"))
            .bearer_auth(&raw)
            .json(&json!({"display_name":"MJAI selected"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 201);
        let joined_raw = response.bytes().await.unwrap();
        assert!(!joined_raw.contains(&0));
        let joined: Value = serde_json::from_slice(&joined_raw).unwrap();
        let agent_id = joined["participant_id"].as_str().unwrap().to_owned();
        let agent_participant_id: double_riichi_core::ParticipantId = agent_id.clone().into();
        let mut probe =
            probe_fifo.then(|| state.task8_attach_room_mjai_probe(agent_participant_id.clone()));
        let mut probe_identity: Option<(Value, Value)> = None;
        let mut prior_probe_count = 0;
        http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::POST,
            &format!("/api/v1/admin/rooms/{code}/participants/{agent_id}/select"),
            Value::Null,
            200,
        )
        .await;
        let mut request =
            format!("ws://{address}/ws/v1/rooms/{code}/mjai?participant_id={agent_id}")
                .into_client_request()
                .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {raw}").parse().unwrap());
        let (wire, _) = connect_async(request).await.unwrap();
        let (sockets, transcripts) = join_humans_with_preloads_at_origin(
            &admin,
            &base,
            &base,
            address,
            code,
            3,
            json!(["player-red", "mjai-bot"]),
        )
        .await;
        let diagnostic = sockets[0].diagnostic.clone();
        diagnostic.record(
            4,
            "MJAI_actual_join_response",
            json!({"raw":std::str::from_utf8(&joined_raw).unwrap()}),
        );
        let mut agent = HumanSocket {
            wire,
            peer: 4,
            participant_id: agent_id.clone(),
            diagnostic: diagnostic.clone(),
        };
        let mut agent_transcript = Vec::new();
        http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::POST,
            &format!("/api/v1/admin/rooms/{code}/start"),
            Value::Null,
            200,
        )
        .await;
        let detail = http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::GET,
            &format!("/api/v1/admin/rooms/{code}"),
            Value::Null,
            200,
        )
        .await;
        diagnostic.record(4, "MJAI_actual_Admin_detail", detail.clone());
        let roster = detail["match_players"].as_array().unwrap();
        assert_eq!(roster.len(), 4);
        assert_eq!(roster.iter().filter(|p| p["kind"] == "human").count(), 3);
        assert_eq!(roster.iter().filter(|p| p["kind"] == "mjai").count(), 1);
        let agent_seat = roster
            .iter()
            .find(|p| p["participant_id"] == agent_id)
            .unwrap()["seat"]
            .as_u64()
            .unwrap() as usize;
        let mut mapped = Vec::new();
        for (mut socket, mut transcript) in sockets.into_iter().zip(transcripts) {
            let view = receive(
                &mut socket,
                &mut transcript,
                json!({"phase":"MJAI Human actual seat mapping"}),
                |v| v["state"].is_object(),
            )
            .await;
            let seat = view["state"]["viewer_seat"].as_u64().unwrap() as usize;
            assert_eq!(view["room"]["match_players"], detail["match_players"]);
            assert_eq!(
                roster.iter().find(|p| p["seat"] == seat).unwrap()["participant_id"],
                socket.participant_id
            );
            mapped.push((seat, socket, transcript));
        }
        receive(
            &mut agent,
            &mut agent_transcript,
            json!({"phase":"MJAI canonical opening Tsumo"}),
            |v| v["type"] == "tsumo",
        )
        .await;
        assert_eq!(agent_transcript[0]["type"], "start_game");
        assert_eq!(agent_transcript[0]["id"], agent_seat);
        assert_eq!(agent_transcript[1]["type"], "start_kyoku");
        let opening = &agent_transcript[1];
        assert_eq!(opening["scores"], json!([100000, 100000, 100000, 100000]));
        assert_eq!(opening["oya"], 0);
        assert_eq!(opening["honba"], 2);
        assert_eq!(opening["kyotaku"], 2);
        let hands = opening["tehais"].as_array().unwrap();
        assert_eq!(hands.len(), 4);
        for (seat, hand) in hands.iter().enumerate() {
            assert_eq!(hand.as_array().unwrap().len(), 13);
            if seat == agent_seat {
                assert!(hand.as_array().unwrap().iter().all(|tile| tile != "?"));
            } else {
                assert!(hand.as_array().unwrap().iter().all(|tile| tile == "?"));
            }
        }
        assert_eq!(agent_transcript[2]["actor"], 0);
        assert_eq!(
            agent_transcript[2]["pai"],
            if agent_seat == 0 { "2s" } else { "?" }
        );
        mapped.push((agent_seat, agent, agent_transcript));
        mapped.sort_by_key(|(seat, _, _)| *seat);
        assert_eq!(
            mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) =
            mapped.into_iter().map(|(_, s, t)| (s, t)).unzip();
        for seat in 0..3 {
            if seat == agent_seat {
                choose_mjai(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    seat,
                    if seat == 0 { "dahai" } else { "hora" },
                )
                .await;
            } else {
                choose(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    if seat == 0 { "discard" } else { "Ron" },
                )
                .await;
            }
        }
        let humans: Vec<_> = (0..4).filter(|seat| *seat != agent_seat).collect();
        let mut prior_id: Option<Value> = None;
        for step in 0..3_u64 {
            let mut publications = Vec::new();
            for &seat in &humans {
                let bound = |v: &Value| {
                    v["state"].is_object()
                        && v["room"]["kyoku_end"]["id"]["step_index"] == step
                        && prior_id.as_ref().is_none_or(|id| {
                            v["room"]["kyoku_end"]["id"]["match_id"] == id["match_id"]
                                && v["room"]["kyoku_end"]["id"]["kyoku_sequence"]
                                    == id["kyoku_sequence"]
                        })
                };
                let latest = transcripts[seat]
                    .iter()
                    .rev()
                    .find(|v| v["state"].is_object())
                    .cloned();
                let view = if latest.as_ref().is_some_and(bound) {
                    latest.unwrap()
                } else {
                    receive(
                        &mut sockets[seat],
                        &mut transcripts[seat],
                        json!({"phase":"MJAI Human held","step":step,"prior_id":prior_id}),
                        bound,
                    )
                    .await
                };
                publications.push((seat, view));
            }
            let publication = &publications[0].1["room"]["kyoku_end"];
            let id = publication["id"].clone();
            assert!(id["match_id"].is_string());
            assert_eq!(
                id["kyoku_sequence"],
                publications[0].1["state"]["kyoku_results"]["kyoku_sequence"]
            );
            assert_eq!(publication["required"].as_array().unwrap().len(), 3);
            assert!(
                !publication["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(agent_id))
            );
            for &seat in &humans {
                assert!(
                    publication["required"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(sockets[seat].participant_id))
                );
            }
            let winners = if step == 0 { 1 } else { 2 };
            for (seat, view) in &publications {
                assert_eq!(view["state"]["viewer_seat"], *seat);
                assert_eq!(view["state"]["decision"], Value::Null);
                assert_eq!(view["room"]["result"], Value::Null);
                assert_eq!(view["room"]["kyoku_end"]["id"], id);
                assert_eq!(
                    view["room"]["kyoku_end"]["published_at_ms"],
                    publication["published_at_ms"]
                );
                assert_eq!(
                    view["room"]["kyoku_end"]["deadline_at_ms"],
                    publication["deadline_at_ms"]
                );
                let wins = view["state"]["kyoku_results"]["wins"].as_array().unwrap();
                assert_eq!(wins.len(), winners as usize);
                for (index, win) in wins.iter().enumerate() {
                    assert_eq!(win["seat"], index + 1);
                    assert_eq!(win["winning_tile"], 77);
                    assert_eq!(win["winning_tile_index"], 13);
                    assert_eq!(win["win_points"], 2600);
                }
                if step < 2 {
                    assert_eq!(view["state"]["kyoku_results"]["settlement"], Value::Null);
                } else {
                    let scores: Vec<_> = view["state"]["kyoku_results"]["settlement"]["changes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c["after"].as_i64().unwrap())
                        .collect();
                    assert_eq!(scores, vec![94200, 105200, 102600, 100000]);
                }
                assert_boundary(view, winners, Some(*seat as u64));
            }
            diagnostic.record(4, "MJAI_correlated_Human_publication_NOT_queue_drained", json!({"id":id,"revision":publications[0].1["room"]["revision"],"published_at_ms":publication["published_at_ms"],"deadline_at_ms":publication["deadline_at_ms"],"canonical_frames_received_so_far":transcripts[agent_seat],"scope":"actual received boundaries only; delayed/inflight absence UNPROVED"}));
            if step == 2 {
                receive(&mut sockets[agent_seat], &mut transcripts[agent_seat], json!({"phase":"MJAI settlement second canonical Hora","actual_Human_step_id":id}), |v| v["type"] == "hora" && v["actor"] == 2).await;
                let canonical: Vec<_> = transcripts[agent_seat]
                    .iter()
                    .filter(|v| v["type"] != "request_action" && v["type"] != "action_ack")
                    .collect();
                assert_eq!(
                    canonical
                        .iter()
                        .map(|v| v["type"].as_str().unwrap())
                        .collect::<Vec<_>>(),
                    vec![
                        "start_game",
                        "start_kyoku",
                        "tsumo",
                        "dahai",
                        "hora",
                        "hora"
                    ]
                );
                assert_eq!(
                    canonical[3],
                    &json!({"type":"dahai","actor":0,"pai":"2s","tsumogiri":true})
                );
                assert_eq!(canonical[4]["actor"], 1);
                assert_eq!(canonical[4]["deltas"], json!([-3200, 5200, 0, 0]));
                assert_eq!(canonical[5]["deltas"], json!([-2600, 0, 2600, 0]));
                for hora in &canonical[4..] {
                    assert_eq!(hora["target"], 0);
                    for absent in ["pai", "han", "fu"] {
                        assert!(
                            hora.get(absent).is_none(),
                            "pinned canonical Hora does not serialize {absent}"
                        );
                    }
                    assert_eq!(hora["uradora_markers"], json!([]));
                }
            }
            if let Some(probe) = &mut probe {
                let marker = tokio::time::timeout(Duration::from_secs(3), probe.recv())
                    .await
                    .unwrap()
                    .expect("Snapshot writer marker");
                assert_eq!(marker["snapshot_step"]["id"], id);
                assert_eq!(marker["read_step"]["id"], id);
                assert_eq!(
                    marker["snapshot_revision"],
                    publications[0].1["room"]["revision"]
                );
                assert_eq!(marker["read_revision"], marker["snapshot_revision"]);
                assert_eq!(marker["exact_same_read"], true);
                assert_eq!(marker["generation_current"], true);
                assert_eq!(marker["closed_before"], false);
                assert_eq!(marker["closed_after_read"], false);
                assert!(room_snapshot_steps_match_fixed_authority(
                    &marker["snapshot_step"],
                    &marker["read_step"]
                ));
                assert!(
                    marker["read_step"]["remaining_ms"].as_u64().unwrap()
                        <= marker["snapshot_step"]["remaining_ms"].as_u64().unwrap()
                );
                for sampled in [&marker["snapshot_step"], &marker["read_step"]] {
                    assert!(
                        sampled["remaining_ms"].as_u64().unwrap() > 0,
                        "actively held frontier expired"
                    );
                }
                for key in ["published_at_ms", "deadline_at_ms", "required", "confirmed"] {
                    assert_eq!(marker["snapshot_step"][key], publication[key]);
                    assert_eq!(marker["read_step"][key], publication[key]);
                }
                assert_eq!(marker["last_request_id"], Value::Null);
                assert_eq!(marker["last_decision"], Value::Null);
                let identity = (
                    marker["subscriber_id"].clone(),
                    marker["generation"].clone(),
                );
                assert!(identity.0.as_u64().unwrap() > 0 && identity.1.as_u64().unwrap() > 0);
                if let Some(prior) = &probe_identity {
                    assert_eq!(&identity, prior);
                } else {
                    probe_identity = Some(identity);
                }
                let target = marker["text_count"].as_u64().unwrap() as usize;
                assert!(target >= prior_probe_count && target <= 4096);
                assert!(
                    marker["text_bytes"].as_u64().unwrap()
                        <= (double_riichi_mjai::MAX_FRAME_BYTES * 16) as u64
                );
                assert!(
                    transcripts[agent_seat].len() <= target,
                    "peer advanced beyond sampled prefix"
                );
                if transcripts[agent_seat].len() < target {
                    receive(&mut sockets[agent_seat], &mut transcripts[agent_seat], json!({"phase":"exact private FIFO prefix peer completion","id":id,"target":target}), |_| {
                        diagnostic.records.lock().unwrap().iter().filter(|r| r["peer"] == 4 && r["kind"] == "received").count() == target
                    }).await;
                }
                let peer_texts: Vec<_> = diagnostic
                    .records
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|r| r["peer"] == 4 && r["kind"] == "received")
                    .map(|r| r["value"]["raw"].as_str().unwrap().to_owned())
                    .collect();
                assert_eq!(peer_texts.len(), target);
                assert_eq!(
                    json!(peer_texts),
                    marker["texts"],
                    "every successful writer text equals actual peer prefix in order"
                );
                for text in &peer_texts {
                    let value: Value = serde_json::from_str(text).unwrap();
                    if value["type"] == "request_action" {
                        assert_mjai_request_boundary(&value, agent_seat);
                    } else {
                        assert_boundary(
                            &value,
                            if step < 2 { 0 } else { 2 },
                            Some(agent_seat as u64),
                        );
                    }
                }
                let canonical_types: Vec<_> = transcripts[agent_seat]
                    .iter()
                    .filter_map(|v| {
                        v["type"]
                            .as_str()
                            .filter(|kind| !["request_action", "action_ack"].contains(kind))
                    })
                    .collect();
                assert_eq!(
                    canonical_types,
                    if step < 2 {
                        vec!["start_game", "start_kyoku", "tsumo", "dahai"]
                    } else {
                        vec![
                            "start_game",
                            "start_kyoku",
                            "tsumo",
                            "dahai",
                            "hora",
                            "hora",
                        ]
                    }
                );
                let health = tokio::time::timeout(
                    Duration::from_secs(3),
                    state.task8_room_mjai_health(&agent_participant_id),
                )
                .await
                .unwrap();
                assert_eq!(health["closed"], false);
                assert_eq!(health["generation_current"], true);
                assert_eq!(health["subscriber_id"], marker["subscriber_id"]);
                assert_eq!(health["generation"], marker["generation"]);
                diagnostic.record(4, "MJAI_bounded_Snapshot_same_read_writer_peer_prefix", json!({"marker":marker,"final_health_sample":health,"actual_peer_texts":peer_texts,"scope":"this subscriber/generation FIFO through sampled Snapshot only; not later notifications/all-time emptiness"}));
                let response = admin
                    .get(format!("{base}/api/v1/admin/rooms/{code}"))
                    .header("origin", &base)
                    .send()
                    .await
                    .unwrap();
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get("content-type")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let request_id = response
                    .headers()
                    .get("x-request-id")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let bytes = response.bytes().await.unwrap();
                let raw_body = std::str::from_utf8(&bytes).unwrap();
                diagnostic.record(4, "MJAI_actual_current_Admin_RoomDetail_HTTP", json!({"path":format!("/api/v1/admin/rooms/{code}"),"status":status,"content_type":content_type,"request_id":request_id,"raw":raw_body}));
                assert_eq!(status, 200);
                assert_eq!(content_type, "application/json");
                assert!(!request_id.is_empty());
                assert!(!bytes.contains(&0));
                let current: Value = serde_json::from_slice(&bytes).unwrap();
                assert_actual_admin_room_detail_schema(&current);
                assert_eq!(current["revision"], marker["read_revision"]);
                for (_, view) in &publications {
                    assert_eq!(view["room"]["revision"], current["revision"]);
                    assert!(room_snapshot_steps_match_fixed_authority(
                        &view["room"]["kyoku_end"],
                        &marker["read_step"]
                    ));
                    assert!(
                        view["room"]["kyoku_end"]["remaining_ms"].as_u64().unwrap() > 0,
                        "actively held Human frontier expired"
                    );
                }
                for key in [
                    "join_code",
                    "room_name",
                    "game_mode",
                    "participant_limit",
                    "time_control",
                    "replay_save",
                    "match_players",
                    "roster",
                    "persistence_degraded",
                    "replay_available",
                ] {
                    assert_eq!(
                        current[key], detail[key],
                        "Admin identity/roster/control: {key}"
                    );
                }
                assert_eq!(current["join_code"], code);
                assert_eq!(current["game_mode"], "4p-red-east");
                assert_eq!(current["phase"], "playing");
                for key in ["connected_count", "participant_count", "selected_count"] {
                    assert_eq!(current[key], 4);
                }
                let participants = current["participants"].as_array().unwrap();
                assert_eq!(participants.len(), 4);
                let mut current_ids: Vec<_> = participants
                    .iter()
                    .map(|p| {
                        assert_eq!(p["presence"], "connected");
                        assert_eq!(p["selected"], true);
                        assert_eq!(
                            p["kind"],
                            if p["participant_id"] == agent_id {
                                "mjai"
                            } else {
                                "human"
                            }
                        );
                        p["participant_id"].as_str().unwrap().to_owned()
                    })
                    .collect();
                let mut expected_ids: Vec<_> =
                    sockets.iter().map(|s| s.participant_id.clone()).collect();
                current_ids.sort();
                expected_ids.sort();
                assert_eq!(current_ids, expected_ids);
                assert_eq!(current["result"], Value::Null);
                prior_probe_count = target;
            }
            for (index, &seat) in humans.iter().enumerate() {
                sockets[seat]
                    .send(Message::Text(
                        json!({"type":"confirm_result","step_id":id})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                let pid = sockets[seat].participant_id.clone();
                receive(
                    &mut sockets[seat],
                    &mut transcripts[seat],
                    json!({"phase":"MJAI Human confirmation","id":id,"participant_id":pid}),
                    |v| {
                        let end = &v["room"]["kyoku_end"];
                        if index < 2 {
                            end["id"] == id
                                && end["confirmed"]
                                    .as_array()
                                    .is_some_and(|ps| ps.contains(&json!(pid)))
                        } else if step < 2 {
                            end["id"]["match_id"] == id["match_id"]
                                && end["id"]["kyoku_sequence"] == id["kyoku_sequence"]
                                && end["id"]["step_index"] == step + 1
                        } else {
                            end.is_null()
                                && v["state"]["kyoku"] == 2
                                && v["state"]["dealer"] == 1
                                && v["room"]["phase"] == "playing"
                                && v["room"]["match_players"] == detail["match_players"]
                        }
                    },
                )
                .await;
            }
            prior_id = Some(id);
        }
        receive(&mut sockets[agent_seat], &mut transcripts[agent_seat], json!({"phase":"MJAI actual next Kyoku Tsumo after full Human release","prior_id":prior_id}), |v| v["type"] == "tsumo" && v["actor"] == 1).await;
        if agent_seat == 1 {
            let request = receive(
                &mut sockets[agent_seat],
                &mut transcripts[agent_seat],
                json!({"phase":"MJAI next actual request"}),
                |v| v["type"] == "request_action",
            )
            .await;
            assert_mjai_observation(&request, agent_seat, &diagnostic);
        }
        let canonical: Vec<_> = transcripts[agent_seat]
            .iter()
            .filter(|v| v["type"] != "request_action" && v["type"] != "action_ack")
            .collect();
        assert_eq!(
            canonical
                .iter()
                .map(|v| v["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "start_game",
                "start_kyoku",
                "tsumo",
                "dahai",
                "hora",
                "hora",
                "end_kyoku",
                "start_kyoku",
                "tsumo"
            ]
        );
        assert_eq!(canonical[7]["kyoku"], 2);
        assert_eq!(canonical[7]["oya"], 1);
        assert_eq!(canonical[7]["kyotaku"], 0);
        assert_eq!(
            canonical[7]["scores"],
            json!([94200, 105200, 102600, 100000])
        );
        for (seat, hand) in canonical[7]["tehais"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            assert_eq!(hand.as_array().unwrap().len(), 13);
            assert!(
                hand.as_array()
                    .unwrap()
                    .iter()
                    .all(|tile| if seat == agent_seat {
                        tile != "?"
                    } else {
                        tile == "?"
                    })
            );
        }
        assert_eq!(canonical[8]["pai"] == "?", agent_seat != 1);
        if agent_seat == 3 {
            assert!(
                !transcripts[agent_seat]
                    .iter()
                    .any(|v| v["type"] == "request_action")
            );
            assert!(
                !diagnostic
                    .records
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|r| r["peer"] == 4 && r["kind"] == "sent")
            );
        }
        for &seat in &humans {
            let mut winners = 0;
            for value in &transcripts[seat] {
                if let Some(step) = value["room"]["kyoku_end"]["id"]["step_index"].as_u64() {
                    winners = if step == 0 { 1 } else { 2 };
                }
                assert_boundary(value, winners, Some(seat as u64));
                if seat == 3 && value["state"].is_object() {
                    assert!(
                        value["state"]["decision"].is_null()
                            || value["state"]["decision"]["kind"] != "response"
                    );
                }
            }
        }
        diagnostic.record(4, "MJAI_actual_wire_scope_limit", json!({"actual_seat":agent_seat,"canonical_frames":canonical,"complete_consumed_text_frames":transcripts[agent_seat],"causal_held_queue_frontier":"UNPROVED; no protocol barrier; Pong NOT used"}));
        println!(
            "Task8 actual4p MJAI seat{agent_seat}+3Human: literal canonical prefix/masking/settlement/release, exact Human cohort; observed frames only, causal held queue frontier BLOCKED"
        );
        for socket in &mut sockets {
            socket.close(None).await.unwrap();
        }
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
        storage.close().await;
    }

    #[tokio::test]
    async fn three_player_normal_mjai_join_is_explicitly_unsupported() {
        let (state, storage, raw, _, base, stop, server) =
            mjai_server("task8-mjai-3p-negative").await;
        let admin = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::POST,
            "/api/v1/admin/login",
            json!({"username":"admin","password":"correct horse battery staple"}),
            200,
        )
        .await;
        let created = http_at_origin(&admin, &base, &base, reqwest::Method::POST, "/api/v1/admin/rooms", json!({"room_name":"Task8 unsupported MJAI","game_mode":"3p-red-east","time_control":"unlimited","participant_limit":3,"replay_save":false}), 201).await;
        let code = created["join_code"].as_str().unwrap();
        let diagnostic = ReceiveDiagnostic {
            started: std::time::Instant::now(),
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/sdd/2026-10-02-authoritative-kyoku-results/task-8-native1/finish-task8-quota1").join(format!("mjai-3p-negative-{}-{}.json", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())),
            records: std::sync::Mutex::new(Vec::new()),
        };
        diagnostic.record(4, "actual_3p_created", created.clone());
        let before = http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::GET,
            &format!("/api/v1/admin/rooms/{code}"),
            Value::Null,
            200,
        )
        .await;
        diagnostic.record(4, "actual_3p_before", before.clone());
        assert_eq!(before["join_code"], code);
        assert_eq!(before["game_mode"], "3p-red-east");
        assert_eq!(before["phase"], "lobby");
        assert_eq!(before["participants"], json!([]));
        diagnostic.record(4, "actual_3p_nonsecret_join_request", json!({"method":"POST","path":format!("/api/v1/rooms/{code}/agents/join"),"body":{"display_name":"unsupported"},"authentication":"ordinary private token from this server; secret NOT recorded"}));
        let response = reqwest::Client::new()
            .post(format!("{base}/api/v1/rooms/{code}/agents/join"))
            .bearer_auth(&raw)
            .json(&json!({"display_name":"unsupported"}))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let headers: Vec<_> = response
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_owned(), value.to_str().unwrap().to_owned()))
            .collect();
        let request_id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let content_type = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = response.bytes().await.unwrap();
        assert!(!bytes.contains(&0));
        let text = std::str::from_utf8(&bytes).unwrap();
        diagnostic.record(
            4,
            "actual_3p_external_response",
            json!({"status":status,"headers":headers,"raw":text}),
        );
        assert_eq!(status, 409);
        assert_eq!(content_type, "application/problem+json");
        assert!(!request_id.is_empty());
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body,
            json!({"type":"about:blank","title":"Invalid request","status":409,"detail":"The request could not be processed.","code":"invalid_request","request_id":request_id})
        );
        let detail = http_at_origin(
            &admin,
            &base,
            &base,
            reqwest::Method::GET,
            &format!("/api/v1/admin/rooms/{code}"),
            Value::Null,
            200,
        )
        .await;
        diagnostic.record(4, "actual_3p_after", detail.clone());
        for field in [
            "join_code",
            "game_mode",
            "phase",
            "participants",
            "roster",
            "match_players",
            "revision",
        ] {
            assert_eq!(detail[field], before[field], "rejection changed {field}");
        }
        println!(
            "Task8 actual TCP authenticated3p MJAI join409 complete externalproblem body {text}; roster/revision unchanged; innerunsupported branch is source proof ONLY, activecompatcount NOT OBSERVED; not3p gameplay"
        );
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
        storage.close().await;
    }
    #[tokio::test]
    async fn standalone_ranked_half_four_mjai_final_double_ron_preserves_wire_and_saved_authority()
    {
        use sqlx::Row;
        if run_in_fixture_process("double-ron-final") {
            return;
        }
        assert_eq!(
            std::env::var("DRIICHI_TASK8_FIXTURE").unwrap(),
            "double-ron-final"
        );
        let storage = Arc::new(
            Storage::connect(&temp_root("task8-standalone-half"))
                .await
                .unwrap(),
        );
        let tokens = Arc::new(BotTokenService::new(
            storage.clone(),
            Arc::new(BotTokenAuthority::empty()),
        ));
        let mut raw_tokens = Vec::new();
        for index in 0..4 {
            let token = tokens
                .create("runner", 1, &format!("task8-ranked-{index}"))
                .await
                .unwrap();
            raw_tokens.push(token.secret().expose().to_owned());
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let state = Arc::new(
            ServerState::for_tests(
                &base,
                Arc::new(
                    AdminAuthenticator::new(
                        "admin",
                        hash_password("correct horse battery staple").unwrap(),
                    )
                    .unwrap(),
                ),
                RoomRegistry::with_max_rooms(8),
            )
            .with_bot_token_service(tokens),
        );
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                server_router(serve_state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        let diagnostic = Arc::new(ReceiveDiagnostic {
            started: std::time::Instant::now(),
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/sdd/2026-10-02-authoritative-kyoku-results/task-8-native1/fresh-native-fallback1").join(format!("standalone-receive-{}-{}.json", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())),
            records: std::sync::Mutex::new(Vec::new()),
        });
        let mut sockets = Vec::new();
        for (index, token) in raw_tokens.iter().enumerate() {
            let mut request = format!("ws://{address}/ws/ranked?display_name=Standalone{index}")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("authorization", format!("Bearer {token}").parse().unwrap());
            let (wire, _) = connect_async(request).await.unwrap();
            sockets.push(HumanSocket {
                wire,
                peer: index,
                participant_id: format!("Standalone{index}"),
                diagnostic: diagnostic.clone(),
            });
        }
        let mut mapped = Vec::new();
        for mut socket in sockets {
            let mut transcript = Vec::new();
            receive(
                &mut socket,
                &mut transcript,
                json!({"phase":"standalone ordinary ranked actual startup"}),
                |v| v["type"] == "tsumo",
            )
            .await;
            assert_eq!(
                transcript
                    .iter()
                    .map(|v| v["type"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                vec!["start_game", "start_kyoku", "tsumo"]
            );
            let seat = transcript[0]["id"].as_u64().unwrap() as usize;
            assert!(seat < 4);
            assert_eq!(transcript[0], json!({"type":"start_game","id":seat}));
            assert!(transcript[0].get("names").is_none());
            let start = &transcript[1];
            let own_hands = [
                json!([
                    "1m", "1m", "1m", "1m", "9m", "9m", "9m", "9m", "1p", "3p", "5p", "7p", "9p"
                ]),
                json!([
                    "1p", "2p", "3p", "4p", "5p", "6p", "7p", "8p", "9p", "1s", "1s", "1s", "2s"
                ]),
                json!([
                    "1p", "2p", "3p", "4p", "5p", "6p", "7p", "8p", "9p", "2s", "3s", "3s", "3s"
                ]),
                json!([
                    "2m", "3m", "4m", "5m", "6m", "7m", "8m", "1p", "2p", "3p", "4p", "6p", "8p"
                ]),
            ];
            assert_eq!(
                start["tehais"][seat], own_hands[seat],
                "frozen physical deal to literal own wire tiles"
            );
            assert_eq!(start["scores"], json!([1000, 100000, 100000, 100000]));
            assert_eq!(start["oya"], 0);
            assert_eq!(start["kyoku"], 1);
            assert_eq!(start["honba"], 2);
            assert_eq!(start["kyotaku"], 2);
            for (other, hand) in start["tehais"].as_array().unwrap().iter().enumerate() {
                assert_eq!(hand.as_array().unwrap().len(), 13);
                assert!(
                    hand.as_array()
                        .unwrap()
                        .iter()
                        .all(|tile| if other == seat {
                            tile != "?"
                        } else {
                            tile == "?"
                        })
                );
            }
            assert_eq!(transcript[2]["actor"], 0);
            assert_eq!(transcript[2]["pai"], if seat == 0 { "2s" } else { "?" });
            diagnostic.record(socket.peer, "standalone_actual_seat_mapping", json!({"actual_start_game":transcript[0],"seat":seat,"original_connection":socket.participant_id,"participant_identity_scope":"ordinary standalone protocol exposes seat/names; no Room ParticipantID on wire"}));
            mapped.push((seat, socket, transcript));
        }
        mapped.sort_by_key(|(seat, _, _)| *seat);
        assert_eq!(
            mapped.iter().map(|(seat, _, _)| *seat).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let (mut sockets, mut transcripts): (Vec<_>, Vec<_>) =
            mapped.into_iter().map(|(_, s, t)| (s, t)).unzip();
        for seat in 0..3 {
            choose_mjai(
                &mut sockets[seat],
                &mut transcripts[seat],
                seat,
                if seat == 0 { "dahai" } else { "hora" },
            )
            .await;
        }
        for seat in 0..4 {
            receive(
                &mut sockets[seat],
                &mut transcripts[seat],
                json!({"phase":"standalone canonical final EndGame","actual_seat":seat}),
                |v| v["type"] == "end_game",
            )
            .await;
            let canonical: Vec<_> = transcripts[seat]
                .iter()
                .filter(|v| v["type"] != "request_action" && v["type"] != "action_ack")
                .collect();
            assert_eq!(
                canonical
                    .iter()
                    .map(|v| v["type"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                vec![
                    "start_game",
                    "start_kyoku",
                    "tsumo",
                    "dahai",
                    "hora",
                    "hora",
                    "end_kyoku",
                    "end_game"
                ]
            );
            assert_eq!(
                canonical[3],
                &json!({"type":"dahai","actor":0,"pai":"2s","tsumogiri":true})
            );
            for (index, delta) in [json!([-3200, 5200, 0, 0]), json!([-2600, 0, 2600, 0])]
                .iter()
                .enumerate()
            {
                assert_eq!(canonical[index + 4]["actor"], index + 1);
                assert_eq!(canonical[index + 4]["target"], 0);
                assert_eq!(canonical[index + 4]["deltas"], *delta);
            }
            assert_eq!(
                canonical[7]["scores"],
                json!([-4800, 105200, 102600, 100000])
            );
            let mut terminal_seen = false;
            for value in &transcripts[seat] {
                if value["type"] == "hora" {
                    terminal_seen = true;
                }
                if value["type"] == "request_action" {
                    assert!(
                        !terminal_seen,
                        "standalone requested next action after final terminal"
                    );
                    assert_mjai_observation(value, seat, &diagnostic);
                } else {
                    assert_boundary(value, if terminal_seen { 2 } else { 0 }, Some(seat as u64));
                }
            }
            let close = tokio::time::timeout(Duration::from_secs(3), sockets[seat].wire.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            diagnostic.record(
                sockets[seat].peer,
                "standalone_after_EndGame_control",
                json!({"actual":format!("{close:?}")}),
            );
            assert!(
                matches!(close, Message::Close(Some(frame)) if u16::from(frame.code) == 1000 && frame.reason == "complete"),
                "ordinary complete close after replay.finish"
            );
        }
        assert!(!transcripts[3].iter().any(|v| v["type"] == "request_action"));
        let nonwinner_peer = sockets[3].peer;
        assert!(
            !diagnostic
                .records
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["peer"] == nonwinner_peer && r["kind"] == "sent")
        );
        let row = sqlx::query("SELECT match_id,status,source,game_mode,replay_path,file_size,authoritative_result_json FROM matches").fetch_one(storage.pool()).await.unwrap();
        assert_eq!(row.get::<String, _>("status"), "completed");
        assert_eq!(row.get::<String, _>("source"), "ranked");
        assert_eq!(row.get::<String, _>("game_mode"), "4p-red-half");
        let relative = row.get::<String, _>("replay_path");
        let saved = fs::read(storage.replay_root().join(&relative)).unwrap();
        assert_eq!(saved.len() as i64, row.get::<i64, _>("file_size"));
        assert!(!saved.contains(&0));
        let events =
            double_riichi_replay::parse_mjson(std::str::from_utf8(&saved).unwrap()).unwrap();
        assert_eq!(
            events
                .iter()
                .map(double_riichi_core::GameEvent::kind)
                .collect::<Vec<_>>(),
            vec![
                "start_game",
                "start_kyoku",
                "tsumo",
                "dahai",
                "hora",
                "hora",
                "end_kyoku",
                "end_game"
            ]
        );
        let result: double_riichi_core::MatchResult =
            serde_json::from_str(&row.get::<String, _>("authoritative_result_json")).unwrap();
        assert_eq!(result.mode, GameMode::FourPlayerRedHalf);
        assert_eq!(result.final_scores, vec![-4800, 105200, 102600, 100000]);
        assert_eq!(
            result.players.iter().map(|p| p.rank).collect::<Vec<_>>(),
            vec![4, 1, 2, 3]
        );
        assert!(
            result
                .players
                .iter()
                .all(|p| p.kind == double_riichi_core::ParticipantKind::MJAI)
        );
        assert_eq!(
            result
                .players
                .iter()
                .map(|p| p.participant_id.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
        for (seat, player) in result.players.iter().enumerate() {
            assert_eq!(player.seat.index() as usize, seat);
            assert_eq!(transcripts[seat][0]["id"], seat);
            assert_eq!(
                player.display_name, sockets[seat].participant_id,
                "persisted identity matches original requested socket label, NOT names on wire"
            );
        }
        let record: double_riichi_core::KyokuResultRecord = serde_json::from_str(
            &sqlx::query_scalar::<_, String>(
                "SELECT payload_json FROM match_kyoku_results WHERE match_id = ?",
            )
            .bind(row.get::<String, _>("match_id"))
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(record.mode, GameMode::FourPlayerRedHalf);
        assert_eq!(
            record
                .wins
                .iter()
                .map(|w| w.seat.index())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        for win in &record.wins {
            assert_eq!(
                (
                    win.winning_tile.id(),
                    win.winning_tile_index,
                    win.win_points
                ),
                (77, 13, 2600)
            );
        }
        diagnostic.record(4, "supplementary_saved_standalone_authority_NOT_wire", json!({"match_id":row.get::<String,_>("match_id"),"mode":"4p-red-half","replay_path":relative,"canonical_mjson_utf8":std::str::from_utf8(&saved).unwrap(),"result":result,"kyoku_record":record}));
        println!(
            "Task8 standalone actualRanked4MJAI Half same physical wall/genuine bankruptcy: complete canonical wire, base64 observations, ordinary exact offers/acks, no Human holds, saved MJSON/metadata/literal scores/ranks"
        );
        state.shutdown().await;
        stop.send(()).unwrap();
        server.await.unwrap();
        storage.close().await;
    }
}
