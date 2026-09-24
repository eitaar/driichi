use std::sync::Arc;

#[cfg(debug_assertions)]
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
#[cfg(debug_assertions)]
use axum::{
    Router,
    body::to_bytes,
    http::header,
};
#[cfg(debug_assertions)]
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use double_riichi_core::RoomRegistry;
use double_riichi_server::{AdminAuthenticator, ServerState, hash_password, server_router};
#[cfg(debug_assertions)]
use double_riichi_server::{ChatgptOAuthConfig, Storage};
#[cfg(debug_assertions)]
use serde_json::Value;
#[cfg(debug_assertions)]
use sha2::{Digest, Sha256};
#[cfg(debug_assertions)]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceExt;
#[cfg(debug_assertions)]
use url::Url;

#[cfg(debug_assertions)]
const ISSUER: &str = "https://driichi.example";
#[cfg(debug_assertions)]
const CLIENT_ID: &str = "https://chatgpt.com/oauth/client.json";
#[cfg(debug_assertions)]
const REDIRECT_URI: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
#[cfg(debug_assertions)]
const RESOURCE: &str = "https://driichi.example/chatgpt/mcp";
#[cfg(debug_assertions)]
const TEST_PASSWORD: &str = "a sufficiently long test password";
#[cfg(debug_assertions)]
const TEST_VERIFIER: &str = "a-very-long-test-verifier-which-is-at-least-43-characters";
#[cfg(debug_assertions)]
const TEST_STATE: &str = "state-value-preserved-exactly";

#[cfg(debug_assertions)]
struct TestServer {
    app: Router,
    storage: Arc<Storage>,
    root: PathBuf,
}

#[cfg(debug_assertions)]
async fn test_server(name: &str) -> TestServer {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "driichi-oauth-http-{name}-{}-{nonce}",
        std::process::id()
    ));
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let admin = Arc::new(
        AdminAuthenticator::new("admin", hash_password(TEST_PASSWORD).unwrap()).unwrap(),
    );
    let config = ChatgptOAuthConfig {
        issuer: Url::parse("https://driichi.example/").unwrap(),
        resource: Url::parse(RESOURCE).unwrap(),
        client_id: Url::parse(CLIENT_ID).unwrap(),
        redirect_uri: Url::parse(REDIRECT_URI).unwrap(),
        allowed_origins: vec![Url::parse("https://chatgpt.com/").unwrap()],
    };
    let state = Arc::new(
        ServerState::for_tests(ISSUER, admin, RoomRegistry::new())
            .with_chatgpt_oauth_for_tests(config, storage.clone()),
    );
    TestServer {
        app: server_router(state),
        storage,
        root,
    }
}

#[cfg(debug_assertions)]
async fn authorization_code_count(storage: &Storage) -> i64 {
    let options = SqliteConnectOptions::new()
        .filename(storage.database_path())
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count
}

#[cfg(debug_assertions)]
fn verifier_challenge() -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(TEST_VERIFIER.as_bytes()))
}

#[cfg(debug_assertions)]
fn authorization_fields(state: &str) -> Vec<(String, String)> {
    vec![
        ("client_id".into(), CLIENT_ID.into()),
        ("redirect_uri".into(), REDIRECT_URI.into()),
        ("response_type".into(), "code".into()),
        ("state".into(), state.into()),
        ("scope".into(), "driichi:play".into()),
        ("resource".into(), RESOURCE.into()),
        ("code_challenge".into(), verifier_challenge()),
        ("code_challenge_method".into(), "S256".into()),
    ]
}

#[cfg(debug_assertions)]
fn encoded_form(fields: &[(String, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        serializer.append_pair(name, value);
    }
    serializer.finish()
}

#[cfg(debug_assertions)]
fn authorization_uri(state: &str) -> String {
    format!(
        "/api/v1/admin/oauth/authorize?{}",
        encoded_form(&authorization_fields(state))
    )
}

#[cfg(debug_assertions)]
fn cookie_header(response: &axum::response::Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("missing {name} cookie"))
        .to_owned()
}

#[cfg(debug_assertions)]
fn cookie_pair(response: &axum::response::Response, name: &str) -> String {
    cookie_header(response, name)
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[cfg(debug_assertions)]
fn cookie_value(pair: &str) -> &str {
    pair.split_once('=').unwrap().1
}

#[cfg(debug_assertions)]
fn request_with_form(path: &str, form: &[(String, String)], cookies: &[String]) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::ORIGIN, ISSUER)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, cookies.join("; "))
        .body(Body::from(encoded_form(form)))
        .unwrap()
}

