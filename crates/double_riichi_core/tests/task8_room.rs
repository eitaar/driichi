use double_riichi_core::room::{
    CharacterCatalog, CharacterUsage, RoomActor, RoomAuxiliaryEvent, RoomCommand, RoomConfig,
    RoomController, RoomEffect, RoomError, RoomEvent, RoomPhase, RoomRegistry, RoomResponse,
    ShutdownMode,
};
use double_riichi_core::{GameMode, Participant, ParticipantKind, TimeControl};

fn config() -> RoomConfig {
    RoomConfig::new(
        "Task 8",
        GameMode::FourPlayerRedEast,
        CharacterCatalog::starter(),
    )
}

fn human(id: &str, _character: &str) -> Participant {
    Participant::new(id, id, ParticipantKind::Human)
}

#[tokio::test]
async fn actor_lifecycle_selects_fills_readies_and_starts_atomically() {
    let (handle, mut effects) = RoomActor::spawn_with_effect_channel(config());
    let mut events = handle.subscribe().await.unwrap();

    for (id, character) in [("h0", "player-red"), ("h1", "player-blue")] {
        let response = handle
            .send(RoomCommand::join(human(id, character)))
            .await
            .unwrap();
        assert!(matches!(response, RoomResponse::Joined(_)));
    }
    handle
        .send(RoomCommand::select_with_character("h0", "player-red"))
        .await
        .unwrap();
    handle
        .send(RoomCommand::select_with_character("h1", "player-blue"))
        .await
        .unwrap();
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();

    for (id, characters) in [
        (
            "h0",
            vec![
                "player-red".to_owned(),
                "player-blue".to_owned(),
                "tsumogiri-bot".to_owned(),
            ],
        ),
        (
            "h1",
            vec![
                "player-red".to_owned(),
                "player-blue".to_owned(),
                "tsumogiri-bot".to_owned(),
            ],
        ),
    ] {
        handle
            .send(RoomCommand::set_ready(id, characters))
            .await
            .unwrap();
    }

    let start = tokio::spawn({
        let handle = handle.clone();
        async move { handle.send(RoomCommand::start()).await }
    });
    let effect = effects.recv().await.expect("open effect");
    effect.acknowledge(Ok(()));
    let response = start.await.unwrap().unwrap();
    assert!(matches!(response, RoomResponse::Started(_)));
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::Playing(_)
    ));

    let _ = effects.recv().await.expect("open effect");
    assert!(matches!(events.recv().await, Some(RoomEvent::Snapshot(_))));
}

#[tokio::test]
async fn three_player_mode_rejects_mjai_join_and_mode_change_clears_selection() {
    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    assert!(matches!(
        handle
            .send(RoomCommand::join(Participant::new(
                "bot",
                "bot",
                ParticipantKind::MJAI,
            )))
            .await,
        Err(RoomError::InvalidCharacter)
    ));

    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle.send(RoomCommand::select("h")).await.unwrap();
    handle
        .send(RoomCommand::set_mode(GameMode::FourPlayerRedEast))
        .await
        .unwrap();
    let snapshot = handle.snapshot().await.unwrap();
    assert_eq!(snapshot.mode, GameMode::FourPlayerRedEast);
    assert!(snapshot.participants.iter().all(|p| !p.selected));
}

#[tokio::test]
async fn mjai_join_and_mode_change_are_serialized_by_the_room_actor() {
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(config());
    let mode_change = handle
        .try_send(RoomCommand::set_mode(GameMode::ThreePlayerRedEast))
        .unwrap();
    let join = handle
        .try_send(RoomCommand::join(Participant::new(
            "bot",
            "bot",
            ParticipantKind::MJAI,
        )))
        .unwrap();

    assert!(matches!(
        mode_change.await.unwrap(),
        Ok(RoomResponse::Accepted(_))
    ));
    assert!(matches!(
        join.await.unwrap(),
        Err(RoomError::InvalidCharacter)
    ));
    let snapshot = handle.snapshot().await.unwrap();
    assert_eq!(snapshot.mode, GameMode::ThreePlayerRedEast);
    assert!(snapshot.participants.is_empty());
}

