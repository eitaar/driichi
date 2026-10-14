use std::{fs, path::PathBuf, time::UNIX_EPOCH};

use double_riichi_core::{
    Audience, GameEvent, GameMode, KyokuResultRecord, KyokuSettlement, Participant,
    ParticipantKind, ScoreChange, Seat, TableState, Tile, WinEvaluation, WinResult, Wind,
    YakuValue,
};
use double_riichi_replay::{
    AuxiliaryEvent, AuxiliaryPhase, CanonicalEvent, MAX_REPLAY_FRAME_BYTES, ReplayError,
    ReplayFrame, ReplayWriter, attach_result_records, build_replay_frames,
    build_replay_frames_for_mode, encode_replay_frames, frames_from_artifact, parse_mjson,
    resolve_replay_path, serialize_event, startup_cleanup,
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

#[test]
fn replay_riichi_discard_index_tracks_declaration_not_latest_discard_and_resets() {
    for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
        let mut events = three_player_start_events();
        if mode.seat_count() == 4 {
            if let GameEvent::StartKyoku { scores, tehais, .. } = &mut events[1] {
                scores.push(25_000);
                tehais.push(vec![Tile::from_id(0).unwrap(); 13]);
            }
        }
        let start = events[1].clone();
        let actor = Seat::new(0).unwrap();
        let discard = GameEvent::Dahai {
            actor,
            tile: Tile::from_id(0).unwrap(),
            tsumogiri: false,
        };
        events.extend([
            discard.clone(),
            GameEvent::Reach { actor },
            discard.clone(),
            GameEvent::ReachAccepted { actor },
            discard,
        ]);
        events.extend([GameEvent::EndKyoku, start]);
        let frames = build_replay_frames_for_mode(&events, mode).unwrap();
        let player =
            |index: usize| serde_json::to_value(&frames[index].visible_state.players[0]).unwrap();
        assert!(player(2).get("riichi_discard_index").is_none());
        assert!(
            player(3).get("riichi_discard_index").is_none(),
            "Reach alone has no committed discard"
        );
        for index in 4..=7 {
            assert_eq!(player(index)["riichi_discard_index"], 1);
        }
        assert_eq!(frames[6].visible_state.players[0].discards.len(), 3);
        assert!(
            player(8).get("riichi_discard_index").is_none(),
            "new Kyoku resets declaration"
        );
        let accepted_only = [
            events[..2].to_vec(),
            vec![GameEvent::ReachAccepted { actor }],
        ]
        .concat();
        let frames = build_replay_frames_for_mode(&accepted_only, mode).unwrap();
        assert!(
            serde_json::to_value(&frames[2].visible_state.players[0])
                .unwrap()
                .get("riichi_discard_index")
                .is_none()
        );
    }
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

fn result_draw_record(
    sequence: u64,
    end: usize,
    before: [i32; 4],
    delta: [i32; 4],
    pot_before: u32,
    pot_after: u32,
) -> KyokuResultRecord {
    KyokuResultRecord {
        version: 1,
        kyoku_sequence: sequence,
        mode: GameMode::FourPlayerRedEast,
        end_event_index: end,
        wins: vec![],
        draw_event_delta_adjustment: Some(vec![0; 4]),
        settlement: KyokuSettlement {
            changes: (0..4)
                .map(|seat| ScoreChange {
                    participant_id: format!("seat{seat}").into(),
                    seat: Seat::new(seat as u8).unwrap(),
                    before: before[seat],
                    delta: delta[seat],
                    after: before[seat] + delta[seat],
                })
                .collect(),
            pot_before,
            pot_after,
            draw_reason: Some("fanpai".into()),
        },
    }
}

fn result_draw_events() -> Vec<GameEvent> {
    let mut events = start_events();
    events.extend([
        GameEvent::Ryukyoku {
            reason: Some("fanpai".into()),
            tehais: None,
            delta: Some(vec![0; 4]),
            scores: Some(vec![25_000; 4]),
        },
        GameEvent::EndKyoku,
        GameEvent::EndGame,
    ]);
    events
}

#[test]
fn result_pinned_draw_captures_join_without_canonical_rewrite() {
    // Frozen direct pinned-engine terminal suffix + explicitly controlled prefix;
    // core's unit test verifies this exact fixture against fresh EngineAdapter output.
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../double_riichi_core/tests/fixtures/authoritative-draw-captures.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
        let record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
        let scenario = case["scenario"].as_str().unwrap();
        let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
        attach_result_records(&mut frames, &[record.clone()])
            .unwrap_or_else(|error| panic!("{scenario} {:?}: {error}", record.mode));
        let end = record.end_event_index;
        assert_eq!(frames[end].settlement, Some(record.settlement.clone()));
        assert_eq!(frames[end - 1].visible_event, events[end - 1]);
        let mut invalid = record.clone();
        invalid.draw_event_delta_adjustment.as_mut().unwrap()[0] += 1;
        let original = encode_replay_frames(&frames).unwrap();
        assert!(attach_result_records(&mut frames, &[invalid]).is_err());
        assert_eq!(encode_replay_frames(&frames).unwrap(), original);
        let mut missing = case["record"].clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("draw_event_delta_adjustment");
        let missing: KyokuResultRecord = serde_json::from_value(missing).unwrap();
        assert!(attach_result_records(&mut frames, &[missing]).is_err());
        let mut wrong_scores = record.clone();
        wrong_scores.settlement.changes[0].after += 1;
        wrong_scores.settlement.changes[0].delta += 1;
        assert!(attach_result_records(&mut frames, &[wrong_scores]).is_err());
        let mut wrong_pot = record.clone();
        wrong_pot.settlement.pot_after = if scenario == "final" { 1 } else { 0 };
        assert!(attach_result_records(&mut frames, &[wrong_pot]).is_err());
        assert_eq!(encode_replay_frames(&frames).unwrap(), original);
        let mut tampered = events.clone();
        if let GameEvent::Ryukyoku {
            delta: Some(delta), ..
        } = &mut tampered[end - 1]
        {
            delta[0] += 1;
        } else {
            panic!("expected pinned draw event");
        }
        let mut bad_frames = build_replay_frames_for_mode(&tampered, record.mode).unwrap();
        assert!(attach_result_records(&mut bad_frames, &[record]).is_err());
    }
}

