use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use double_riichi_core::{GameMode, RoomRegistry, ShutdownMode};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerState, Storage, hash_password,
    server_router, spawn_room_effect_worker,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

async fn body(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn request(
    app: &axum::Router,
    cookie: &str,
    method: &str,
    uri: &str,
    data: Value,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("cookie", cookie)
                .header("origin", "http://localhost:3000")
                .header("content-type", "application/json")
                .body(Body::from(data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_lifecycle_requires_explicit_selection_and_retains_history_after_room_deletion() {
    let root = std::env::temp_dir().join(format!(
        "benchmark-http-{}-{}",
        std::process::id(),
        double_riichi_core::MatchId::generate()
    ));
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let worker_storage = storage.clone();
    let rooms = RoomRegistry::new().with_effect_spawner(move |effects| {
        spawn_room_effect_worker(worker_storage.clone(), effects)
    });
    let admin = Arc::new(
        AdminAuthenticator::new("admin", hash_password("benchmark-password").unwrap()).unwrap(),
    );
    let service = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::from_records(vec![])),
    ));
    let app = server_router(Arc::new(
        ServerState::for_tests("http://localhost:3000", admin, rooms.clone())
            .with_bot_token_service(service),
    ));
    for path in [
        "/api/v1/admin/benchmark/runs",
        "/api/v1/admin/benchmark/runs/missing",
    ] {
        let response = request(&app, "", "GET", path, Value::Null).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let login = request(
        &app,
        "",
        "POST",
        "/api/v1/admin/login",
        json!({"username":"admin", "password":"benchmark-password"}),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login.headers()["set-cookie"].to_str().unwrap().to_owned();
    let response = request(
        &app,
        &cookie,
        "POST",
        "/api/v1/admin/benchmark/rooms",
        json!({"room_name":"Benchmark", "game_mode":"4p-red-east"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let room = body(response).await;
    assert_eq!(room["benchmark_mode"], true);
    assert_eq!(room["replay_save"], true);
    let code = room["join_code"].as_str().unwrap();
    let base = format!("/api/v1/admin/benchmark/rooms/{code}");
    let response = request(
        &app,
        &cookie,
        "POST",
        &format!("{base}/runs"),
        json!({"target":1}),
    )
    .await;
    assert!(response.status().is_client_error());
    let rejected = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/bots"))
                .header("cookie", &cookie)
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    for _ in 0..4 {
        let response = request(&app, &cookie, "POST", &format!("{base}/bots"), json!({})).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let room = rooms.get(code).await.unwrap();
    for participant in room.snapshot().await.unwrap().participants {
        assert!(!participant.selected);
        let response = request(
            &app,
            &cookie,
            "POST",
            &format!(
                "/api/v1/admin/rooms/{code}/participants/{}/select",
                participant.id
            ),
            json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    for target in [0, 1001] {
        assert!(
            request(
                &app,
                &cookie,
                "POST",
                &format!("{base}/runs"),
                json!({"target":target})
            )
            .await
            .status()
            .is_client_error()
        );
    }
    let response = request(
        &app,
        &cookie,
        "POST",
        &format!("{base}/runs"),
        json!({"target":2}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let started = body(response).await;
    let id = started["benchmark"]["run_id"].as_str().unwrap();
    let path = format!("/api/v1/admin/benchmark/runs/{id}");
    let run = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let response = request(&app, &cookie, "GET", &path, Value::Null).await;
            assert_eq!(response.status(), StatusCode::OK);
            let run = body(response).await;
            if run["status"] != "running" {
                break run;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(run["status"], "completed");
    assert_eq!(run["completed"], 2);
    assert_eq!(run["statistics"].as_array().unwrap().len(), 4);
    for stat in run["statistics"].as_array().unwrap() {
        assert!(stat["average_rank"].is_number());
        assert_eq!(stat["cumulative_net_scores"].as_array().unwrap().len(), 2);
    }
    assert_eq!(
        request(&app, &cookie, "POST", &format!("{base}/stop"), json!({}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, &cookie, "POST", &format!("{base}/stop"), json!({}))
            .await
            .status(),
        StatusCode::OK
    );
    rooms.remove(code).await.unwrap();
    let history = body(request(&app, &cookie, "GET", &path, Value::Null).await).await;
    assert_eq!(history["completed"], 2);
    assert_eq!(
        body(
            request(
                &app,
                &cookie,
                "GET",
                "/api/v1/admin/benchmark/runs",
                Value::Null
            )
            .await
        )
        .await
        .as_array()
        .unwrap()
        .len(),
        1
    );
    let _ = ShutdownMode::Forced;
    let _ = GameMode::FourPlayerRedEast;
    storage.close().await;
}
