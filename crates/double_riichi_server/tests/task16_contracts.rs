use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    http::{Request, header},
};
use double_riichi_core::{CharacterCatalog, GameMode, RoomConfig, RoomJoinCode, RoomRegistry};
use double_riichi_server::{
    AdminAuthenticator, BUILD_COMMIT, BUILD_VERSION, BotTokenAuthority, BotTokenService,
    RuntimeConfig, ServerState, Storage, TracingFormat, hash_password, server_router,
};
use futures_util::StreamExt;
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
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM matches WHERE match_id = 'task16-shutdown'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(remaining, 0);
    drop(state);
    storage.close().await;
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