#[test]
fn result_draw_delta_evidence_reconciles_paid_reach_without_rewriting_canonical_event() {
    let mut events = start_events();
    events.extend([
        GameEvent::ReachAccepted {
            actor: Seat::new(0).unwrap(),
        },
        GameEvent::Tsumo {
            actor: Seat::new(1).unwrap(),
            tile: Tile::from_id(0).unwrap(),
        },
        GameEvent::Dahai {
            actor: Seat::new(1).unwrap(),
            tile: Tile::from_id(0).unwrap(),
            tsumogiri: true,
        },
        GameEvent::Ryukyoku {
            reason: Some("exhaustive_draw".into()),
            tehais: None,
            delta: Some(vec![-1_000, 0, 0, 0]),
            scores: Some(vec![24_000, 25_000, 25_000, 25_000]),
        },
        GameEvent::EndKyoku,
    ]);
    let mut record = result_draw_record(1, 6, [24_000, 25_000, 25_000, 25_000], [0; 4], 1, 1);
    record.settlement.draw_reason = Some("exhaustive_draw".into());
    record.draw_event_delta_adjustment = Some(vec![-1_000, 0, 0, 0]);
    let frames = build_replay_frames(&events).unwrap();
    let mut valid = frames.clone();
    attach_result_records(&mut valid, &[record.clone()]).unwrap();
    assert_eq!(valid[5].visible_event, events[5]);
    let mut corrupted = record.clone();
    corrupted.draw_event_delta_adjustment.as_mut().unwrap()[0] = 0;
    let mut candidate = frames.clone();
    assert!(attach_result_records(&mut candidate, &[corrupted]).is_err());
    assert_eq!(
        encode_replay_frames(&candidate).unwrap(),
        encode_replay_frames(&frames).unwrap()
    );
    let mut events = events;
    if let GameEvent::Ryukyoku {
        delta: Some(delta), ..
    } = &mut events[5]
    {
        delta[0] = 0;
    }
    assert!(attach_result_records(&mut build_replay_frames(&events).unwrap(), &[record]).is_err());
}

