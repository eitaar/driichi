use std::collections::HashSet;

use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use thiserror::Error;

use crate::{
    domain::{
        GameAction, GameEvent, GameMode, MatchAbort, MatchPlayerResult, MatchResult, MatchStatus,
        Participant, ParticipantId, Seat,
    },
    engine::{EngineAdapter, EngineError},
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MatchError {
    #[error("invalid participant roster: {0}")]
    InvalidParticipants(String),
    #[error("seat {0} is not part of this match")]
    InvalidSeat(Seat),
    #[error("action is not legal for seat {seat}")]
    IllegalAction { seat: Seat },
    #[error("match has already finished")]
    Finished,
    #[error("match aborted: {0}")]
    Aborted(String),
}

pub struct MatchMachine {
    mode: GameMode,
    players: Vec<Participant>,
    engine: EngineAdapter,
    status: MatchStatus,
    events: Vec<GameEvent>,
}

impl MatchMachine {
    pub fn new(mode: GameMode, participants: Vec<Participant>) -> Result<Self, MatchError> {
        Self::new_internal(mode, participants, rand::random())
    }

    pub fn mode(&self) -> GameMode {
        self.mode
    }

    pub fn players(&self) -> &[Participant] {
        &self.players
    }

    pub fn participant(&self, seat: Seat) -> Option<&Participant> {
        self.players.get(seat.index() as usize)
    }

    pub fn status(&self) -> &MatchStatus {
        &self.status
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.status, MatchStatus::Completed(_))
    }

    pub fn result(&self) -> Option<&MatchResult> {
        match &self.status {
            MatchStatus::Completed(result) => Some(result),
            _ => None,
        }
    }

    pub fn abort(&self) -> Option<&MatchAbort> {
        match &self.status {
            MatchStatus::Aborted(abort) => Some(abort),
            _ => None,
        }
    }

    pub fn events(&self) -> &[GameEvent] {
        &self.events
    }

    pub fn legal_actions(&mut self, seat: Seat) -> Result<Vec<GameAction>, MatchError> {
        self.ensure_running()?;
        self.validate_seat(seat)?;
        self.engine
            .legal_actions(seat)
            .map_err(|error| self.abort_for_engine(error))
    }

    pub fn apply(&mut self, seat: Seat, action: GameAction) -> Result<Vec<GameEvent>, MatchError> {
        self.ensure_running()?;
        self.validate_seat(seat)?;
        let action = action.canonicalize();
        let legal = self
            .engine
            .legal_actions(seat)
            .map_err(|error| self.abort_for_engine(error))?;
        if !legal.contains(&action) {
            return Err(MatchError::IllegalAction { seat });
        }
        let new_events = match self.engine.apply(seat, &action) {
            Ok(events) => events,
            Err(error @ EngineError::Rejected(_)) => return Err(self.abort_for_engine(error)),
            Err(error) => return Err(self.abort_for_engine(error)),
        };
        self.events.extend(new_events.iter().cloned());
        if self.engine.is_done() {
            self.status = MatchStatus::Completed(self.build_result());
        }
        Ok(new_events)
    }

    fn new_internal(
        mode: GameMode,
        participants: Vec<Participant>,
        seed: u64,
    ) -> Result<Self, MatchError> {
        validate_participants(mode, &participants)?;
        let mut players = participants;
        let mut rng = StdRng::seed_from_u64(seed);
        players.shuffle(&mut rng);
        let (engine, events) = EngineAdapter::new(mode, Some(seed))
            .map_err(|error| MatchError::Aborted(error.to_string()))?;
        Ok(Self {
            mode,
            players,
            engine,
            status: MatchStatus::Running,
            events,
        })
    }

    #[cfg(test)]
    fn new_with_seed(
        mode: GameMode,
        participants: Vec<Participant>,
        seed: u64,
    ) -> Result<Self, MatchError> {
        Self::new_internal(mode, participants, seed)
    }

    fn ensure_running(&self) -> Result<(), MatchError> {
        if matches!(self.status, MatchStatus::Running) {
            Ok(())
        } else {
            Err(match &self.status {
                MatchStatus::Aborted(abort) => MatchError::Aborted(abort.reason.clone()),
                MatchStatus::Completed(_) => MatchError::Finished,
                MatchStatus::Running => unreachable!(),
            })
        }
    }

    fn validate_seat(&self, seat: Seat) -> Result<(), MatchError> {
        if usize::from(seat.index()) < self.mode.seat_count() {
            Ok(())
        } else {
            Err(MatchError::InvalidSeat(seat))
        }
    }

    fn abort_for_engine(&mut self, error: EngineError) -> MatchError {
        let reason = error.to_string();
        self.status = MatchStatus::Aborted(MatchAbort::new(reason.clone()));
        MatchError::Aborted(reason)
    }

    fn build_result(&self) -> MatchResult {
        let scores = self.engine.scores();
        let mut seats: Vec<usize> = (0..scores.len()).collect();
        seats.sort_by_key(|&seat| (std::cmp::Reverse(scores[seat]), seat));
        let mut ranks = vec![0u8; seats.len()];
        for (rank, seat) in seats.into_iter().enumerate() {
            ranks[seat] = rank as u8 + 1;
        }
        let players = self
            .players
            .iter()
            .enumerate()
            .map(|(seat, participant)| MatchPlayerResult {
                participant_id: participant.id.clone(),
                display_name: participant.display_name.clone(),
                kind: participant.kind,
                seat: Seat::new(seat as u8).expect("validated participant roster"),
                final_score: scores[seat],
                rank: ranks[seat],
            })
            .collect();
        MatchResult {
            mode: self.mode,
            players,
            final_scores: scores,
        }
    }
}

