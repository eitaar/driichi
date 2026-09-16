use std::{collections::HashSet, time::Duration};

use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use thiserror::Error;
use tokio::time::Instant;

use crate::{
    decision::{
        ActionId, ControllerState, Decision, DecisionError, DecisionId, DecisionKind,
        DecisionResolution, DecisionSubmission, Presence, ResolvedAction, TimeControl,
        TimingConfig, TimingError,
    },
    domain::{
        GameAction, GameEvent, GameMode, MatchAbort, MatchPlayerResult, MatchResult, MatchStatus,
        Participant, ParticipantId, ParticipantKind, Seat,
    },
    engine::{EngineAdapter, EngineError},
    projection::{
        Audience, AudienceProjection, ProjectionError, project_table_state, serialize_projection,
    },
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MatchError {
    #[error("invalid participant roster: {0}")]
    InvalidParticipants(String),
    #[error("seat {0} is not part of this match")]
    InvalidSeat(Seat),
    #[error("action is not legal for seat {seat}")]
    IllegalAction { seat: Seat },
    #[error("no open decision")]
    NoDecision,
    #[error("decision error: {0}")]
    Decision(DecisionError),
    #[error("timing error: {0}")]
    Timing(TimingError),
    #[error("projection error: {0}")]
    Projection(ProjectionError),
    #[error("match has already finished")]
    Finished,
    #[error("match aborted: {0}")]
    Aborted(String),
}

impl From<DecisionError> for MatchError {
    fn from(error: DecisionError) -> Self {
        Self::Decision(error)
    }
}

impl From<TimingError> for MatchError {
    fn from(error: TimingError) -> Self {
        Self::Timing(error)
    }
}

impl From<ProjectionError> for MatchError {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionResult {
    Waiting {
        decision_id: DecisionId,
    },
    Resolved {
        decision_id: DecisionId,
        actions: Vec<ResolvedAction>,
        events: Vec<GameEvent>,
    },
}

impl DecisionResult {
    pub const fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved { .. })
    }

    pub fn events(&self) -> &[GameEvent] {
        match self {
            Self::Waiting { .. } => &[],
            Self::Resolved { events, .. } => events,
        }
    }
}

pub struct MatchMachine {
    mode: GameMode,
    players: Vec<Participant>,
    engine: EngineAdapter,
    status: MatchStatus,
    events: Vec<GameEvent>,
    time_control: TimeControl,
    timing: TimingConfig,
    presence: Vec<Presence>,
    controllers: Vec<ControllerState>,
    next_decision_id: u64,
    next_action_id: u64,
    decision: Option<Decision>,
}

impl MatchMachine {
    pub fn new(mode: GameMode, participants: Vec<Participant>) -> Result<Self, MatchError> {
        Self::new_internal(mode, participants, rand::random())
    }