#[test]
fn result_unknown_version_and_duplicate_terminal_index_are_corrupt() {
    let mut events = result_draw_events();
    events.pop();
    let mut second = events[1].clone();
    if let GameEvent::StartKyoku { kyoku, .. } = &mut second {
        *kyoku = 2;
    }
    let terminal = events[2].clone();
    events.extend([second, terminal, GameEvent::EndKyoku, GameEvent::EndGame]);
    let mut frames = build_replay_frames(&events).unwrap();
    let first = result_draw_record(1, 3, [25_000; 4], [0; 4], 0, 0);
    let second = result_draw_record(2, 6, [25_000; 4], [0; 4], 0, 0);
    assert!(attach_result_records(&mut frames, &[first.clone(), second.clone()]).is_ok());
    let mut invalid = first.clone();
    invalid.version = 2;
    assert!(attach_result_records(&mut frames, &[invalid, second.clone()]).is_err());
    let mut duplicate = second;
    duplicate.end_event_index = first.end_event_index;
    assert!(attach_result_records(&mut frames, &[first.clone(), duplicate]).is_err());
    assert_eq!(frames[3].settlement, Some(first.settlement));
    assert!(frames[6].settlement.is_some());
}

#[test]
fn result_rejects_canonical_delta_score_and_round_continuity_contradictions() {
    let events = result_draw_events();
    let mut frames = build_replay_frames(&events).unwrap();
    let contradictory = result_draw_record(
        1,
        3,
        [26_000, 24_000, 25_000, 25_000],
        [-1_000, 1_000, 0, 0],
        0,
        0,
    );
    assert!(attach_result_records(&mut frames, &[contradictory]).is_err());
    let wrong_delta = result_draw_record(1, 3, [25_000; 4], [-100, 100, 0, 0], 0, 0);
    assert!(attach_result_records(&mut frames, &[wrong_delta]).is_err());
    let mut bad_scores = events.clone();
    if let GameEvent::Ryukyoku {
        scores: Some(scores),
        ..
    } = &mut bad_scores[2]
    {
        scores[0] += 100;
    }
    let mut bad_frames = build_replay_frames(&bad_scores).unwrap();
    assert!(
        attach_result_records(
            &mut bad_frames,
            &[result_draw_record(1, 3, [25_000; 4], [0; 4], 0, 0)]
        )
        .is_err()
    );
    let mut events = events;
    let mut next = events[1].clone();
    if let GameEvent::StartKyoku { kyoku, scores, .. } = &mut next {
        *kyoku = 2;
        scores[0] += 100;
    }
    events.pop();
    let terminal = events[2].clone();
    events.extend([next, terminal, GameEvent::EndKyoku, GameEvent::EndGame]);
    let mut frames = build_replay_frames(&events).unwrap();
    let records = [
        result_draw_record(1, 3, [25_000; 4], [0; 4], 0, 0),
        result_draw_record(2, 6, [25_100, 25_000, 25_000, 25_000], [0; 4], 0, 0),
    ];
    assert!(attach_result_records(&mut frames, &records).is_err());
}

