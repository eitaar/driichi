use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use double_riichi_core::{GameMode, RoomCommand, RoomConfig, RoomRegistry};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ChatgptOAuthConfig, RuntimeConfig,
    ServerState, Storage, hash_password, server_router,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tower::ServiceExt;
use url::Url;

const ISSUER: &str = "https://driichi.example";
const RESOURCE: &str = "https://driichi.example/chatgpt/mcp";
const CLIENT_ID: &str = "https://chatgpt.com/oauth/client.json";
const REDIRECT_URI: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
const CHATGPT_ORIGIN: &str = "https://chatgpt.com";
const PROTOCOL_VERSION: &str = "2025-06-18";
const TEST_PASSWORD: &str = "a sufficiently long test password";
const TEST_VERIFIER: &str = "a-very-long-test-verifier-which-is-at-least-43-characters";

struct Fixture {
    state: Arc<ServerState>,
    service: Arc<BotTokenService>,
    storage: Arc<Storage>,
    root: PathBuf,
    app: Router,
    dedicated_token: String,
    pi_token: String,
}

fn root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("driichi-chatgpt-mcp-{label}-{nonce}"))
}

fn oauth_config(resource: &str) -> ChatgptOAuthConfig {
    ChatgptOAuthConfig {
        issuer: Url::parse(&format!("{ISSUER}/")).unwrap(),
        resource: Url::parse(resource).unwrap(),
        client_id: Url::parse(CLIENT_ID).unwrap(),
        redirect_uri: Url::parse(REDIRECT_URI).unwrap(),
        allowed_origins: vec![Url::parse(&format!("{CHATGPT_ORIGIN}/")).unwrap()],
    }
}

async fn fixture(label: &str, resource: &str) -> Fixture {
    let root = root(label);
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let service = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::empty()),
    ));
    let dedicated = service.create("ChatGPT", 1, "task5-chatgpt").await.unwrap();
    let dedicated_token = dedicated.secret().expose().to_owned();
    let pi = service.create("Pi", 1, "task5-pi").await.unwrap();
    let pi_token = pi.secret().expose().to_owned();
    let state = Arc::new(gateway_state(
        &service,
        &storage,
        &dedicated_token,
        resource,
    ));
    let app = server_router(state.clone());
    Fixture {
        state,
        service,
        storage,
        root,
        app,
        dedicated_token,
        pi_token,
    }
}

fn gateway_state(
    service: &Arc<BotTokenService>,
    storage: &Arc<Storage>,
    dedicated_token: &str,
    resource: &str,
) -> ServerState {
    ServerState::for_tests(
        ISSUER,
        Arc::new(AdminAuthenticator::new("admin", hash_password(TEST_PASSWORD).unwrap()).unwrap()),
        RoomRegistry::with_max_rooms(8),
    )
    .with_bot_token_service(service.clone())
    .with_chatgpt_oauth_for_tests(oauth_config(resource), storage.clone())
    .with_chatgpt_bot_token_for_tests(dedicated_token)
}

fn verifier_challenge() -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(Sha256::digest(TEST_VERIFIER.as_bytes()))
}

fn authorization_fields(resource: &str, state: &str) -> Vec<(String, String)> {
    vec![
        ("client_id".into(), CLIENT_ID.into()),
        ("redirect_uri".into(), REDIRECT_URI.into()),
        ("response_type".into(), "code".into()),
        ("state".into(), state.into()),
        ("scope".into(), "driichi:play".into()),
        ("resource".into(), resource.into()),
        ("code_challenge".into(), verifier_challenge()),
        ("code_challenge_method".into(), "S256".into()),
    ]
}

