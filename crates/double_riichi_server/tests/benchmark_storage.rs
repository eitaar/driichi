use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use double_riichi_core::{
    GameMode, MatchPlayerResult, MatchPlayerSnapshot, MatchResult, ParticipantId, ParticipantKind,
    PermanentAutoReason, RoomController, Seat,
};
use double_riichi_replay::ReplayArtifact;
use double_riichi_server::{BenchmarkRunStatus, Storage};

fn test_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "driichi-benchmark-storage-{}-{nanos}",
        std::process::id()
    ))
}

fn roster() -> Vec<MatchPlayerSnapshot> {
    (0..4)
        .map(|seat| MatchPlayerSnapshot {
            participant_id: ParticipantId::new(format!("bot-{seat}")),
            display_name: format!("Bot {seat}"),
            kind: ParticipantKind::BuiltInBot,
            seat: Seat::new(seat).unwrap(),
            character_id: None,
            controller: RoomController::PermanentAuto(PermanentAutoReason::BuiltInBot),
        })
        .collect()
}

#[tokio::test]
async fn run_roster_survives_restart_and_active_run_becomes_interrupted() {
    let root = test_root();
    let storage = Storage::connect(&root).await.unwrap();
    storage
        .create_benchmark_run(
            "run-one",
            "123456",
            GameMode::FourPlayerRedEast,
            4,
            &roster(),
        )
        .await
        .unwrap();
    let before = storage.load_benchmark_run("run-one").await.unwrap();
    assert_eq!(before.status, BenchmarkRunStatus::Running);
    assert_eq!(before.target, 4);
    assert_eq!(
        before
            .roster
            .iter()
            .map(|player| player.participant_id.as_str())
            .collect::<Vec<_>>(),
        ["bot-0", "bot-1", "bot-2", "bot-3"]
    );
    storage.close().await;
    drop(storage);

    let storage = Storage::connect(&root).await.unwrap();
    let after = storage.load_benchmark_run("run-one").await.unwrap();
    assert_eq!(after.status, BenchmarkRunStatus::Interrupted);
    assert_eq!(after.completed, 0);
    storage.close().await;
}

#[tokio::test]
async fn failed_attempt_is_separate_from_completed_matches_and_room_lifetime() {
    let root = test_root();
    let storage = Storage::connect(&root).await.unwrap();
    storage
        .create_benchmark_run(
            "run-failed",
            "654321",
            GameMode::FourPlayerRedEast,
            5,
            &roster(),
        )
        .await
        .unwrap();
    storage
        .fail_benchmark_run("run-failed", Some("incomplete-match"), "bot disconnected")
        .await
        .unwrap();
    let run = storage.load_benchmark_run("run-failed").await.unwrap();
    assert_eq!(run.status, BenchmarkRunStatus::Failed);
    assert_eq!(run.completed, 0);
    assert!(run.matches.is_empty());
    assert_eq!(run.failed_match_id.as_deref(), Some("incomplete-match"));
    assert_eq!(run.reason.as_deref(), Some("bot disconnected"));
    assert_eq!(storage.list_benchmark_runs().await.unwrap().len(), 1);
    storage.close().await;
}

#[tokio::test]
async fn completion_is_atomic_idempotent_and_keeps_distinct_participants() {
    let root = test_root();
    let storage = Storage::connect(&root).await.unwrap();
    let roster = roster();
    storage
        .create_benchmark_run("run-two", "123456", GameMode::FourPlayerRedEast, 2, &roster)
        .await
        .unwrap();
    sqlx::query("INSERT INTO matches (match_id, source, game_mode, started_at, status, replay_path) VALUES ('match-one', 'room', '4p-red-east', 1, 'writing', '4p/match-one.mjson')")
        .execute(storage.pool()).await.unwrap();
    for player in &roster {
        sqlx::query("INSERT INTO match_players (match_id, participant_id, display_name, participant_kind, seat) VALUES ('match-one', ?, ?, 'builtin_bot', ?)")
            .bind(player.participant_id.as_str()).bind(&player.display_name).bind(i64::from(player.seat.index()))
            .execute(storage.pool()).await.unwrap();
    }
    let artifact = ReplayArtifact {
        relative_path: "4p/match-one.mjson".into(),
        file_size: 100,
        auxiliary_events: vec![],
    };
    let result = MatchResult {
        mode: GameMode::FourPlayerRedEast,
        players: roster
            .iter()
            .enumerate()
            .map(|(seat, player)| MatchPlayerResult {
                participant_id: player.participant_id.clone(),
                display_name: player.display_name.clone(),
                kind: player.kind,
                seat: player.seat,
                final_score: [45000, 30000, 15000, 10000][seat],
                rank: (seat + 1) as u8,
            })
            .collect(),
        final_scores: vec![45000, 30000, 15000, 10000],
    };
    assert!(
        storage
            .record_benchmark_completion("run-two", 2, "match-one", &result, &artifact, 2)
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .load_benchmark_run("run-two")
            .await
            .unwrap()
            .completed,
        0
    );
    storage
        .record_benchmark_completion("run-two", 1, "match-one", &result, &artifact, 2)
        .await
        .unwrap();
    storage
        .record_benchmark_completion("run-two", 1, "match-one", &result, &artifact, 2)
        .await
        .unwrap();
    assert!(
        storage
            .record_benchmark_completion("run-two", 2, "match-one", &result, &artifact, 2)
            .await
            .is_err()
    );
    let run = storage.load_benchmark_run("run-two").await.unwrap();
    assert_eq!(run.completed, 1);
    assert_eq!(
        run.matches[0]
            .results
            .iter()
            .map(|entry| entry.participant_id.as_str())
            .collect::<Vec<_>>(),
        ["bot-0", "bot-1", "bot-2", "bot-3"]
    );
    assert_eq!(
        run.matches[0]
            .results
            .iter()
            .map(|entry| (entry.rank, entry.final_score))
            .collect::<Vec<_>>(),
        [(1, 45000), (2, 30000), (3, 15000), (4, 10000)]
    );
    assert!(
        sqlx::query("DELETE FROM matches WHERE match_id = 'match-one'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    storage.close().await;
}