#[test]
fn result_fourth_reach_accepted_inside_terminal_resolution_is_not_paid_twice() {
    let mut events = start_events();
    for actor in 0..4 {
        let seat = Seat::new(actor).unwrap();
        events.extend([
            GameEvent::Reach { actor: seat },
            GameEvent::Dahai {
                actor: seat,
                tile: Tile::from_id(0).unwrap(),
                tsumogiri: false,
            },
            GameEvent::ReachAccepted { actor: seat },
        ]);
        if actor < 3 {
            events.push(GameEvent::Tsumo {
                actor: Seat::new(actor + 1).unwrap(),
                tile: Tile::from_id(0).unwrap(),
            });
        }
    }
    events.push(GameEvent::Ryukyoku {
        reason: Some("suucha_riichi".into()),
        tehais: None,
        delta: Some(vec![0; 4]),
        scores: None,
    });
    events.push(GameEvent::EndKyoku);
    let first_end_index = events.len() - 1;
    let mut next = events[1].clone();
    if let GameEvent::StartKyoku {
        kyoku,
        scores,
        kyotaku,
        ..
    } = &mut next
    {
        *kyoku = 2;
        scores.fill(24_000);
        *kyotaku = 4;
    }
    events.extend([
        next,
        GameEvent::Ryukyoku {
            reason: Some("fanpai".into()),
            tehais: None,
            delta: Some(vec![0; 4]),
            scores: None,
        },
        GameEvent::EndKyoku,
    ]);
    let second_end_index = events.len() - 1;
    let mut frames = build_replay_frames(&events).unwrap();
    let mut first = result_draw_record(
        1,
        first_end_index,
        [24_000, 24_000, 24_000, 25_000],
        [0, 0, 0, -1_000],
        3,
        4,
    );
    first.settlement.draw_reason = Some("suucha_riichi".into());
    first.draw_event_delta_adjustment = Some(vec![0, 0, 0, 1_000]);
    let second = result_draw_record(2, second_end_index, [24_000; 4], [0; 4], 4, 4);
    attach_result_records(&mut frames, &[first.clone(), second.clone()]).unwrap();
    assert_eq!(
        frames[first_end_index].settlement,
        Some(first.settlement.clone())
    );
    let mut impossible = result_draw_record(1, first_end_index, [25_000; 4], [-1_000; 4], 0, 4);
    impossible.settlement.draw_reason = Some("suucha_riichi".into());
    impossible.draw_event_delta_adjustment = Some(vec![1_000; 4]);
    assert!(attach_result_records(&mut frames, &[impossible, second]).is_err());
}

#[test]
fn result_rejects_earlier_reach_separated_from_terminal_batch() {
    let mut events = start_events();
    events.extend([
        GameEvent::ReachAccepted {
            actor: Seat::new(0).unwrap(),
        },
        GameEvent::Tsumo {
            actor: Seat::new(1).unwrap(),
            tile: Tile::from_id(0).unwrap(),
        },
        GameEvent::Dahai {
            actor: Seat::new(1).unwrap(),
            tile: Tile::from_id(0).unwrap(),
            tsumogiri: true,
        },
        GameEvent::Ryukyoku {
            reason: Some("fanpai".into()),
            tehais: None,
            delta: Some(vec![0; 4]),
            scores: None,
        },
        GameEvent::EndKyoku,
    ]);
    let mut frames = build_replay_frames(&events).unwrap();
    let impossible = result_draw_record(1, 6, [25_000; 4], [-1_000, 0, 0, 0], 0, 1);
    assert!(attach_result_records(&mut frames, &[impossible]).is_err());
}

#[test]
fn result_allows_paid_reach_and_verified_final_remaining_pot_award() {
    let mut events = start_events();
    events.extend([
        GameEvent::ReachAccepted {
            actor: Seat::new(0).unwrap(),
        },
        GameEvent::Ryukyoku {
            reason: Some("fanpai".into()),
            tehais: None,
            delta: Some(vec![0; 4]),
            scores: None,
        },
        GameEvent::EndKyoku,
        GameEvent::EndGame,
    ]);
    let mut frames = build_replay_frames(&events).unwrap();
    let record = result_draw_record(
        1,
        4,
        [24_000, 25_000, 25_000, 25_000],
        [0, 1_000, 0, 0],
        1,
        0,
    );
    attach_result_records(&mut frames, &[record.clone()]).unwrap();
    assert_eq!(frames[4].settlement, Some(record.settlement));
    for frame in &frames[4..] {
        assert_eq!(
            frame
                .visible_state
                .players
                .iter()
                .map(|p| p.score)
                .collect::<Vec<_>>(),
            vec![24_000, 26_000, 25_000, 25_000]
        );
        assert_eq!(frame.visible_state.kyotaku, Some(0));
    }
}