fn encoded_form(fields: &[(String, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        serializer.append_pair(name, value);
    }
    serializer.finish()
}

fn request_with_form(
    path: &str,
    fields: &[(String, String)],
    cookies: &[String],
    origin: Option<&str>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    if !cookies.is_empty() {
        request = request.header(header::COOKIE, cookies.join("; "));
    }
    request.body(Body::from(encoded_form(fields))).unwrap()
}

fn cookie_pair(response: &Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("missing {name} cookie"))
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

fn cookie_value(pair: &str) -> &str {
    pair.split_once('=').unwrap().1
}

async fn response_text(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

fn redirect_value(response: &Response, name: &str) -> Option<String> {
    let location = response.headers().get(header::LOCATION)?.to_str().ok()?;
    Url::parse(location)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

async fn mint_access(app: &Router, resource: &str, label: &str) -> String {
    let state = format!("state-{label}");
    let fields = authorization_fields(resource, &state);
    let authorize_uri = format!("/api/v1/admin/oauth/authorize?{}", encoded_form(&fields));
    let initial = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(authorize_uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial.status(), StatusCode::OK);
    let csrf = cookie_pair(&initial, "driichi_oauth_csrf");

    let mut login = authorization_fields(resource, &state);
    login.extend([
        ("csrf".into(), cookie_value(&csrf).into()),
        ("username".into(), "admin".into()),
        ("password".into(), TEST_PASSWORD.into()),
    ]);
    let login_response = app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/login",
            &login,
            std::slice::from_ref(&csrf),
            Some(ISSUER),
        ))
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);
    let admin = cookie_pair(&login_response, "driichi_admin");

    let mut consent = authorization_fields(resource, &state);
    consent.extend([
        ("csrf".into(), cookie_value(&csrf).into()),
        ("decision".into(), "approve".into()),
    ]);
    let consent_response = app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/authorize",
            &consent,
            &[csrf.clone(), admin],
            Some(ISSUER),
        ))
        .await
        .unwrap();
    assert_eq!(consent_response.status(), StatusCode::SEE_OTHER);
    let code = redirect_value(&consent_response, "code").expect("approved consent returns code");

    let exchange = vec![
        ("grant_type".into(), "authorization_code".into()),
        ("client_id".into(), CLIENT_ID.into()),
        ("redirect_uri".into(), REDIRECT_URI.into()),
        ("code".into(), code),
        ("code_verifier".into(), TEST_VERIFIER.into()),
        ("resource".into(), resource.into()),
    ];
    let token_response = app
        .clone()
        .oneshot(request_with_form("/oauth/token", &exchange, &[], None))
        .await
        .unwrap();
    assert_eq!(token_response.status(), StatusCode::OK);
    let body: Value = serde_json::from_str(&response_text(token_response).await).unwrap();
    body["access_token"].as_str().unwrap().to_owned()
}

fn rpc_request(
    path: &str,
    method: &str,
    access_token: Option<&str>,
    session_id: Option<&str>,
    id: Option<u64>,
    rpc_method: &str,
    params: Value,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "driichi.example")
        .header("accept", "application/json, text/event-stream")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = access_token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(session) = session_id {
        request = request
            .header("mcp-session-id", session)
            .header("mcp-protocol-version", PROTOCOL_VERSION);
    }
    let mut message = json!({"jsonrpc":"2.0","method":rpc_method,"params":params});
    if let Some(id) = id {
        message["id"] = json!(id);
    }
    request.body(Body::from(message.to_string())).unwrap()
}

async fn rpc(
    app: &Router,
    access_token: &str,
    session_id: Option<&str>,
    id: Option<u64>,
    method: &str,
    params: Value,
) -> Response {
    app.clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            Some(access_token),
            session_id,
            id,
            method,
            params,
        ))
        .await
        .unwrap()
}

async fn rpc_body(response: Response) -> Value {
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    if let Ok(value) = serde_json::from_slice(&body) {
        return value;
    }
    let text = String::from_utf8_lossy(&body);
    text.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|line| !line.is_empty())
        .last()
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or(Value::Null)
}

