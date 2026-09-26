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

#[tokio::test(start_paused = true)]
async fn temporary_auto_turn_is_immediate_after_casual_or_riichi_dev_timeout() {
    for time_control in [TimeControl::Casual, TimeControl::RiichiDev] {
        let mut machine = MatchMachine::with_time_control(
            GameMode::FourPlayerRedEast,
            roster(GameMode::FourPlayerRedEast, ParticipantKind::Human),
            time_control,
        )
        .unwrap();
        let first = machine.current_decision().unwrap().unwrap();
        let seat = first.eligible().next().expect("initial turn");
        let initial_duration = first.duration_for(seat).expect("turn deadline");
        machine.disconnect(seat).unwrap();
        time::advance(initial_duration).await;
        machine.resolve_expired().unwrap().expect("initial timeout");
        assert_eq!(
            machine.controller(seat).unwrap(),
            ControllerState::TemporaryAuto
        );

        for _ in 0..2_000 {
            let decision = machine.current_decision().unwrap().expect("decision");
            if decision.kind() == DecisionKind::Turn && !decision.actions_for(seat).is_empty() {
                assert_eq!(decision.duration_for(seat), Some(Duration::ZERO));
                return;
            }
            let pending = decision.actions_for(seat).is_empty();
            for eligible in decision.eligible().collect::<Vec<_>>() {
                if eligible == seat {
                    continue;
                }
                machine
                    .submit_action(
                        eligible,
                        decision.id().clone(),
                        decision.default_action_id(eligible).clone(),
                    )
                    .unwrap();
            }
            if !pending || !decision.actions_for(seat).is_empty() {
                let current = machine.current_decision().unwrap().expect("decision");
                if current.actions_for(seat).is_empty() {
                    continue;
                }
                if let Some(duration) = current.duration_for(seat) {
                    time::advance(duration).await;
                }
                machine.resolve_expired().unwrap();
            }
        }
        panic!("temporary auto seat did not receive a subsequent turn");
    }
}

#[tokio::test(start_paused = true)]
async fn temporary_auto_response_is_immediate_after_casual_or_riichi_dev_timeout() {
    let mode = GameMode::FourPlayerRedEast;
    for time_control in [TimeControl::Casual, TimeControl::RiichiDev] {
        let mut machine =
            MatchMachine::with_seed(mode, roster(mode, ParticipantKind::Human), 42).unwrap();
        machine.set_time_control(time_control);

        let first = machine.current_decision().unwrap().unwrap();
        let seat = first.eligible().next().expect("initial turn");
        let initial_duration = first.duration_for(seat).expect("turn deadline");
        machine.disconnect(seat).unwrap();
        time::advance(initial_duration).await;
        machine.resolve_expired().unwrap().expect("initial timeout");
        assert_eq!(
            machine.controller(seat).unwrap(),
            ControllerState::TemporaryAuto
        );

        let received_response = loop {
            let Some(decision) = machine.current_decision().unwrap() else {
                break false;
            };
            if decision.kind() == DecisionKind::Response && !decision.actions_for(seat).is_empty() {
                assert_eq!(decision.duration_for(seat), Some(Duration::ZERO));
                break true;
            }
            for eligible in decision.eligible().collect::<Vec<_>>() {
                if eligible == seat {
                    continue;
                }
                machine
                    .submit_action(
                        eligible,
                        decision.id().clone(),
                        decision.default_action_id(eligible).clone(),
                    )
                    .unwrap();
            }
            let Some(current) = machine.current_decision().unwrap() else {
                break false;
            };
            if !current.actions_for(seat).is_empty() {
                if let Some(duration) = current.duration_for(seat) {
                    time::advance(duration).await;
                }
                machine.resolve_expired().unwrap();
            }
        };
        assert!(
            received_response,
            "temporary auto seat did not receive a subsequent response"
        );
    }
}

#[test]
fn permanent_auto_remains_immediate_in_casual_and_riichi_dev() {
    let mode = GameMode::FourPlayerRedEast;
    for time_control in [TimeControl::Casual, TimeControl::RiichiDev] {
        let mut machine = MatchMachine::with_time_control(
            mode,
            roster(mode, ParticipantKind::BuiltInBot),
            time_control,
        )
        .unwrap();
        let decision = machine.current_decision().unwrap().unwrap();
        assert!(
            decision
                .eligible()
                .all(|seat| decision.duration_for(seat) == Some(Duration::ZERO))
        );
    }
}

#[test]
fn seat_validation_remains_mode_specific() {
    let mode = GameMode::ThreePlayerRedEast;
    let mut machine = MatchMachine::new(mode, roster(mode, ParticipantKind::Human)).unwrap();
    assert!(machine.disconnect(Seat::new(3).unwrap()).is_err());
}
