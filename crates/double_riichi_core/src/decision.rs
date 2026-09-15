use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::Instant;

use crate::{GameAction, Seat};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DecisionId(String);

impl DecisionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&DecisionId> for DecisionId {
    fn from(value: &DecisionId) -> Self {
        value.clone()
    }
}

impl From<String> for DecisionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for DecisionId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl std::fmt::Display for DecisionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionId(String);

impl ActionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&ActionId> for ActionId {
    fn from(value: &ActionId) -> Self {
        value.clone()
    }
}

impl From<String> for ActionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ActionId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecisionKind {
    Turn,
    Response,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionAction {
    pub id: ActionId,
    pub action: GameAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionSeat {
    pub seat: Seat,
    pub actions: Vec<DecisionAction>,
    default_action_id: ActionId,
    submitted_action_id: Option<ActionId>,
    opened_at: Instant,
    duration: Option<Duration>,
    deadline: Option<Instant>,
    watchdog: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presence {
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerState {
    Interactive,
    TemporaryAuto,
    PermanentAuto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeControl {
    RiichiDev,
    Casual,
    Unlimited,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self::Casual
    }
}

impl TimeControl {
    pub const fn turn_duration(self) -> Duration {
        match self {
            Self::RiichiDev | Self::Casual => Duration::from_secs(30),
            Self::Unlimited => Duration::from_secs(0),
        }
    }

    pub const fn response_duration(self) -> Duration {
        match self {
            Self::RiichiDev | Self::Casual => Duration::from_secs(10),
            Self::Unlimited => Duration::from_secs(0),
        }
    }

    pub const fn watchdog_duration(self) -> Option<Duration> {
        match self {
            Self::Unlimited => Some(Duration::from_secs(5 * 60)),
            Self::RiichiDev | Self::Casual => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingConfig {
    pub turn: Duration,
    pub response: Duration,
    pub watchdog: Duration,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TimingError {
    #[error("{name} duration must be between {min} and {max} seconds")]
    OutOfRange {
        name: &'static str,
        min: u64,
        max: u64,
        value: u64,
    },
}

impl TimingConfig {
    pub fn new(
        turn_seconds: u64,
        response_seconds: u64,
        watchdog_seconds: u64,
    ) -> Result<Self, TimingError> {
        validate_duration("turn", turn_seconds, 1, 3600)?;
        validate_duration("response", response_seconds, 1, 3600)?;
        validate_duration("watchdog", watchdog_seconds, 10, 3600)?;
        Ok(Self {
            turn: Duration::from_secs(turn_seconds),
            response: Duration::from_secs(response_seconds),
            watchdog: Duration::from_secs(watchdog_seconds),
        })
    }

    pub const fn casual() -> Self {
        Self {
            turn: Duration::from_secs(30),
            response: Duration::from_secs(10),
            watchdog: Duration::from_secs(5 * 60),
        }
    }
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self::casual()
    }
}

fn validate_duration(
    name: &'static str,
    value: u64,
    min: u64,
    max: u64,
) -> Result<(), TimingError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(TimingError::OutOfRange {
            name,
            min,
            max,
            value,
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecisionError {
    #[error("stale decision {received}; current decision is {expected}")]
    StaleDecision {
        expected: DecisionId,
        received: DecisionId,
    },
    #[error("action {action_id} does not belong to this decision or seat {seat}")]
    ForeignAction { seat: Seat, action_id: ActionId },
    #[error("seat {seat} already consumed its response")]
    AlreadyConsumed { seat: Seat },
    #[error("seat {seat} is not eligible for this decision")]
    NotEligible { seat: Seat },
    #[error("decision has no deterministic safe default for seat {seat}")]
    NoSafeDefault { seat: Seat },
    #[error("decision has expired")]
    Expired,
    #[error("decision is already closed")]
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    pub seat: Seat,
    pub action_id: ActionId,
    pub action: GameAction,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionResolution {
    pub decision_id: DecisionId,
    pub actions: Vec<ResolvedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionSubmission {
    Accepted { complete: bool },
    Resolved(DecisionResolution),
}

impl DecisionSubmission {
    pub const fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }
}

#[derive(Debug, Clone)]
pub struct Decision {
    id: DecisionId,
    kind: DecisionKind,
    entries: Vec<DecisionSeat>,
    opened_at: Instant,
    duration: Option<Duration>,
    deadline: Option<Instant>,
    watchdog: bool,
    closed: bool,
}

impl Decision {
    pub fn new(
        id: DecisionId,
        kind: DecisionKind,
        options: Vec<(Seat, Vec<GameAction>)>,
        opened_at: Instant,
        duration: Option<Duration>,
        watchdog: bool,
    ) -> Result<Self, DecisionError> {
        let mut next_action = 1u64;
        let options = options
            .into_iter()
            .map(|(seat, actions)| {
                let action_ids = actions
                    .into_iter()
                    .map(|action| {
                        let action = action.canonicalize();
                        let id = ActionId::new(format!("a{next_action}"));
                        next_action += 1;
                        (id, action)
                    })
                    .collect();
                (seat, action_ids)
            })
            .collect();
        Self::from_action_ids(id, kind, options, opened_at, duration, watchdog)
    }

    pub fn new_with_timings(
        id: DecisionId,
        kind: DecisionKind,
        options: Vec<(Seat, Vec<GameAction>, Option<Duration>, bool)>,
        opened_at: Instant,
    ) -> Result<Self, DecisionError> {
        let mut next_action = 1u64;
        let options = options
            .into_iter()
            .map(|(seat, actions, duration, watchdog)| {
                let actions = actions
                    .into_iter()
                    .map(|action| {
                        let action = action.canonicalize();
                        let id = ActionId::new(format!("a{next_action}"));
                        next_action += 1;
                        (id, action)
                    })
                    .collect();
                (seat, actions, duration, watchdog)
            })
            .collect();
        Self::from_action_ids_with_timings(id, kind, options, opened_at)
    }

    pub(crate) fn from_action_ids(
        id: DecisionId,
        kind: DecisionKind,
        options: Vec<(Seat, Vec<(ActionId, GameAction)>)>,
        opened_at: Instant,
        duration: Option<Duration>,
        watchdog: bool,
    ) -> Result<Self, DecisionError> {
        let options = options
            .into_iter()
            .map(|(seat, actions)| (seat, actions, duration, watchdog))
            .collect();
        Self::from_action_ids_with_timings(id, kind, options, opened_at)
    }

    pub(crate) fn from_action_ids_with_timings(
        id: DecisionId,
        kind: DecisionKind,
        options: Vec<(Seat, Vec<(ActionId, GameAction)>, Option<Duration>, bool)>,
        opened_at: Instant,
    ) -> Result<Self, DecisionError> {
        let entries = options
            .into_iter()
            .map(|(seat, actions, duration, watchdog)| {
                let actions: Vec<DecisionAction> = actions
                    .into_iter()
                    .map(|(id, action)| DecisionAction { id, action })
                    .collect();
                let default_action_id = default_action_id(seat, &actions)?;
                Ok(DecisionSeat {
                    seat,
                    actions,
                    default_action_id,
                    submitted_action_id: None,
                    opened_at,
                    duration,
                    deadline: duration.map(|duration| opened_at + duration),
                    watchdog,
                })
            })
            .collect::<Result<Vec<_>, DecisionError>>()?;
        let mut decision = Self {
            id,
            kind,
            entries,
            opened_at,
            duration: None,
            deadline: None,
            watchdog: false,
            closed: false,
        };
        decision.recompute_timing();
        Ok(decision)
    }

    pub fn id(&self) -> &DecisionId {
        &self.id
    }

    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    pub fn eligible(&self) -> impl Iterator<Item = Seat> + '_ {
        self.entries.iter().map(|entry| entry.seat)
    }

    pub fn entries(&self) -> &[DecisionSeat] {
        &self.entries
    }

    pub fn actions_for(&self, seat: Seat) -> &[DecisionAction] {
        self.entries
            .iter()
            .find(|entry| entry.seat == seat)
            .map(|entry| entry.actions.as_slice())
            .unwrap_or(&[])
    }

    pub fn default_for(&self, seat: Seat) -> &GameAction {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.seat == seat)
            .expect("default requested for an ineligible seat");
        &entry
            .actions
            .iter()
            .find(|action| action.id == entry.default_action_id)
            .expect("decision default must refer to a legal action")
            .action
    }

    pub fn default_action_id(&self, seat: Seat) -> &ActionId {
        &self
            .entries
            .iter()
            .find(|entry| entry.seat == seat)
            .expect("default requested for an ineligible seat")
            .default_action_id
    }

    pub fn submitted_action_id(&self, seat: Seat) -> Option<&ActionId> {
        self.entries
            .iter()
            .find(|entry| entry.seat == seat)
            .and_then(|entry| entry.submitted_action_id.as_ref())
    }

    pub fn opened_at(&self) -> Instant {
        self.opened_at
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn duration_for(&self, seat: Seat) -> Option<Duration> {
        self.entry(seat).and_then(|entry| entry.duration)
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn deadline_for(&self, seat: Seat) -> Option<Instant> {
        self.entry(seat).and_then(|entry| entry.deadline)
    }

    pub fn is_watchdog(&self) -> bool {
        self.watchdog
    }

    pub fn is_watchdog_for(&self, seat: Seat) -> bool {
        self.entry(seat).is_some_and(|entry| entry.watchdog)
    }

    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    pub fn remaining_for(&self, seat: Seat, now: Instant) -> Option<Duration> {
        self.deadline_for(seat)
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    pub fn is_expired_at(&self, now: Instant) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.deadline.is_some_and(|deadline| now >= deadline))
    }

    pub fn is_expired_for(&self, seat: Seat, now: Instant) -> bool {
        self.entry(seat)
            .is_some_and(|entry| entry.deadline.is_some_and(|deadline| now >= deadline))
    }

    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Instant::now())
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub(crate) fn retime_for(
        &mut self,
        seat: Seat,
        opened_at: Instant,
        duration: Option<Duration>,
        watchdog: bool,
    ) {
        if let Some(entry) = self.entry_mut(seat) {
            entry.opened_at = opened_at;
            entry.duration = duration;
            entry.deadline = duration.map(|duration| opened_at + duration);
            entry.watchdog = watchdog;
        }
        self.recompute_timing();
    }

    fn entry(&self, seat: Seat) -> Option<&DecisionSeat> {
        self.entries.iter().find(|entry| entry.seat == seat)
    }

    fn entry_mut(&mut self, seat: Seat) -> Option<&mut DecisionSeat> {
        self.entries.iter_mut().find(|entry| entry.seat == seat)
    }

    fn recompute_timing(&mut self) {
        let Some(first) = self.entries.first() else {
            self.duration = None;
            self.deadline = None;
            self.watchdog = false;
            return;
        };
        let uniform = self.entries.iter().all(|entry| {
            entry.opened_at == first.opened_at
                && entry.duration == first.duration
                && entry.watchdog == first.watchdog
        });
        if uniform {
            self.opened_at = first.opened_at;
            self.duration = first.duration;
            self.deadline = first.deadline;
            self.watchdog = first.watchdog;
        } else {
            self.duration = None;
            self.deadline = None;
            self.watchdog = false;
        }
    }

    pub fn submit(
        &mut self,
        seat: Seat,
        action_id: impl Into<ActionId>,
    ) -> Result<DecisionSubmission, DecisionError> {
        self.submit_at(seat, action_id, Instant::now())
    }

    pub fn submit_at(
        &mut self,
        seat: Seat,
        action_id: impl Into<ActionId>,
        now: Instant,
    ) -> Result<DecisionSubmission, DecisionError> {
        if self.closed {
            return Err(DecisionError::Closed);
        }
        if self.entry(seat).is_none() {
            return Err(DecisionError::NotEligible { seat });
        }
        if self.is_expired_for(seat, now) {
            return Err(DecisionError::Expired);
        }
        let action_id = action_id.into();
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.seat == seat)
            .expect("seat was checked above");
        if entry.submitted_action_id.is_some() {
            return Err(DecisionError::AlreadyConsumed { seat });
        }
        if !entry.actions.iter().any(|action| action.id == action_id) {
            return Err(DecisionError::ForeignAction { seat, action_id });
        }
        entry.submitted_action_id = Some(action_id);
        if self
            .entries
            .iter()
            .all(|entry| entry.submitted_action_id.is_some())
        {
            self.closed = true;
            Ok(DecisionSubmission::Resolved(self.accepted_resolution()))
        } else {
            Ok(DecisionSubmission::Accepted { complete: false })
        }
    }

    pub fn submit_with_decision_id(
        &mut self,
        decision_id: &DecisionId,
        seat: Seat,
        action_id: impl Into<ActionId>,
    ) -> Result<DecisionSubmission, DecisionError> {
        if decision_id != &self.id {
            return Err(DecisionError::StaleDecision {
                expected: self.id.clone(),
                received: decision_id.clone(),
            });
        }
        self.submit(seat, action_id)
    }

    pub fn resolve_at(
        &mut self,
        now: Instant,
    ) -> Result<Option<DecisionResolution>, DecisionError> {
        if self.closed {
            return Ok(None);
        }
        if !self.is_expired_at(now) {
            return Ok(None);
        }
        let mut timed_out = Vec::new();
        for entry in &mut self.entries {
            if entry.submitted_action_id.is_none()
                && entry.deadline.is_some_and(|deadline| now >= deadline)
            {
                entry.submitted_action_id = Some(entry.default_action_id.clone());
                timed_out.push(entry.seat);
            }
        }
        if timed_out.is_empty()
            || self
                .entries
                .iter()
                .any(|entry| entry.submitted_action_id.is_none())
        {
            return Ok(None);
        }
        self.closed = true;
        let mut resolution = self.accepted_resolution();
        for action in &mut resolution.actions {
            action.timed_out = timed_out.contains(&action.seat);
        }
        Ok(Some(resolution))
    }

    fn accepted_resolution(&self) -> DecisionResolution {
        let actions = self
            .entries
            .iter()
            .map(|entry| {
                let action_id = entry
                    .submitted_action_id
                    .as_ref()
                    .expect("resolved decision response");
                let action = entry
                    .actions
                    .iter()
                    .find(|action| &action.id == action_id)
                    .expect("resolved action id must be legal");
                ResolvedAction {
                    seat: entry.seat,
                    action_id: action.id.clone(),
                    action: action.action.clone(),
                    timed_out: false,
                }
            })
            .collect();
        DecisionResolution {
            decision_id: self.id.clone(),
            actions,
        }
    }
}

fn default_action_id(seat: Seat, actions: &[DecisionAction]) -> Result<ActionId, DecisionError> {
    actions
        .iter()
        .find(|action| matches!(action.action, GameAction::Pass))
        .or_else(|| {
            actions.iter().find(|action| {
                matches!(
                    action.action,
                    GameAction::Discard {
                        tsumogiri: true,
                        ..
                    }
                )
            })
        })
        .or_else(|| {
            actions
                .iter()
                .find(|action| matches!(action.action, GameAction::Discard { .. }))
        })
        .map(|action| action.id.clone())
        .ok_or(DecisionError::NoSafeDefault { seat })
}