#[cfg(debug_assertions)]
async fn body_text(response: axum::response::Response) -> String {
    String::from_utf8(to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap()
}

#[cfg(debug_assertions)]
fn redirect_value(response: &axum::response::Response, key: &str) -> Option<String> {
    let location = response.headers().get(header::LOCATION)?.to_str().ok()?;
    Url::parse(location)
        .ok()?
        .query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

#[cfg(debug_assertions)]
async fn post_login(server: &TestServer, state: &str) -> (axum::response::Response, String, String) {
    let initial = server
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(authorization_uri(state))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(authorization_code_count(&server.storage).await, 0);
    let csrf_cookie = cookie_pair(&initial, "driichi_oauth_csrf");
    let page = body_text(initial).await;
    assert!(page.contains("name=\"password\""));
    assert!(!page.contains(TEST_PASSWORD));

    let mut fields = authorization_fields(state);
    fields.push(("csrf".into(), cookie_value(&csrf_cookie).into()));
    fields.push(("username".into(), "admin".into()));
    fields.push(("password".into(), TEST_PASSWORD.into()));
    let response = server
        .app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/login",
            &fields,
            std::slice::from_ref(&csrf_cookie),
        ))
        .await
        .unwrap();
    (response, csrf_cookie, page)
}

#[cfg(debug_assertions)]
async fn finish_test_server(server: TestServer) {
    drop(server.app);
    server.storage.close().await;
    let _ = std::fs::remove_dir_all(server.root);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn admin_login_requires_same_origin_csrf_and_keeps_admin_cookie_path() {
    let server = test_server("csrf").await;
    let initial = server
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(authorization_uri(TEST_STATE))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(authorization_code_count(&server.storage).await, 0);
    let csrf_cookie = cookie_pair(&initial, "driichi_oauth_csrf");

    let mut fields = authorization_fields(TEST_STATE);
    fields.push(("csrf".into(), cookie_value(&csrf_cookie).into()));
    fields.push(("username".into(), "admin".into()));
    fields.push(("password".into(), TEST_PASSWORD.into()));
    let foreign_origin = Request::builder()
        .method("POST")
        .uri("/api/v1/admin/oauth/login")
        .header(header::ORIGIN, "https://foreign.example")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &csrf_cookie)
        .body(Body::from(encoded_form(&fields)))
        .unwrap();
    let rejected = server.app.clone().oneshot(foreign_origin).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let mut bad_csrf = fields;
    bad_csrf.iter_mut().find(|(key, _)| key == "csrf").unwrap().1 = "wrong-token".into();
    let rejected = server
        .app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/login",
            &bad_csrf,
            std::slice::from_ref(&csrf_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let (login, _, page) = post_login(&server, TEST_STATE).await;
    assert_eq!(login.status(), StatusCode::OK);
    assert_eq!(authorization_code_count(&server.storage).await, 0);
    assert!(page.contains("ChatGPT &lt;Driichi&gt;"));
    assert!(!page.contains("<Driichi>"));
    let admin_cookie_header = cookie_header(&login, "driichi_admin");
    assert!(admin_cookie_header.contains("Path=/api/v1/admin"));
    assert!(admin_cookie_header.contains("HttpOnly"));
    assert!(admin_cookie_header.contains("SameSite=Strict"));
    assert!(admin_cookie_header.contains("Secure"));
    assert!(!body_text(login).await.contains("code="));

    finish_test_server(server).await;
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn denied_consent_redirects_with_state_and_issuer_without_a_code() {
    let server = test_server("denied").await;
    let (login, csrf_cookie, _) = post_login(&server, TEST_STATE).await;
    assert_eq!(login.status(), StatusCode::OK);
    assert_eq!(authorization_code_count(&server.storage).await, 0);
    let admin_cookie = cookie_pair(&login, "driichi_admin");
    let mut fields = authorization_fields(TEST_STATE);
    fields.push(("csrf".into(), cookie_value(&csrf_cookie).into()));
    fields.push(("decision".into(), "deny".into()));
    let response = server
        .app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/authorize",
            &fields,
            &[csrf_cookie.clone(), admin_cookie],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get(header::LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with(&format!("{REDIRECT_URI}?")));
    assert_eq!(authorization_code_count(&server.storage).await, 0);
    assert_eq!(redirect_value(&response, "error").as_deref(), Some("access_denied"));
    assert_eq!(redirect_value(&response, "state").as_deref(), Some(TEST_STATE));
    assert_eq!(redirect_value(&response, "iss").as_deref(), Some(ISSUER));
    assert_eq!(redirect_value(&response, "code"), None);
    assert!(cookie_header(&response, "driichi_oauth_csrf").contains("Max-Age=0"));

    finish_test_server(server).await;
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn approved_consent_mints_single_use_code_and_token_endpoint_is_form_encoded_no_store() {
    let server = test_server("approved").await;
    let (login, csrf_cookie, _) = post_login(&server, TEST_STATE).await;
    assert_eq!(login.status(), StatusCode::OK);
    assert_eq!(authorization_code_count(&server.storage).await, 0);
    let admin_cookie = cookie_pair(&login, "driichi_admin");
    let login_body = body_text(login).await;
    assert!(!login_body.contains("code="));
    assert!(!login_body.contains(TEST_PASSWORD));
    assert!(!login_body.contains("DRIICHI_CHATGPT_BOT_TOKEN"));
    let mut consent = authorization_fields(TEST_STATE);
    consent.push(("csrf".into(), cookie_value(&csrf_cookie).into()));
    consent.push(("decision".into(), "approve".into()));
    let consent_cookies = [csrf_cookie.clone(), admin_cookie.clone()];

    let mut wrong_csrf = consent.clone();
    wrong_csrf
        .iter_mut()
        .find(|(key, _)| key == "csrf")
        .unwrap()
        .1 = "wrong-token".into();
    let rejected = server
        .app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/authorize",
            &wrong_csrf,
            &consent_cookies,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let foreign_origin = Request::builder()
        .method("POST")
        .uri("/api/v1/admin/oauth/authorize")
        .header(header::ORIGIN, "https://foreign.example")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, consent_cookies.join("; "))
        .body(Body::from(encoded_form(&consent)))
        .unwrap();
    let rejected = server.app.clone().oneshot(foreign_origin).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    assert_eq!(authorization_code_count(&server.storage).await, 0);

    let response = server
        .app
        .clone()
        .oneshot(request_with_form(
            "/api/v1/admin/oauth/authorize",
            &consent,
            &consent_cookies,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get(header::LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with(&format!("{REDIRECT_URI}?")));
    assert_eq!(authorization_code_count(&server.storage).await, 1);
    let code = redirect_value(&response, "code").expect("approved consent returns a code");
    assert_eq!(redirect_value(&response, "state").as_deref(), Some(TEST_STATE));
    assert_eq!(redirect_value(&response, "iss").as_deref(), Some(ISSUER));

    let mut exchange = vec![
        ("grant_type".into(), "authorization_code".into()),
        ("client_id".into(), CLIENT_ID.into()),
        ("redirect_uri".into(), REDIRECT_URI.into()),
        ("code".into(), code.clone()),
        ("code_verifier".into(), TEST_VERIFIER.into()),
        ("resource".into(), RESOURCE.into()),
    ];
    let token_response = server
        .app
        .clone()
        .oneshot(request_with_form("/oauth/token", &exchange, &[]))
        .await
        .unwrap();
    assert_eq!(token_response.status(), StatusCode::OK);
    assert_eq!(
        token_response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let token: Value = serde_json::from_str(&body_text(token_response).await).unwrap();
    assert_eq!(token["token_type"], "Bearer");
    assert_eq!(token["expires_in"], 600);
    assert_eq!(token["scope"], "driichi:play");
    let refresh_token = token["refresh_token"].as_str().unwrap().to_owned();

    let replay = server
        .app
        .clone()
        .oneshot(request_with_form("/oauth/token", &exchange, &[]))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
    assert_eq!(replay.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(serde_json::from_str::<Value>(&body_text(replay).await).unwrap()["error"], "invalid_grant");

    exchange = vec![
        ("grant_type".into(), "refresh_token".into()),
        ("client_id".into(), CLIENT_ID.into()),
        ("resource".into(), RESOURCE.into()),
        ("refresh_token".into(), refresh_token),
    ];
    let refreshed = server
        .app
        .clone()
        .oneshot(request_with_form("/oauth/token", &exchange, &[]))
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    assert_eq!(refreshed.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(serde_json::from_str::<Value>(&body_text(refreshed).await).unwrap()["refresh_token"].is_string());

    finish_test_server(server).await;
}

#[tokio::test]
async fn discovery_routes_remain_absent_when_oauth_is_omitted() {
    let admin = Arc::new(
        AdminAuthenticator::new(
            "admin",
            hash_password("a sufficiently long test password").unwrap(),
        )
        .unwrap(),
    );
    let state = Arc::new(ServerState::for_tests(
        "http://127.0.0.1:3000",
        admin,
        RoomRegistry::new(),
    ));
    let app = server_router(state);

    for path in [
        "/.well-known/oauth-protected-resource/chatgpt/mcp",
        "/.well-known/oauth-authorization-server",
        "/api/v1/admin/oauth/authorize",
        "/api/v1/admin/oauth/login",
        "/oauth/token",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}
