use std::{fs, path::PathBuf, time::UNIX_EPOCH};

use double_riichi_core::{
    Audience, GameEvent, GameMode, Participant, ParticipantKind, Seat, TableState, Tile, Wind,
};
use double_riichi_replay::{
    AuxiliaryEvent, AuxiliaryPhase, CanonicalEvent, MAX_REPLAY_FRAME_BYTES, ReplayError,
    ReplayFrame, ReplayWriter, build_replay_frames, build_replay_frames_for_mode,
    encode_replay_frames, frames_from_artifact, parse_mjson, resolve_replay_path, serialize_event,
    startup_cleanup,
};

fn three_player_start_events() -> Vec<CanonicalEvent> {
    vec![
        GameEvent::StartGame {
            names: Some(vec!["East".into(), "South".into(), "West".into()]),
            id: Some("match-3p".into()),
        },
        GameEvent::StartKyoku {
            bakaze: Wind::East,
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: Seat::new(0).unwrap(),
            scores: vec![25_000; 3],
            dora_marker: Tile::from_id(0).unwrap(),
            tehais: vec![
                {
                    let mut hand = vec![Tile::from_id(0).unwrap(); 12];
                    hand.push(Tile::from_id(120).unwrap());
                    hand
                },
                vec![Tile::from_id(0).unwrap(); 13],
                vec![Tile::from_id(0).unwrap(); 13],
            ],
        },
    ]
}

fn default_action(actions: &[double_riichi_core::GameAction]) -> double_riichi_core::GameAction {
    actions
        .iter()
        .find(|action| matches!(action, double_riichi_core::GameAction::Discard { .. }))
        .or_else(|| {
            actions
                .iter()
                .find(|action| matches!(action, double_riichi_core::GameAction::Pass))
        })
        .or_else(|| actions.first())
        .expect("running engine exposes an action")
        .clone()
}

fn temp_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("double-riichi-task5-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn start_events() -> Vec<CanonicalEvent> {
    vec![
        GameEvent::StartGame {
            names: Some(vec![
                "East".into(),
                "South".into(),
                "West".into(),
                "North".into(),
            ]),
            id: Some("match-1".into()),
        },
        GameEvent::StartKyoku {
            bakaze: Wind::East,
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: Seat::new(0).unwrap(),
            scores: vec![25_000; 4],
            dora_marker: Tile::from_id(0).unwrap(),
            tehais: vec![vec![Tile::from_id(0).unwrap(); 13]; 4],
        },
    ]
}