#[test]
fn result_rejects_a_corrupt_record_without_mutating_existing_join() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
    ))
    .unwrap();
    let case = &cases.as_array().unwrap()[0];
    let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
    let record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
    let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
    attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
    let before = encode_replay_frames(&frames).unwrap();
    let mut payload = case["record"].clone();
    payload["wins"][0]["winning_tile"] = serde_json::json!(255);
    let corrupt: KyokuResultRecord = serde_json::from_value(payload).unwrap();
    let borrowed_before = corrupt.clone();
    assert!(
        matches!(attach_result_records(&mut frames, std::slice::from_ref(&corrupt)), Err(ReplayError::InvalidEvent(message))
        if message == "result contains an invalid tile for its mode")
    );
    assert_eq!(corrupt, borrowed_before);
    assert_eq!(encode_replay_frames(&frames).unwrap(), before);
}

#[test]
fn result_pinned_multi_ron_captures_join_ordered_winners_and_combined_settlement() {
    // Frozen real 4p/3p EngineAdapter multi-Ron terminal resolution and record;
    // only the preceding StartKyoku/Tsumo/Dahai prefix is test-controlled.
    // The core unit test compares this identical payload to fresh pinned output.
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
        let record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
        let indices = record
            .wins
            .iter()
            .map(|win| win.event_index)
            .collect::<Vec<_>>();
        assert_eq!(record.draw_event_delta_adjustment, None);
        assert_eq!(record.wins.len(), 2);
        assert!(indices[0] < indices[1] && indices[1] < record.end_event_index);
        let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
        let unjoined_events = frames
            .iter()
            .map(|frame| frame.visible_event.clone())
            .collect::<Vec<_>>();
        attach_result_records(&mut frames, &[record.clone()]).unwrap();
        let expected_scores = if record.mode.is_three_player() {
            vec![91_800, 107_600, 102_600]
        } else {
            vec![91_600, 107_800, 102_600, 100_000]
        };
        for frame in &frames[record.end_event_index..] {
            assert_eq!(
                frame
                    .visible_state
                    .players
                    .iter()
                    .map(|p| p.score)
                    .collect::<Vec<_>>(),
                expected_scores
            );
            assert_eq!(frame.visible_state.kyotaku, Some(0));
        }
        assert_eq!(frames[indices[0]].result, Some(record.wins[0].clone()));
        assert_eq!(frames[indices[1]].result, Some(record.wins[1].clone()));
        assert!(frames[indices[0]].settlement.is_none() && frames[indices[1]].settlement.is_none());
        assert_eq!(
            frames[record.end_event_index].settlement,
            Some(record.settlement.clone())
        );
        assert_eq!(
            frames
                .iter()
                .map(|frame| frame.visible_event.clone())
                .collect::<Vec<_>>(),
            unjoined_events
        );
        let before = encode_replay_frames(&frames).unwrap();
        assert!(
            !std::str::from_utf8(&before)
                .unwrap()
                .contains("draw_event_delta_adjustment")
        );
        let mut wrong_evidence = record.clone();
        wrong_evidence.draw_event_delta_adjustment = Some(vec![0; record.mode.seat_count()]);
        assert!(attach_result_records(&mut frames, &[wrong_evidence]).is_err());
        let mut wrong_index = record.clone();
        wrong_index.wins[0].event_index = indices[1];
        assert!(attach_result_records(&mut frames, &[wrong_index]).is_err());
        assert_eq!(encode_replay_frames(&frames).unwrap(), before);
        let mut changed = events.clone();
        if let GameEvent::Hora {
            delta: Some(delta), ..
        } = &mut changed[indices[1]]
        {
            delta[0] += 100;
        } else {
            panic!("pinned second Ron delta missing");
        }
        let mut bad_frames = build_replay_frames_for_mode(&changed, record.mode).unwrap();
        assert!(attach_result_records(&mut bad_frames, &[record]).is_err());
        assert!(
            bad_frames
                .iter()
                .all(|frame| frame.result.is_none() && frame.settlement.is_none())
        );
    }
}

