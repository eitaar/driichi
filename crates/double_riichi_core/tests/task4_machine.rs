use std::time::Duration;

use double_riichi_core::{
    ControllerState, DecisionKind, GameMode, MatchMachine, Participant, ParticipantKind, Seat,
    TimeControl,
};
use tokio::time;

fn roster(mode: GameMode, kind: ParticipantKind) -> Vec<Participant> {
    (0..mode.seat_count())
        .map(|index| Participant::new(format!("p{index}"), format!("Player {index}"), kind))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn casual_turn_decision_expires_at_exactly_thirty_seconds() {
    let mut machine = MatchMachine::with_time_control(
        GameMode::FourPlayerRedEast,
        roster(GameMode::FourPlayerRedEast, ParticipantKind::Human),
        TimeControl::Casual,
    )
    .unwrap();
    let decision = machine.current_decision().unwrap().unwrap();
    assert_eq!(decision.kind(), DecisionKind::Turn);
    assert_eq!(decision.duration(), Some(Duration::from_secs(30)));
    time::advance(Duration::from_secs(29)).await;
    assert!(machine.resolve_expired().unwrap().is_none());
    time::advance(Duration::from_secs(1)).await;
    assert!(machine.resolve_expired().unwrap().is_some());
}

#[tokio::test(start_paused = true)]
async fn unlimited_disconnect_uses_watchdog_then_temporary_auto_until_reconnect() {
    let mode = GameMode::FourPlayerRedEast;
    let mut machine = MatchMachine::with_time_control(
        mode,
        roster(mode, ParticipantKind::Human),
        TimeControl::Unlimited,
    )
    .unwrap();
    let decision = machine.current_decision().unwrap().unwrap();
    let seat = decision.eligible().next().unwrap();
    machine.disconnect(seat).unwrap();
    let decision = machine.current_decision().unwrap().unwrap();
    assert_eq!(decision.duration(), Some(Duration::from_secs(300)));
    assert!(decision.is_watchdog());
    time::advance(Duration::from_secs(299)).await;
    assert!(machine.resolve_expired().unwrap().is_none());
    time::advance(Duration::from_secs(1)).await;
    machine.resolve_expired().unwrap().unwrap();
    assert_eq!(
        machine.controller(seat).unwrap(),
        ControllerState::TemporaryAuto
    );
    machine.reconnect(seat).unwrap();
    assert_eq!(
        machine.controller(seat).unwrap(),
        ControllerState::Interactive
    );
}

#[test]
fn unlimited_connected_human_has_no_deadline_but_built_in_bot_is_immediate() {
    let mode = GameMode::FourPlayerRedEast;
    let mut human = MatchMachine::with_time_control(
        mode,
        roster(mode, ParticipantKind::Human),
        TimeControl::Unlimited,
    )
    .unwrap();
    assert_eq!(human.current_decision().unwrap().unwrap().deadline(), None);

    let mut bot = MatchMachine::with_time_control(
        mode,
        roster(mode, ParticipantKind::BuiltInBot),
        TimeControl::Unlimited,
    )
    .unwrap();
    let decision = bot.current_decision().unwrap().unwrap();
    assert_eq!(decision.duration(), Some(Duration::ZERO));
    assert!(decision.is_expired_at(tokio::time::Instant::now()));
}

#[test]
fn seat_validation_remains_mode_specific() {
    let mode = GameMode::ThreePlayerRedEast;
    let mut machine = MatchMachine::new(mode, roster(mode, ParticipantKind::Human)).unwrap();
    assert!(machine.disconnect(Seat::new(3).unwrap()).is_err());
}
