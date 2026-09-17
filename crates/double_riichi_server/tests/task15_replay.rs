use std::{fs, path::PathBuf, sync::Arc};

use axum::{body::Body, http::Request};
use double_riichi_core::{GameEvent, GameMode, Seat, Tile, Wind};
use double_riichi_replay::{MAX_DECOMPRESSED_REPLAY_BYTES, ReplayWriter};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerState, Storage, StorageError,
    hash_password, server_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "double-riichi-task15-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn replay_events() -> Vec<GameEvent> {
    vec![
        GameEvent::StartGame {
            names: Some(vec![
                "East".into(),
                "South".into(),
                "West".into(),
                "North".into(),
            ]),
            id: Some("task15-match".into()),
        },
        GameEvent::StartKyoku {
            bakaze: Wind::East,
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: Seat::new(0).unwrap(),
            scores: vec![25_000; 4],
            dora_marker: Tile::from_id(0).unwrap(),
            tehais: vec![vec![Tile::from_id(0).unwrap(); 13]; 4],
        },
        GameEvent::EndKyoku,
        GameEvent::EndGame,
    ]
}

async fn app_for_storage(storage: Arc<Storage>) -> axum::Router {
    let password_hash = hash_password("correct horse battery staple").unwrap();
    let admin = Arc::new(AdminAuthenticator::new("admin", password_hash).unwrap());
    let authority = BotTokenAuthority::from_records(vec![]);
    let service = Arc::new(BotTokenService::new(storage.clone(), Arc::new(authority)));
    let state = ServerState::for_tests(
        "http://127.0.0.1:3000",
        admin,
        double_riichi_core::RoomRegistry::with_max_rooms(8),
    )
    .with_bot_token_service(service);
    server_router(Arc::new(state))
}

async fn app_fixture(name: &str) -> (axum::Router, Arc<Storage>, PathBuf) {
    let root = temp_root(name);
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let writer_root = storage.replay_root().to_path_buf();
    let mut writer =
        ReplayWriter::new(&writer_root, "MATCH15", GameMode::FourPlayerRedEast).unwrap();
    for event in replay_events() {
        writer.append(event).unwrap();
    }
    let artifact = writer.finalize().unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES (?, 'room', ?, ?, ?, ?, 'completed', ?, ?)",
    )
    .bind("MATCH15")
    .bind("Night Market")
    .bind("4p-red-east")
    .bind(1_i64)
    .bind(2_i64)
    .bind(artifact.relative_path_string())
    .bind(i64::try_from(artifact.file_size).unwrap())
    .execute(storage.pool())
    .await
    .unwrap();
    (app_for_storage(storage.clone()).await, storage, root)
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 80 * 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
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
                    json!({"username":"admin","password":"correct horse battery staple"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn admin_replay_list_view_and_delete_are_authenticated_and_server_built() {
    let (app, storage, root) = app_fixture("api").await;
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);
    for (method, uri) in [
        ("GET", "/api/v1/admin/replays/MATCH15"),
        ("DELETE", "/api/v1/admin/replays/MATCH15"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{method} {uri}");
    }

    let cookie = admin_cookie(&app).await;
    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays?offset=0&limit=50")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let list_body = json_body(list).await;
    assert_eq!(list_body["replays"][0]["match_id"], "MATCH15");
    assert_eq!(list_body["replays"][0]["room_name"], "Night Market");

    let unsafe_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/replays/MATCH15")
                .header("origin", "https://evil.example")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unsafe_delete.status(), 403);
    assert_eq!(json_body(unsafe_delete).await["code"], "origin_not_allowed");

    let view = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays/MATCH15")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view.status(), 200);
    let view_body = json_body(view).await;
    assert_eq!(view_body["match_id"], "MATCH15");
    assert_eq!(view_body["frames"][0]["event_index"], 0);
    assert!(view_body["frames"][1]["visible_state"]["players"].is_array());

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/replays/MATCH15")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE match_id = 'MATCH15'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'replay_delete'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT summary_json FROM audit_logs WHERE action = 'replay_delete'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        r#"{"match_id":"MATCH15"}"#
    );
    assert!(!root.join("replays/4p").read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("MATCH15")
    }));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn admin_replay_list_rejects_oversized_limits_and_corrupt_replays_stay_deletable() {
    let (app, storage, root) = app_fixture("corrupt").await;
    fs::write(storage.replay_root().join("4p/corrupt.mjson"), "not-json\n").unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES ('CORRUPT15', 'ranked', NULL, '4p-red-east', 1, 2, 'completed', '4p/corrupt.mjson', 9)",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES ('OVERSIZED15', 'ranked', NULL, '4p-red-east', 1, 2, 'completed', '4p/oversized.mjson', ?)",
    )
    .bind(i64::try_from(MAX_DECOMPRESSED_REPLAY_BYTES).unwrap() + 1)
    .execute(storage.pool())
    .await
    .unwrap();
    let cookie = admin_cookie(&app).await;
    let invalid = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays?limit=101")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    assert_eq!(json_body(invalid).await["code"], "invalid_pagination");
    let malformed = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays?offset=not-a-number")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), 400);
    assert_eq!(json_body(malformed).await["code"], "invalid_pagination");
    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list_body = json_body(list).await;
    let oversized = list_body["replays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|replay| replay["match_id"] == "OVERSIZED15")
        .unwrap();
    assert_eq!(oversized["availability"], "too_large");
    assert_eq!(oversized["replay_available"], false);

    let oversized_view = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays/OVERSIZED15")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_view.status(), 413);
    assert_eq!(json_body(oversized_view).await["code"], "replay_too_large");

    let view = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays/CORRUPT15")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view.status(), 422);
    assert_eq!(json_body(view).await["code"], "replay_unavailable");
    let health = app
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
    assert_eq!(health.status(), 503);
    assert_eq!(json_body(health).await["replay_storage"], "degraded");

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/replays/CORRUPT15")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    let oversized_deleted = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/replays/OVERSIZED15")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_deleted.status(), 204);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn registered_replay_paths_cannot_escape_the_replay_root() {
    let root = temp_root("paths");
    let storage = Storage::connect(&root).await.unwrap();
    for path in [
        "../outside.mjson",
        "replays/../../outside.mjson",
        "/outside.mjson",
    ] {
        assert!(matches!(
            storage.resolve_replay_path(path),
            Err(StorageError::UnsafeReplayPath)
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = root.join("outside.mjson");
        fs::write(&outside, "outside\n").unwrap();
        symlink(&outside, storage.replay_root().join("4p/link.mjson")).unwrap();
        assert!(matches!(
            storage.resolve_replay_path("4p/link.mjson"),
            Err(StorageError::UnsafeReplayPath)
        ));
    }
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn replay_delete_keeps_metadata_retryable_after_database_failure() {
    let (app, storage, root) = app_fixture("retry").await;
    let cookie = admin_cookie(&app).await;
    sqlx::query(
        "CREATE TRIGGER task15_fail_replay_delete BEFORE DELETE ON matches WHEN OLD.match_id = 'MATCH15' BEGIN SELECT RAISE(ABORT, 'forced delete failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    let failed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/replays/MATCH15")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed.status(), 500);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE match_id = 'MATCH15'",)
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        1
    );
    assert!(!root.join("replays/4p").read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("MATCH15")
    }));

    sqlx::query("DROP TRIGGER task15_fail_replay_delete")
        .execute(storage.pool())
        .await
        .unwrap();
    let retried = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/replays/MATCH15")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retried.status(), 204);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE match_id = 'MATCH15'",)
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'replay_delete'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}
