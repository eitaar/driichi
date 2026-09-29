use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use double_riichi_core::{
    CharacterCatalog, GameMode, MatchId, RoomCommand, RoomConfig, RoomRegistry, RoomResponse,
};
use double_riichi_server::{
    AdminAuthenticator, BotTokenAuthority, BotTokenService, ServerState, Storage, hash_password,
    server_router, spawn_room_effect_worker,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tower::ServiceExt;

#[tokio::test]
async fn mjai_socket_reinitializes_with_rotated_seat_before_second_match_actions() {
    let root = std::env::temp_dir().join(format!("benchmark-mjai-{}", MatchId::generate()));
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let worker_storage = storage.clone();
    let rooms = RoomRegistry::new().with_effect_spawner(move |effects| {
        spawn_room_effect_worker(worker_storage.clone(), effects)
    });
    let tokens = Arc::new(BotTokenService::new(
        storage.clone(),
        Arc::new(BotTokenAuthority::empty()),
    ));
    let token = tokens.create("benchmark-mjai", 1, "test").await.unwrap();
    let state = Arc::new(
        ServerState::for_tests(
            "http://localhost:3000",
            Arc::new(
                AdminAuthenticator::new("admin", hash_password("benchmark-password").unwrap())
                    .unwrap(),
            ),
            rooms.clone(),
        )
        .with_bot_token_service(tokens),
    );
    let mut config = RoomConfig::new(
        "MJAI series",
        GameMode::FourPlayerRedEast,
        CharacterCatalog::starter(),
    );
    config.benchmark = true;
    let room = rooms.create(config).await.unwrap();
    let code = room.join_code().to_string();
    let response = server_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/rooms/{code}/agents/join"))
                .header(
                    "authorization",
                    format!("Bearer {}", token.secret().expose()),
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"display_name":"MJAI challenger"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let response: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let id = response["participant_id"].as_str().unwrap();
    room.send(RoomCommand::select(id)).await.unwrap();
    for _ in 0..3 {
        let RoomResponse::Joined(bot) = room.send(RoomCommand::AddBenchmarkBot).await.unwrap()
        else {
            panic!("bot join");
        };
        room.send(RoomCommand::select(bot.id)).await.unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let state = state.clone();
        async move {
            axum::serve(
                listener,
                server_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        }
    });
    let mut request = format!("ws://{address}/ws/v1/rooms/{code}/mjai?participant_id={id}")
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", token.secret().expose())
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = connect_async(request).await.unwrap();
    room.send(RoomCommand::StartBenchmark {
        run_id: "mjai-series".into(),
        target: 2,
    })
    .await
    .unwrap();
    let mut starts = Vec::new();
    let mut kyoku_started = false;
    let mut request_id = 0;
    tokio::time::timeout(Duration::from_secs(90), async {
        let mut poll = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                frame = socket.next() => {
                    let Message::Text(text) = frame.unwrap().unwrap() else { continue; };
                    let value: Value = serde_json::from_str(&text).unwrap();
                    match value["type"].as_str() {
                        Some("start_game") => { starts.push(value["id"].as_u64().unwrap()); kyoku_started = false; },
                        Some("start_kyoku") => kyoku_started = true,
                        Some("request_action") => {
                            let counted = room.snapshot().await.unwrap().benchmark.unwrap().completed;
                            assert_eq!(starts.len(), usize::from(counted) + 1, "each Match needs fresh initialization before requesting actions");
                            assert!(kyoku_started);
                            let next = value["request_id"].as_u64().unwrap();
                            assert!(next > request_id, "connection request IDs must remain monotonic across Matches");
                            request_id = next;
                            let mut action = value["possible_actions"].as_array().unwrap()[0].clone();
                            action["request_id"] = value["request_id"].clone();
                            socket.send(Message::Text(action.to_string().into())).await.unwrap();
                        },
                        Some("action_ack") => assert_ne!(value["status"], "rejected"),
                        _ => {},
                    }
                },
                _ = poll.tick() => {
                    let run = storage.load_benchmark_run("mjai-series").await.unwrap();
                    if run.status != double_riichi_server::BenchmarkRunStatus::Running {
                        assert_eq!(run.status, double_riichi_server::BenchmarkRunStatus::Completed);
                        assert_eq!(run.completed, 2);
                        break;
                    }
                },
            }
        }
    }).await.unwrap();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[1], (starts[0] + 1) % 4);
    socket.close(None).await.unwrap();
    state.shutdown().await;
    server.abort();
    storage.close().await;
}
