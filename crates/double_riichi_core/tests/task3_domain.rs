use std::str::FromStr;

use double_riichi_core::{
    GameAction, GameEvent, GameMode, Participant, ParticipantKind, Seat, Tile,
};

#[test]
fn modes_have_only_the_four_fixed_red_presets() {
    let presets = ["4p-red-east", "4p-red-half", "3p-red-east", "3p-red-half"];

    for preset in presets {
        let mode = GameMode::from_str(preset).expect("fixed preset parses");
        assert_eq!(mode.as_str(), preset);
        assert_eq!(
            mode.seat_count(),
            if preset.starts_with('4') { 4 } else { 3 }
        );
    }
    assert!(GameMode::from_str("4p-east").is_err());
}

#[test]
fn canonical_tiles_keep_red_fives_distinct_and_have_no_dummy_three_player_seat() {
    let four_player = Tile::canonical_order(GameMode::FourPlayerRedEast);
    assert_eq!(four_player.len(), 136);
    assert!(four_player.contains(&Tile::RED_FIVE_MAN));
    assert!(four_player.contains(&Tile::RED_FIVE_PIN));
    assert!(four_player.contains(&Tile::RED_FIVE_SOU));
    assert!(Tile::RED_FIVE_MAN.is_red());
    assert_eq!(Tile::RED_FIVE_MAN.tile_type(), Tile::FIVE_MAN);

    let three_player = Tile::canonical_order(GameMode::ThreePlayerRedEast);
    assert_eq!(three_player.len(), 108);
    assert!(
        three_player
            .iter()
            .all(|tile| tile.tile_type() != Tile::TWO_MAN)
    );
    assert_eq!(Seat::all(GameMode::ThreePlayerRedEast).len(), 3);
}

#[test]
fn participants_and_actions_are_protocol_neutral() {
    let participant = Participant::new("p0", "Alice", ParticipantKind::Human);
    assert_eq!(participant.id.as_str(), "p0");
    assert_eq!(participant.display_name, "Alice");

    let seat = Seat::new(1).unwrap();
    let tile = Tile::from_id(8).unwrap();
    let consumed = vec![Tile::from_id(0).unwrap(), Tile::from_id(4).unwrap()];
    let actions = vec![
        GameAction::Discard {
            tile,
            tsumogiri: false,
        },
        GameAction::RiichiDiscard { tile },
        GameAction::Chi {
            target: seat,
            called: tile,
            consumed: consumed.clone(),
        },
        GameAction::Pon {
            target: seat,
            called: tile,
            consumed: consumed.clone(),
        },
        GameAction::Daiminkan {
            target: seat,
            called: tile,
            consumed: consumed.clone(),
        },
        GameAction::Ankan {
            consumed: consumed.clone(),
        },
        GameAction::Kakan {
            called: tile,
            consumed,
        },
        GameAction::Nuki { tile },
        GameAction::Tsumo,
        GameAction::Ron(seat),
        GameAction::Pass,
        GameAction::AbortiveDraw,
    ];
    assert_eq!(actions[2].target_seat(), Some(seat));
    assert_eq!(actions[9].target_seat(), Some(seat));

    let _events: Vec<GameEvent> = vec![GameEvent::EndGame];
}