#[test]
fn result_oversized_borrowed_identity_preserves_existing_join() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
    ))
    .unwrap();
    let case = &cases.as_array().unwrap()[0];
    let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
    let mut record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
    let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
    attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
    let before = encode_replay_frames(&frames).unwrap();
    let seat = record.wins[0].seat.index() as usize;
    let identity = "q".repeat(MAX_REPLAY_FRAME_BYTES + 1);
    record.wins[0].winner = identity.clone().into();
    record.settlement.changes[seat].participant_id = identity.into();
    let borrowed_len = record.wins[0].winner.as_str().len();

    assert!(matches!(
        attach_result_records(&mut frames, std::slice::from_ref(&record)),
        Err(ReplayError::ReplayTooLarge { .. })
    ));
    assert_eq!(encode_replay_frames(&frames).unwrap(), before);
    assert_eq!(record.wins[0].winner.as_str().len(), borrowed_len);
    assert_eq!(
        record.wins[0].winner,
        record.settlement.changes[seat].participant_id
    );
}

#[test]
fn result_rejects_contradictory_saved_win_without_mutating_frames() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
        let record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
        let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
        attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
        let before = encode_replay_frames(&frames).unwrap();
        for corruption in [
            "points",
            "dealer_payment",
            "duplicate",
            "incomplete",
            "extra_meld",
        ] {
            let mut bad = record.clone();
            let win = &mut bad.wins[0];
            match corruption {
                "points" => win.win_points = 1,
                "dealer_payment" => win.win_points = 7_800,
                "duplicate" => win.concealed[0] = win.concealed[1],
                "incomplete" => {
                    win.concealed.remove(0);
                    win.winning_tile_index -= 1;
                }
                "extra_meld" => win.melds.push(double_riichi_core::ResultMeld {
                    kind: double_riichi_core::ResultMeldKind::Pon,
                    tiles: vec![
                        Tile::from_id(0).unwrap(),
                        Tile::from_id(1).unwrap(),
                        Tile::from_id(2).unwrap(),
                    ],
                    opened: true,
                    from_who: Some(Seat::new(0).unwrap()),
                    called_tile: Some(Tile::from_id(0).unwrap()),
                }),
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    attach_result_records(&mut frames, &[bad]),
                    Err(ReplayError::InvalidEvent(_))
                ),
                "{corruption}"
            );
            assert_eq!(encode_replay_frames(&frames).unwrap(), before);
        }
    }
}

#[test]
fn result_allows_ordered_multi_ron_with_one_combined_settlement() {
    let mut events = start_events();
    if let GameEvent::StartKyoku { kyotaku, .. } = &mut events[1] {
        *kyotaku = 1;
    }
    for (actor, delta) in [
        (1, vec![-1_000, 2_000, 0, 0]),
        (2, vec![-2_000, 0, 2_000, 0]),
    ] {
        events.push(GameEvent::Hora {
            actor: Seat::new(actor).unwrap(),
            target: Seat::new(0).unwrap(),
            tile: Some(Tile::from_id(0).unwrap()),
            ura_markers: None,
            yaku: None,
            fu: None,
            han: None,
            scores: None,
            delta: Some(delta),
        });
    }
    events.extend([GameEvent::EndKyoku, GameEvent::EndGame]);
    let mut frames = build_replay_frames(&events).unwrap();
    let mut record = result_draw_record(1, 4, [25_000; 4], [-3_000, 2_000, 2_000, 0], 1, 0);
    record.settlement.draw_reason = None;
    record.draw_event_delta_adjustment = None;
    record.wins = (1..=2)
        .map(|seat| WinResult {
            event_index: seat + 1,
            winner: format!("seat{seat}").into(),
            seat: Seat::new(seat as u8).unwrap(),
            target: Seat::new(0).unwrap(),
            tsumo: false,
            concealed: [36, 40, 44, 48, 52, 56, 60, 64, 68, 108, 109, 0, 1, 2]
                .into_iter()
                .map(|id| Tile::from_id(id).unwrap())
                .collect(),
            melds: vec![],
            winning_tile: Tile::from_id(0).unwrap(),
            winning_tile_index: 11,
            yaku: vec![YakuValue {
                id: 2,
                name: "立直".into(),
                han: Some(1),
                yakuman_multiplier: None,
            }],
            evaluation: WinEvaluation::Ordinary {
                han: 1,
                fu: 30,
                limit: None,
            },
            win_points: 1_000,
        })
        .collect();
    attach_result_records(&mut frames, &[record.clone()]).unwrap();
    assert_eq!(frames[2].result, Some(record.wins[0].clone()));
    assert_eq!(frames[3].result, Some(record.wins[1].clone()));
    assert!(frames[2].settlement.is_none() && frames[3].settlement.is_none());
    assert_eq!(frames[4].settlement, Some(record.settlement));
}