async fn initialize(app: &Router, access_token: &str) -> String {
    let response = rpc(
        app,
        access_token,
        None,
        Some(1),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name":"task5-gateway","version":"1"}
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let session = response
        .headers()
        .get("mcp-session-id")
        .expect("initialize response contains session ID")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(rpc_body(response).await["result"].is_object());
    let initialized = rpc(
        app,
        access_token,
        Some(&session),
        None,
        "notifications/initialized",
        json!({}),
    )
    .await;
    assert!(initialized.status().is_success());
    session
}

fn tool_value(body: &Value) -> Value {
    if let Some(value) = body["result"]["structuredContent"].as_object() {
        return Value::Object(value.clone());
    }
    let text = body["result"]["content"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["text"].as_str())
        .expect("tool result contains text");
    serde_json::from_str(text).unwrap()
}

async fn tool_call(
    app: &Router,
    access_token: &str,
    session: &str,
    id: u64,
    name: &str,
    arguments: Value,
) -> Value {
    let response = rpc(
        app,
        access_token,
        Some(session),
        Some(id),
        "tools/call",
        json!({"name":name,"arguments":arguments}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    rpc_body(response).await
}

async fn mutate_scope(storage: &Storage, access_token: &str) {
    let options = SqliteConnectOptions::new()
        .filename(storage.database_path())
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
    let hash = Sha256::digest(access_token.as_bytes()).to_vec();
    let row = sqlx::query("SELECT family_id FROM oauth_access_tokens WHERE token_hash = ?")
        .bind(hash.as_slice())
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let family_id: String = row.try_get("family_id").unwrap();
    sqlx::query("UPDATE oauth_access_tokens SET scope = ? WHERE token_hash = ?")
        .bind("driichi:other")
        .bind(hash.as_slice())
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("UPDATE oauth_refresh_families SET scope = ? WHERE family_id = ?")
        .bind("driichi:other")
        .bind(family_id)
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);
    pool.close().await;
}

async fn expire_access(storage: &Storage, access_token: &str) {
    let options = SqliteConnectOptions::new()
        .filename(storage.database_path())
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let hash = Sha256::digest(access_token.as_bytes()).to_vec();
    sqlx::query("UPDATE oauth_access_tokens SET expires_at = 0 WHERE token_hash = ?")
        .bind(hash)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

#[tokio::test]
async fn startup_disables_oauth_routes_without_an_active_dedicated_token() {
    let root = root("startup-disabled");
    std::fs::create_dir_all(&root).unwrap();
    let characters = root.join("character-packs");
    for (id, usage, name) in [
        ("player-red", "human", "Player Red"),
        ("player-blue", "human", "Player Blue"),
        ("mjai-bot", "mjai", "MJAI Bot"),
        ("tsumogiri-bot", "builtin", "Tsumogiri Bot"),
        ("mcp-agent", "mcp", "MCP Agent"),
    ] {
        let pack = characters.join(id);
        std::fs::create_dir_all(pack.join("voices")).unwrap();
        std::fs::write(
            pack.join("manifest.json"),
            format!(r#"{{"id":"{id}","name":"{name}","usage":"{usage}"}}"#),
        )
        .unwrap();
        std::fs::write(pack.join("LICENSE"), "CC0 1.0 Universal\n").unwrap();
        let mut webp = b"RIFF\0\0\0\0WEBP".to_vec();
        webp.extend_from_slice(b"starter");
        std::fs::write(pack.join("portrait.webp"), &webp).unwrap();
        std::fs::write(pack.join("icon.webp"), &webp).unwrap();
        for voice in ["chi", "pon", "kan", "riichi", "ron", "tsumo"] {
            std::fs::write(
                pack.join("voices").join(format!("{voice}.ogg")),
                b"OggS\0starter",
            )
            .unwrap();
        }
    }
    let password_hash = hash_password(TEST_PASSWORD).unwrap();
    std::fs::write(
        root.join(".env"),
        format!("ADMIN_USERNAME=admin\nADMIN_PASSWORD_HASH={password_hash}\n"),
    )
    .unwrap();
    let config_path = root.join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"public_origin = "https://driichi.example.com"

[chatgpt_oauth]
enabled = true
client_id = "{CLIENT_ID}"
redirect_uri = "{REDIRECT_URI}"
allowed_origins = ["{CHATGPT_ORIGIN}"]
"#
        ),
    )
    .unwrap();

    let config = RuntimeConfig::from_path(&config_path).unwrap();
    let state = Arc::new(ServerState::from_config(config).await.unwrap());
    let app = server_router(state.clone());

    for path in [
        "/.well-known/oauth-protected-resource/chatgpt/mcp",
        "/.well-known/oauth-authorization-server",
        "/chatgpt/mcp",
    ] {
        let method = if path == "/chatgpt/mcp" {
            "POST"
        } else {
            "GET"
        };
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "route {path} remained enabled without an active dedicated token"
        );
    }

    state.shutdown().await;
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn gateway_requires_oauth_for_discovery_but_challenges_tool_calls() {
    let fixture = fixture("oauth-discovery", RESOURCE).await;

    let anonymous_initialize = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            None,
            Some(20),
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name":"anonymous-discovery","version":"1"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(anonymous_initialize.status(), StatusCode::UNAUTHORIZED);
    assert!(
        anonymous_initialize
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("resource_metadata=\\"https://driichi.example/.well-known/oauth-protected-resource/chatgpt/mcp\\"")
    );
    assert!(!anonymous_initialize.headers().contains_key("mcp-session-id"));

    let anonymous_initialized = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            None,
            None,
            "notifications/initialized",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(anonymous_initialized.status(), StatusCode::UNAUTHORIZED);

    let anonymous_listing = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            None,
            Some(21),
            "tools/list",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(anonymous_listing.status(), StatusCode::UNAUTHORIZED);
    assert!(
        anonymous_listing
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .is_some()
    );
    assert!(!anonymous_listing.headers().contains_key("mcp-session-id"));

    let access = mint_access(&fixture.app, RESOURCE, "tool-challenges").await;
    let session = initialize(&fixture.app, &access).await;
    let listing = rpc(
        &fixture.app,
        &access,
        Some(&session),
        Some(22),
        "tools/list",
        json!({}),
    )
    .await;
    assert_eq!(listing.status(), StatusCode::OK);
    let listing_body = rpc_body(listing).await;
    assert_eq!(listing_body["id"], 22);
    let tools = listing_body["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    for tool in tools {
        let schemes = json!([{"type":"oauth2","scopes":["driichi:play"]}]);
        assert_eq!(tool["securitySchemes"], schemes);
        assert_eq!(tool["_meta"]["securitySchemes"], schemes);
    }

    let missing = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            Some(&session),
            Some(23),
            "tools/call",
            json!({"name":"get_my_state","arguments":{}}),
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::OK);
    assert_eq!(
        missing
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok()),
        Some(session.as_str())
    );
    let missing_body = rpc_body(missing).await;
    assert_eq!(missing_body["id"], 23);
    assert_eq!(missing_body["result"]["isError"], true);
    let challenge = missing_body["result"]["_meta"]["mcp/www_authenticate"][0]
        .as_str()
        .unwrap();
    assert!(challenge.contains("error=\"insufficient_scope\""));
    assert!(challenge.contains("error_description="));
    assert!(challenge.contains("scope=\"driichi:play\""));

    let invalid = rpc(
        &fixture.app,
        "invalid-access-token",
        Some(&session),
        Some(24),
        "tools/call",
        json!({"name":"get_my_state","arguments":{}}),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::OK);
    let invalid_body = rpc_body(invalid).await;
    assert_eq!(invalid_body["id"], 24);
    assert_eq!(invalid_body["result"]["isError"], true);
    assert!(
        invalid_body["result"]["_meta"]["mcp/www_authenticate"][0]
            .as_str()
            .unwrap()
            .contains("error=\"invalid_token\"")
    );

    let duplicate = Request::builder()
        .method("POST")
        .uri("/chatgpt/mcp")
        .header("host", "driichi.example")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header(header::CONTENT_TYPE, "application/json")
        .header("mcp-session-id", &session)
        .header("mcp-protocol-version", PROTOCOL_VERSION)
        .body(Body::from(
            json!({
                "jsonrpc":"2.0",
                "id":25,
                "method":"tools/call",
                "params":{"name":"get_my_state","arguments":{}}
            })
            .to_string(),
        ))
        .unwrap();
    let duplicate = fixture.app.clone().oneshot(duplicate).await.unwrap();
    assert_eq!(duplicate.status(), StatusCode::OK);
    let duplicate_body = rpc_body(duplicate).await;
    assert_eq!(duplicate_body["id"], 25);
    assert_eq!(duplicate_body["result"]["isError"], true);
    assert!(
        duplicate_body["result"]["_meta"]["mcp/www_authenticate"][0]
            .as_str()
            .unwrap()
            .contains("error=\"invalid_token\"")
    );

    let arbitrary_notification = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            Some(&session),
            None,
            "notifications/cancelled",
            json!({"requestId":1}),
        ))
        .await
        .unwrap();
    assert_eq!(arbitrary_notification.status(), StatusCode::UNAUTHORIZED);

    let resources = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            Some(&session),
            Some(26),
            "resources/list",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(resources.status(), StatusCode::UNAUTHORIZED);

    fixture.state.shutdown().await;
    fixture.storage.close().await;
    let _ = std::fs::remove_dir_all(fixture.root);
}

