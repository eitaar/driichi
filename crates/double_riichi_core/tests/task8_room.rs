use double_riichi_core::room::{
    CharacterCatalog, CharacterUsage, RoomActor, RoomCommand, RoomConfig, RoomController,
    RoomEvent, RoomPhase, RoomRegistry, RoomResponse, ShutdownMode,
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

    let response = handle.send(RoomCommand::start()).await.unwrap();
    assert!(matches!(response, RoomResponse::Started(_)));
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::Playing(_)
    ));

    let _ = effects.recv().await.expect("open effect");
    assert!(matches!(events.recv().await, Some(RoomEvent::Snapshot(_))));
}

#[tokio::test]
async fn three_player_mode_rejects_mjai_selection_and_mode_change_clears_selection() {
    let mut cfg = config();
    cfg.mode = GameMode::ThreePlayerRedEast;
    let (handle, _effects) = RoomActor::spawn_with_effect_channel(cfg);
    handle
        .send(RoomCommand::join(Participant::new(
            "bot",
            "bot",
            ParticipantKind::MJAI,
        )))
        .await
        .unwrap();
    assert!(handle.send(RoomCommand::select("bot")).await.is_err());

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
        })
        .unwrap();
    let handle = RoomActor::spawn_with_effect_sender(config(), effects);
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    let response = handle.send(RoomCommand::start()).await.unwrap();
    assert!(matches!(response, RoomResponse::Started(_)));
    assert!(matches!(
        handle.snapshot().await.unwrap().phase,
        RoomPhase::Playing(_) | RoomPhase::PostMatch(_)
    ));
    let _ = receiver.try_recv();
}

#[tokio::test]
async fn persistence_ack_failure_marks_replay_unavailable() {
    let (handle, mut effects) = RoomActor::spawn_with_effect_channel(config());
    handle.send(RoomCommand::fill_with_bots()).await.unwrap();
    handle.send(RoomCommand::start()).await.unwrap();
    let effect = effects.recv().await.expect("open effect");
    effect.acknowledge(Err(double_riichi_core::RoomEffectError::Failed(
        "disk full".to_owned(),
    )));
    tokio::task::yield_now().await;
    let snapshot = handle.snapshot().await.unwrap();
    assert!(snapshot.persistence_degraded);
    assert!(!snapshot.replay_available);
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
    let response = start.await.unwrap().unwrap();
    assert!(matches!(response, RoomResponse::Started(_)));
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