#[test]
fn result_join_size_error_preserves_caller_frames() {
    let mut frames = build_replay_frames(&result_draw_events()).unwrap();
    let record = result_draw_record(1, 3, [25_000; 4], [0; 4], 0, 0);
    // Existing unrelated and target-index metadata must survive an error too.
    frames[0].settlement = Some(record.settlement.clone());
    frames[3].settlement = Some(KyokuSettlement {
        changes: vec![],
        pot_before: 0,
        pot_after: 0,
        draw_reason: None,
    });
    let previous_target_settlement = frames[3].settlement.clone();
    frames[3].result = Some(WinResult {
        event_index: 3,
        winner: "seat0".into(),
        seat: Seat::new(0).unwrap(),
        target: Seat::new(0).unwrap(),
        tsumo: true,
        concealed: vec![],
        melds: vec![],
        winning_tile: Tile::from_id(0).unwrap(),
        winning_tile_index: 0,
        yaku: vec![],
        evaluation: WinEvaluation::Ordinary {
            han: 1,
            fu: 30,
            limit: None,
        },
        win_points: 1,
    });
    let original_result = frames[3].result.clone();
    let original_settlement = frames[0].settlement.clone();
    let original_size = encode_replay_frames(&frames).unwrap().len();
    // Leave space for the original frame but not for the added settlement.
    frames[0].visible_state.players[0].display_name =
        "X".repeat(MAX_REPLAY_FRAME_BYTES - original_size - 32);
    assert!(encode_replay_frames(&frames).is_ok());
    assert!(matches!(
        attach_result_records(&mut frames, &[record]),
        Err(ReplayError::ReplayTooLarge { .. })
    ));
    assert_eq!(frames[3].result, original_result);
    assert_eq!(frames[0].settlement, original_settlement);
    assert_eq!(frames[3].settlement, previous_target_settlement);
    assert_eq!(
        frames[0].visible_state.players[0].display_name.len(),
        MAX_REPLAY_FRAME_BYTES - original_size - 32
    );
}

#[test]
fn result_legacy_replay_omits_unknown_details() {
    let mut events = start_events();
    events.push(GameEvent::EndKyoku);
    events.push(GameEvent::EndGame);
    let frames = build_replay_frames(&events).unwrap();
    assert!(
        frames
            .iter()
            .all(|frame| frame.result.is_none() && frame.settlement.is_none())
    );
}