#[tokio::test]
async fn gateway_rejects_bad_origin_identity_audience_scope_and_expiry() {
    let fixture = fixture("reject", RESOURCE).await;
    let valid_access = mint_access(&fixture.app, RESOURCE, "valid").await;

    let missing = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/chatgpt/mcp",
            "POST",
            None,
            None,
            Some(1),
            "tools/call",
            json!({"name":"get_my_state","arguments":{}}),
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::OK);
    let missing_body = rpc_body(missing).await;
    assert_eq!(missing_body["id"], 1);
    assert_eq!(missing_body["result"]["isError"], true);
    let challenge = missing_body["result"]["_meta"]["mcp/www_authenticate"][0]
        .as_str()
        .unwrap();
    assert!(
        challenge.contains(
            "resource_metadata=\"https://driichi.example/.well-known/oauth-protected-resource/chatgpt/mcp\""
        )
    );
    assert!(challenge.contains("error=\"insufficient_scope\""));

    let duplicate_bearer = Request::builder()
        .method("POST")
        .uri("/chatgpt/mcp")
        .header("host", "driichi.example")
        .header(header::AUTHORIZATION, format!("Bearer {valid_access}"))
        .header(header::AUTHORIZATION, format!("Bearer {valid_access}"))
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":8,"method":"initialize","params":{}}"#,
        ))
        .unwrap();
    let duplicate_response = fixture.app.clone().oneshot(duplicate_bearer).await.unwrap();
    assert_eq!(duplicate_response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        duplicate_response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .is_some()
    );

    let foreign_origin = Request::builder()
        .method("POST")
        .uri("/chatgpt/mcp")
        .header("host", "driichi.example")
        .header(header::ORIGIN, "https://foreign.example")
        .header(header::AUTHORIZATION, format!("Bearer {valid_access}"))
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}"#,
        ))
        .unwrap();
    assert_eq!(
        fixture
            .app
            .clone()
            .oneshot(foreign_origin)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let spoofed_identity = Request::builder()
        .method("POST")
        .uri("/chatgpt/mcp")
        .header("host", "driichi.example")
        .header("x-driichi-user-id", "admin")
        .header(header::AUTHORIZATION, format!("Bearer {valid_access}"))
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{}}"#,
        ))
        .unwrap();
    assert_eq!(
        fixture
            .app
            .clone()
            .oneshot(spoofed_identity)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let wrong_audience_resource = "https://driichi.example/chatgpt/other-mcp";
    let wrong_audience_state = Arc::new(gateway_state(
        &fixture.service,
        &fixture.storage,
        &fixture.dedicated_token,
        wrong_audience_resource,
    ));
    let wrong_audience_app = server_router(wrong_audience_state.clone());
    let wrong_audience = mint_access(
        &wrong_audience_app,
        wrong_audience_resource,
        "wrong-audience",
    )
    .await;
    let accepted_at_issuer = rpc(
        &wrong_audience_app,
        &wrong_audience,
        None,
        Some(4),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name":"audience-check","version":"1"}
        }),
    )
    .await;
    assert_eq!(accepted_at_issuer.status(), StatusCode::OK);
    let rejected_audience = rpc(
        &fixture.app,
        &wrong_audience,
        None,
        Some(5),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name":"audience-check","version":"1"}
        }),
    )
    .await;
    assert_eq!(rejected_audience.status(), StatusCode::UNAUTHORIZED);
    assert!(!rejected_audience.headers().contains_key("mcp-session-id"));
    assert!(
        rejected_audience
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .is_some()
    );

    let wrong_scope = mint_access(&fixture.app, RESOURCE, "wrong-scope").await;
    mutate_scope(&fixture.storage, &wrong_scope).await;
    let scope_response = rpc(
        &fixture.app,
        &wrong_scope,
        None,
        Some(6),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name":"scope-check","version":"1"}
        }),
    )
    .await;
    assert_eq!(scope_response.status(), StatusCode::FORBIDDEN);
    let scope_tool = rpc(
        &fixture.app,
        &wrong_scope,
        None,
        Some(60),
        "tools/call",
        json!({"name":"get_my_state","arguments":{}}),
    )
    .await;
    assert_eq!(scope_tool.status(), StatusCode::OK);
    let scope_tool_body = rpc_body(scope_tool).await;
    assert_eq!(scope_tool_body["id"], 60);
    assert_eq!(scope_tool_body["result"]["isError"], true);
    let scope_challenge =
        scope_tool_body["result"]["_meta"]["mcp/www_authenticate"][0]
            .as_str()
            .unwrap();
    assert!(scope_challenge.contains("error=\"insufficient_scope\""));
    assert!(scope_challenge.contains("error_description="));
    assert!(scope_challenge.contains("scope=\"driichi:play\""));

    let expired = mint_access(&fixture.app, RESOURCE, "expired").await;
    expire_access(&fixture.storage, &expired).await;
    let expired_response = rpc(
        &fixture.app,
        &expired,
        None,
        Some(7),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name":"expiry-check","version":"1"}
        }),
    )
    .await;
    assert_eq!(expired_response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        expired_response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .is_some()
    );
    let expired_tool = rpc(
        &fixture.app,
        &expired,
        None,
        Some(61),
        "tools/call",
        json!({"name":"get_my_state","arguments":{}}),
    )
    .await;
    assert_eq!(expired_tool.status(), StatusCode::OK);
    let expired_tool_body = rpc_body(expired_tool).await;
    assert_eq!(expired_tool_body["id"], 61);
    assert_eq!(expired_tool_body["result"]["isError"], true);
    assert!(
        expired_tool_body["result"]["_meta"]["mcp/www_authenticate"][0]
            .as_str()
            .unwrap()
            .contains("error=\"invalid_token\"")
    );

    fixture.state.shutdown().await;
    wrong_audience_state.shutdown().await;
    fixture.storage.close().await;
    let _ = std::fs::remove_dir_all(fixture.root);
}