#[test]
fn canonical_mjson_is_ordered_json_lines_and_round_trips_calls_kans_riichi_scores_and_draws() {
    let mut events = start_events();
    events.extend([
        GameEvent::Tsumo {
            actor: Seat::new(0).unwrap(),
            tile: Tile::from_id(1).unwrap(),
        },
        GameEvent::Dahai {
            actor: Seat::new(0).unwrap(),
            tile: Tile::from_id(1).unwrap(),
            tsumogiri: true,
        },
        GameEvent::Chi {
            actor: Seat::new(1).unwrap(),
            target: Seat::new(0).unwrap(),
            called: Tile::from_id(1).unwrap(),
            consumed: vec![Tile::from_id(2).unwrap(), Tile::from_id(3).unwrap()],
        },
        GameEvent::Pon {
            actor: Seat::new(2).unwrap(),
            target: Seat::new(1).unwrap(),
            called: Tile::from_id(4).unwrap(),
            consumed: vec![Tile::from_id(5).unwrap(), Tile::from_id(6).unwrap()],
        },
        GameEvent::Daiminkan {
            actor: Seat::new(3).unwrap(),
            target: Seat::new(2).unwrap(),
            called: Tile::from_id(7).unwrap(),
            consumed: vec![
                Tile::from_id(8).unwrap(),
                Tile::from_id(9).unwrap(),
                Tile::from_id(10).unwrap(),
            ],
        },
        GameEvent::Ankan {
            actor: Seat::new(0).unwrap(),
            consumed: vec![Tile::from_id(11).unwrap(); 4],
        },
        GameEvent::Kakan {
            actor: Seat::new(1).unwrap(),
            called: Tile::from_id(12).unwrap(),
        },
        GameEvent::Dora {
            dora_marker: Tile::from_id(13).unwrap(),
        },
        GameEvent::Reach {
            actor: Seat::new(0).unwrap(),
        },
        GameEvent::ReachAccepted {
            actor: Seat::new(0).unwrap(),
        },
        GameEvent::Hora {
            actor: Seat::new(1).unwrap(),
            target: Seat::new(0).unwrap(),
            tile: Some(Tile::from_id(14).unwrap()),
            ura_markers: Some(vec![Tile::from_id(15).unwrap()]),
            yaku: Some(vec![("riichi".into(), 1)]),
            fu: Some(30),
            han: Some(3),
            scores: Some(vec![24_000, 26_000, 25_000, 25_000]),
            delta: Some(vec![-1_000, 1_000, 0, 0]),
        },
        GameEvent::Hora {
            actor: Seat::new(2).unwrap(),
            target: Seat::new(0).unwrap(),
            tile: Some(Tile::from_id(14).unwrap()),
            ura_markers: None,
            yaku: None,
            fu: None,
            han: None,
            scores: None,
            delta: None,
        },
        GameEvent::Ryukyoku {
            reason: Some("fanpai".into()),
            tehais: None,
            delta: Some(vec![0, 0, 0, 0]),
            scores: Some(vec![24_000, 26_000, 25_000, 25_000]),
        },
        GameEvent::EndKyoku,
        GameEvent::EndGame,
    ]);

    let text = events
        .iter()
        .map(serialize_event)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
        + "\n";
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), events.len());
    assert!(lines[0].starts_with(r#"{"type":"start_game""#));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["type"],
        "start_game"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[2]).unwrap()["type"],
        "tsumo"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[4]).unwrap()["type"],
        "chi"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[5]).unwrap()["type"],
        "pon"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[9]).unwrap()["type"],
        "dora"
    );
    let parsed = parse_mjson(&text).unwrap();
    let canonical_text = parsed
        .iter()
        .map(serialize_event)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
        + "\n";
    assert_eq!(canonical_text, text);
}