#[test]
fn canonical_mjson_accepts_and_preserves_kakan_consumed_tiles() {
    let line = r#"{"type":"kakan","actor":3,"pai":"9m","consumed":["9m","9m","9m"]}"#;
    let text = start_events()
        .iter()
        .map(serialize_event)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
        + "\n"
        + line
        + "\n{\"type\":\"end_kyoku\"}\n{\"type\":\"end_game\"}";

    let events = parse_mjson(&text).unwrap();
    let kakan = events
        .iter()
        .find(|event| matches!(event, GameEvent::Kakan { .. }))
        .unwrap();

    assert_eq!(serialize_event(kakan).unwrap(), line);
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
            consumed: vec![Tile::from_id(12).unwrap(); 3],
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
        if let Some(held) = machine.held_kyoku() {
            let winner_count = held.wins.len();
            for step in 0..=winner_count {
                machine.publish_kyoku_step(step).unwrap();
            }
            machine.release_kyoku().unwrap();
            continue;
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
    if let Ok(path) = std::env::var("DRIICHI_YAMAI_REPLAY_OUTPUT") {
        std::fs::write(path, &text).unwrap();
    }
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
        if let Some(held) = machine.held_kyoku() {
            let winner_count = held.wins.len();
            for step in 0..=winner_count {
                machine.publish_kyoku_step(step).unwrap();
            }
            machine.release_kyoku().unwrap();
            continue;
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
fn reconstructed_round_facts_survive_events_and_reset_at_the_next_kyoku() {
    for mode in [GameMode::ThreePlayerRedHalf, GameMode::FourPlayerRedHalf] {
        let mut events = if mode.seat_count() == 3 {
            three_player_start_events()
        } else {
            start_events()
        };
        let GameEvent::StartKyoku {
            bakaze,
            kyoku,
            honba,
            kyotaku,
            oya,
            ..
        } = &mut events[1]
        else {
            unreachable!();
        };
        *bakaze = Wind::South;
        *kyoku = 3;
        *honba = 2;
        *kyotaku = 3;
        *oya = Seat::new(1).unwrap();
        let mut next_round = events[1].clone();
        let GameEvent::StartKyoku {
            honba,
            kyotaku,
            oya,
            ..
        } = &mut next_round
        else {
            unreachable!();
        };
        *honba = 4;
        *kyotaku = 1;
        *oya = Seat::new(2).unwrap();
        events.extend([
            GameEvent::Reach {
                actor: Seat::new(1).unwrap(),
            },
            GameEvent::ReachAccepted {
                actor: Seat::new(1).unwrap(),
            },
            GameEvent::Hora {
                actor: Seat::new(1).unwrap(),
                target: Seat::new(0).unwrap(),
                tile: None,
                ura_markers: None,
                yaku: None,
                fu: None,
                han: None,
                scores: None,
                delta: None,
            },
            GameEvent::EndKyoku,
            next_round,
        ]);
        let frames = build_replay_frames_for_mode(&events, mode).unwrap();
        let facts = |index: usize| {
            let state = &frames[index].visible_state;
            (
                state.round,
                state.kyoku,
                state.dealer,
                state.honba,
                state.kyotaku,
            )
        };
        assert_eq!(facts(0), (None, None, None, None, None));
        let initial = (
            Some(Wind::South),
            Some(3),
            Some(Seat::new(1).unwrap()),
            Some(2),
            Some(3),
        );
        assert_eq!(facts(1), initial);
        assert_eq!(facts(2), initial, "declaration does not pay the deposit");
        assert_eq!(
            facts(3),
            (
                Some(Wind::South),
                Some(3),
                Some(Seat::new(1).unwrap()),
                Some(2),
                Some(4)
            )
        );
        assert_eq!(
            facts(4),
            (
                Some(Wind::South),
                Some(3),
                Some(Seat::new(1).unwrap()),
                Some(2),
                Some(0)
            )
        );
        assert_eq!(facts(5), facts(4));
        assert_eq!(
            facts(6),
            (
                Some(Wind::South),
                Some(3),
                Some(Seat::new(2).unwrap()),
                Some(4),
                Some(1)
            )
        );
        assert!(
            frames
                .iter()
                .all(|frame| frame.visible_state.remaining_wall.is_none()),
            "do not invent an unknown wall count"
        );
        let json: serde_json::Value = serde_json::from_str(&frames[1].to_json().unwrap()).unwrap();
        assert_eq!(json["visible_state"]["round"], "South");
        assert_eq!(json["visible_state"]["kyoku"], 3);
        assert_eq!(json["visible_state"]["honba"], 2);
    }
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
            consumed: vec![Tile::from_id(0).unwrap(); 3],
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
