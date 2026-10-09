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
    engine::{EngineAdapter, EngineApplyOutcome, EngineError},
    projection::{
        Audience, AudienceProjection, ProjectionError, TableState, project_table_state,
        serialize_projection,
    },
    results::{KyokuResultRecord, PublishedKyokuResults},
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
    #[error("kyoku results are pending publication")]
    ResultsPending,
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

struct HeldKyoku {
    table: TableState,
    record: KyokuResultRecord,
    next_step: usize,
    settlement_published: bool,
    terminal_events: Vec<GameEvent>,
    future_events: Vec<GameEvent>,
    final_result: Option<MatchResult>,
}

pub struct MatchMachine {
    mode: GameMode,
    players: Vec<Participant>,
    engine: EngineAdapter,
    status: MatchStatus,
    events: Vec<GameEvent>,
    results: Vec<KyokuResultRecord>,
    held: Option<HeldKyoku>,
    journal_event_count: usize,
    persistence_events: Vec<GameEvent>,
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
        Self::new_internal(mode, participants, rand::random(), true)
    }

    pub fn with_seed(
        mode: GameMode,
        participants: Vec<Participant>,
        seed: u64,
    ) -> Result<Self, MatchError> {
        Self::new_internal(mode, participants, seed, true)
    }

    /// Preserve caller-assigned seats while dealing independently for each Match.
    pub fn with_fixed_seats(
        mode: GameMode,
        participants: Vec<Participant>,
        time_control: TimeControl,
    ) -> Result<Self, MatchError> {
        let mut machine = Self::new_internal(mode, participants, rand::random(), false)?;
        machine.time_control = time_control;
        Ok(machine)
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

    #[cfg(feature = "task8-test-support")]
    pub fn with_task8_fixture(
        mode: GameMode,
        participants: Vec<Participant>,
        time_control: TimeControl,
        fixture: &str,
    ) -> Result<Self, MatchError> {
        let (engine, events) = match fixture {
            "double-ron" => EngineAdapter::task8_double_ron(mode),
            "double-ron-final" => EngineAdapter::task8_double_ron_final(mode),
            "dealer-tenhou" => EngineAdapter::task8_dealer_tenhou(mode),
            "final-draw" => EngineAdapter::task8_final_draw(mode),
            _ => return Err(MatchError::Aborted("unknown Task8 fixture".into())),
        }
        .map_err(|error| MatchError::Aborted(error.to_string()))?;
        let mut machine = Self::new_internal(mode, participants, 230, false)?;
        machine.engine = engine;
        machine.journal_event_count = events.len();
        machine.events = events;
        machine.time_control = time_control;
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

    #[allow(dead_code)] // Task 4/6 persistence consumes the complete private record.
    pub(crate) fn kyoku_results(&self) -> &[KyokuResultRecord] {
        &self.results
    }

    pub fn held_kyoku(&self) -> Option<&KyokuResultRecord> {
        self.held.as_ref().map(|hold| &hold.record)
    }

    pub(crate) fn published_kyoku_results(&self) -> Option<PublishedKyokuResults> {
        let hold = self.held.as_ref()?;
        (hold.next_step > 0).then(|| PublishedKyokuResults {
            kyoku_sequence: hold.record.kyoku_sequence,
            wins: hold.record.wins[..hold.next_step.min(hold.record.wins.len())].to_vec(),
            settlement: hold
                .settlement_published
                .then(|| hold.record.settlement.clone()),
        })
    }

    #[allow(dead_code)] // Task 4 connects the private persistence effect consumer.
    pub(crate) fn take_persistence_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.persistence_events)
    }

    pub fn publish_kyoku_step(&mut self, step_index: usize) -> Result<Vec<GameEvent>, MatchError> {
        let hold = self.held.as_mut().ok_or(MatchError::NoDecision)?;
        if step_index != hold.next_step || step_index > hold.record.wins.len() {
            return Err(MatchError::ResultsPending);
        }
        hold.next_step += 1;
        if step_index == hold.record.wins.len() {
            hold.settlement_published = true;
            let after = hold
                .record
                .settlement
                .changes
                .iter()
                .map(|change| change.after)
                .collect::<Vec<_>>();
            hold.table = hold
                .table
                .clone()
                .with_settlement_state(&after, hold.record.settlement.pot_after);
            let events = std::mem::take(&mut hold.terminal_events);
            self.events.extend(events.iter().cloned());
            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn release_kyoku(&mut self) -> Result<Vec<GameEvent>, MatchError> {
        if !self
            .held
            .as_ref()
            .is_some_and(|hold| hold.settlement_published)
        {
            return Err(MatchError::ResultsPending);
        }
        let hold = self.held.take().expect("checked hold");
        self.events.extend(hold.future_events.iter().cloned());
        if let Some(result) = hold.final_result {
            self.status = MatchStatus::Completed(result);
        }
        Ok(hold.future_events)
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
            && let Some(decision) = &mut self.decision
        {
            decision.retime_for(seat, Instant::now(), Some(self.timing.watchdog), true);
        }
        Ok(())
    }

    pub fn reconnect(&mut self, seat: Seat) -> Result<(), MatchError> {
        self.validate_seat(seat)?;
        let index = seat.index() as usize;
        self.presence[index] = Presence::Connected;
        if self.controllers[index] == ControllerState::TemporaryAuto {
            self.controllers[index] = ControllerState::Interactive;
            if let Some(decision) = &self.decision
                && !decision.actions_for(seat).is_empty()
            {
                let (duration, watchdog) = self.decision_timing_for(decision.kind(), seat);
                if let Some(decision) = &mut self.decision {
                    decision.retime_for(seat, Instant::now(), duration, watchdog);
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
        self.validate_seat(seat)?;
        if self.held.is_some() {
            return Ok(Vec::new());
        }
        self.ensure_running()?;
        self.engine
            .legal_actions(seat)
            .map_err(|error| self.abort_for_engine(error))
    }

    /// Compatibility path for the engine-facing Task 3 API. New callers should
    /// use `current_decision` and submit its ephemeral action ID.
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
        self.decision = None;
        let outcome = self
            .engine
            .apply(seat, &action)
            .map_err(|error| self.abort_for_engine(error))?;
        self.record_outcome(outcome)
    }

    /// Open the one canonical Decision for the current engine state. A
    /// response Decision remains open until every eligible seat has submitted.
    pub fn current_decision(&mut self) -> Result<Option<Decision>, MatchError> {
        if self.held.is_some() {
            return Ok(None);
        }
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
        let state = if let Some(hold) = &self.held {
            hold.table.clone()
        } else {
            self.engine
                .table_state(self.mode, &self.players)
                .map_err(|error| self.abort_for_engine(error))?
        };
        let state = match decision {
            Some(decision) => state.with_decision(decision),
            None => state,
        };
        let projection = project_table_state(&state, audience);
        Ok(match self.published_kyoku_results() {
            Some(results) => projection.with_kyoku_results(results),
            None => projection,
        })
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
        let outcome = if actions.len() == 1 {
            self.engine
                .apply(actions[0].0, &actions[0].1)
                .map_err(|error| self.abort_for_engine(error))?
        } else {
            self.engine
                .apply_simultaneous(&actions)
                .map_err(|error| self.abort_for_engine(error))?
        };
        let new_events = self.record_outcome(outcome)?;
        Ok(DecisionResult::Resolved {
            decision_id,
            actions: resolution.actions,
            events: new_events,
        })
    }

    fn record_outcome(
        &mut self,
        outcome: EngineApplyOutcome,
    ) -> Result<Vec<GameEvent>, MatchError> {
        if let Some(ended) = &outcome.ended {
            let sequence = self
                .events
                .iter()
                .filter(|event| matches!(event, GameEvent::StartKyoku { .. }))
                .count() as u64
                + outcome
                    .events
                    .iter()
                    .take_while(|event| !matches!(event, GameEvent::EndKyoku))
                    .filter(|event| matches!(event, GameEvent::StartKyoku { .. }))
                    .count() as u64;
            let record = self
                .engine
                .result_record(
                    ended,
                    &self.players,
                    sequence,
                    self.journal_event_count,
                    &outcome.events,
                )
                .map_err(|error| self.abort_for_engine(error))?;
            let before_scores = record
                .settlement
                .changes
                .iter()
                .map(|change| change.before)
                .collect::<Vec<_>>();
            let table = self
                .engine
                .ended_table_state(
                    ended,
                    &self.players,
                    &before_scores,
                    record.settlement.pot_before,
                )
                .map_err(|error| self.abort_for_engine(error))?;
            let final_result = if self.engine.is_done() {
                let result = self.build_result();
                result
                    .validate()
                    .map_err(|error| self.abort_for_engine(EngineError::Divergence(error)))?;
                if record
                    .settlement
                    .changes
                    .iter()
                    .map(|change| change.after)
                    .collect::<Vec<_>>()
                    != result.final_scores
                {
                    return Err(self.abort_for_engine(EngineError::Divergence(
                        "final scores differ from settlement".into(),
                    )));
                }
                Some(result)
            } else {
                None
            };
            let terminal_start = outcome
                .events
                .iter()
                .position(|event| {
                    matches!(event, GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. })
                })
                .ok_or_else(|| {
                    self.abort_for_engine(EngineError::Divergence(
                        "ended kyoku without terminal event".into(),
                    ))
                })?;
            let end = outcome
                .events
                .iter()
                .position(|event| matches!(event, GameEvent::EndKyoku))
                .ok_or_else(|| {
                    self.abort_for_engine(EngineError::Divergence(
                        "ended kyoku without EndKyoku".into(),
                    ))
                })?;
            if terminal_start >= end {
                return Err(self.abort_for_engine(EngineError::Divergence(
                    "terminal follows EndKyoku".into(),
                )));
            }
            self.results.push(record.clone());
            let public = outcome.events[..terminal_start].to_vec();
            self.journal_event_count += outcome.events.len();
            self.persistence_events
                .extend(outcome.events.iter().cloned());
            self.events.extend(public.iter().cloned());
            self.decision = None;
            self.held = Some(HeldKyoku {
                table,
                record,
                next_step: 0,
                settlement_published: false,
                terminal_events: outcome.events[terminal_start..end].to_vec(),
                future_events: outcome.events[end..].to_vec(),
                final_result,
            });
            return Ok(public);
        }
        self.record_events(outcome.events)
    }

    fn record_events(&mut self, new_events: Vec<GameEvent>) -> Result<Vec<GameEvent>, MatchError> {
        self.journal_event_count += new_events.len();
        self.persistence_events.extend(new_events.iter().cloned());
        self.events.extend(new_events.iter().cloned());
        if self.engine.is_done() {
            let result = self.build_result();
            result
                .validate()
                .map_err(|error| self.abort_for_engine(EngineError::Divergence(error)))?;
            if let Some(last) = self.results.last()
                && last
                    .settlement
                    .changes
                    .iter()
                    .map(|change| change.after)
                    .collect::<Vec<_>>()
                    != result.final_scores
            {
                return Err(self.abort_for_engine(EngineError::Divergence(
                    "final scores differ from settlement".into(),
                )));
            }
            self.status = MatchStatus::Completed(result);
        }
        Ok(new_events)
    }

    fn decision_timing_for(&self, kind: DecisionKind, seat: Seat) -> (Option<Duration>, bool) {
        let index = seat.index() as usize;
        if matches!(
            self.controllers[index],
            ControllerState::TemporaryAuto | ControllerState::PermanentAuto
        ) {
            return (Some(Duration::ZERO), false);
        }
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
            TimeControl::Unlimited => match self.players[index].kind {
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

    fn new_internal(
        mode: GameMode,
        participants: Vec<Participant>,
        seed: u64,
        shuffle_seats: bool,
    ) -> Result<Self, MatchError> {
        validate_participants(mode, &participants)?;
        let mut players = participants;
        if shuffle_seats {
            players.shuffle(&mut StdRng::seed_from_u64(seed));
        }
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
            journal_event_count: events.len(),
            events,
            results: Vec::new(),
            held: None,
            persistence_events: Vec::new(),
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
        Self::new_internal(mode, participants, seed, true)
    }

    fn ensure_running(&self) -> Result<(), MatchError> {
        if self.held.is_some() {
            return Err(MatchError::ResultsPending);
        }
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
        self.held = None;
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
            if let Some(hold) = machine.held_kyoku() {
                let steps = hold.wins.len();
                for step in 0..=steps {
                    machine.publish_kyoku_step(step).unwrap();
                }
                machine.release_kyoku().unwrap();
            }
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

    fn first_held(mode: GameMode) -> MatchMachine {
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0).unwrap();
        for _ in 0..2000 {
            if machine.held_kyoku().is_some() {
                return machine;
            }
            let (seat, action) = Seat::all(mode)
                .into_iter()
                .find_map(|seat| {
                    let actions = machine.legal_actions(seat).unwrap();
                    (!actions.is_empty()).then(|| (seat, default_action(&actions)))
                })
                .expect("running match action");
            machine.apply(seat, action).unwrap();
        }
        panic!("no ended kyoku");
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_final_draw_holds_pot_and_tied_ranks_until_one_release_in_four_modes() {
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            let count = mode.seat_count();
            let humans = (0..count)
                .map(|i| Participant::new(format!("tie-{i}"), "same", ParticipantKind::Human))
                .collect();
            let mut machine = MatchMachine::with_task8_fixture(
                mode,
                humans,
                TimeControl::Unlimited,
                "final-draw",
            )
            .unwrap();
            let mut submitted = 0;
            for _ in 0..280 {
                if machine.held_kyoku().is_some() {
                    break;
                }
                let decision = machine
                    .current_decision()
                    .unwrap()
                    .expect("actual final draw decision");
                for entry in decision.entries() {
                    let action = entry
                        .actions
                        .iter()
                        .find(|a| {
                            matches!(
                                a.action,
                                GameAction::Pass
                                    | GameAction::Discard {
                                        tsumogiri: true,
                                        ..
                                    }
                            )
                        })
                        .unwrap();
                    machine
                        .submit_action(entry.seat, decision.id(), &action.id)
                        .unwrap();
                    submitted += 1;
                    assert!(submitted <= 280);
                }
            }
            let held = machine.held_kyoku().unwrap();
            assert!(held.wins.is_empty());
            assert_eq!(
                held.settlement.draw_reason.as_deref(),
                Some("exhaustive_draw")
            );
            assert_eq!(
                (held.settlement.pot_before, held.settlement.pot_after),
                (2, 0)
            );
            assert!(machine.result().is_none());
            assert!(machine.current_decision().unwrap().is_none());
            assert!(
                !machine
                    .events()
                    .iter()
                    .any(|e| matches!(e, GameEvent::EndKyoku | GameEvent::EndGame))
            );
            machine.publish_kyoku_step(0).unwrap();
            assert!(machine.result().is_none());
            assert!(machine.current_decision().unwrap().is_none());
            let suffix = machine.release_kyoku().unwrap();
            assert_eq!(suffix, vec![GameEvent::EndKyoku, GameEvent::EndGame]);
            let result = machine.result().unwrap();
            let mut scores = vec![100000; count];
            scores[0] = 102000;
            assert_eq!(result.final_scores, scores);
            assert_eq!(
                result.players.iter().map(|p| p.rank).collect::<Vec<_>>(),
                (1..=count as u8).collect::<Vec<_>>()
            );
            assert_eq!(
                result
                    .players
                    .iter()
                    .map(|p| p.seat.index())
                    .collect::<Vec<_>>(),
                (0..count as u8).collect::<Vec<_>>()
            );
            assert!(machine.release_kyoku().is_err());
            assert_eq!(
                machine
                    .events()
                    .iter()
                    .filter(|e| matches!(e, GameEvent::EndGame))
                    .count(),
                1
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_final_double_ron_holds_canonical_end_and_literal_standings_until_release() {
        for (mode, scores, ranks) in [
            (
                GameMode::ThreePlayerRedEast,
                vec![-4_600, 105_000, 102_600],
                vec![3, 1, 2],
            ),
            (
                GameMode::FourPlayerRedEast,
                vec![-4_800, 105_200, 102_600, 100_000],
                vec![4, 1, 2, 3],
            ),
        ] {
            let humans = (0..mode.seat_count())
                .map(|i| Participant::new(format!("p{i}"), "same", ParticipantKind::Human))
                .collect();
            let mut machine = MatchMachine::with_task8_fixture(
                mode,
                humans,
                TimeControl::Unlimited,
                "double-ron-final",
            )
            .unwrap();
            assert_eq!(machine.journal_event_count, 3);
            assert_eq!(machine.events().len(), 3);
            let decision = machine.current_decision().unwrap().unwrap();
            let dealer = Seat::new(0).unwrap();
            let discard = decision.actions_for(dealer).iter().find(|offer| {
                matches!(&offer.action, GameAction::Discard { tile, .. } if tile.id() == 77)
            }).unwrap();
            machine
                .submit_action(dealer, decision.id(), &discard.id)
                .unwrap();
            let response = machine.current_decision().unwrap().unwrap();
            for entry in response.entries() {
                let action = entry
                    .actions
                    .iter()
                    .find(|offer| {
                        if entry.seat.index() <= 2 {
                            matches!(offer.action, GameAction::Ron(_))
                        } else {
                            matches!(offer.action, GameAction::Pass)
                        }
                    })
                    .unwrap();
                machine
                    .submit_action(entry.seat, response.id(), &action.id)
                    .unwrap();
            }
            assert!(machine.held_kyoku().is_some());
            assert!(machine.result().is_none());
            assert!(machine.current_decision().unwrap().is_none());
            assert!(
                !machine
                    .events()
                    .iter()
                    .any(|e| matches!(e, GameEvent::EndGame))
            );
            for step in 0..=2 {
                machine.publish_kyoku_step(step).unwrap();
                assert!(machine.result().is_none());
                assert!(machine.current_decision().unwrap().is_none());
                assert!(
                    !machine
                        .events()
                        .iter()
                        .any(|e| matches!(e, GameEvent::EndGame))
                );
            }
            let suffix = machine.release_kyoku().unwrap();
            assert_eq!(suffix, [GameEvent::EndKyoku, GameEvent::EndGame]);
            let result = machine.result().unwrap();
            assert_eq!(result.final_scores, scores);
            assert_eq!(
                result.players.iter().map(|p| p.rank).collect::<Vec<_>>(),
                ranks
            );
            assert_eq!(
                result
                    .players
                    .iter()
                    .map(|p| p.seat.index())
                    .collect::<Vec<_>>(),
                (0..mode.seat_count() as u8).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn held_results_do_not_open_decision() {
        let mut machine = first_held(GameMode::FourPlayerRedEast);
        let seat = Seat::new(0).unwrap();
        assert!(machine.current_decision().unwrap().is_none());
        assert!(machine.legal_actions(seat).unwrap().is_empty());
        assert_eq!(machine.resolve_expired(), Err(MatchError::ResultsPending));
        assert_eq!(
            machine.apply(seat, GameAction::Pass),
            Err(MatchError::ResultsPending)
        );
        assert_eq!(
            machine.submit_action(seat, "old", "old"),
            Err(MatchError::ResultsPending)
        );
        assert!(
            !machine
                .project(Audience::Public)
                .unwrap()
                .to_json()
                .unwrap()
                .contains("kyoku_results")
        );
    }

    #[test]
    fn draw_projection_releases_reason_and_settlement() {
        let mut machine = first_held(GameMode::ThreePlayerRedEast);
        let steps = machine.held_kyoku().unwrap().wins.len();
        assert_eq!(steps, 0);
        assert_eq!(
            machine
                .held_kyoku()
                .unwrap()
                .settlement
                .draw_reason
                .as_deref(),
            Some("exhaustive_draw")
        );
        let sequence = machine.held_kyoku().unwrap().kyoku_sequence;
        let before = machine
            .held_kyoku()
            .unwrap()
            .settlement
            .changes
            .iter()
            .map(|c| c.before)
            .collect::<Vec<_>>();
        let first = machine.publish_kyoku_step(0).unwrap();
        if steps == 0 {
            assert_eq!(first.len(), 1);
            assert!(
                matches!(&first[0], GameEvent::Ryukyoku { reason: Some(reason), .. } if reason == "exhaustive_draw")
            );
        } else {
            assert!(first.is_empty());
        }
        for audience in [
            Audience::Public,
            Audience::Player(Seat::new(0).unwrap()),
            Audience::Player(Seat::new(1).unwrap()),
            Audience::ReplayAdmin,
            Audience::BenchmarkAdmin,
        ] {
            let projection = machine.project(audience).unwrap().to_json().unwrap();
            let json: serde_json::Value = serde_json::from_str(&projection).unwrap();
            assert_eq!(json["kyoku_results"]["kyoku_sequence"], sequence);
            assert_eq!(
                json["kyoku_results"]["wins"].as_array().unwrap().len(),
                usize::from(steps > 0)
            );
            assert_eq!(json["kyoku_results"]["settlement"].is_object(), steps == 0);
            assert_eq!(
                json["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["score"].as_i64().unwrap() as i32)
                    .collect::<Vec<_>>(),
                before
            );
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
        for step in 1..=steps {
            machine.publish_kyoku_step(step).unwrap();
        }
        let json: serde_json::Value =
            serde_json::from_str(&machine.serialize(Audience::Public).unwrap()).unwrap();
        assert!(json["kyoku_results"]["settlement"].is_object());
        assert_eq!(
            json["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["score"].as_i64().unwrap() as i32)
                .collect::<Vec<_>>(),
            machine
                .held_kyoku()
                .unwrap()
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>()
        );
        assert_eq!(machine.held_kyoku().unwrap().kyoku_sequence, sequence);
        assert_eq!(
            machine
                .events()
                .iter()
                .filter(|event| matches!(event, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn result_projection_releases_one_winner_after_prior_draws() {
        let mode = GameMode::FourPlayerRedEast;
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0).unwrap();
        for _ in 0..20_000 {
            if let Some(hold) = machine.held_kyoku() {
                if !hold.wins.is_empty() {
                    let all = hold.wins.len();
                    assert!(machine.publish_kyoku_step(0).unwrap().is_empty());
                    let public: serde_json::Value =
                        serde_json::from_str(&machine.serialize(Audience::Public).unwrap())
                            .unwrap();
                    assert_eq!(public["kyoku_results"]["wins"].as_array().unwrap().len(), 1);
                    assert!(public["kyoku_results"]["settlement"].is_null());
                    assert!(
                        !machine
                            .events()
                            .iter()
                            .any(|event| matches!(event, GameEvent::Hora { .. }))
                    );
                    for step in 1..=all {
                        machine.publish_kyoku_step(step).unwrap();
                    }
                    return;
                }
                machine.publish_kyoku_step(0).unwrap();
                machine.release_kyoku().unwrap();
            }
            if machine.is_complete() {
                break;
            }
            let (seat, action) = Seat::all(mode)
                .into_iter()
                .find_map(|seat| {
                    let actions = machine.legal_actions(seat).unwrap();
                    let chosen = actions
                        .iter()
                        .find(|action| matches!(action, GameAction::Tsumo | GameAction::Ron(_)))
                        .or_else(|| {
                            actions
                                .iter()
                                .find(|action| matches!(action, GameAction::RiichiDiscard { .. }))
                        })
                        .or_else(|| {
                            actions.iter().find(|action| {
                                matches!(action, GameAction::Discard { .. } | GameAction::Pass)
                            })
                        })
                        .or_else(|| actions.first())?;
                    Some((seat, chosen.clone()))
                })
                .unwrap();
            machine.apply(seat, action).unwrap();
        }
        panic!("seed did not reach a win");
    }

    #[test]
    fn two_ron_steps_keep_future_winner_and_pot_private() {
        let mode = GameMode::FourPlayerRedEast;
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 230).unwrap();
        machine.engine = crate::engine::tests::prepared_capture(mode, true, false);
        let outcome = machine
            .engine
            .apply_simultaneous(&[
                (
                    Seat::new(1).unwrap(),
                    GameAction::Ron(Seat::new(0).unwrap()),
                ),
                (
                    Seat::new(2).unwrap(),
                    GameAction::Ron(Seat::new(0).unwrap()),
                ),
            ])
            .unwrap();
        machine.record_outcome(outcome).unwrap();
        let record = machine.held_kyoku().unwrap().clone();
        assert_eq!(record.wins.len(), 2);
        assert_eq!(
            record
                .wins
                .iter()
                .map(|win| win.seat.index())
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(record.settlement.pot_before, 2);
        machine.publish_kyoku_step(0).unwrap();
        for audience in [
            Audience::Public,
            Audience::Player(Seat::new(0).unwrap()),
            Audience::Player(Seat::new(1).unwrap()),
            Audience::Player(Seat::new(2).unwrap()),
            Audience::BenchmarkAdmin,
            Audience::ReplayAdmin,
        ] {
            let json: serde_json::Value =
                serde_json::from_str(&machine.serialize(audience).unwrap()).unwrap();
            assert_eq!(json["kyotaku"], 2, "{audience:?}");
            assert!(json["decision"].is_null());
            assert_eq!(
                json["kyoku_results"]["wins"],
                serde_json::json!([record.wins[0]])
            );
            assert!(json["kyoku_results"]["settlement"].is_null());
            assert_eq!(
                json["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["score"].as_i64().unwrap() as i32)
                    .collect::<Vec<_>>(),
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>()
            );
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
                .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
        assert!(machine.publish_kyoku_step(1).unwrap().is_empty());
        for audience in [
            Audience::Public,
            Audience::Player(Seat::new(0).unwrap()),
            Audience::Player(Seat::new(1).unwrap()),
            Audience::Player(Seat::new(2).unwrap()),
            Audience::BenchmarkAdmin,
            Audience::ReplayAdmin,
        ] {
            let second: serde_json::Value =
                serde_json::from_str(&machine.serialize(audience).unwrap()).unwrap();
            assert_eq!(
                second["kyoku_results"]["wins"],
                serde_json::json!(record.wins)
            );
            assert_eq!(second["kyotaku"], 2);
            assert!(second["decision"].is_null());
            assert!(second["kyoku_results"]["settlement"].is_null());
        }
        machine.publish_kyoku_step(2).unwrap();
        let settled: serde_json::Value =
            serde_json::from_str(&machine.serialize(Audience::Public).unwrap()).unwrap();
        assert_eq!(settled["kyotaku"], record.settlement.pot_after);
        assert_eq!(
            settled["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["score"].as_i64().unwrap() as i32)
                .collect::<Vec<_>>(),
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>()
        );
        assert!(settled["kyoku_results"]["settlement"].is_object());
    }

    #[test]
    fn terminal_event_prefix_hides_combined_settlement() {
        let mut machine = first_held(GameMode::FourPlayerRedEast);
        let steps = machine.held_kyoku().unwrap().wins.len();
        let end = machine.held_kyoku().unwrap().end_event_index;
        machine
            .held
            .as_mut()
            .unwrap()
            .future_events
            .push(GameEvent::Ryukyoku {
                reason: Some("next-kyoku-sentinel".into()),
                tehais: None,
                delta: None,
                scores: None,
            });
        assert!(
            !machine
                .serialize(Audience::Public)
                .unwrap()
                .contains("next-kyoku-sentinel")
        );
        assert!(!machine.events().iter().any(|e| matches!(
            e,
            GameEvent::EndKyoku | GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. }
        )));
        assert_eq!(machine.release_kyoku(), Err(MatchError::ResultsPending));
        for step in 0..steps {
            assert!(machine.publish_kyoku_step(step).unwrap().is_empty());
            assert!(
                !machine
                    .events()
                    .iter()
                    .any(|e| matches!(e, GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. }))
            );
        }
        assert_eq!(
            machine.publish_kyoku_step(steps + 1),
            Err(MatchError::ResultsPending)
        );
        let released = machine.publish_kyoku_step(steps).unwrap();
        assert!(
            released
                .iter()
                .any(|e| matches!(e, GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. }))
        );
        assert!(
            !released
                .iter()
                .any(|e| matches!(e, GameEvent::EndKyoku | GameEvent::StartKyoku { .. }))
        );
        assert_eq!(machine.held_kyoku().unwrap().end_event_index, end);
        assert!(
            !machine
                .serialize(Audience::Public)
                .unwrap()
                .contains("next-kyoku-sentinel")
        );
        machine.held.as_mut().unwrap().future_events.pop();
        let suffix = machine.release_kyoku().unwrap();
        assert!(matches!(suffix.first(), Some(GameEvent::EndKyoku)));
        assert_eq!(machine.events().len(), machine.journal_event_count);
        assert_eq!(machine.release_kyoku(), Err(MatchError::ResultsPending));
        let persisted = machine.take_persistence_events();
        assert!(!persisted.is_empty());
        assert!(!persisted.iter().any(|event| matches!(event, GameEvent::Ryukyoku { reason: Some(reason), .. } if reason == "next-kyoku-sentinel")));
        assert!(machine.take_persistence_events().is_empty());
    }

    #[test]
    fn held_final_result_is_not_complete() {
        let mut machine = MatchMachine::new_with_seed(
            GameMode::ThreePlayerRedEast,
            roster(GameMode::ThreePlayerRedEast),
            0xD0,
        )
        .unwrap();
        for _ in 0..20000 {
            if machine.engine.is_done() {
                break;
            }
            if machine.held_kyoku().is_some() {
                for step in 0..=machine.held_kyoku().unwrap().wins.len() {
                    machine.publish_kyoku_step(step).unwrap();
                }
                machine.release_kyoku().unwrap();
                continue;
            }
            let (seat, action) = Seat::all(machine.mode())
                .into_iter()
                .find_map(|seat| {
                    let actions = machine.legal_actions(seat).unwrap();
                    (!actions.is_empty()).then(|| (seat, default_action(&actions)))
                })
                .unwrap();
            machine.apply(seat, action).unwrap();
        }
        assert!(machine.held_kyoku().is_some());
        assert!(machine.result().is_none());
        assert!(!machine.is_complete());
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
            if let Some(hold) = machine.held_kyoku() {
                let steps = hold.wins.len();
                for step in 0..=steps {
                    machine.publish_kyoku_step(step).unwrap();
                }
                machine.release_kyoku().unwrap();
            }
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
                if let Some(hold) = machine.held_kyoku() {
                    let steps = hold.wins.len();
                    for step in 0..=steps {
                        machine.publish_kyoku_step(step).unwrap();
                    }
                    machine.release_kyoku().unwrap();
                }
                machine.resolve_expired().unwrap();
                if machine.held_kyoku().is_some() || machine.is_complete() {
                    continue;
                }
                machine.current_decision().unwrap().expect("decision");
                machine.resolve_expired().unwrap();
                if machine.held_kyoku().is_some() || machine.is_complete() {
                    continue;
                }
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
    fn player_projection_suppresses_a_submitted_response_until_resolution() {
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
            if let Some(hold) = machine.held_kyoku() {
                let steps = hold.wins.len();
                for step in 0..=steps {
                    machine.publish_kyoku_step(step).unwrap();
                }
                machine.release_kyoku().unwrap();
            }
            let decision = machine.current_decision().unwrap().expect("decision");
            let seats: Vec<_> = decision.eligible().collect();
            if decision.kind() == DecisionKind::Response && seats.len() > 1 {
                let submitted = seats[0];
                let decision_id = decision.id().clone();
                let action_id = decision.default_action_id(submitted).clone();
                assert!(
                    !machine
                        .submit_action(submitted, decision_id, action_id)
                        .unwrap()
                        .is_resolved()
                );
                let projection = machine.project(Audience::Player(submitted)).unwrap();
                let AudienceProjection::Player(projection) = projection else {
                    panic!("expected player projection");
                };
                assert!(
                    projection
                        .decision
                        .expect("submitted response remains open")
                        .actions
                        .is_empty()
                );
                return;
            }
            let seat = seats[0];
            let action = decision.default_action_id(seat).clone();
            machine
                .submit_action(seat, decision.id().clone(), action)
                .unwrap();
        }
        panic!("seed did not expose a simultaneous response window")
    }

    #[test]
    fn illegal_compatibility_action_preserves_open_decision() {
        let mode = GameMode::FourPlayerRedEast;
        let mut machine = MatchMachine::new_with_seed(mode, roster(mode), 0xD0_u64).unwrap();
        let seat = Seat::all(mode)[0];
        let before = machine.current_decision().unwrap().expect("open decision");
        let before_id = before.id().clone();
        let before_action_ids: Vec<Vec<_>> = before
            .entries()
            .iter()
            .map(|entry| {
                entry
                    .actions
                    .iter()
                    .map(|action| action.id.clone())
                    .collect()
            })
            .collect();
        let legal = machine.legal_actions(seat).unwrap();
        let illegal = if !legal.contains(&GameAction::Pass) {
            GameAction::Pass
        } else {
            GameAction::Tsumo
        };
        assert!(!legal.contains(&illegal));

        assert!(matches!(
            machine.apply(seat, illegal),
            Err(MatchError::IllegalAction { .. })
        ));
        let after = machine
            .current_decision()
            .unwrap()
            .expect("decision remains open");
        assert_eq!(after.id(), &before_id);
        let after_action_ids: Vec<Vec<_>> = after
            .entries()
            .iter()
            .map(|entry| {
                entry
                    .actions
                    .iter()
                    .map(|action| action.id.clone())
                    .collect()
            })
            .collect();
        assert_eq!(after_action_ids, before_action_ids);
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
