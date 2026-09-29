use double_riichi_core::room::BenchmarkStatus;
use double_riichi_core::{
    CharacterCatalog, GameMode, Participant, ParticipantKind, RoomActor, RoomCommand, RoomConfig,
    RoomHandle, RoomPhase, RoomResponse, ShutdownMode,
};
use double_riichi_server::{BenchmarkRunStatus, Storage, spawn_room_effect_worker};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

async fn room(built_in: bool) -> (RoomHandle, Arc<Storage>, tokio::task::JoinHandle<()>) {
    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "benchmark-run-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    let storage = Arc::new(Storage::connect(&root).await.unwrap());
    let mut config = RoomConfig::new(
        "benchmark",
        GameMode::FourPlayerRedEast,
        CharacterCatalog::starter(),
    );
    config.benchmark = true;
    config.empty_room_cleanup = Duration::from_secs(60);
    let (room, effects) = RoomActor::spawn_with_effect_channel(config);
    let worker = spawn_room_effect_worker(storage.clone(), effects);
    for seat in 0..4 {
        let response = if built_in {
            room.send(RoomCommand::AddBenchmarkBot).await.unwrap()
        } else {
            room.send(RoomCommand::join_with_token(
                Participant::new(
                    format!("bot-{seat}"),
                    format!("Bot {seat}"),
                    ParticipantKind::MCP,
                ),
                "shared-token",
            ))
            .await
            .unwrap()
        };
        let RoomResponse::Joined(player) = response else {
            panic!("join response")
        };
        room.send(RoomCommand::select(player.id)).await.unwrap();
    }
    (room, storage, worker)
}

async fn terminal(room: &RoomHandle) -> double_riichi_core::RoomSnapshot {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let state = room.snapshot().await.unwrap();
            if state
                .benchmark
                .as_ref()
                .is_some_and(|run| run.status != BenchmarkStatus::Running)
            {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("run should terminate")
}

async fn close(room: RoomHandle, storage: Arc<Storage>, worker: tokio::task::JoinHandle<()>) {
    room.send(RoomCommand::shutdown(ShutdownMode::Forced))
        .await
        .unwrap();
    drop(room);
    worker.await.unwrap();
    storage.close().await;
}

#[tokio::test]
async fn built_in_series_counts_only_durable_matches_and_rotates_fixed_ids() {
    for target in [1, 2] {
        let (room, storage, worker) = room(true).await;
        room.send(RoomCommand::StartBenchmark {
            run_id: "series".into(),
            target,
        })
        .await
        .unwrap();
        let state = terminal(&room).await;
        assert_eq!(state.benchmark.as_ref().unwrap().completed, target);
        assert_eq!(state.benchmark.unwrap().status, BenchmarkStatus::Completed);
        let run = storage.load_benchmark_run("series").await.unwrap();
        assert_eq!(run.status, BenchmarkRunStatus::Completed);
        assert_eq!(run.matches.len(), usize::from(target));
        for player in &run.roster {
            for (index, game) in run.matches.iter().enumerate() {
                let result = game
                    .results
                    .iter()
                    .find(|result| result.participant_id == player.participant_id)
                    .unwrap();
                assert_eq!(
                    result.seat as usize,
                    (player.initial_seat as usize + index) % 4
                );
            }
        }
        assert!(matches!(state.phase, RoomPhase::PostMatch(_)));
        close(room, storage, worker).await;
    }
}

#[tokio::test]
async fn graceful_stop_counts_current_match_only_and_is_idempotent() {
    let (room, storage, worker) = room(true).await;
    room.send(RoomCommand::StartBenchmark {
        run_id: "stop".into(),
        target: 1000,
    })
    .await
    .unwrap();
    room.send(RoomCommand::StopBenchmark).await.unwrap();
    let state = terminal(&room).await;
    assert_eq!(state.benchmark.as_ref().unwrap().completed, 1);
    assert_eq!(state.benchmark.unwrap().status, BenchmarkStatus::Stopped);
    room.send(RoomCommand::StopBenchmark).await.unwrap();
    assert_eq!(
        storage.load_benchmark_run("stop").await.unwrap().completed,
        1
    );
    close(room, storage, worker).await;
}

#[tokio::test]
async fn disconnect_leave_kick_and_revocation_fail_before_autoplay_and_cannot_resume() {
    for command in [
        RoomCommand::disconnect("bot-0"),
        RoomCommand::leave("bot-0"),
        RoomCommand::kick("bot-0"),
        RoomCommand::revoke_token("shared-token"),
    ] {
        let (room, storage, worker) = room(false).await;
        room.send(RoomCommand::StartBenchmark {
            run_id: "lost".into(),
            target: 2,
        })
        .await
        .unwrap();
        room.send(RoomCommand::StopBenchmark).await.unwrap(); // failure must beat graceful stop
        room.send(command).await.unwrap();
        let state = terminal(&room).await;
        assert_eq!(state.benchmark.as_ref().unwrap().completed, 0);
        assert_eq!(state.benchmark.unwrap().status, BenchmarkStatus::Failed);
        let run = storage.load_benchmark_run("lost").await.unwrap();
        assert_eq!(run.status, BenchmarkRunStatus::Failed);
        assert!(run.matches.is_empty());
        assert!(run.failed_match_id.is_some());
        let _ = room.send(RoomCommand::reconnect("bot-0")).await;
        assert_eq!(
            room.snapshot().await.unwrap().benchmark.unwrap().status,
            BenchmarkStatus::Failed
        );
        assert!(
            room.send(RoomCommand::StartBenchmark {
                run_id: "revive".into(),
                target: 1
            })
            .await
            .is_err()
        );
        assert!(
            room.send(RoomCommand::join(Participant::new(
                "late",
                "Late",
                ParticipantKind::MCP
            )))
            .await
            .is_err()
        );
        assert!(room.send(RoomCommand::AddBenchmarkBot).await.is_err());
        assert!(room.send(RoomCommand::select("bot-1")).await.is_err());
        close(room, storage, worker).await;
    }
}

#[tokio::test]
async fn association_failure_rolls_back_replay_and_does_not_advance_series() {
    let (room, storage, worker) = room(true).await;
    sqlx::query("CREATE TRIGGER fail_benchmark_association BEFORE INSERT ON benchmark_matches BEGIN SELECT RAISE(FAIL, 'injected association failure'); END;").execute(storage.pool()).await.unwrap();
    room.send(RoomCommand::StartBenchmark {
        run_id: "failed-save".into(),
        target: 2,
    })
    .await
    .unwrap();
    let state = terminal(&room).await;
    assert_eq!(state.benchmark.as_ref().unwrap().completed, 0);
    assert_eq!(state.benchmark.unwrap().status, BenchmarkStatus::Failed);
    let run = storage.load_benchmark_run("failed-save").await.unwrap();
    assert_eq!(run.completed, 0);
    assert!(run.matches.is_empty());
    let completed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM matches WHERE status = 'completed'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(completed, 0);
    close(room, storage, worker).await;
}
