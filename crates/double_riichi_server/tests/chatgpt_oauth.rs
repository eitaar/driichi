use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use double_riichi_core::RoomRegistry;
use double_riichi_server::{AdminAuthenticator, ServerState, hash_password, server_router};
use tower::ServiceExt;

#[tokio::test]
async fn discovery_routes_are_absent_when_oauth_is_omitted() {
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
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}
