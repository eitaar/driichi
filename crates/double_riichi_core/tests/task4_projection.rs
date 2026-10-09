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
fn declaration_index_is_public_optional_and_never_reveals_concealed_hands() {
    for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
        let mut players = (0..mode.seat_count())
            .map(|index| {
                TablePlayerState::new(
                    Seat::new(index as u8).unwrap(),
                    participant(index as u8),
                    25_000,
                    vec![Tile::from_id(120).unwrap()],
                )
            })
            .collect::<Vec<_>>();
        players[0].discards = vec![Tile::from_id(0).unwrap(), Tile::from_id(36).unwrap()];
        players[0].riichi = true;
        players[0].riichi_discard_index = Some(0);
        // An out-of-range field is unknown, not permission to rotate the latest discard.
        players[1].riichi_discard_index = Some(0);
        let state = TableState::new(mode, players, vec![]).unwrap();
        for audience in [
            Audience::Public,
            Audience::Player(Seat::new(1).unwrap()),
            Audience::ReplayAdmin,
            Audience::BenchmarkAdmin,
        ] {
            let json: serde_json::Value = serde_json::from_str(
                &serialize_projection(&project_table_state(&state, audience)).unwrap(),
            )
            .unwrap();
            assert_eq!(json["players"].as_array().unwrap().len(), mode.seat_count());
            assert_eq!(json["players"][0]["riichi_discard_index"], 0);
            assert!(json["players"][1].get("riichi_discard_index").is_none());
            for (seat, player) in json["players"].as_array().unwrap().iter().enumerate() {
                let hand_visible = audience == Audience::ReplayAdmin
                    || audience == Audience::BenchmarkAdmin
                    || audience == Audience::Player(Seat::new(seat as u8).unwrap());
                assert_eq!(player.get("hand").is_some(), hand_visible);
            }
        }
    }
}

#[test]
fn held_kyoku_projection_serializes_only_published_results_without_next_decision() {
    let mode = GameMode::FourPlayerRedEast;
    let mut machine =
        MatchMachine::with_seed(mode, (0..4).map(participant).collect(), 0xD0).unwrap();
    for _ in 0..2000 {
        if machine.held_kyoku().is_some() {
            break;
        }
        let (seat, action) = Seat::all(mode)
            .into_iter()
            .find_map(|seat| {
                let actions = machine.legal_actions(seat).unwrap();
                let chosen = actions
                    .iter()
                    .find(|action| matches!(action, GameAction::Discard { .. } | GameAction::Pass))
                    .or_else(|| actions.first())?;
                Some((seat, chosen.clone()))
            })
            .unwrap();
        machine.apply(seat, action).unwrap();
    }
    let wins = machine.held_kyoku().expect("ended Kyoku").wins.len();
    let before = machine.serialize(Audience::Public).unwrap();
    assert!(!before.contains("kyoku_results"));
    assert!(!before.contains("\"hand\""));
    assert!(machine.current_decision().unwrap().is_none());
    machine.publish_kyoku_step(0).unwrap();
    for audience in [
        Audience::Public,
        Audience::Player(Seat::new(0).unwrap()),
        Audience::ReplayAdmin,
        Audience::BenchmarkAdmin,
    ] {
        let json: serde_json::Value =
            serde_json::from_str(&machine.serialize(audience).unwrap()).unwrap();
        assert!(json["decision"].is_null());
        assert_eq!(
            json["kyoku_results"]["wins"].as_array().unwrap().len(),
            usize::from(wins > 0)
        );
        assert_eq!(json["kyoku_results"]["settlement"].is_object(), wins == 0);
        if matches!(audience, Audience::Public) {
            assert!(
                json["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| p.get("hand").is_none())
            );
        }
    }
    assert_eq!(
        machine
            .events()
            .iter()
            .filter(|event| matches!(event, double_riichi_core::GameEvent::StartKyoku { .. }))
            .count(),
        1
    );
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
fn match_machine_projection_includes_authoritative_round_and_wall_data_for_all_audiences() {
    for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
        let participants = (0..mode.seat_count())
            .map(|seat| participant(seat as u8))
            .collect();
        let mut machine = MatchMachine::new(mode, participants).expect("match");
        for audience in [
            Audience::Player(Seat::new(0).unwrap()),
            Audience::Public,
            Audience::ReplayAdmin,
        ] {
            let value =
                serde_json::from_str::<serde_json::Value>(&machine.serialize(audience).unwrap())
                    .unwrap();
            assert_eq!(value["round"], "East");
            assert_eq!(value["kyoku"], 1);
            assert_eq!(value["dealer"], 0);
            assert_eq!(value["honba"], 0);
            assert_eq!(value["kyotaku"], 0);
            assert!(value["remaining_wall"].as_u64().is_some());
        }
    }
}

#[test]
fn frontend_task12_fixture_is_checked_against_the_real_audience_projection() {
    let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../frontend/tests/fixtures/task12-projection.json"
    ))
    .unwrap();
    for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
        let order: &[(&str, &str)] = if mode.is_three_player() {
            &[("P2", "Nori"), ("P1", "Mika"), ("P3", "Ren")]
        } else {
            &[("P2", "Nori"), ("P1", "Mika"), ("P4", "Aya"), ("P3", "Ren")]
        };
        let participants = order
            .iter()
            .map(|(id, name)| Participant::new(*id, *name, ParticipantKind::Human))
            .collect();
        let mut machine = MatchMachine::with_seed(mode, participants, 42).expect("match");
        let serialized = machine
            .serialize(Audience::Player(Seat::new(0).unwrap()))
            .unwrap();
        let mut actual = serde_json::from_str::<serde_json::Value>(&serialized).unwrap();
        let expected = &fixture["projections"][mode.as_str()];
        for field in [
            "audience",
            "viewer_seat",
            "mode",
            "round",
            "kyoku",
            "dealer",
            "honba",
            "kyotaku",
            "remaining_wall",
            "players",
            "dora_indicators",
            "decision",
        ] {
            assert!(actual.get(field).is_some(), "real projection lacks {field}");
            assert!(expected.get(field).is_some(), "fixture lacks {field}");
        }
        assert_eq!(actual["audience"], expected["audience"]);
        assert_eq!(actual["mode"], expected["mode"]);
        assert_eq!(actual["round"], expected["round"]);
        assert_eq!(actual["kyoku"], expected["kyoku"]);
        assert_eq!(actual["dealer"], expected["dealer"]);
        assert_eq!(actual["honba"], expected["honba"]);
        assert_eq!(actual["kyotaku"], expected["kyotaku"]);
        assert!(actual["remaining_wall"].as_u64().is_some());
        assert!(expected["remaining_wall"].as_u64().is_some());
        assert_eq!(
            actual["players"].as_array().unwrap().len(),
            mode.seat_count()
        );
        assert_eq!(
            expected["players"].as_array().unwrap().len(),
            mode.seat_count()
        );
        // A live deadline is the only volatile value; every nested wire field is exact.
        actual["decision"]["remaining_ms"] = expected["decision"]["remaining_ms"].clone();
        assert_eq!(actual, *expected);
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