#[test]
fn registered_relative_paths_are_revalidated_below_the_replay_root() {
    let root = temp_root("paths");
    fs::create_dir_all(root.join("4p")).unwrap();
    fs::write(root.join("4p/replay.mjson"), "ok").unwrap();
    assert!(resolve_replay_path(&root, "4p/replay.mjson").is_ok());
    assert!(resolve_replay_path(&root, "../replay.mjson").is_err());
    assert!(resolve_replay_path(&root, root.join("4p/replay.mjson")).is_err());
    fs::remove_file(root.join("4p/replay.mjson")).unwrap();
    fs::remove_dir(root.join("4p")).unwrap();
    assert!(resolve_replay_path(&root, "4p/missing/replay.mjson").is_ok());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replay_filename_uses_utc_mode_and_portable_relative_components() {
    let root = temp_root("filename");
    let writer =
        ReplayWriter::create_at(&root, "01JPATH", GameMode::ThreePlayerRedHalf, UNIX_EPOCH)
            .unwrap();
    assert_eq!(writer.relative_path_string().matches('/').count(), 1);
    assert!(
        writer
            .relative_path_string()
            .starts_with("3p/19700101T000000Z_3p-red-half_01JPATH.mjson")
    );
    assert!(!writer.relative_path().is_absolute());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn writer_flushes_atomically_to_portable_relative_mode_path_and_orders_auxiliary_positions() {
    let root = temp_root("writer");
    let mut writer = ReplayWriter::new(&root, "01JTESTMATCH", GameMode::FourPlayerRedEast).unwrap();
    assert!(
        writer
            .part_path()
            .to_string_lossy()
            .ends_with(".mjson.part")
    );
    assert_eq!(
        writer.part_path(),
        root.join(".incomplete/01JTESTMATCH.mjson.part")
    );
    assert!(writer.relative_path().to_string_lossy().contains("4p/"));
    writer
        .record_auxiliary(
            AuxiliaryEvent::Disconnected {
                seat: Seat::new(0).unwrap(),
            },
            AuxiliaryPhase::Before,
        )
        .unwrap();
    writer.append(start_events().remove(0)).unwrap();
    writer
        .record_auxiliary(
            AuxiliaryEvent::Reconnected {
                seat: Seat::new(0).unwrap(),
            },
            AuxiliaryPhase::After,
        )
        .unwrap();
    writer.append(start_events().remove(0)).unwrap();
    writer.flush_kyoku().unwrap();
    let part_path = writer.part_path().to_path_buf();
    let artifact = writer.finalize().unwrap();
    assert!(!part_path.exists());
    assert!(root.join(artifact.relative_path()).is_file());
    assert!(artifact.file_size > 0);
    assert!(!artifact.relative_path().is_absolute());
    assert!(
        artifact
            .auxiliary_events
            .iter()
            .any(|event| event.phase == AuxiliaryPhase::Before)
    );
    assert!(
        artifact
            .auxiliary_events
            .iter()
            .any(|event| event.phase == AuxiliaryPhase::After)
    );
    let frames = frames_from_artifact(&start_events(), &artifact).unwrap();
    assert_eq!(frames[0].auxiliary_events.len(), 2);
    assert_eq!(frames[0].auxiliary_events[0].phase, AuxiliaryPhase::Before);
    assert_eq!(frames[0].auxiliary_events[1].phase, AuxiliaryPhase::After);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn auxiliary_positions_outside_timeline_and_after_before_any_event_are_rejected() {
    let events = start_events();
    let invalid = double_riichi_replay::AuxiliaryRecord {
        event: AuxiliaryEvent::Disconnected {
            seat: Seat::new(0).unwrap(),
        },
        line_index: events.len(),
        phase: AuxiliaryPhase::Before,
        sequence: 0,
    };
    assert!(matches!(
        double_riichi_replay::build_replay_frames_with_auxiliary(&events, &[invalid]),
        Err(ReplayError::InvalidEvent(_))
    ));

    let root = temp_root("auxiliary-invalid");
    let mut writer = ReplayWriter::new(&root, "01JAUX", GameMode::FourPlayerRedEast).unwrap();
    assert!(matches!(
        writer.record_auxiliary(
            AuxiliaryEvent::Reconnected {
                seat: Seat::new(0).unwrap(),
            },
            AuxiliaryPhase::After,
        ),
        Err(ReplayError::InvalidEvent(_))
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn persistence_failures_are_typed_and_do_not_panic() {
    let root = temp_root("failure");
    let mut writer =
        ReplayWriter::with_failure_after_writes(&root, "01JFAIL", GameMode::FourPlayerRedEast, 1)
            .unwrap();
    let part_path = writer.part_path().to_path_buf();
    writer.append(start_events().remove(0)).unwrap();
    let error = writer.append(start_events().remove(0)).unwrap_err();
    assert!(matches!(error, ReplayError::Persistence(_)));
    assert!(!part_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn finalize_failure_cleans_partial_artifact() {
    let root = temp_root("finalize-failure");
    let mut writer = ReplayWriter::new(&root, "01JFINAL", GameMode::FourPlayerRedEast).unwrap();
    writer.append(start_events()[0].clone()).unwrap();
    let part_path = writer.part_path().to_path_buf();
    fs::write(root.join("4p"), "not a directory").unwrap();
    assert!(matches!(
        writer.finalize(),
        Err(ReplayError::Persistence(_))
    ));
    assert!(!part_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn startup_cleanup_removes_parts_and_renamed_files_for_incomplete_matches_but_keeps_completed_replays()
 {
    let root = temp_root("cleanup");
    fs::create_dir_all(root.join(".incomplete")).unwrap();
    fs::create_dir_all(root.join("4p")).unwrap();
    fs::write(root.join(".incomplete/unfinished.mjson.part"), "partial").unwrap();
    fs::write(
        root.join("4p/20260915T153845Z_4p-red-east_UNFINISHED.mjson"),
        "partial",
    )
    .unwrap();
    fs::write(
        root.join("4p/20260915T153845Z_4p-red-east_COMPLETE.mjson"),
        "complete",
    )
    .unwrap();
    fs::write(
        root.join("4p/20260915T153845Z_4p-red-east_ABCDEF.mjson"),
        "unrelated",
    )
    .unwrap();
    startup_cleanup(&root, ["UNFINISHED", "ABC"]).unwrap();
    assert!(!root.join(".incomplete/unfinished.mjson.part").exists());
    assert!(
        !root
            .join("4p/20260915T153845Z_4p-red-east_UNFINISHED.mjson")
            .exists()
    );
    assert!(
        root.join("4p/20260915T153845Z_4p-red-east_COMPLETE.mjson")
            .exists()
    );
    assert!(
        root.join("4p/20260915T153845Z_4p-red-east_ABCDEF.mjson")
            .exists()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reconstruction_uses_the_stored_half_game_mode() {
    let frames =
        build_replay_frames_for_mode(&start_events(), GameMode::FourPlayerRedHalf).unwrap();
    assert_eq!(frames[1].visible_state.mode, GameMode::FourPlayerRedHalf);
}

#[test]
fn reconstruction_rejects_semantically_impossible_discards_and_melds() {
    let mut outside_kyoku = start_events();
    outside_kyoku.insert(
        1,
        GameEvent::Tsumo {
            actor: Seat::new(0).unwrap(),
            tile: Tile::from_id(1).unwrap(),
        },
    );
    assert!(matches!(
        build_replay_frames(&outside_kyoku),
        Err(ReplayError::InvalidEvent(message)) if message.contains("outside an active kyoku")
    ));

    let mut missing_discard = start_events();
    missing_discard.push(GameEvent::Dahai {
        actor: Seat::new(0).unwrap(),
        tile: Tile::from_id(1).unwrap(),
        tsumogiri: false,
    });
    assert!(matches!(
        build_replay_frames(&missing_discard),
        Err(ReplayError::InvalidEvent(message)) if message.contains("not in concealed hand")
    ));

    let mut missing_meld_tile = start_events();
    missing_meld_tile.push(GameEvent::Pon {
        actor: Seat::new(1).unwrap(),
        target: Seat::new(0).unwrap(),
        called: Tile::from_id(0).unwrap(),
        consumed: vec![Tile::from_id(1).unwrap(), Tile::from_id(1).unwrap()],
    });
    assert!(matches!(
        build_replay_frames(&missing_meld_tile),
        Err(ReplayError::InvalidEvent(message)) if message.contains("not in concealed hand")
    ));
}

#[test]
fn corrupted_or_oversized_replays_are_rejected_at_the_exact_limit() {
    assert!(matches!(
        parse_mjson(r#"{"type":"future_event"}"#),
        Err(ReplayError::Corrupt { .. })
    ));
    assert!(matches!(
        parse_mjson(r#"{"type":"end_game","unexpected":true}"#),
        Err(ReplayError::Corrupt { .. })
    ));
    assert!(matches!(
        parse_mjson("{\"type\":\"end_game\"}\n\n"),
        Err(ReplayError::Corrupt { .. })
    ));
    assert!(matches!(
        parse_mjson(r#"{"type":"end_game"}"#),
        Err(ReplayError::Corrupt { .. })
    ));
    let noncanonical_start = serialize_event(&start_events()[1])
        .unwrap()
        .replace("kyoutaku", "kyotaku");
    assert!(matches!(
        parse_mjson(&noncanonical_start),
        Err(ReplayError::Corrupt { .. })
    ));
    assert_eq!(MAX_REPLAY_FRAME_BYTES, 64 * 1024 * 1024);
    let frames: Vec<ReplayFrame> = build_replay_frames(&start_events()).unwrap();
    assert_eq!(frames[0].event_index, 0);
    assert!(frames[0].visible_event.kind() == "start_game");
    let mut boundary = frames[0].clone();
    boundary.visible_state.players[0].display_name.clear();
    let baseline = encode_replay_frames(&[boundary.clone()]).unwrap().len();
    boundary.visible_state.players[0].display_name = "x".repeat(MAX_REPLAY_FRAME_BYTES - baseline);
    let exact = encode_replay_frames(&[boundary.clone()]).unwrap();
    assert_eq!(exact.len(), MAX_REPLAY_FRAME_BYTES);
    boundary.visible_state.players[0].display_name.push('x');
    assert!(matches!(
        encode_replay_frames(&[boundary]),
        Err(ReplayError::ReplayTooLarge { .. })
    ));
}

#[test]
fn generated_four_player_match_events_are_valid_canonical_mjson_and_reconstructable() {
    use double_riichi_core::{MatchMachine, Participant, ParticipantKind, Seat};

    let participants = (0..4)
        .map(|seat| {
            Participant::new(
                format!("p{seat}"),
                format!("Player {seat}"),
                ParticipantKind::BuiltInBot,
            )
        })
        .collect();
    let mut machine = MatchMachine::new(GameMode::FourPlayerRedEast, participants).unwrap();
    for _ in 0..20_000 {
        if machine.is_complete() {
            break;
        }
        let mut progressed = false;
        for seat in Seat::all(GameMode::FourPlayerRedEast) {
            let actions = machine.legal_actions(seat).unwrap();
            if let Some(action) = actions.first() {
                machine.apply(seat, default_action(&actions)).unwrap();
                let _ = action;
                progressed = true;
                break;
            }
        }
        assert!(progressed);
    }
    assert!(machine.is_complete());
    let text = machine
        .events()
        .iter()
        .map(serialize_event)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
        + "\n";
    let parsed = parse_mjson(&text).unwrap();
    let frames = build_replay_frames(&parsed).unwrap();
    assert_eq!(frames.len(), parsed.len());
    assert!(
        frames
            .iter()
            .any(|frame| frame.visible_event.kind() == "end_game")
    );
}

#[test]
fn three_player_kita_removes_north_from_reconstructed_hand() {
    let mut events = three_player_start_events();
    events.push(GameEvent::Kita {
        actor: Seat::new(0).unwrap(),
    });
    let frames = build_replay_frames(&events).unwrap();
    let player = &frames[2].visible_state.players[0];
    assert_eq!(player.concealed_count, 12);
    assert!(
        player
            .hand
            .as_ref()
            .unwrap()
            .iter()
            .all(|tile| tile.tile_type() != Tile::NORTH)
    );
}

#[test]
fn four_player_kita_is_rejected() {
    let mut events = start_events();
    let GameEvent::StartKyoku { tehais, .. } = &mut events[1] else {
        unreachable!();
    };
    tehais[0][0] = Tile::from_id(120).unwrap();
    events.push(GameEvent::Kita {
        actor: Seat::new(0).unwrap(),
    });
    assert!(matches!(
        build_replay_frames(&events),
        Err(ReplayError::InvalidEvent(message)) if message.contains("three-player")
    ));
}

#[test]
fn three_player_frames_have_no_dummy_fourth_player() {
    use double_riichi_core::{MatchMachine, Participant, ParticipantKind, Seat};

    let participants = (0..3)
        .map(|seat| {
            Participant::new(
                format!("p{seat}"),
                format!("Player {seat}"),
                ParticipantKind::BuiltInBot,
            )
        })
        .collect();
    let mut machine = MatchMachine::new(GameMode::ThreePlayerRedEast, participants).unwrap();
    for _ in 0..20_000 {
        if machine.is_complete() {
            break;
        }
        let mut progressed = false;
        for seat in Seat::all(GameMode::ThreePlayerRedEast) {
            let actions = machine.legal_actions(seat).unwrap();
            if !actions.is_empty() {
                machine.apply(seat, default_action(&actions)).unwrap();
                progressed = true;
                break;
            }
        }
        assert!(progressed);
    }
    assert!(machine.is_complete());
    let frames = build_replay_frames(machine.events()).unwrap();
    let players = &frames[1].visible_state.players;
    assert_eq!(players.len(), 3);
    assert!(players.iter().all(|player| player.seat.index() < 3));
}

#[test]
fn reconstructed_frames_include_calls_kans_riichi_multi_ron_draws_and_score_updates() {
    let mut events = start_events();
    events.extend([
        GameEvent::Pon {
            actor: Seat::new(1).unwrap(),
            target: Seat::new(0).unwrap(),
            called: Tile::from_id(0).unwrap(),
            consumed: vec![Tile::from_id(0).unwrap(), Tile::from_id(0).unwrap()],
        },
        GameEvent::Kakan {
            actor: Seat::new(1).unwrap(),
            called: Tile::from_id(0).unwrap(),
        },
        GameEvent::Reach {
            actor: Seat::new(2).unwrap(),
        },
        GameEvent::ReachAccepted {
            actor: Seat::new(2).unwrap(),
        },
        GameEvent::Hora {
            actor: Seat::new(1).unwrap(),
            target: Seat::new(0).unwrap(),
            tile: Some(Tile::from_id(0).unwrap()),
            ura_markers: None,
            yaku: None,
            fu: Some(30),
            han: Some(1),
            scores: Some(vec![24_000, 26_000, 25_000, 25_000]),
            delta: Some(vec![-1_000, 1_000, 0, 0]),
        },
        GameEvent::Hora {
            actor: Seat::new(2).unwrap(),
            target: Seat::new(0).unwrap(),
            tile: Some(Tile::from_id(0).unwrap()),
            ura_markers: None,
            yaku: None,
            fu: None,
            han: None,
            scores: None,
            delta: None,
        },
        GameEvent::Ryukyoku {
            reason: Some("exhaustive".into()),
            tehais: None,
            delta: Some(vec![0, 0, 0, 0]),
            scores: Some(vec![24_000, 26_000, 25_000, 25_000]),
        },
        GameEvent::Ryukyoku {
            reason: Some("abortive".into()),
            tehais: None,
            delta: None,
            scores: None,
        },
    ]);
    let frames = build_replay_frames(&events).unwrap();
    let pon = &frames[2].visible_state.players[1];
    assert_eq!(pon.melds[0].tiles.len(), 3);
    let kakan = &frames[3].visible_state.players[1];
    assert_eq!(kakan.melds[0].tiles.len(), 4);
    assert!(frames[5].visible_state.players[2].riichi);
    assert_eq!(frames[6].visible_state.players[1].score, 26_000);
    assert_eq!(frames[7].visible_event.kind(), "hora");
    assert_eq!(frames[8].visible_event.kind(), "ryukyoku");
    assert_eq!(frames[9].visible_event.kind(), "ryukyoku");
}

#[test]
fn frame_reconstruction_rejects_expansion_incrementally() {
    let large_name = "x".repeat(20 * 1024 * 1024);
    let events = vec![
        GameEvent::StartGame {
            names: Some(vec![
                large_name,
                "South".into(),
                "West".into(),
                "North".into(),
            ]),
            id: Some("large-expansion".into()),
        },
        GameEvent::StartKyoku {
            bakaze: Wind::East,
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: Seat::new(0).unwrap(),
            scores: vec![25_000; 4],
            dora_marker: Tile::from_id(0).unwrap(),
            tehais: vec![vec![Tile::from_id(0).unwrap(); 13]; 4],
        },
        GameEvent::EndKyoku,
        GameEvent::EndGame,
    ];
    assert!(matches!(
        build_replay_frames(&events),
        Err(ReplayError::ReplayTooLarge { .. })
    ));
}

#[test]
fn auxiliary_events_are_not_in_canonical_mjson_or_player_public_projection() {
    let line = serialize_event(&GameEvent::StartGame {
        names: None,
        id: None,
    })
    .unwrap();
    assert!(!line.contains("disconnected"));

    let mode = GameMode::FourPlayerRedEast;
    let players = (0..mode.seat_count())
        .map(|seat| {
            (
                Seat::new(seat as u8).unwrap(),
                Participant::new(
                    format!("p{seat}"),
                    format!("Player {seat}"),
                    ParticipantKind::Human,
                ),
                25_000,
            )
        })
        .collect();
    let table = TableState::from_hands(mode, players, vec![vec![Tile::from_id(0).unwrap(); 13]; 4])
        .unwrap();
    let public_json = table.project(Audience::Public).to_json().unwrap();
    let player_json = table
        .project(Audience::Player(Seat::new(0).unwrap()))
        .to_json()
        .unwrap();
    assert!(!public_json.contains("\"hand\""));
    assert_eq!(player_json.matches("\"hand\"").count(), 1);
    assert!(!public_json.contains("start_game"));
    assert!(!player_json.contains("start_game"));
}
