//! Protocol-neutral domain crate for double-riichi.

mod decision;
mod domain;
pub mod engine;
mod match_machine;
mod projection;
pub mod room;

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
pub use room::{
    CONNECTION_OUTBOUND_CAPACITY, CharacterCatalog, CharacterUsage, MatchId, MatchPlayerSnapshot,
    MatchRole, PermanentAutoReason, ROOM_COMMAND_CAPACITY, ROOM_EFFECT_CAPACITY, RoomActor,
    RoomAuxiliaryEvent, RoomAuxiliaryPhase, RoomCommand, RoomConfig, RoomConnection,
    RoomController, RoomEffect, RoomEffectError, RoomError, RoomEvent, RoomHandle,
    RoomHistoryEvent, RoomHistoryProjection, RoomId, RoomJoinCode, RoomKyokuProjection,
    RoomKyokuResult, RoomKyokuSummary, RoomParticipantSnapshot, RoomPersistenceFailure, RoomPhase,
    RoomRegistry, RoomRegistryError, RoomRemoval, RoomResponse, RoomSnapshot, RoomState,
    ShutdownMode,
};