#[tokio::test]
async fn deselecting_an_unselected_participant_preserves_other_humans_ready_state() {
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(config());
    for (id, character) in [
        ("h0", "player-red"),
        ("h1", "player-blue"),
        ("h2", "tsumogiri-bot"),
    ] {
        handle
            .send(RoomCommand::join(human(id, character)))
            .await
            .unwrap();
    }
    for id in ["h0", "h1"] {
        handle.send(RoomCommand::select(id)).await.unwrap();
    }
    for id in ["h0", "h1"] {
        handle
            .send(RoomCommand::set_ready(
                id,
                vec!["player-red".to_owned(), "player-blue".to_owned()],
            ))
            .await
            .unwrap();
    }
    let before = handle.snapshot().await.unwrap();
    handle.send(RoomCommand::deselect("h2")).await.unwrap();
    let after = handle.snapshot().await.unwrap();

    assert_eq!(after.revision, before.revision);
    for id in ["h0", "h1"] {
        assert!(
            after
                .participants
                .iter()
                .find(|participant| participant.id.as_str() == id)
                .unwrap()
                .ready
        );
    }
}

#[tokio::test]
async fn fill_with_bots_respects_participant_capacity() {
    let mut cfg = config();
    cfg.max_participants = 1;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    let result = handle.send(RoomCommand::fill_with_bots()).await;
    assert!(result.is_err());
    assert!(handle.snapshot().await.unwrap().participants.len() <= 1);
}

#[test]
fn character_catalog_keeps_usage_metadata_for_selection_validation() {
    let catalog = CharacterCatalog::starter();
    assert_eq!(catalog.usage("player-red"), Some(CharacterUsage::Human));
    assert_eq!(catalog.usage("mjai-bot"), Some(CharacterUsage::Mjai));
    assert_eq!(TimeControl::Casual.turn_duration().as_secs(), 30);
}

#[tokio::test(start_paused = true)]
async fn disconnected_selected_lobby_participant_releases_its_seat_at_expiry() {
    use std::time::Duration;

    let mut cfg = config();
    cfg.disconnected_participant_expiry = Duration::from_secs(1);
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle
        .send(RoomCommand::join(human("h0", "player-red")))
        .await
        .unwrap();
    handle.send(RoomCommand::select("h0")).await.unwrap();
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle.send(RoomCommand::disconnect("h0")).await.unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    handle
        .send(RoomCommand::join(human("h1", "player-blue")))
        .await
        .unwrap();
    handle.send(RoomCommand::select("h1")).await.unwrap();
    assert!(
        handle
            .snapshot()
            .await
            .unwrap()
            .participants
            .iter()
            .any(|participant| participant.id.as_str() == "h1" && participant.selected)
    );
}

