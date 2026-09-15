use std::time::Duration;

use double_riichi_core::{
    Audience, Decision, DecisionId, DecisionKind, GameAction, GameMode, MatchMachine, MeldState,
    Participant, ParticipantKind, Seat, TablePlayerState, TableState, Tile, project_table_state,
    serialize_projection,
};
use tokio::time::Instant;

fn participant(index: u8) -> Participant {
    Participant::new(
        format!("p{index}"),
        format!("Player {index}"),
        ParticipantKind::Human,
    )
}

fn state(mode: GameMode) -> TableState {
    let hands = (0..mode.seat_count())
        .map(|seat| vec![Tile::from_id(seat as u8).unwrap()])
        .collect();
    let players = (0..mode.seat_count())
        .map(|seat| {
            (
                Seat::new(seat as u8).unwrap(),
                participant(seat as u8),
                25_000,
            )
        })
        .collect();
    let decision = Decision::new(
        DecisionId::new("d1"),
        DecisionKind::Turn,
        vec![(
            Seat::new(0).unwrap(),
            vec![GameAction::Discard {
                tile: Tile::from_id(42).unwrap(),
                tsumogiri: false,
            }],
        )],
        Instant::now(),
        Some(Duration::from_secs(30)),
        false,
    )
    .unwrap();
    TableState::from_hands(mode, players, hands)
        .unwrap()
        .with_decision(decision)
}

#[test]
fn player_public_and_replay_admin_json_have_distinct_visibility_for_four_players() {
    let state = state(GameMode::FourPlayerRedEast);
    let player = serialize_projection(&project_table_state(
        &state,
        Audience::Player(Seat::new(0).unwrap()),
    ))
    .unwrap();
    let public = serialize_projection(&project_table_state(&state, Audience::Public)).unwrap();
    let replay = serialize_projection(&project_table_state(&state, Audience::ReplayAdmin)).unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player).unwrap()["players"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(player.contains("42"));
    assert!(player.contains("actions"));
    assert!(!public.contains("\"hand\""));
    assert!(!public.contains("42"));
    assert!(!public.contains("actions"));
    assert!(replay.contains("42"));
    assert!(replay.contains("actions"));
}

fn state_with_closed_meld(mode: GameMode) -> TableState {
    let players = (0..mode.seat_count())
        .map(|seat| {
            let mut player = TablePlayerState::new(
                Seat::new(seat as u8).unwrap(),
                participant(seat as u8),
                25_000,
                vec![Tile::from_id(seat as u8).unwrap()],
            );
            if seat == 0 {
                player.melds.push(MeldState {
                    tiles: vec![
                        Tile::from_id(100).unwrap(),
                        Tile::from_id(101).unwrap(),
                        Tile::from_id(102).unwrap(),
                        Tile::from_id(103).unwrap(),
                    ],
                    opened: false,
                    from_who: None,
                    called_tile: None,
                });
            }
            player
        })
        .collect();
    TableState::new(mode, players, Vec::new()).unwrap()
}

#[test]
fn closed_meld_tiles_are_redacted_from_public_and_opponent_players_in_both_modes() {
    for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
        let state = state_with_closed_meld(mode);
        let public = serde_json::from_str::<serde_json::Value>(
            &serialize_projection(&project_table_state(&state, Audience::Public)).unwrap(),
        )
        .unwrap();
        let opponent = serde_json::from_str::<serde_json::Value>(
            &serialize_projection(&project_table_state(
                &state,
                Audience::Player(Seat::new(1).unwrap()),
            ))
            .unwrap(),
        )
        .unwrap();
        let owner = serde_json::from_str::<serde_json::Value>(
            &serialize_projection(&project_table_state(
                &state,
                Audience::Player(Seat::new(0).unwrap()),
            ))
            .unwrap(),
        )
        .unwrap();
        let replay = serde_json::from_str::<serde_json::Value>(
            &serialize_projection(&project_table_state(&state, Audience::ReplayAdmin)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            public["players"][0]["melds"][0]["tiles"],
            serde_json::json!([])
        );
        assert_eq!(
            opponent["players"][0]["melds"][0]["tiles"],
            serde_json::json!([])
        );
        assert_eq!(
            owner["players"][0]["melds"][0]["tiles"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            replay["players"][0]["melds"][0]["tiles"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
}

#[test]
fn match_machine_projection_uses_the_same_boundary_for_three_and_four_players() {
    for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
        let participants = (0..mode.seat_count())
            .map(|seat| participant(seat as u8))
            .collect();
        let mut machine = MatchMachine::new(mode, participants).unwrap();
        let public = machine.serialize(Audience::Public).unwrap();
        let replay = machine.serialize(Audience::ReplayAdmin).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&public).unwrap()["players"]
                .as_array()
                .unwrap()
                .len(),
            mode.seat_count()
        );
        assert!(!public.contains("\"hand\""));
        assert!(replay.contains("\"hand\""));
    }
}

#[test]
fn three_player_projection_has_no_dummy_fourth_player_or_private_choices() {
    let state = state(GameMode::ThreePlayerRedEast);
    let player = serde_json::from_str::<serde_json::Value>(
        &serialize_projection(&project_table_state(
            &state,
            Audience::Player(Seat::new(0).unwrap()),
        ))
        .unwrap(),
    )
    .unwrap();
    let public = serialize_projection(&project_table_state(&state, Audience::Public)).unwrap();
    let replay = serialize_projection(&project_table_state(&state, Audience::ReplayAdmin)).unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&public).unwrap()["players"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(!public.contains("p3"));
    assert!(!public.contains("actions"));
    assert!(player["players"][0]["hand"].is_array());
    assert!(player["decision"]["actions"].is_array());
    assert!(player["players"][1]["hand"].is_null());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&replay).unwrap()["players"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(replay.contains("actions"));
}