    pub fn with_time_control(
        mode: GameMode,
        participants: Vec<Participant>,
        time_control: TimeControl,
    ) -> Result<Self, MatchError> {
        let mut machine = Self::new(mode, participants)?;
        machine.time_control = time_control;
        machine.timing = TimingConfig::casual();
        Ok(machine)
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

    pub fn time_control(&self) -> TimeControl {
        self.time_control
    }

    pub fn timing(&self) -> TimingConfig {
        self.timing
    }

    pub fn set_time_control(&mut self, time_control: TimeControl) {
        self.time_control = time_control;
        self.decision = None;
    }

    pub fn set_timing(&mut self, timing: TimingConfig) {
        self.timing = timing;
        self.decision = None;
    }

    pub fn presence(&self, seat: Seat) -> Result<Presence, MatchError> {
        self.validate_seat(seat)?;
        Ok(self.presence[seat.index() as usize])
    }

    pub fn controller(&self, seat: Seat) -> Result<ControllerState, MatchError> {
        self.validate_seat(seat)?;
        Ok(self.controllers[seat.index() as usize])
    }

    pub fn disconnect(&mut self, seat: Seat) -> Result<(), MatchError> {
        self.validate_seat(seat)?;
        let index = seat.index() as usize;
        self.presence[index] = Presence::Disconnected;
        if self.time_control == TimeControl::Unlimited
            && self.players[index].kind == ParticipantKind::Human
            && self.controllers[index] == ControllerState::Interactive
            && self.decision.as_ref().is_some_and(|decision| {
                !decision.actions_for(seat).is_empty() && decision.deadline_for(seat).is_none()
            })
        {
            if let Some(decision) = &mut self.decision {
                decision.retime_for(seat, Instant::now(), Some(self.timing.watchdog), true);
            }
        }
        Ok(())
    }

    pub fn reconnect(&mut self, seat: Seat) -> Result<(), MatchError> {
        self.validate_seat(seat)?;
        let index = seat.index() as usize;
        self.presence[index] = Presence::Connected;
        if self.controllers[index] == ControllerState::TemporaryAuto {
            self.controllers[index] = ControllerState::Interactive;
            if let Some(decision) = &self.decision {
                if !decision.actions_for(seat).is_empty() {
                    let (duration, watchdog) = self.decision_timing_for(decision.kind(), seat);
                    if let Some(decision) = &mut self.decision {
                        decision.retime_for(seat, Instant::now(), duration, watchdog);
                    }
                }
            }
        } else if self.time_control == TimeControl::Unlimited
            && self.players[index].kind == ParticipantKind::Human
            && self.decision.as_ref().is_some_and(|decision| {
                decision.is_watchdog_for(seat) && !decision.actions_for(seat).is_empty()
            })
        {
            let (duration, watchdog) = self
                .decision
                .as_ref()
                .map(|decision| self.decision_timing_for(decision.kind(), seat))
                .expect("watchdog check above guarantees a decision");
            if let Some(decision) = &mut self.decision {
                decision.retime_for(seat, Instant::now(), duration, watchdog);
            }
        }
        Ok(())
    }

    pub fn set_permanent_auto(&mut self, seat: Seat) -> Result<(), MatchError> {
        self.validate_seat(seat)?;
        self.controllers[seat.index() as usize] = ControllerState::PermanentAuto;
        self.decision = None;
        Ok(())
    }

    pub fn legal_actions(&mut self, seat: Seat) -> Result<Vec<GameAction>, MatchError> {
        self.ensure_running()?;
        self.validate_seat(seat)?;
        self.engine
            .legal_actions(seat)
            .map_err(|error| self.abort_for_engine(error))
    }

    /// Compatibility path for the engine-facing Task 3 API. New callers should
    /// use `current_decision` and submit its ephemeral action ID.
    pub fn apply(&mut self, seat: Seat, action: GameAction) -> Result<Vec<GameEvent>, MatchError> {
        self.ensure_running()?;
        self.validate_seat(seat)?;
        self.decision = None;
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
        self.record_events(new_events)
    }

    /// Open the one canonical Decision for the current engine state. A
    /// response Decision remains open until every eligible seat has submitted.
    pub fn current_decision(&mut self) -> Result<Option<Decision>, MatchError> {
        self.ensure_running()?;
        if let Some(decision) = &self.decision {
            return Ok(Some(decision.clone()));
        }

        let mut options = Vec::new();
        for seat in Seat::all(self.mode) {
            let actions = self
                .engine
                .legal_actions(seat)
                .map_err(|error| self.abort_for_engine(error))?;
            if !actions.is_empty() {
                options.push((seat, actions));
            }
        }
        if options.is_empty() {
            return Ok(None);
        }
        let kind = if options
            .iter()
            .any(|(_, actions)| actions.iter().any(is_response_action))
        {
            DecisionKind::Response
        } else {
            DecisionKind::Turn
        };
        let decision_id = DecisionId::new(format!("d{}", self.next_decision_id));
        self.next_decision_id += 1;
        let options = options
            .into_iter()
            .map(|(seat, actions)| {
                let actions = actions
                    .into_iter()
                    .map(|action| {
                        let id = ActionId::new(format!("a{}", self.next_action_id));
                        self.next_action_id += 1;
                        (id, action)
                    })
                    .collect();
                let (duration, watchdog) = self.decision_timing_for(kind, seat);
                (seat, actions, duration, watchdog)
            })
            .collect();
        let decision = match Decision::from_action_ids_with_timings(
            decision_id,
            kind,
            options,
            Instant::now(),
        ) {
            Ok(decision) => decision,
            Err(error) => {
                let reason = error.to_string();
                self.status = MatchStatus::Aborted(MatchAbort::new(reason.clone()));
                return Err(MatchError::Aborted(reason));
            }
        };
        self.decision = Some(decision.clone());
        Ok(Some(decision))
    }

    pub fn decision(&mut self) -> Result<Option<Decision>, MatchError> {
        self.current_decision()
    }

    pub fn open_decision(&mut self) -> Result<Option<Decision>, MatchError> {
        self.current_decision()
    }

    pub fn submit_action(
        &mut self,
        seat: Seat,
        decision_id: impl Into<DecisionId>,
        action_id: impl Into<ActionId>,
    ) -> Result<DecisionResult, MatchError> {
        self.ensure_running()?;
        self.validate_seat(seat)?;
        let decision_id = decision_id.into();
        let action_id = action_id.into();
        if self.decision.is_none() {
            self.current_decision()?;
        }
        let decision = self.decision.as_mut().ok_or(MatchError::NoDecision)?;
        let submission = decision.submit_with_decision_id(&decision_id, seat, action_id)?;
        match submission {
            DecisionSubmission::Accepted { .. } => Ok(DecisionResult::Waiting { decision_id }),
            DecisionSubmission::Resolved(resolution) => self.resolve_actions(resolution),
        }
    }

    pub fn submit(
        &mut self,
        decision_id: impl Into<DecisionId>,
        action_id: impl Into<ActionId>,
        seat: Seat,
    ) -> Result<DecisionResult, MatchError> {
        self.submit_action(seat, decision_id, action_id)
    }

    pub fn resolve_expired(&mut self) -> Result<Option<DecisionResult>, MatchError> {
        self.ensure_running()?;
        if self.decision.is_none() {
            self.current_decision()?;
        }
        let Some(decision) = self.decision.as_mut() else {
            return Ok(None);
        };
        let resolution = decision.resolve_at(Instant::now())?;
        let timed_out: Vec<Seat> = decision
            .eligible()
            .filter(|seat| decision.timed_out(*seat))
            .collect();
        for seat in timed_out {
            let index = seat.index() as usize;
            if self.presence[index] == Presence::Disconnected
                && self.controllers[index] == ControllerState::Interactive
            {
                self.controllers[index] = ControllerState::TemporaryAuto;
            }
        }
        match resolution {
            Some(resolution) => Ok(Some(self.resolve_actions(resolution)?)),
            None => Ok(None),
        }
    }

    pub fn resolve_timeouts(&mut self) -> Result<Option<DecisionResult>, MatchError> {
        self.resolve_expired()
    }

    pub fn project(&mut self, audience: Audience) -> Result<AudienceProjection, MatchError> {
        if matches!(self.status, MatchStatus::Aborted(_)) {
            return Err(MatchError::Aborted(
                self.abort().expect("aborted status").reason.clone(),
            ));
        }
        let decision = if matches!(self.status, MatchStatus::Running) {
            self.current_decision()?
        } else {
            None
        };
        let state = self
            .engine
            .table_state(self.mode, &self.players)
            .map_err(|error| self.abort_for_engine(error))?;
        let state = match decision {
            Some(decision) => state.with_decision(decision),
            None => state,
        };
        Ok(project_table_state(&state, audience))
    }

    pub fn serialize(&mut self, audience: Audience) -> Result<String, MatchError> {
        let projection = self.project(audience)?;
        serialize_projection(&projection).map_err(|error| MatchError::Aborted(error.to_string()))
    }

    fn resolve_actions(
        &mut self,
        resolution: DecisionResolution,
    ) -> Result<DecisionResult, MatchError> {
        let decision_id = resolution.decision_id.clone();
        let actions: Vec<(Seat, GameAction)> = resolution
            .actions
            .iter()
            .map(|action| (action.seat, action.action.clone()))
            .collect();
        self.decision = None;
        let new_events = if actions.len() == 1 {
            self.engine
                .apply(actions[0].0, &actions[0].1)
                .map_err(|error| self.abort_for_engine(error))?
        } else {
            self.engine
                .apply_simultaneous(&actions)
                .map_err(|error| self.abort_for_engine(error))?
        };
        self.record_events(new_events.clone())?;
        Ok(DecisionResult::Resolved {
            decision_id,
            actions: resolution.actions,
            events: new_events,
        })
    }

    fn record_events(&mut self, new_events: Vec<GameEvent>) -> Result<Vec<GameEvent>, MatchError> {
        self.events.extend(new_events.iter().cloned());
        if self.engine.is_done() {
            self.status = MatchStatus::Completed(self.build_result());
        }
        Ok(new_events)
    }

    fn decision_timing_for(&self, kind: DecisionKind, seat: Seat) -> (Option<Duration>, bool) {
        match self.time_control {
            TimeControl::Casual => (
                Some(match kind {
                    DecisionKind::Turn => self.time_control.turn_duration(),
                    DecisionKind::Response => self.time_control.response_duration(),
                }),
                false,
            ),
            TimeControl::RiichiDev => (
                Some(match kind {
                    DecisionKind::Turn => self.timing.turn,
                    DecisionKind::Response => self.timing.response,
                }),
                false,
            ),
            TimeControl::Unlimited => {
                let index = seat.index() as usize;
                match self.controllers[index] {
                    ControllerState::PermanentAuto | ControllerState::TemporaryAuto => {
                        (Some(Duration::ZERO), false)
                    }
                    ControllerState::Interactive => match self.players[index].kind {
                        ParticipantKind::BuiltInBot => (Some(Duration::ZERO), false),
                        ParticipantKind::Human if self.presence[index] == Presence::Connected => {
                            (None, false)
                        }
                        ParticipantKind::Human | ParticipantKind::MJAI | ParticipantKind::MCP => {
                            (Some(self.timing.watchdog), true)
                        }
                    },
                }
            }
        }
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
        let controllers = players
            .iter()
            .map(|participant| {
                if participant.kind == ParticipantKind::BuiltInBot {
                    ControllerState::PermanentAuto
                } else {
                    ControllerState::Interactive
                }
            })
            .collect();
        Ok(Self {
            mode,
            presence: vec![Presence::Connected; mode.seat_count()],
            players,
            engine,
            status: MatchStatus::Running,
            events,
            time_control: TimeControl::Casual,
            timing: TimingConfig::casual(),
            controllers,
            next_decision_id: 1,
            next_action_id: 1,
            decision: None,
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

fn is_response_action(action: &GameAction) -> bool {
    matches!(
        action,
        GameAction::Chi { .. }
            | GameAction::Pon { .. }
            | GameAction::Daiminkan { .. }
            | GameAction::Ron(_)
            | GameAction::Pass
    )
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
    fn decision_boundary_waits_for_all_simultaneous_responses() {
        let mode = GameMode::FourPlayerRedEast;
        let participants = (0..mode.seat_count())
            .map(|index| {
                Participant::new(
                    format!("p{index}"),
                    format!("Player {index}"),
                    ParticipantKind::Human,
                )
            })
            .collect();
        let mut machine = MatchMachine::new_with_seed(mode, participants, 0xD0_u64).unwrap();
        for _ in 0..2_000 {
            let decision = machine.current_decision().unwrap().expect("decision");
            let seats: Vec<_> = decision.eligible().collect();
            if decision.kind() == DecisionKind::Response && seats.len() > 1 {
                let mut result = None;
                for (index, seat) in seats.iter().enumerate() {
                    let action = decision.default_action_id(*seat).clone();
                    let submission = machine
                        .submit_action(*seat, decision.id().clone(), action)
                        .unwrap();
                    if index + 1 < seats.len() {
                        assert!(!submission.is_resolved());
                    } else {
                        assert!(submission.is_resolved());
                    }
                    result = Some(submission);
                }
                assert!(result.expect("last response").is_resolved());
                return;
            }
            let seat = seats[0];
            let action = decision.default_action_id(seat).clone();
            let result = machine
                .submit_action(seat, decision.id().clone(), action)
                .unwrap();
            assert!(result.is_resolved());
        }
        panic!("seed did not expose a simultaneous response window")
    }

    #[test]
    fn unlimited_mixed_response_keeps_connected_human_without_deadline() {
        let mode = GameMode::FourPlayerRedEast;
        let participants = vec![
            Participant::new("human0", "Human 0", ParticipantKind::Human),
            Participant::new("human1", "Human 1", ParticipantKind::Human),
            Participant::new("bot0", "Bot 0", ParticipantKind::BuiltInBot),
            Participant::new("bot1", "Bot 1", ParticipantKind::BuiltInBot),
        ];
        for seed in 0..128 {
            let mut machine =
                MatchMachine::new_with_seed(mode, participants.clone(), seed).unwrap();
            machine.set_time_control(TimeControl::Unlimited);
            for _ in 0..2_000 {
                machine.resolve_expired().unwrap();
                machine.current_decision().unwrap().expect("decision");
                machine.resolve_expired().unwrap();
                let decision = machine.current_decision().unwrap().expect("decision");
                let seats: Vec<_> = decision.eligible().collect();
                let human = seats.iter().copied().find(|seat| {
                    machine.participant(*seat).expect("seat participant").kind
                        == ParticipantKind::Human
                });
                let bot = seats.iter().copied().find(|seat| {
                    machine.participant(*seat).expect("seat participant").kind
                        == ParticipantKind::BuiltInBot
                });
                if let (Some(human), Some(bot)) = (human, bot) {
                    assert_eq!(decision.duration_for(human), None);
                    assert_eq!(decision.duration_for(bot), Some(Duration::ZERO));
                    return;
                }
                for seat in seats {
                    if decision.submitted_action_id(seat).is_some() {
                        continue;
                    }
                    if decision.is_expired_for(seat, Instant::now()) {
                        machine.resolve_expired().unwrap();
                        break;
                    }
                    let action = decision.default_action_id(seat).clone();
                    machine
                        .submit_action(seat, decision.id().clone(), action)
                        .unwrap();
                }
            }
        }
        panic!("seed did not expose a mixed response window")
    }

    #[tokio::test(start_paused = true)]
    async fn partial_timeout_promotes_disconnected_seat_and_preserves_timeout_marker() {
        let mode = GameMode::FourPlayerRedEast;
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0_u64).unwrap();
        let disconnected = Seat::new(0).unwrap();
        let human = Seat::new(1).unwrap();
        machine.players[disconnected.index() as usize].kind = ParticipantKind::Human;
        machine.players[human.index() as usize].kind = ParticipantKind::Human;
        machine.presence[disconnected.index() as usize] = Presence::Disconnected;
        machine.controllers[disconnected.index() as usize] = ControllerState::Interactive;
        machine.controllers[human.index() as usize] = ControllerState::Interactive;
        let decision = Decision::new_with_timings(
            DecisionId::new("partial-timeout"),
            DecisionKind::Response,
            vec![
                (
                    disconnected,
                    vec![GameAction::Pass],
                    Some(Duration::ZERO),
                    true,
                ),
                (human, vec![GameAction::Pass], None, false),
            ],
            Instant::now(),
        )
        .unwrap();
        machine.decision = Some(decision);

        assert!(machine.resolve_expired().unwrap().is_none());
        assert_eq!(
            machine.controller(disconnected).unwrap(),
            ControllerState::TemporaryAuto
        );
        assert!(
            machine
                .decision
                .as_ref()
                .expect("pending decision")
                .timed_out(disconnected)
        );
    }

    #[test]
    fn reconnect_preserves_pending_decision_responses() {
        let mode = GameMode::FourPlayerRedEast;
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0_u64).unwrap();
        let reconnecting = Seat::new(0).unwrap();
        let other = Seat::new(1).unwrap();
        let decision = Decision::new(
            DecisionId::new("pending"),
            DecisionKind::Response,
            vec![
                (reconnecting, vec![GameAction::Pass]),
                (other, vec![GameAction::Pass]),
            ],
            Instant::now(),
            Some(Duration::from_secs(10)),
            false,
        )
        .unwrap();
        let decision_id = decision.id().clone();
        let other_action = decision.actions_for(other)[0].id.clone();
        machine.decision = Some(decision);
        machine.presence[reconnecting.index() as usize] = Presence::Disconnected;
        machine.controllers[reconnecting.index() as usize] = ControllerState::TemporaryAuto;
        assert!(
            !machine
                .submit_action(other, decision_id.clone(), other_action)
                .unwrap()
                .is_resolved()
        );

        machine.reconnect(reconnecting).unwrap();
        let current = machine.current_decision().unwrap().unwrap();
        assert_eq!(current.id(), &decision_id);
        assert!(current.submitted_action_id(other).is_some());
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