#[tokio::test(start_paused = true)]
async fn disconnected_unselected_participant_expires_without_a_new_join() {
    use tokio::time;

    let mut cfg = config();
    cfg.disconnected_participant_expiry = std::time::Duration::from_secs(1);
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle.send(RoomCommand::disconnect("h")).await.unwrap();
    time::advance(std::time::Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(handle.snapshot().await.unwrap().participants.is_empty());
}

#[tokio::test(start_paused = true)]
async fn human_can_ready_again_in_post_match_for_rematch() {
    use std::time::Duration;

    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    cfg.time_control = double_riichi_core::TimeControl::Unlimited;
    cfg.replay_save = false;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle
        .send(RoomCommand::select_with_character("h", "player-red"))
        .await
        .unwrap();
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle
        .send(RoomCommand::set_ready(
            "h",
            vec!["player-red".to_owned(), "tsumogiri-bot".to_owned()],
        ))
        .await
        .unwrap();
    handle.send(RoomCommand::start()).await.unwrap();
    handle.send(RoomCommand::disconnect("h")).await.unwrap();
    tokio::time::advance(Duration::from_secs(300)).await;
    tokio::task::yield_now().await;
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::PostMatch(_)
    ));
    assert!(handle.send(RoomCommand::rematch()).await.is_err());
    handle.send(RoomCommand::reconnect("h")).await.unwrap();
    let snapshot = handle.snapshot().await.unwrap();
    assert_eq!(
        snapshot
            .participants
            .iter()
            .find(|participant| participant.id.as_str() == "h")
            .unwrap()
            .controller,
        RoomController::Interactive
    );
    assert_eq!(
        snapshot
            .match_players
            .iter()
            .find(|player| player.participant_id.as_str() == "h")
            .unwrap()
            .controller,
        RoomController::Interactive
    );
    handle
        .send(RoomCommand::set_ready(
            "h",
            vec!["player-red".to_owned(), "tsumogiri-bot".to_owned()],
        ))
        .await
        .unwrap();
    assert!(matches!(
        handle.send(RoomCommand::rematch()).await.unwrap(),
        RoomResponse::Started(_)
    ));
}

#[tokio::test(start_paused = true)]
async fn registry_purges_empty_room_after_actor_cleanup() {
    use std::time::Duration;

    let registry = RoomRegistry::with_max_rooms(1);
    let mut cfg = config();
    cfg.empty_room_cleanup = Duration::from_secs(1);
    let handle = registry.create(cfg).await.unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(registry.len().await, 0);
    assert!(handle.snapshot().await.is_err());
}

#[tokio::test]
async fn registry_shutdown_clears_rooms_and_revoke_token_reaches_room() {
    let registry = RoomRegistry::with_max_rooms(2);
    let handle = registry.create(config()).await.unwrap();
    handle
        .send(RoomCommand::join_with_token(
            Participant::new("agent", "agent", ParticipantKind::MJAI),
            "token-1",
        ))
        .await
        .unwrap();
    registry.revoke_token("token-1").await.unwrap();
    assert!(handle.snapshot().await.unwrap().participants.is_empty());
    registry.shutdown(ShutdownMode::Forced).await;
    assert_eq!(registry.len().await, 0);
}

#[tokio::test]
async fn explicit_leave_and_delete_invalidate_room_membership() {
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(config());
    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle.send(RoomCommand::leave("h")).await.unwrap();
    assert!(handle.snapshot().await.unwrap().participants.is_empty());
    handle.send(RoomCommand::Delete).await.unwrap();
    assert!(handle.snapshot().await.is_err());
}

#[tokio::test]
async fn persistence_backpressure_does_not_prevent_match_start() {
    use tokio::sync::mpsc;

    let (effects, mut receiver) = mpsc::channel(1);
    effects
        .try_send(double_riichi_core::RoomEffect::AppendEvents {
            match_id: double_riichi_core::MatchId::generate(),
            events: Vec::new(),
            failure_sender: tokio::sync::mpsc::channel(1).0,
        })
        .unwrap();
    let handle = RoomActor::spawn_with_effect_sender(config(), effects);
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    let start = tokio::spawn({
        let handle = handle.clone();
        async move { handle.send(RoomCommand::start()).await }
    });
    let _queued = receiver.recv().await.expect("queued append effect");
    let open = receiver.recv().await.expect("open effect");
    open.acknowledge(Ok(()));
    let response = start.await.unwrap().unwrap();
    assert!(matches!(response, RoomResponse::Started(_)));
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::Playing(_) | RoomPhase::PostMatch(_)
    ));
}