fn validate_participants(mode: GameMode, participants: &[Participant]) -> Result<(), MatchError> {
    if participants.len() != mode.seat_count() {
        return Err(MatchError::InvalidParticipants(format!(
            "{} requires {} participants, got {}",
            mode,
            mode.seat_count(),
            participants.len()
        )));
    }
    let mut ids = HashSet::<&ParticipantId>::new();
    for participant in participants {
        if participant.display_name.trim().is_empty() {
            return Err(MatchError::InvalidParticipants(
                "display name must not be empty".into(),
            ));
        }
        if !ids.insert(&participant.id) {
            return Err(MatchError::InvalidParticipants(format!(
                "duplicate participant {}",
                participant.id
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GameAction, GameEvent, ParticipantKind};

    fn roster(mode: GameMode) -> Vec<Participant> {
        (0..mode.seat_count())
            .map(|index| {
                Participant::new(
                    format!("p{index}"),
                    format!("Player {index}"),
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect()
    }

    fn default_action(actions: &[GameAction]) -> GameAction {
        actions
            .iter()
            .find(|action| matches!(action, GameAction::Discard { .. }))
            .or_else(|| {
                actions
                    .iter()
                    .find(|action| matches!(action, GameAction::Pass))
            })
            .or_else(|| {
                actions
                    .iter()
                    .find(|action| matches!(action, GameAction::Nuki { .. }))
            })
            .or_else(|| actions.first())
            .expect("a running engine always exposes an action")
            .clone()
    }

    fn complete(mode: GameMode) -> MatchMachine {
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0_u64).unwrap();
        for _ in 0..20_000 {
            if machine.is_complete() {
                return machine;
            }
            let mut progressed = false;
            for seat in Seat::all(mode) {
                let actions = machine.legal_actions(seat).unwrap();
                if actions.is_empty() {
                    continue;
                }
                let action = default_action(&actions);
                machine.apply(seat, action).unwrap();
                progressed = true;
                break;
            }
            assert!(progressed, "a running match must expose a decision");
        }
        panic!("mode {mode} did not complete")
    }

    #[test]
    fn all_four_presets_complete_with_a_test_only_seed() {
        for mode in GameMode::all() {
            let machine = complete(mode);
            let result = machine.result().expect("completed match result");
            assert_eq!(result.mode, mode);
            assert_eq!(result.players.len(), mode.seat_count());
            assert_eq!(result.final_scores.len(), mode.seat_count());
            assert!(
                machine
                    .events()
                    .iter()
                    .any(|event| matches!(event, GameEvent::EndGame))
            );
        }
    }

    #[test]
    fn adapter_divergence_aborts_without_a_result() {
        let mode = GameMode::FourPlayerRedEast;
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0_u64).unwrap();
        let seat = Seat::all(mode)[0];
        let action = machine
            .legal_actions(seat)
            .unwrap()
            .into_iter()
            .next()
            .expect("initial decision has an action");
        machine.engine.force_event_divergence_for_test();

        assert!(matches!(
            machine.apply(seat, action),
            Err(MatchError::Aborted(_))
        ));
        assert!(matches!(machine.status(), MatchStatus::Aborted(_)));
        assert!(machine.result().is_none());
    }
}
