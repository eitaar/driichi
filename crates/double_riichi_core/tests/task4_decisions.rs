use std::time::Duration;

use double_riichi_core::{
    Decision, DecisionError, DecisionId, DecisionKind, GameAction, Seat, Tile, TimeControl,
};
use tokio::time::{self, Instant};

fn seat(index: u8) -> Seat {
    Seat::new(index).expect("valid test seat")
}

#[tokio::test(start_paused = true)]
async fn decision_keeps_compound_actions_and_waits_for_every_response() {
    let target = seat(0);
    let responder = seat(1);
    let called = Tile::from_id(8).unwrap();
    let compound = GameAction::Pon {
        target,
        called,
        consumed: vec![Tile::from_id(9).unwrap(), Tile::from_id(10).unwrap()],
    };
    let decision = Decision::new(
        DecisionId::new("d42"),
        DecisionKind::Response,
        vec![
            (responder, vec![compound.clone(), GameAction::Pass]),
            (seat(2), vec![GameAction::Pass]),
        ],
        Instant::now(),
        Some(Duration::from_secs(10)),
        false,
    )
    .unwrap();

    assert_eq!(decision.actions_for(responder)[0].action, compound);
    let first = decision.actions_for(responder)[0].id.clone();
    let second = decision.actions_for(seat(2))[0].id.clone();
    let mut decision = decision;

    assert!(!decision.submit(responder, first).unwrap().is_resolved());
    assert!(!decision.is_closed());
    assert!(decision.submit(seat(2), second).unwrap().is_resolved());
}

#[test]
fn deterministic_defaults_prefer_pass_then_tsumogiri() {
    let current = seat(0);
    let tile = Tile::from_id(8).unwrap();
    let decision = Decision::new(
        DecisionId::new("d1"),
        DecisionKind::Turn,
        vec![(
            current,
            vec![
                GameAction::Discard {
                    tile,
                    tsumogiri: false,
                },
                GameAction::Discard {
                    tile,
                    tsumogiri: true,
                },
            ],
        )],
        Instant::now(),
        Some(Duration::from_secs(30)),
        false,
    )
    .unwrap();
    assert_eq!(
        decision.default_for(current),
        &GameAction::Discard {
            tile,
            tsumogiri: true
        }
    );

    let response = Decision::new(
        DecisionId::new("d2"),
        DecisionKind::Response,
        vec![(current, vec![GameAction::Ron(seat(1)), GameAction::Pass])],
        Instant::now(),
        Some(Duration::from_secs(10)),
        false,
    )
    .unwrap();
    assert_eq!(response.default_for(current), &GameAction::Pass);
}

#[tokio::test(start_paused = true)]
async fn expired_decision_rejects_late_action() {
    let current = seat(0);
    let mut decision = Decision::new(
        DecisionId::new("d-expired"),
        DecisionKind::Turn,
        vec![(current, vec![GameAction::Pass])],
        Instant::now(),
        Some(Duration::from_secs(1)),
        false,
    )
    .unwrap();
    let action = decision.actions_for(current)[0].id.clone();
    time::advance(Duration::from_secs(1)).await;
    assert!(matches!(
        decision.submit(current, action),
        Err(DecisionError::Expired)
    ));
}

#[test]
fn ids_reject_stale_foreign_and_already_consumed_submissions() {
    let current = seat(0);
    let other = seat(1);
    let mut decision = Decision::new(
        DecisionId::new("d1"),
        DecisionKind::Turn,
        vec![
            (current, vec![GameAction::Pass]),
            (other, vec![GameAction::Pass]),
        ],
        Instant::now(),
        Some(Duration::from_secs(30)),
        false,
    )
    .unwrap();
    let action = decision.actions_for(current)[0].id.clone();

    assert!(matches!(
        decision.submit_with_decision_id(&DecisionId::new("old"), current, action.clone(),),
        Err(DecisionError::StaleDecision { .. })
    ));
    assert!(matches!(
        decision.submit(current, decision.actions_for(other)[0].id.clone()),
        Err(DecisionError::ForeignAction { .. })
    ));
    decision.submit(current, action.clone()).unwrap();
    assert!(matches!(
        decision.submit(current, action),
        Err(DecisionError::AlreadyConsumed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn casual_and_unlimited_timing_is_monotonic() {
    let casual = TimeControl::Casual;
    assert_eq!(casual.turn_duration(), Duration::from_secs(30));
    assert_eq!(casual.response_duration(), Duration::from_secs(10));

    let opened = time::Instant::now();
    time::advance(Duration::from_secs(29)).await;
    assert_eq!(
        time::Instant::now().duration_since(opened),
        Duration::from_secs(29)
    );
    time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        time::Instant::now().duration_since(opened),
        Duration::from_secs(30)
    );

    assert!(TimeControl::Unlimited.watchdog_duration().is_some());
}
