//! Protocol-neutral domain crate for double-riichi.

mod decision;
mod domain;
pub mod engine;
mod match_machine;
mod projection;

pub use decision::{
    ActionId, ControllerState, Decision, DecisionAction, DecisionError, DecisionId, DecisionKind,
    DecisionResolution, DecisionSeat, DecisionSubmission, Presence, ResolvedAction, TimeControl,
    TimingConfig, TimingError,
};
pub use domain::{
    GameAction, GameEvent, GameMode, InvalidGameMode, InvalidSeat, MatchAbort, MatchPlayerResult,
    MatchResult, MatchStatus, Mode, Participant, ParticipantId, ParticipantKind, Seat, Tile,
    TileType, Wind,
};
pub use match_machine::{DecisionResult, MatchError, MatchMachine};
pub use projection::{
    Audience, AudienceProjection, MeldState, PlayerDecisionProjection, PlayerProjection,
    ProjectionError, PublicDecisionProjection, PublicProjection, ReplayAdminProjection,
    ReplayDecisionEntry, ReplayDecisionProjection, TablePlayerState, TableState, VisibleAction,
    VisibleMeld, VisiblePlayer, project_table_state, serialize_projection,
};
