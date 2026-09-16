use double_riichi_core::room::{
    CharacterCatalog, CharacterUsage, RoomActor, RoomCommand, RoomConfig, RoomEvent, RoomPhase,
    RoomResponse,
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

#[test]
fn character_catalog_keeps_usage_metadata_for_selection_validation() {
    let catalog = CharacterCatalog::starter();
    assert_eq!(catalog.usage("player-red"), Some(CharacterUsage::Human));
    assert_eq!(catalog.usage("mjai-bot"), Some(CharacterUsage::Mjai));
    assert_eq!(TimeControl::Casual.turn_duration().as_secs(), 30);
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
