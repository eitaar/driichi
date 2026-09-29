use double_riichi_core::room::BenchmarkStatus;
use double_riichi_core::{
    CharacterCatalog, GameMode, Participant, ParticipantKind, RoomActor, RoomCommand, RoomConfig,
    RoomEffect, RoomHandle, RoomResponse, ShutdownMode,
};
use std::time::Duration;

async fn selected_room(built_in: bool) -> (RoomHandle, tokio::sync::mpsc::Receiver<RoomEffect>) {
    let mut config = RoomConfig::new(
        "benchmark",
        GameMode::FourPlayerRedEast,
        CharacterCatalog::starter(),
    );
    config.benchmark = true;
    let (handle, effects) = RoomActor::spawn_with_effect_channel(config);
    for seat in 0..4 {
        let response = if built_in {
            handle.send(RoomCommand::AddBenchmarkBot).await.unwrap()
        } else {
            handle
                .send(RoomCommand::join(Participant::new(
                    format!("bot-{seat}"),
                    "MCP",
                    ParticipantKind::MCP,
                )))
                .await
                .unwrap()
        };
        let RoomResponse::Joined(player) = response else {
            panic!("join response")
        };
        handle.send(RoomCommand::select(player.id)).await.unwrap();
    }
    (handle, effects)
}

#[tokio::test]
async fn series_waits_for_header_and_completion_ack_before_opening_matches() {
    let (handle, mut effects) = selected_room(true).await;
    let start = tokio::spawn({
        let handle = handle.clone();
        async move {
            handle
                .send(RoomCommand::StartBenchmark {
                    run_id: "ack-series".into(),
                    target: 2,
                })
                .await
        }
    });
    let header = effects.recv().await.unwrap();
    assert!(matches!(header, RoomEffect::CreateBenchmarkRun { .. }));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), effects.recv())
            .await
            .is_err()
    );
    header.acknowledge(Ok(()));
    let open = effects.recv().await.unwrap();
    let RoomEffect::OpenMatch { roster: first, .. } = &open else {
        panic!("first open")
    };
    let first = first.clone();
    open.acknowledge(Ok(()));
    let worker = tokio::spawn(async move {
        while let Some(effect) = effects.recv().await {
            if matches!(effect, RoomEffect::FinalizeMatch { .. }) {
                return (effect, effects);
            }
            effect.acknowledge(Ok(()));
        }
        panic!("missing finalization")
    });
    start.await.unwrap().unwrap();
    let (finalize, mut effects) = worker.await.unwrap();
    assert!(matches!(
        &finalize,
        RoomEffect::FinalizeMatch {
            benchmark: Some((_, 1)),
            ..
        }
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), effects.recv())
            .await
            .is_err()
    );
    finalize.acknowledge(Ok(()));
    let next = effects.recv().await.unwrap();
    let RoomEffect::OpenMatch { roster: second, .. } = &next else {
        panic!("second open")
    };
    assert_eq!(second[0].participant_id, first[3].participant_id);
    assert_eq!(second[1].participant_id, first[0].participant_id);
    next.acknowledge(Ok(()));
    let worker = tokio::spawn(async move {
        while let Some(effect) = effects.recv().await {
            effect.acknowledge(Ok(()));
        }
    });
    assert_eq!(
        handle
            .snapshot()
            .await
            .unwrap()
            .benchmark
            .unwrap()
            .completed,
        1
    );
    handle
        .send(RoomCommand::shutdown(ShutdownMode::Forced))
        .await
        .unwrap();
    worker.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn external_bot_timeout_stops_before_temporary_autoplay_can_count_results() {
    let (handle, mut effects) = selected_room(false).await;
    tokio::spawn(async move {
        while let Some(effect) = effects.recv().await {
            effect.acknowledge(Ok(()));
        }
    });
    handle
        .send(RoomCommand::StartBenchmark {
            run_id: "timeout".into(),
            target: 2,
        })
        .await
        .unwrap();
    tokio::time::advance(Duration::from_secs(120)).await;
    handle.send(RoomCommand::Tick).await.unwrap();
    let state = handle.snapshot().await.unwrap();
    assert_eq!(state.benchmark.as_ref().unwrap().completed, 0);
    assert_eq!(state.benchmark.unwrap().status, BenchmarkStatus::Failed);
    handle
        .send(RoomCommand::shutdown(ShutdownMode::Forced))
        .await
        .unwrap();
}
