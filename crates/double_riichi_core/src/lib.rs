//! Protocol-neutral domain crate for double-riichi.

mod domain;
pub mod engine;
mod match_machine;

pub use domain::{
    GameAction, GameEvent, GameMode, InvalidGameMode, InvalidSeat, MatchAbort, MatchPlayerResult,
    MatchResult, MatchStatus, Mode, Participant, ParticipantId, ParticipantKind, Seat, Tile,
    TileType, Wind,
};
pub use match_machine::{MatchError, MatchMachine};