#[tokio::test]
async fn gateway_delegates_sessions_streams_and_keeps_legacy_pi_tokens_independent() {
    let fixture = fixture("compat", RESOURCE).await;
    let access = mint_access(&fixture.app, RESOURCE, "compat").await;

    let oauth_on_legacy = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/mcp",
            "POST",
            Some(&access),
            None,
            Some(1),
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name":"legacy-reject","version":"1"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(oauth_on_legacy.status(), StatusCode::UNAUTHORIZED);

    let session = initialize(&fixture.app, &access).await;
    let list = rpc(
        &fixture.app,
        &access,
        Some(&session),
        Some(2),
        "tools/list",
        json!({}),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let listing_body = rpc_body(list).await;
    let tool_descriptors = listing_body["result"]["tools"].as_array().unwrap();
    let my_state_tool = tool_descriptors
        .iter()
        .find(|tool| tool["name"] == "get_my_state")
        .expect("get_my_state tool is listed");
    let expected_schemes = json!([{"type":"oauth2","scopes":["driichi:play"]}]);
    assert_eq!(my_state_tool["securitySchemes"], expected_schemes);
    assert_eq!(my_state_tool["_meta"]["securitySchemes"], expected_schemes);
    let tools = tool_descriptors
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(tools.iter().any(|name| name == "get_my_state"));

    let room = fixture
        .state
        .rooms()
        .create(RoomConfig::new(
            "ChatGPT integration",
            GameMode::FourPlayerRedEast,
            double_riichi_core::CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    let joined = tool_call(
        &fixture.app,
        &access,
        &session,
        3,
        "join_room",
        json!({
            "room_code": room.join_code(),
            "provider": "chatgpt",
            "display_name": "ChatGPT"
        }),
    )
    .await;
    let participant_id = tool_value(&joined)["participant_id"]
        .as_str()
        .unwrap()
        .to_owned();
    room.send(RoomCommand::select(participant_id.as_str()))
        .await
        .unwrap();
    room.send(RoomCommand::fill_with_bots()).await.unwrap();
    assert!(matches!(
        room.send(RoomCommand::start()).await.unwrap(),
        double_riichi_core::RoomResponse::Started(_)
    ));
    let my_state = tool_call(
        &fixture.app,
        &access,
        &session,
        4,
        "get_my_state",
        json!({}),
    )
    .await;
    let my_state_value = tool_value(&my_state);
    let legal_actions = my_state_value["legal_actions"]
        .as_array()
        .expect("started game exposes legal actions");
    let action_id = legal_actions
        .first()
        .and_then(|action| action["action_id"].as_str())
        .expect("legal action exposes ID")
        .to_owned();
    let submitted = tool_call(
        &fixture.app,
        &access,
        &session,
        5,
        "submit_action",
        json!({"action_id":action_id}),
    )
    .await;
    assert_eq!(tool_value(&submitted)["accepted"], true);

    let stream_request = Request::builder()
        .method("GET")
        .uri("/chatgpt/mcp")
        .header("host", "driichi.example")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .header(header::ACCEPT, "text/event-stream")
        .header("mcp-session-id", &session)
        .header("mcp-protocol-version", PROTOCOL_VERSION)
        .body(Body::empty())
        .unwrap();
    let stream = fixture.app.clone().oneshot(stream_request).await.unwrap();
    assert_eq!(stream.status(), StatusCode::OK);
    assert!(
        stream
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    drop(stream);

    let delete = Request::builder()
        .method("DELETE")
        .uri("/chatgpt/mcp")
        .header("host", "driichi.example")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .header("mcp-protocol-version", PROTOCOL_VERSION)
        .body(Body::empty())
        .unwrap();
    let deleted = fixture.app.clone().oneshot(delete).await.unwrap();
    assert!(deleted.status().is_success() || deleted.status() == StatusCode::NOT_FOUND);

    let pi_session = initialize_legacy(&fixture.app, &fixture.pi_token).await;
    fixture
        .service
        .revoke(
            fixture
                .service
                .authenticate(&fixture.dedicated_token)
                .unwrap()
                .token_id(),
            2,
            "task5-revoke-chatgpt",
        )
        .await
        .unwrap();
    let revoked = rpc(
        &fixture.app,
        &access,
        None,
        Some(6),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name":"revoked-dedicated","version":"1"}
        }),
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::SERVICE_UNAVAILABLE);

    let pi_still_works = fixture
        .app
        .clone()
        .oneshot(rpc_request(
            "/mcp",
            "POST",
            Some(&fixture.pi_token),
            Some(&pi_session),
            Some(7),
            "tools/list",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(pi_still_works.status(), StatusCode::OK);

    fixture.state.shutdown().await;
    fixture.storage.close().await;
    let _ = std::fs::remove_dir_all(fixture.root);
}

async fn initialize_legacy(app: &Router, token: &str) -> String {
    let response = app
        .clone()
        .oneshot(rpc_request(
            "/mcp",
            "POST",
            Some(token),
            None,
            Some(99),
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name":"legacy-pi","version":"1"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let session = response
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let notification = app
        .clone()
        .oneshot(rpc_request(
            "/mcp",
            "POST",
            Some(token),
            Some(&session),
            None,
            "notifications/initialized",
            json!({}),
        ))
        .await
        .unwrap();
    assert!(notification.status().is_success());
    session
}
