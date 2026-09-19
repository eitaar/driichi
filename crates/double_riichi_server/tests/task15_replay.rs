use std::{fs, io::Read, path::PathBuf, sync::Arc};

use axum::{body::Body, http::Request};
use double_riichi_core::{
    GameEvent, GameMode, Participant, ParticipantKind, RoomCommand, RoomPhase, RoomRegistry, Seat,
    Tile, TimeControl, Wind,
};
use double_riichi_replay::{MAX_DECOMPRESSED_REPLAY_BYTES, ReplayWriter};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerState, Storage, StorageError,
    hash_password, server_router, spawn_room_effect_worker,
};
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use sqlx::Row;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_room_match_is_visible_through_admin_replay_api() {
    let root = temp_root("room-persistence");
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let (effects, receiver) = tokio::sync::mpsc::channel(double_riichi_core::ROOM_EFFECT_CAPACITY);
    spawn_room_effect_worker(storage.clone(), receiver);
    let registry = RoomRegistry::with_max_rooms(8).with_effect_sender(effects);
    let app = ServerState::for_tests(
        "http://127.0.0.1:3000",
        Arc::new(
            AdminAuthenticator::new(
                "admin",
                hash_password("correct horse battery staple").unwrap(),
            )
            .unwrap(),
        ),
        registry.clone(),
    )
    .with_bot_token_service(Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::from_records(vec![])),
    )));
    let app = server_router(Arc::new(app));
    let room = registry
        .create(
            double_riichi_core::RoomConfig::new(
                "Persistence Room",
                GameMode::ThreePlayerRedEast,
                double_riichi_core::CharacterCatalog::starter(),
            )
            .with_time_control(TimeControl::Unlimited),
        )
        .await
        .unwrap();
    room.send(RoomCommand::fill_with_bots()).await.unwrap();
    let match_id = match room.send(RoomCommand::start()).await.unwrap() {
        double_riichi_core::RoomResponse::Started(match_id) => match_id.to_string(),
        response => panic!("unexpected start response: {response:?}"),
    };
    assert!(matches!(
        room.snapshot().await.unwrap().phase,
        RoomPhase::PostMatch(_)
    ));

    let cookie = admin_cookie(&app).await;
    let mut replay = None;
    for _ in 0..1000 {
        let response = app
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
        assert_eq!(response.status(), 200);
        let body = json_body(response).await;
        replay = body["replays"]
            .as_array()
            .and_then(|entries| entries.iter().find(|entry| entry["match_id"] == match_id))
            .cloned();
        if replay.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let replay = replay.expect("room replay should be persisted");
    assert_eq!(replay["source"], "room");
    assert_eq!(replay["room_name"], "Persistence Room");
    assert_eq!(replay["game_mode"], "3p-red-east");
    assert_eq!(replay["availability"], "available");

    let view = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/admin/replays/{match_id}"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view.status(), 200);
    let view_body = json_body(view).await;
    assert_eq!(view_body["match_id"], match_id);
    assert_eq!(view_body["source"], "room");
    assert_eq!(view_body["room_name"], "Persistence Room");
    assert_eq!(view_body["players"].as_array().map(Vec::len), Some(3));
    assert!(!view_body["frames"].as_array().unwrap().is_empty());
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn replay_view_negotiates_gzip_without_changing_bounded_json() {
    let (app, storage, root) = app_fixture("gzip").await;
    let cookie = admin_cookie(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays/MATCH15")
                .header("accept-encoding", "gzip")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let compressed = axum::body::to_bytes(response.into_body(), 80 * 1024 * 1024)
        .await
        .unwrap();
    let mut decoder = GzDecoder::new(compressed.as_ref());
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    let body: Value = serde_json::from_slice(&decompressed).unwrap();
    assert_eq!(body["match_id"], "MATCH15");
    assert!(body["frames"].is_array());
    storage.close().await;
    let _ = fs::remove_dir_all(root);
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
async fn ranked_half_replay_reconstruction_uses_metadata_mode() {
    let (app, storage, root) = app_fixture("half-mode").await;
    sqlx::query("UPDATE matches SET game_mode = '4p-red-half' WHERE match_id = 'MATCH15'")
        .execute(storage.pool())
        .await
        .unwrap();
    let cookie = admin_cookie(&app).await;
    let view = app
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
    assert_eq!(
        json_body(view).await["frames"][1]["visible_state"]["mode"],
        "FourPlayerRedHalf"
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
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

#[tokio::test]
async fn missing_registered_replay_ancestor_is_unavailable_but_deletable() {
    let (app, storage, root) = app_fixture("missing-ancestor").await;
    let replay_path: String =
        sqlx::query_scalar("SELECT replay_path FROM matches WHERE match_id = 'MATCH15'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    fs::remove_dir_all(root.join("replays/4p")).unwrap();
    let cookie = admin_cookie(&app).await;

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
    assert_eq!(view.status(), 422);
    assert_eq!(json_body(view).await["code"], "replay_unavailable");

    let deleted = app
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
    assert!(replay_path.starts_with("4p/"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE match_id = 'MATCH15'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn replay_list_validates_reconstructed_frames() {
    let root = temp_root("frame-validation");
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let mut writer = ReplayWriter::new(
        storage.replay_root(),
        "FRAMEBAD",
        GameMode::ThreePlayerRedEast,
    )
    .unwrap();
    writer
        .append(GameEvent::StartGame {
            names: Some(vec!["East".into(), "South".into(), "West".into()]),
            id: Some("frame-invalid".into()),
        })
        .unwrap();
    writer
        .append(GameEvent::StartKyoku {
            bakaze: Wind::East,
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: Seat::new(0).unwrap(),
            scores: vec![25_000; 3],
            dora_marker: Tile::from_id(0).unwrap(),
            tehais: vec![vec![Tile::from_id(0).unwrap(); 13]; 3],
        })
        .unwrap();
    writer
        .append(GameEvent::Kita {
            actor: Seat::new(0).unwrap(),
        })
        .unwrap();
    writer.append(GameEvent::EndGame).unwrap();
    let artifact = writer.finalize().unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES (?, 'ranked', NULL, ?, ?, ?, 'completed', ?, ?)",
    )
    .bind("FRAMEBAD")
    .bind("3p-red-east")
    .bind(1_i64)
    .bind(2_i64)
    .bind(artifact.relative_path_string())
    .bind(i64::try_from(artifact.file_size).unwrap())
    .execute(storage.pool())
    .await
    .unwrap();
    let app = app_for_storage(storage.clone()).await;
    let cookie = admin_cookie(&app).await;
    let list = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = json_body(list).await;
    let replay = body["replays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|replay| replay["match_id"] == "FRAMEBAD")
        .unwrap();
    assert_eq!(replay["availability"], "unavailable");
    assert_eq!(replay["replay_available"], false);
    assert!(storage.replay_degraded());
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn corrupt_completed_replay_degrades_health_during_storage_startup() {
    let root = temp_root("startup-validation");
    let storage = Storage::connect(&root).await.unwrap();
    let replay_path = root.join("replays/4p/startup-corrupt.mjson");
    fs::write(&replay_path, "not-json\n").unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES ('STARTUPBAD', 'ranked', NULL, '4p-red-east', 1, 2, 'completed', '4p/startup-corrupt.mjson', 9)",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    storage.close().await;
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert!(reopened.replay_degraded());
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn storage_startup_retries_pending_admin_audits() {
    let root = temp_root("pending-audit");
    let storage = Storage::connect(&root).await.unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sqlx::query(
        "INSERT INTO admin_audit_pending (request_id, occurred_at, action, target_type, target_id, summary_json, state) VALUES ('PENDING15', ?, 'room_create', 'room', 'PENDING-ROOM', '{\"room_name\":\"Pending\"}', 'applied')",
    )
    .bind(now)
    .execute(storage.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO admin_audit_pending (request_id, occurred_at, action, target_type, target_id, summary_json, state) VALUES ('PREPARED15', ?, 'room_create', 'room', 'PREPARED-ROOM', '{\"room_name\":\"Prepared\"}', 'prepared')",
    )
    .bind(now)
    .execute(storage.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO admin_audit_pending (request_id, occurred_at, action, target_type, target_id, summary_json, state) VALUES ('ROLLEDBACK15', ?, 'room_create', 'room', 'ROLLEDBACK-ROOM', '{\"room_name\":\"Rolled back\"}', 'rolled_back')",
    )
    .bind(now)
    .execute(storage.pool())
    .await
    .unwrap();
    storage.close().await;
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE request_id = 'PENDING15'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE request_id = 'PENDING15'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE request_id = 'PREPARED15'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE request_id = 'PREPARED15'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE request_id = 'ROLLEDBACK15'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE request_id = 'ROLLEDBACK15' AND state = 'rolled_back'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn storage_startup_cleanup_removes_orphan_replay_parts() {
    let root = temp_root("orphan-part");
    let incomplete = root.join("replays/.incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    let orphan = incomplete.join("ORPHAN.mjson.part");
    fs::write(&orphan, "partial").unwrap();

    let storage = Storage::connect(&root).await.unwrap();
    assert!(!orphan.exists());
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn replay_list_and_view_agree_when_metadata_crosses_the_response_limit() {
    let (app, storage, root) = app_fixture("response-boundary").await;
    sqlx::query(
        "INSERT INTO match_players (match_id, participant_id, display_name, participant_kind, seat, character_id, final_points) VALUES ('MATCH15', 'large-player', ?, 'human', 0, NULL, 25000)",
    )
    .bind("x".repeat(MAX_DECOMPRESSED_REPLAY_BYTES - 512))
    .execute(storage.pool())
    .await
    .unwrap();
    let cookie = admin_cookie(&app).await;

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
    let replay = json_body(list).await["replays"][0].clone();
    assert_eq!(replay["availability"], "too_large");
    assert_eq!(replay["replay_available"], false);

    let view = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/replays/MATCH15")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view.status(), 413);
    assert_eq!(json_body(view).await["code"], "replay_too_large");
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn admin_state_changing_operations_are_audited_but_reads_and_failures_are_not() {
    let root = temp_root("admin-audit");
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let admin = Arc::new(
        AdminAuthenticator::new(
            "admin",
            hash_password("correct horse battery staple").unwrap(),
        )
        .unwrap(),
    );
    let service = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::empty()),
    ));
    let state = Arc::new(
        ServerState::for_tests(
            "http://127.0.0.1:3000",
            admin,
            RoomRegistry::with_max_rooms(4),
        )
        .with_bot_token_service(service),
    );
    let app = server_router(state.clone());
    let cookie = admin_cookie(&app).await;

    let login_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'login'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(login_count, 1);

    let failed_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/login")
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin","password":"wrong"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed_login.status(), 401);

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
                    json!({"room_name":"Audit Room","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let create_request_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let room = json_body(response).await;
    let join_code = room["join_code"].as_str().unwrap().to_owned();
    let handle = state.rooms().get(&join_code).await.unwrap();
    let room_id = handle.id().to_string();
    let created_request_id: String = sqlx::query_scalar(
        "SELECT request_id FROM audit_logs WHERE action = 'room_create' AND target_id = ?",
    )
    .bind(&join_code)
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(created_request_id, create_request_id);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(json!({"room_name":"Audit Room 2"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let configure_request_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let configure_summary: String = sqlx::query_scalar(
        "SELECT summary_json FROM audit_logs WHERE action = 'room_configure' AND target_id = ?",
    )
    .bind(&join_code)
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(configure_summary, r#"{"changed_fields":["room_name"]}"#);
    let configured_request_id: String = sqlx::query_scalar(
        "SELECT request_id FROM audit_logs WHERE action = 'room_configure' AND target_id = ?",
    )
    .bind(&join_code)
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(configured_request_id, configure_request_id);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/rooms")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/admin/replays/NO-SUCH-REPLAY"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_client_error());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);

    handle
        .send(RoomCommand::join(Participant::new(
            "participant-1",
            "Hidden display name",
            ParticipantKind::Human,
        )))
        .await
        .unwrap();
    for (path, expected_action) in [
        (
            format!("/api/v1/admin/rooms/{join_code}/participants/participant-1/select"),
            "participant_select",
        ),
        (
            format!("/api/v1/admin/rooms/{join_code}/participants/participant-1/deselect"),
            "participant_deselect",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("origin", "http://127.0.0.1:3000")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{expected_action}");
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/admin/rooms/{join_code}/participants/participant-1/kick"
                ))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/start"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_client_error());

    for (path, action) in [
        ("fill-with-bots", "fill_with_bots"),
        ("start", "match_start"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/admin/rooms/{join_code}/{path}"))
                    .header("origin", "http://127.0.0.1:3000")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{action}");
    }
    for _ in 0..250 {
        if matches!(
            &handle.snapshot().await.unwrap().phase,
            RoomPhase::PostMatch(_)
        ) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(matches!(
        &handle.snapshot().await.unwrap().phase,
        RoomPhase::PostMatch(_)
    ));

    for (path, action) in [("rematch", "rematch")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/admin/rooms/{join_code}/{path}"))
                    .header("origin", "http://127.0.0.1:3000")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{action}");
    }
    for _ in 0..250 {
        if matches!(
            &handle.snapshot().await.unwrap().phase,
            RoomPhase::PostMatch(_)
        ) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(matches!(
        &handle.snapshot().await.unwrap().phase,
        RoomPhase::PostMatch(_)
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/back-to-lobby"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/logout")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let rows = sqlx::query("SELECT action, request_id, summary_json FROM audit_logs ORDER BY id")
        .fetch_all(storage.pool())
        .await
        .unwrap();
    let actions: Vec<String> = rows
        .iter()
        .map(|row| row.try_get("action").unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            "login",
            "room_create",
            "room_configure",
            "participant_select",
            "participant_deselect",
            "participant_kick",
            "fill_with_bots",
            "match_start",
            "rematch",
            "back_to_lobby",
            "room_delete",
            "logout",
        ]
    );
    let summaries: Vec<String> = rows
        .iter()
        .map(|row| row.try_get("summary_json").unwrap())
        .collect();
    assert!(summaries.iter().all(|summary| {
        !summary.contains("Hidden display name")
            && !summary.contains("password")
            && !summary.contains("cookie")
            && !summary.contains("driichi_")
    }));
    assert_eq!(room_id.len(), 26);

    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn admin_room_mutation_is_not_applied_when_audit_insert_fails() {
    let (app, storage, root) = app_fixture("audit-failure").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Audit Failure","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "CREATE TRIGGER task15_fail_fill_audit BEFORE INSERT ON audit_logs WHEN NEW.action = 'fill_with_bots' BEGIN SELECT RAISE(ABORT, 'forced audit failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let failed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/fill-with-bots"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed.status(), 500);

    sqlx::query("DROP TRIGGER task15_fail_fill_audit")
        .execute(storage.pool())
        .await
        .unwrap();
    let room = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let room = json_body(room).await;
    assert_eq!(room["participants"].as_array().unwrap().len(), 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        0
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn accepted_room_audit_does_not_depend_on_pending_state_update() {
    let (app, storage, root) = app_fixture("pending-state-update").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Pending State Update","game_mode":"4p-red-east"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "CREATE TRIGGER task15_fail_pending_state_update BEFORE UPDATE ON admin_audit_pending WHEN NEW.state = 'applied' BEGIN SELECT RAISE(ABORT, 'forced pending state update failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/fill-with-bots"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    sqlx::query("DROP TRIGGER task15_fail_pending_state_update")
        .execute(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    storage.close().await;
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE action = 'fill_with_bots'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        0
    );
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn accepted_room_audit_recovers_a_prepared_row_after_flush_failure() {
    let (app, storage, root) = app_fixture("prepared-room-recovery").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Prepared Recovery","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "CREATE TRIGGER task15_fail_prepared_room_flush BEFORE INSERT ON audit_logs WHEN NEW.action = 'fill_with_bots' AND EXISTS (SELECT 1 FROM admin_audit_pending WHERE request_id = NEW.request_id AND state = 'prepared') BEGIN SELECT RAISE(ABORT, 'forced prepared audit flush failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/fill-with-bots"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    let room = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        json_body(room).await["participants"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE action = 'fill_with_bots' AND state = 'prepared'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER task15_fail_prepared_room_flush")
        .execute(storage.pool())
        .await
        .unwrap();
    storage.close().await;
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE action = 'fill_with_bots'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        0
    );
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn admin_deselect_noop_does_not_write_audit() {
    let (app, storage, root) = app_fixture("deselect-noop").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Deselect No-op","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    let joined = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{join_code}/join"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"nickname":"Unselected","character_id":"player-red"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(joined.status(), 201);
    let participant_id = json_body(joined).await["participant_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/admin/rooms/{join_code}/participants/{participant_id}/deselect"
                ))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'participant_deselect'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        0
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn canceled_noop_survives_audit_pending_delete_failure_without_recovery_audit() {
    let (app, storage, root) = app_fixture("cancel-delete-failure").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Cancel Delete Failure","game_mode":"4p-red-east"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/fill-with-bots"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    sqlx::query(
        "CREATE TRIGGER task15_fail_cancel_delete BEFORE DELETE ON admin_audit_pending WHEN OLD.action = 'fill_with_bots' AND OLD.state = 'prepared' BEGIN SELECT RAISE(ABORT, 'forced cancellation delete failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let noop = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/rooms/{join_code}/fill-with-bots"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(noop.status(), 200);
    sqlx::query("DROP TRIGGER task15_fail_cancel_delete")
        .execute(storage.pool())
        .await
        .unwrap();

    let canceled_request: String = sqlx::query_scalar(
        "SELECT request_id FROM admin_audit_pending WHERE action = 'fill_with_bots' AND state = 'rolled_back'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE request_id = ?",)
            .bind(&canceled_request)
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    storage.close().await;
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE request_id = ?",)
            .bind(&canceled_request)
            .fetch_one(reopened.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE request_id = ? AND state = 'rolled_back'",
        )
        .bind(&canceled_request)
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn admin_fill_noop_does_not_write_audit() {
    let (app, storage, root) = app_fixture("fill-noop").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Fill No-op","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/admin/rooms/{join_code}/fill-with-bots"))
                    .header("origin", "http://127.0.0.1:3000")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'fill_with_bots'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn late_audit_flush_failure_does_not_recover_a_rolled_back_logout() {
    let (app, storage, root) = app_fixture("late-logout-audit-failure").await;
    let cookie = admin_cookie(&app).await;
    sqlx::query(
        "CREATE TRIGGER task15_fail_late_logout_audit BEFORE INSERT ON audit_logs WHEN NEW.action = 'logout' AND EXISTS (SELECT 1 FROM admin_audit_pending WHERE request_id = NEW.request_id AND state IN ('prepared', 'applied')) BEGIN SELECT RAISE(ABORT, 'forced late audit failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let failed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/logout")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed.status(), 500);
    sqlx::query("DROP TRIGGER task15_fail_late_logout_audit")
        .execute(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE action = 'logout'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    storage.close().await;
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE action = 'logout'")
            .fetch_one(reopened.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM admin_audit_pending WHERE state = 'applied'",
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        0
    );
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn admin_logout_audit_failure_restores_the_session() {
    let (app, storage, root) = app_fixture("logout-audit-failure").await;
    let cookie = admin_cookie(&app).await;
    sqlx::query(
        "CREATE TRIGGER task15_fail_logout_audit BEFORE INSERT ON audit_logs WHEN NEW.action = 'logout' BEGIN SELECT RAISE(ABORT, 'forced audit failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    let failed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/logout")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed.status(), 500);
    sqlx::query("DROP TRIGGER task15_fail_logout_audit")
        .execute(storage.pool())
        .await
        .unwrap();
    let still_authenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/rooms")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(still_authenticated.status(), 200);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE action = 'logout'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn concurrent_admin_room_deletes_have_one_success_audit() {
    let (app, storage, root) = app_fixture("concurrent-delete").await;
    let cookie = admin_cookie(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/rooms")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"room_name":"Delete Race","game_mode":"4p-red-east"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_code = json_body(created).await["join_code"]
        .as_str()
        .unwrap()
        .to_owned();
    let request = |app: axum::Router| async {
        app.oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/rooms/{join_code}"))
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    };
    let (first, second) = tokio::join!(request(app.clone()), request(app));
    let statuses = [first.status(), second.status()];
    assert!(statuses.contains(&axum::http::StatusCode::NO_CONTENT));
    assert!(statuses.contains(&axum::http::StatusCode::NOT_FOUND));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'room_delete'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        1
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[tokio::test]
async fn unsafe_registered_replay_can_be_deleted_without_touching_target() {
    use std::os::unix::fs::symlink;

    let (app, storage, root) = app_fixture("unsafe-delete").await;
    let replay_dir = root.join("replays/4p");
    let source = fs::read_dir(&replay_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "mjson")
        })
        .unwrap();
    let outside = root.join("outside.mjson");
    fs::write(&outside, "keep").unwrap();
    let link = replay_dir.join("link.mjson");
    symlink(&outside, &link).unwrap();
    fs::remove_file(source).unwrap();
    sqlx::query("UPDATE matches SET replay_path = '4p/link.mjson' WHERE match_id = 'MATCH15'")
        .execute(storage.pool())
        .await
        .unwrap();
    let cookie = admin_cookie(&app).await;
    let response = app
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
    assert_eq!(response.status(), 204);
    assert!(outside.exists());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE match_id = 'MATCH15'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}