#[tokio::test]
async fn persistence_ack_failure_marks_replay_unavailable() {
    let (handle, mut effects) = RoomActor::spawn_with_effect_channel(config());
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    let start = tokio::spawn({
        let handle = handle.clone();
        async move { handle.send(RoomCommand::start()).await }
    });
    let effect = effects.recv().await.expect("open effect");
    effect.acknowledge(Err(double_riichi_core::RoomEffectError::Failed(
        "disk full".to_owned(),
    )));
    assert!(matches!(start.await.unwrap(), Err(RoomError::Persistence)));
    let snapshot = handle.snapshot().await.unwrap();
    assert!(snapshot.persistence_degraded);
    assert!(!snapshot.replay_available);
    assert!(matches!(snapshot.phase, RoomPhase::Lobby));
}

#[tokio::test]
async fn persistence_ack_delivery_survives_full_command_queue() {
    use double_riichi_core::ROOM_COMMAND_CAPACITY;

    let (handle, mut effects) = RoomActor::spawn_with_effect_channel(config());
    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle
        .send(RoomCommand::select_with_character("h", "player-red"))
        .await
        .unwrap();
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle
        .send(RoomCommand::set_ready(
            "h",
            vec!["player-red".to_owned(), "tsumogiri-bot".to_owned()],
        ))
        .await
        .unwrap();
    let start = tokio::spawn({
        let handle = handle.clone();
        async move { handle.send(RoomCommand::start()).await }
    });
    let effect = effects.recv().await.expect("open effect");
    let mut pending = Vec::new();
    for _ in 0..ROOM_COMMAND_CAPACITY {
        pending.push(handle.try_send(RoomCommand::GetSnapshot).unwrap());
    }
    effect.acknowledge(Err(double_riichi_core::RoomEffectError::Failed(
        "queue pressure".to_owned(),
    )));
    let response = start.await.unwrap();
    assert!(matches!(response, Err(RoomError::Persistence)));
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::Lobby
    ));
    for task in pending {
        let _ = task.await;
    }
    for _ in 0..8 {
        if handle.snapshot().await.unwrap().persistence_degraded {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("persistence acknowledgement was lost under command pressure");
}

#[tokio::test]
async fn slow_connection_is_closed_without_blocking_room_commands() {
    use tokio::time::{self, Duration};

    let (handle, _effects) = RoomActor::spawn_with_effect_channel(config());
    let mut connection = handle.subscribe().await.unwrap();
    assert!(matches!(
        connection.recv().await,
        Some(RoomEvent::Snapshot(_))
    ));
    for index in 0..(double_riichi_core::CONNECTION_OUTBOUND_CAPACITY + 2) {
        let _ = handle
            .send(RoomCommand::set_room_name(format!("room-{index}")))
            .await;
    }
    time::timeout(Duration::from_secs(1), async {
        while connection.recv().await.is_some() {}
    })
    .await
    .expect("slow connection should be disconnected");
}

#[tokio::test]
async fn built_in_match_reaches_post_match_and_can_rematch() {
    let mut cfg = config();
    cfg.replay_save = false;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    let first = match handle.send(RoomCommand::start()).await.unwrap() {
        RoomResponse::Started(match_id) => match_id,
        other => panic!("unexpected response: {other:?}"),
    };
    let phase = handle.snapshot().await.unwrap().phase;
    assert!(matches!(phase, RoomPhase::PostMatch(_)));
    let second = match handle.send(RoomCommand::rematch()).await.unwrap() {
        RoomResponse::Started(match_id) => match_id,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_ne!(first, second);
}

#[tokio::test]
async fn room_history_bounds_current_events_and_prior_kyoku_summaries() {
    let mut cfg = config();
    cfg.mode = GameMode::FourPlayerRedHalf;
    cfg.replay_save = false;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle.send(RoomCommand::start()).await.unwrap();

    let snapshot = handle.snapshot().await.unwrap();
    assert!(matches!(snapshot.phase, RoomPhase::PostMatch(_)));
    let history = handle.public_history_projection().await.unwrap();
    let current = history.current_kyoku.as_ref().expect("final kyoku details");
    assert!(history.previous_kyoku.len() >= 1, "expected multiple kyoku");
    assert!(!current.events.is_empty());
    assert!(
        current.events.len() < 1_000,
        "current kyoku grew without a bound"
    );
    assert!(
        history
            .previous_kyoku
            .iter()
            .all(|summary| { !summary.results.is_empty() })
    );
    let value = serde_json::to_value(history).unwrap();
    let encoded = value.to_string();
    for concealed in ["tehais", "hands", "wall", "private_state", "raw_state"] {
        assert!(
            !encoded.contains(concealed),
            "concealed field leaked: {concealed}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn room_history_keeps_player_visibility_and_survives_match_machine_clear() {
    let mut cfg = config();
    cfg.replay_save = false;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle.send(RoomCommand::select("h")).await.unwrap();
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle
        .send(RoomCommand::set_ready(
            "h",
            vec![
                "player-red".to_owned(),
                "player-blue".to_owned(),
                "tsumogiri-bot".to_owned(),
            ],
        ))
        .await
        .unwrap();
    handle.send(RoomCommand::start()).await.unwrap();

    let private = serde_json::to_value(handle.history_projection("h").await.unwrap()).unwrap();
    let public = serde_json::to_value(handle.public_history_projection().await.unwrap()).unwrap();
    assert!(private.to_string().contains("own_tehai"));
    assert!(!public.to_string().contains("own_tehai"));
    for concealed in ["tehais", "hands", "wall", "private_state", "raw_state"] {
        assert!(!private.to_string().contains(concealed));
        assert!(!public.to_string().contains(concealed));
    }

    handle.send(RoomCommand::disconnect("h")).await.unwrap();
    tokio::time::advance(std::time::Duration::from_secs(300)).await;
    tokio::task::yield_now().await;
    let snapshot = handle.snapshot().await.unwrap();
    assert!(matches!(snapshot.phase, RoomPhase::PostMatch(_)));
    let retained = handle.public_history_projection().await.unwrap();
    assert!(retained.current_kyoku.is_some());
}

#[tokio::test]
async fn rematch_open_failure_preserves_post_match_without_starting_a_new_match() {
    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    cfg.time_control = TimeControl::Unlimited;
    let (effects, mut receiver) =
        tokio::sync::mpsc::channel(double_riichi_core::ROOM_EFFECT_CAPACITY);
    let handle = RoomActor::spawn_with_effect_sender(cfg, effects);
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    let worker = tokio::spawn(async move {
        let mut opens = 0;
        while let Some(effect) = receiver.recv().await {
            let is_open = matches!(&effect, RoomEffect::OpenMatch { .. });
            let is_ack = matches!(
                &effect,
                RoomEffect::OpenMatch { .. }
                    | RoomEffect::FlushKyoku { .. }
                    | RoomEffect::FinalizeMatch { .. }
            );
            if is_open {
                opens += 1;
                if opens == 2 {
                    effect.acknowledge(Err(double_riichi_core::RoomEffectError::Failed(
                        "second open failed".to_owned(),
                    )));
                    continue;
                }
            }
            if is_ack {
                effect.acknowledge(Ok(()));
            }
        }
    });
    assert!(matches!(
        handle.send(RoomCommand::start()).await,
        Ok(RoomResponse::Started(_))
    ));
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::PostMatch(_)
    ));
    let before = handle.snapshot().await.unwrap();
    assert!(matches!(
        handle.send(RoomCommand::rematch()).await,
        Err(RoomError::Persistence)
    ));
    let after = handle.snapshot().await.unwrap();
    assert!(matches!(after.phase, RoomPhase::PostMatch(_)));
    assert_eq!(after.phase, before.phase);
    assert!(after.persistence_degraded);
    assert!(!after.replay_available);
    worker.abort();
}

#[tokio::test]
async fn kick_is_distinct_from_voluntary_leave_and_records_the_kicked_event() {
    let mut cfg = config();
    cfg.replay_save = true;
    cfg.time_control = TimeControl::Unlimited;
    let (handle, mut effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle
        .send(RoomCommand::join(human("h", "player-red")))
        .await
        .unwrap();
    handle.send(RoomCommand::select("h")).await.unwrap();
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle
        .send(RoomCommand::set_ready(
            "h",
            vec![
                "player-red".to_owned(),
                "player-blue".to_owned(),
                "tsumogiri-bot".to_owned(),
            ],
        ))
        .await
        .unwrap();
    let start = tokio::spawn({
        let handle = handle.clone();
        async move { handle.send(RoomCommand::start()).await }
    });
    let open = effects.recv().await.unwrap();
    let match_id = match &open {
        RoomEffect::OpenMatch { match_id, .. } => match_id.clone(),
        _ => panic!("expected open effect"),
    };
    open.acknowledge(Ok(()));
    assert!(matches!(start.await.unwrap(), Ok(RoomResponse::Started(_))));
    handle.send(RoomCommand::kick("h")).await.unwrap();
    let participant = handle
        .snapshot()
        .await
        .unwrap()
        .participants
        .into_iter()
        .find(|participant| participant.id.as_str() == "h")
        .unwrap();
    assert_eq!(
        participant.controller,
        RoomController::PermanentAuto(double_riichi_core::PermanentAutoReason::Kicked)
    );
    let mut saw_kicked = false;
    for _ in 0..32 {
        let Some(effect) = effects.recv().await else {
            break;
        };
        match &effect {
            RoomEffect::RecordAuxiliary {
                match_id: effect_match_id,
                event: RoomAuxiliaryEvent::Kicked { .. },
                ..
            } if *effect_match_id == match_id => saw_kicked = true,
            _ => {}
        }
        effect.acknowledge(Ok(()));
        if saw_kicked {
            break;
        }
    }
    assert!(saw_kicked);
}

#[tokio::test]
async fn append_and_auxiliary_backpressure_degrades_only_the_owning_room() {
    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    cfg.time_control = TimeControl::Unlimited;
    let (effects_a, mut receiver_a) =
        tokio::sync::mpsc::channel(double_riichi_core::ROOM_EFFECT_CAPACITY);
    let filler_a = effects_a.clone();
    let room_a = RoomActor::spawn_with_effect_sender(cfg.clone(), effects_a);
    room_a
        .send(RoomCommand::join(human("owner", "player-red")))
        .await
        .unwrap();
    room_a.send(RoomCommand::select("owner")).await.unwrap();
    room_a.send(RoomCommand::fill_with_bots()).await.unwrap();
    room_a
        .send(RoomCommand::set_ready(
            "owner",
            vec![
                "player-red".to_owned(),
                "player-blue".to_owned(),
                "tsumogiri-bot".to_owned(),
            ],
        ))
        .await
        .unwrap();
    let start_a = tokio::spawn({
        let room_a = room_a.clone();
        async move { room_a.send(RoomCommand::start()).await }
    });
    let open_a = receiver_a.recv().await.unwrap();
    for _ in 0..double_riichi_core::ROOM_EFFECT_CAPACITY {
        filler_a
            .try_send(RoomEffect::AppendEvents {
                match_id: double_riichi_core::MatchId::generate(),
                events: Vec::new(),
                failure_sender: tokio::sync::mpsc::channel(1).0,
            })
            .unwrap();
    }
    open_a.acknowledge(Ok(()));
    assert!(matches!(
        start_a.await.unwrap(),
        Ok(RoomResponse::Started(_))
    ));
    room_a.send(RoomCommand::kick("owner")).await.unwrap();
    let snapshot_a = room_a.snapshot().await.unwrap();
    assert!(snapshot_a.persistence_degraded);
    assert!(!snapshot_a.replay_available);

    let (effects_b, mut receiver_b) =
        tokio::sync::mpsc::channel(double_riichi_core::ROOM_EFFECT_CAPACITY);
    let room_b = RoomActor::spawn_with_effect_sender(cfg, effects_b);
    room_b.send(RoomCommand::fill_with_bots()).await.unwrap();
    let worker_b = tokio::spawn(async move {
        while let Some(effect) = receiver_b.recv().await {
            if matches!(
                &effect,
                RoomEffect::OpenMatch { .. }
                    | RoomEffect::FlushKyoku { .. }
                    | RoomEffect::FinalizeMatch { .. }
            ) {
                effect.acknowledge(Ok(()));
            }
        }
    });
    assert!(matches!(
        room_b.send(RoomCommand::start()).await,
        Ok(RoomResponse::Started(_))
    ));
    let snapshot_b = room_b.snapshot().await.unwrap();
    assert!(!snapshot_b.persistence_degraded);
    assert!(snapshot_b.replay_available);
    drop(filler_a);
    drop(receiver_a);
    worker_b.abort();
}

#[tokio::test]
async fn append_backpressure_fails_replay_but_allows_match_progress() {
    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    cfg.time_control = TimeControl::Unlimited;
    let (effects, mut receiver) =
        tokio::sync::mpsc::channel(double_riichi_core::ROOM_EFFECT_CAPACITY);
    let filler = effects.clone();
    let room = RoomActor::spawn_with_effect_sender(cfg, effects);
    room.send(RoomCommand::fill_with_bots()).await.unwrap();
    let start = tokio::spawn({
        let room = room.clone();
        async move { room.send(RoomCommand::start()).await }
    });
    let open = receiver.recv().await.unwrap();
    for _ in 0..double_riichi_core::ROOM_EFFECT_CAPACITY {
        filler
            .try_send(RoomEffect::AppendEvents {
                match_id: double_riichi_core::MatchId::generate(),
                events: Vec::new(),
                failure_sender: tokio::sync::mpsc::channel(1).0,
            })
            .unwrap();
    }
    open.acknowledge(Ok(()));
    let response = tokio::time::timeout(std::time::Duration::from_secs(3), start)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(response, RoomResponse::Started(_)));
    let snapshot = room.snapshot().await.unwrap();
    assert!(snapshot.persistence_degraded);
    assert!(!snapshot.replay_available);
    assert!(matches!(snapshot.phase, RoomPhase::PostMatch(_)));
    drop(filler);
    drop(receiver);
}

#[tokio::test]
async fn delayed_finalize_is_cancelled_before_a_late_worker_can_commit() {
    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    cfg.time_control = TimeControl::Unlimited;
    let (effects, mut receiver) =
        tokio::sync::mpsc::channel(double_riichi_core::ROOM_EFFECT_CAPACITY);
    let room = RoomActor::spawn_with_effect_sender(cfg, effects);
    room.send(RoomCommand::fill_with_bots()).await.unwrap();
    let (cancelled, cancelled_seen) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let mut cancelled = Some(cancelled);
        while let Some(effect) = receiver.recv().await {
            match &effect {
                RoomEffect::OpenMatch { .. } | RoomEffect::FlushKyoku { .. } => {
                    effect.acknowledge(Ok(()));
                }
                RoomEffect::FinalizeMatch { control, .. } => {
                    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
                    assert!(control.is_cancelled());
                    if let Some(sender) = cancelled.take() {
                        let _ = sender.send(());
                    }
                    effect.acknowledge(Err(double_riichi_core::RoomEffectError::Failed(
                        "finalize cancelled".to_owned(),
                    )));
                    break;
                }
                _ => {}
            }
        }
    });
    let start = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        room.send(RoomCommand::start()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(start, RoomResponse::Started(_)));
    cancelled_seen.await.unwrap();
    let snapshot = room.snapshot().await.unwrap();
    assert!(matches!(snapshot.phase, RoomPhase::PostMatch(_)));
    assert!(snapshot.persistence_degraded);
    assert!(!snapshot.replay_available);
    worker.await.unwrap();
}
