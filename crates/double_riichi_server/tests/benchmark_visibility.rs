use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use double_riichi_core::{
    Audience, CharacterCatalog, GameMode, Participant, ParticipantKind, RoomCommand, RoomConfig,
    RoomRegistry, ShutdownMode,
};
use double_riichi_server::{AdminAuthenticator, ServerState, hash_password, server_router};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn admin_polling_has_current_hands_only_and_revalidates_auth_without_becoming_a_player() {
    let admin = Arc::new(
        AdminAuthenticator::new("admin", hash_password("benchmark-password").unwrap()).unwrap(),
    );
    let rooms = RoomRegistry::new();
    let mut config = RoomConfig::new(
        "benchmark",
        GameMode::FourPlayerRedEast,
        CharacterCatalog::starter(),
    );
    config.benchmark = true;
    let room = rooms.create(config).await.unwrap();
    for seat in 0..4 {
        room.send(RoomCommand::join(Participant::new(
            format!("bot-{seat}"),
            "MCP",
            ParticipantKind::MCP,
        )))
        .await
        .unwrap();
        room.send(RoomCommand::select(format!("bot-{seat}")))
            .await
            .unwrap();
    }
    room.send(RoomCommand::StartBenchmark {
        run_id: "visible".into(),
        target: 2,
    })
    .await
    .unwrap();
    let state = Arc::new(ServerState::for_tests(
        "http://localhost:3000",
        admin.clone(),
        rooms.clone(),
    ));
    let app = server_router(state);
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/login")
                .header("origin", "http://localhost:3000")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin", "password":"benchmark-password"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login.headers()["set-cookie"].to_str().unwrap().to_owned();
    let path = format!("/api/v1/admin/benchmark/rooms/{}/live", room.join_code());
    let response = app
        .clone()
        .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let live = json_body(response).await;
    assert_eq!(live["projection"]["audience"], "benchmark_admin");
    assert!(live["revision"].as_u64().is_some());
    for player in live["projection"]["players"].as_array().unwrap() {
        assert!(player["hand"].is_array());
    }
    let decision = &live["projection"]["decision"];
    assert!(decision.get("actions").is_none());
    assert!(decision.get("entries").is_none());
    assert!(decision.get("eligible").is_none());
    assert!(live["projection"].get("wall").is_none());
    let public = serde_json::to_value(room.public_projection().await.unwrap()).unwrap();
    for player in public["players"].as_array().unwrap() {
        assert!(player.get("hand").is_none());
    }
    let bot = serde_json::to_value(room.projection("bot-0").await.unwrap()).unwrap();
    assert_eq!(
        bot["players"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p.get("hand").is_some())
            .count(),
        1
    );
    assert_eq!(room.snapshot().await.unwrap().participants.len(), 4);
    assert_eq!(
        room.snapshot().await.unwrap().benchmark.unwrap().completed,
        0
    );
    admin.sessions().clear();
    let response = app
        .oneshot(
            Request::builder()
                .uri(&path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        room.snapshot().await.unwrap().benchmark.unwrap().completed,
        0
    );
    let ordinary = rooms
        .create(RoomConfig::new(
            "ordinary",
            GameMode::FourPlayerRedEast,
            CharacterCatalog::starter(),
        ))
        .await
        .unwrap();
    assert!(ordinary.benchmark_admin_projection().await.is_err());
    room.send(RoomCommand::shutdown(ShutdownMode::Forced))
        .await
        .unwrap();
    ordinary
        .send(RoomCommand::shutdown(ShutdownMode::Forced))
        .await
        .unwrap();
}

#[tokio::test]
async fn three_player_benchmark_rejects_mjai_before_admission() {
    let rooms = RoomRegistry::new();
    let mut config = RoomConfig::new(
        "three-player benchmark",
        GameMode::ThreePlayerRedEast,
        CharacterCatalog::starter(),
    );
    config.benchmark = true;
    let room = rooms.create(config).await.unwrap();
    assert!(
        room.send(RoomCommand::join(Participant::new(
            "mjai",
            "MJAI",
            ParticipantKind::MJAI
        )))
        .await
        .is_err()
    );
    assert!(room.snapshot().await.unwrap().participants.is_empty());
    room.send(RoomCommand::shutdown(ShutdownMode::Forced))
        .await
        .unwrap();
}

#[test]
fn benchmark_audience_does_not_reuse_replay_private_decisions() {
    use double_riichi_core::{MatchMachine, TimeControl};
    let mut machine = MatchMachine::with_fixed_seats(
        GameMode::FourPlayerRedEast,
        (0..4)
            .map(|seat| Participant::new(format!("bot-{seat}"), "MCP", ParticipantKind::MCP))
            .collect(),
        TimeControl::Unlimited,
    )
    .unwrap();
    let projection =
        serde_json::to_value(machine.project(Audience::BenchmarkAdmin).unwrap()).unwrap();
    assert_eq!(projection["audience"], "benchmark_admin");
    assert!(projection["decision"].get("entries").is_none());
    assert!(
        projection["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["hand"].is_array())
    );
}
