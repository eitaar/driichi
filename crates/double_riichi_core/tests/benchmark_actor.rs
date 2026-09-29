use double_riichi_core::room::BenchmarkStatus;
use double_riichi_core::{
    AudienceProjection, CharacterCatalog, GameMode, Participant, ParticipantKind, RoomActor,
    RoomCommand, RoomConfig, RoomEffect, RoomHandle, RoomResponse, ShutdownMode,
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
                .send(RoomCommand::join_with_token(
                    Participant::new(format!("bot-{seat}"), "MCP", ParticipantKind::MCP),
                    "shared-token",
                ))
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

#[tokio::test]
async fn queued_roster_loss_at_durable_match_boundary_fails_without_opening_another_match() {
    for command in [
        RoomCommand::disconnect("bot-0"),
        RoomCommand::leave("bot-0"),
        RoomCommand::kick("bot-0"),
        RoomCommand::revoke_token("shared-token"),
    ] {
        let (handle, mut effects) = selected_room(false).await;
        let worker = tokio::spawn(async move {
            while let Some(effect) = effects.recv().await {
                if matches!(effect, RoomEffect::FinalizeMatch { .. }) {
                    return (effect, effects);
                }
                effect.acknowledge(Ok(()));
            }
            panic!("missing finalization");
        });
        handle
            .send(RoomCommand::StartBenchmark {
                run_id: "boundary".into(),
                target: 2,
            })
            .await
            .unwrap();
        let driver = tokio::spawn({
            let handle = handle.clone();
            async move {
                loop {
                    let run = handle.snapshot().await.unwrap().benchmark.unwrap();
                    if run.completed > 0 || run.status != BenchmarkStatus::Running {
                        break;
                    }
                    for id in ["bot-0", "bot-1", "bot-2", "bot-3"] {
                        let Ok(Some(AudienceProjection::Player(projection))) =
                            handle.projection(id).await
                        else {
                            continue;
                        };
                        if let Some(decision) =
                            projection.decision.filter(|d| !d.actions.is_empty())
                        {
                            handle
                                .send(RoomCommand::submit_action(
                                    id,
                                    decision.decision_id,
                                    decision.default_action_id,
                                ))
                                .await
                                .unwrap();
                        }
                    }
                }
            }
        });
        let (finalize, mut effects) = tokio::time::timeout(Duration::from_secs(15), worker)
            .await
            .unwrap()
            .unwrap();
        let loss = tokio::spawn({
            let handle = handle.clone();
            async move { handle.send(command).await }
        });
        tokio::task::yield_now().await; // enqueue loss before releasing finalization
        finalize.acknowledge(Ok(()));
        let terminal = tokio::time::timeout(Duration::from_secs(2), effects.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            terminal,
            RoomEffect::StopBenchmarkRun {
                status: BenchmarkStatus::Failed,
                failed_match_id: None,
                ..
            }
        ));
        terminal.acknowledge(Ok(()));
        loss.await.unwrap().unwrap();
        driver.await.unwrap();
        let run = handle.snapshot().await.unwrap().benchmark.unwrap();
        assert_eq!(run.completed, 1);
        assert_eq!(run.status, BenchmarkStatus::Failed);
        assert!(effects.try_recv().is_err());
        let _ = handle.send(RoomCommand::reconnect("bot-0")).await;
        assert_eq!(
            handle.snapshot().await.unwrap().benchmark.unwrap().status,
            BenchmarkStatus::Failed
        );
        handle
            .send(RoomCommand::shutdown(ShutdownMode::Forced))
            .await
            .unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn delayed_header_ack_retains_run_ownership_until_definitive_outcome() {
    let (handle, mut effects) = selected_room(false).await;
    let start = tokio::spawn({
        let handle = handle.clone();
        async move {
            handle
                .send(RoomCommand::StartBenchmark {
                    run_id: "delayed".into(),
                    target: 2,
                })
                .await
        }
    });
    let header = effects.recv().await.unwrap();
    assert!(matches!(header, RoomEffect::CreateBenchmarkRun { .. }));
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    header.acknowledge(Ok(()));
    let worker = tokio::spawn(async move {
        while let Some(effect) = effects.recv().await {
            effect.acknowledge(Ok(()));
        }
    });
    assert!(
        start.await.unwrap().is_ok(),
        "a late successful header must still have an owning Run"
    );
    assert_eq!(
        handle.snapshot().await.unwrap().benchmark.unwrap().status,
        BenchmarkStatus::Running
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
