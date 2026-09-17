//! Transport-neutral, provisional riichi.dev MJAI adapter.
//!
//! This crate owns only the wire boundary.  Match state, legal actions, and
//! private information remain in `double_riichi_core`; HTTP and WebSocket
//! lifecycle code belongs to the server crate.  The public upstream evidence
//! used here is provisional and does not establish Protocol v2 pinning or
//! authenticated transcript parity.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use double_riichi_core::{
    Audience, AudienceProjection, Decision, DecisionAction, DecisionResult, GameAction, GameEvent,
    GameMode, MatchMachine, PlayerProjection, Seat, Tile,
};
use riichienv_core::{
    action::{Action, ActionType},
    observation::Observation,
    observation_3p::Observation3P,
    types::{Meld, MeldType},
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned, ser::SerializeMap,
};
use serde_json::{Map, Value};
use thiserror::Error;

/// Maximum size of one protocol text frame.
pub const MAX_FRAME_BYTES: usize = 1_048_576;
/// Maximum number of offers in one `request_action`.
pub const MAX_POSSIBLE_ACTIONS: usize = 128;
/// Maximum number of tiles in a compound action.
pub const MAX_ACTION_TILES: usize = 4;
/// Maximum number of player-facing event strings carried by one observation.
pub const MAX_OBSERVATION_EVENTS: usize = 256;
/// Published upstream grace period.
pub const DEFAULT_GRACE_MS: u64 = 3_000;
/// Published upstream per-kyoku bank.
pub const DEFAULT_BANK_MS: u64 = 15_000;
/// Legacy reply debt lifetime.
pub const LEGACY_REPLY_TTL: Duration = Duration::from_secs(30);
/// Bound for protocol strings that are not tile names or observations.
pub const MAX_FIELD_STRING_BYTES: usize = 256;
/// Bound for a single error/reason string.
pub const MAX_REASON_BYTES: usize = 256;
/// Bound for yaku entries in a result event.
pub const MAX_YAKU_ENTRIES: usize = 64;
/// Bound for a single request timing value.
pub const MAX_TIME_MS: u64 = 86_400_000;

/// Errors at the untrusted MJAI boundary.  Error variants intentionally do
/// not carry raw frames, observations, or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtocolError {
    #[error("protocol frame is {actual} bytes; maximum is {max}")]
    FrameTooLarge { actual: usize, max: usize },
    #[error("protocol payload is not a JSON object")]
    NotObject,
    #[error("protocol payload is malformed JSON")]
    MalformedJson,
    #[error("protocol payload has an invalid field")]
    InvalidField,
    #[error("protocol payload is missing its type")]
    MissingType,
    #[error("unsupported protocol message type")]
    UnsupportedType,
    #[error("protocol field exceeds its bound")]
    FieldTooLarge,
    #[error("protocol vector exceeds its bound")]
    VectorTooLarge,
    #[error("protocol tile is invalid")]
    InvalidTile,
    #[error("protocol seat is invalid")]
    InvalidSeat,
    #[error("protocol event is invalid for this mode")]
    InvalidModeValue,
    #[error("client action does not match a complete legal action")]
    NoMatchingAction,
    #[error("client action matches more than one legal action")]
    AmbiguousAction,
    #[error("request ID is not monotonically increasing")]
    NonMonotonicRequestId,
    #[error("request ID is stale")]
    StaleRequest,
    #[error("request ID is from the future")]
    FutureRequest,
    #[error("there is no pending request")]
    NoPendingRequest,
    #[error("request tracking capacity is full")]
    RequestQueueFull,
    #[error("core match operation failed")]
    Core(String),
    #[error("observation serialization failed")]
    Observation,
}

fn check_frame(bytes: &[u8]) -> Result<(), ProtocolError> {
    if bytes.len() > MAX_FRAME_BYTES {
        Err(ProtocolError::FrameTooLarge {
            actual: bytes.len(),
            max: MAX_FRAME_BYTES,
        })
    } else {
        Ok(())
    }
}

fn parse_json(bytes: &[u8]) -> Result<Value, ProtocolError> {
    check_frame(bytes)?;
    serde_json::from_slice(bytes).map_err(|_| ProtocolError::MalformedJson)
}

fn object(value: &Value) -> Result<&Map<String, Value>, ProtocolError> {
    value.as_object().ok_or(ProtocolError::NotObject)
}

fn type_name(value: &Value) -> Result<&str, ProtocolError> {
    object(value)?
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::MissingType)
}

fn parse_typed<T: DeserializeOwned>(value: Value) -> Result<T, ProtocolError> {
    serde_json::from_value(value).map_err(|_| ProtocolError::InvalidField)
}

fn bounded_string(value: &str, max: usize) -> Result<(), ProtocolError> {
    if value.len() > max {
        Err(ProtocolError::FieldTooLarge)
    } else {
        Ok(())
    }
}

fn bounded_vec<T>(value: &[T], max: usize) -> Result<(), ProtocolError> {
    if value.len() > max {
        Err(ProtocolError::VectorTooLarge)
    } else {
        Ok(())
    }
}

/// Convert a physical core tile to its MJAI spelling, preserving red fives.
pub fn tile_to_mjai(tile: Tile) -> String {
    riichienv_core::parser::tid_to_mjai(tile.id())
}

/// Parse an MJAI tile without a mode-specific sanma check.
pub fn parse_tile(value: &str) -> Result<Tile, ProtocolError> {
    bounded_string(value, 8)?;
    let id = riichienv_core::parser::mjai_to_tid(value).ok_or(ProtocolError::InvalidTile)?;
    Tile::from_id(id).ok_or(ProtocolError::InvalidTile)
}

/// Parse an MJAI tile and reject tiles removed from a three-player match.
pub fn parse_tile_for_mode(value: &str, mode: GameMode) -> Result<Tile, ProtocolError> {
    let tile = parse_tile(value)?;
    if tile.is_valid_for(mode) {
        Ok(tile)
    } else {
        Err(ProtocolError::InvalidModeValue)
    }
}

/// A tile in a server event.  `?` is the upstream concealed-tile marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WireTile {
    Known(Tile),
    Hidden,
}

impl WireTile {
    pub const fn known(self) -> Option<Tile> {
        match self {
            Self::Known(tile) => Some(tile),
            Self::Hidden => None,
        }
    }
}

impl Serialize for WireTile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Known(tile) => serializer.serialize_str(&tile_to_mjai(*tile)),
            Self::Hidden => serializer.serialize_str("?"),
        }
    }
}

impl<'de> Deserialize<'de> for WireTile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value == "?" {
            Ok(Self::Hidden)
        } else {
            parse_tile(&value)
                .map(Self::Known)
                .map_err(|_| serde::de::Error::custom("invalid MJAI tile"))
        }
    }
}

fn seat_for_mode(value: u8, mode: GameMode) -> Result<Seat, ProtocolError> {
    if usize::from(value) >= mode.seat_count() {
        return Err(ProtocolError::InvalidModeValue);
    }
    Seat::new(value).ok_or(ProtocolError::InvalidSeat)
}

fn validate_seat(value: u8, mode: GameMode) -> Result<(), ProtocolError> {
    seat_for_mode(value, mode).map(|_| ())
}

fn wind_name(wind: double_riichi_core::Wind) -> &'static str {
    match wind {
        double_riichi_core::Wind::East => "E",
        double_riichi_core::Wind::South => "S",
        double_riichi_core::Wind::West => "W",
        double_riichi_core::Wind::North => "N",
    }
}

fn parse_wind(value: &str) -> Result<double_riichi_core::Wind, ProtocolError> {
    match value {
        "E" => Ok(double_riichi_core::Wind::East),
        "S" => Ok(double_riichi_core::Wind::South),
        "W" => Ok(double_riichi_core::Wind::West),
        "N" => Ok(double_riichi_core::Wind::North),
        _ => Err(ProtocolError::InvalidField),
    }
}

/// Known server events in the provisional MJAI wire vocabulary.
///
/// Unknown server event types are represented by [`ServerMessage::Opaque`]
/// when parsed.  Known event structs deliberately ignore unknown fields so
/// additive upstream fields do not break a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    StartGame {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        names: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    StartKyoku {
        bakaze: String,
        kyoku: u8,
        honba: u8,
        #[serde(rename = "kyotaku")]
        kyotaku: u8,
        oya: u8,
        scores: Vec<i32>,
        dora_marker: WireTile,
        tehais: Vec<Vec<WireTile>>,
    },
    Tsumo {
        actor: u8,
        pai: WireTile,
    },
    Dahai {
        actor: u8,
        pai: WireTile,
        tsumogiri: bool,
    },
    Chi {
        actor: u8,
        target: u8,
        pai: WireTile,
        consumed: Vec<WireTile>,
    },
    Pon {
        actor: u8,
        target: u8,
        pai: WireTile,
        consumed: Vec<WireTile>,
    },
    #[serde(rename = "daiminkan")]
    Daiminkan {
        actor: u8,
        target: u8,
        pai: WireTile,
        consumed: Vec<WireTile>,
    },
    Kakan {
        actor: u8,
        pai: WireTile,
    },
    Ankan {
        actor: u8,
        consumed: Vec<WireTile>,
    },
    Dora {
        dora_marker: WireTile,
    },
    Reach {
        actor: u8,
    },
    ReachAccepted {
        actor: u8,
    },
    Hora {
        actor: u8,
        target: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<WireTile>,
        #[serde(
            rename = "uradora_markers",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        ura_markers: Option<Vec<WireTile>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        yaku: Option<Vec<(String, u32)>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fu: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        han: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scores: Option<Vec<i32>>,
        #[serde(rename = "deltas", default, skip_serializing_if = "Option::is_none")]
        delta: Option<Vec<i32>>,
    },
    Ryukyoku {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tehais: Option<Vec<Vec<WireTile>>>,
        #[serde(rename = "deltas", default, skip_serializing_if = "Option::is_none")]
        delta: Option<Vec<i32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scores: Option<Vec<i32>>,
    },
    Kita {
        actor: u8,
    },
    EndKyoku,
    EndGame,
}

/// Parsed server input.  Opaque events retain only their bounded type name;
/// arbitrary unknown fields never enter canonical state or logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    Event(ServerEvent),
    Opaque { event_type: String },
}

impl Serialize for ServerMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Event(event) => event.serialize(serializer),
            Self::Opaque { event_type } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", event_type)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ServerMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_server_value(value, None).map_err(|_| serde::de::Error::custom("invalid MJAI event"))
    }
}

fn is_known_server_type(value: &str) -> bool {
    matches!(
        value,
        "start_game"
            | "start_kyoku"
            | "tsumo"
            | "dahai"
            | "chi"
            | "pon"
            | "daiminkan"
            | "kan"
            | "kakan"
            | "ankan"
            | "dora"
            | "reach"
            | "reach_accepted"
            | "hora"
            | "ryukyoku"
            | "kita"
            | "end_kyoku"
            | "end_game"
    )
}

fn normalize_server_type(value: &mut Value) -> Result<String, ProtocolError> {
    let kind = type_name(value)?.to_owned();
    bounded_string(&kind, MAX_FIELD_STRING_BYTES)?;
    if kind == "kan" {
        let map = value.as_object_mut().ok_or(ProtocolError::NotObject)?;
        map.insert("type".into(), Value::String("daiminkan".into()));
        Ok("daiminkan".into())
    } else {
        Ok(kind)
    }
}

fn parse_server_value(
    mut value: Value,
    mode: Option<GameMode>,
) -> Result<ServerMessage, ProtocolError> {
    let kind = normalize_server_type(&mut value)?;
    if !is_known_server_type(&kind) {
        return Ok(ServerMessage::Opaque { event_type: kind });
    }
    let event: ServerEvent = parse_typed(value)?;
    validate_server_event(
        &event,
        mode.unwrap_or(GameMode::FourPlayerRedEast),
        mode.is_some(),
    )?;
    Ok(ServerMessage::Event(event))
}

/// Parse one bounded server frame.  Unknown event types are tolerated.
pub fn parse_server_message(bytes: &[u8], mode: GameMode) -> Result<ServerMessage, ProtocolError> {
    parse_server_value(parse_json(bytes)?, Some(mode))
}

/// Parse one known/unknown server event from a JSON frame.
pub fn parse_server_event(bytes: &[u8], mode: GameMode) -> Result<ServerMessage, ProtocolError> {
    parse_server_message(bytes, mode)
}

fn validate_wire_tile(
    tile: WireTile,
    mode: GameMode,
    hidden_allowed: bool,
) -> Result<(), ProtocolError> {
    match tile {
        WireTile::Hidden if hidden_allowed => Ok(()),
        WireTile::Hidden => Err(ProtocolError::InvalidField),
        WireTile::Known(tile) if tile.is_valid_for(mode) => Ok(()),
        WireTile::Known(_) => Err(ProtocolError::InvalidModeValue),
    }
}

fn validate_tile_list(
    tiles: &[WireTile],
    mode: GameMode,
    hidden_allowed: bool,
    max: usize,
) -> Result<(), ProtocolError> {
    bounded_vec(tiles, max)?;
    tiles
        .iter()
        .copied()
        .try_for_each(|tile| validate_wire_tile(tile, mode, hidden_allowed))
}

fn validate_server_event(
    event: &ServerEvent,
    mode: GameMode,
    enforce_mode: bool,
) -> Result<(), ProtocolError> {
    let validate_seat_if_needed = |seat: u8| {
        if enforce_mode {
            validate_seat(seat, mode)
        } else if seat < 4 {
            Ok(())
        } else {
            Err(ProtocolError::InvalidSeat)
        }
    };
    let validate_known_tile = |tile: WireTile| {
        validate_wire_tile(tile, mode, false).or_else(|error| {
            if !enforce_mode {
                match tile {
                    WireTile::Known(tile) if tile.id() < 136 => Ok(()),
                    _ => Err(error),
                }
            } else {
                Err(error)
            }
        })
    };
    match event {
        ServerEvent::StartGame { names, id } => {
            if let Some(names) = names {
                bounded_vec(names, 4)?;
                names
                    .iter()
                    .try_for_each(|name| bounded_string(name, MAX_FIELD_STRING_BYTES))?;
            }
            if let Some(id) = id {
                bounded_string(id, MAX_FIELD_STRING_BYTES)?;
            }
        }
        ServerEvent::StartKyoku {
            bakaze,
            scores,
            oya,
            dora_marker,
            tehais,
            ..
        } => {
            parse_wind(bakaze)?;
            let count = scores.len();
            bounded_vec(scores, 4)?;
            bounded_vec(tehais, 4)?;
            if enforce_mode && count != mode.seat_count() {
                return Err(ProtocolError::InvalidModeValue);
            }
            if tehais.len() != count {
                return Err(ProtocolError::InvalidField);
            }
            validate_seat_if_needed(*oya)?;
            validate_known_tile(*dora_marker)?;
            for hand in tehais {
                validate_tile_list(hand, mode, true, 14)?;
            }
        }
        ServerEvent::Tsumo { actor, pai } => {
            validate_seat_if_needed(*actor)?;
            validate_wire_tile(*pai, mode, true)?;
        }
        ServerEvent::Dahai { actor, pai, .. } => {
            validate_seat_if_needed(*actor)?;
            validate_known_tile(*pai)?;
        }
        ServerEvent::Chi {
            actor,
            target,
            pai,
            consumed,
        }
        | ServerEvent::Pon {
            actor,
            target,
            pai,
            consumed,
        }
        | ServerEvent::Daiminkan {
            actor,
            target,
            pai,
            consumed,
        } => {
            validate_seat_if_needed(*actor)?;
            validate_seat_if_needed(*target)?;
            validate_known_tile(*pai)?;
            validate_tile_list(consumed, mode, false, 3)?;
        }
        ServerEvent::Kakan { actor, pai } => {
            validate_seat_if_needed(*actor)?;
            validate_known_tile(*pai)?;
        }
        ServerEvent::Ankan { actor, consumed } => {
            validate_seat_if_needed(*actor)?;
            validate_tile_list(consumed, mode, false, MAX_ACTION_TILES)?;
        }
        ServerEvent::Dora { dora_marker } => validate_known_tile(*dora_marker)?,
        ServerEvent::Reach { actor }
        | ServerEvent::ReachAccepted { actor }
        | ServerEvent::Kita { actor } => validate_seat_if_needed(*actor)?,
        ServerEvent::Hora {
            actor,
            target,
            pai,
            ura_markers,
            yaku,
            scores,
            delta,
            ..
        } => {
            validate_seat_if_needed(*actor)?;
            validate_seat_if_needed(*target)?;
            if let Some(pai) = pai {
                validate_known_tile(*pai)?;
            }
            if let Some(markers) = ura_markers {
                validate_tile_list(markers, mode, false, 5)?;
            }
            if let Some(yaku) = yaku {
                bounded_vec(yaku, MAX_YAKU_ENTRIES)?;
                for (name, _) in yaku {
                    bounded_string(name, MAX_FIELD_STRING_BYTES)?;
                }
            }
            if let Some(scores) = scores {
                bounded_vec(scores, 4)?;
                if enforce_mode && scores.len() != mode.seat_count() {
                    return Err(ProtocolError::InvalidModeValue);
                }
            }
            if let Some(delta) = delta {
                bounded_vec(delta, 4)?;
                if enforce_mode && delta.len() != mode.seat_count() {
                    return Err(ProtocolError::InvalidModeValue);
                }
            }
        }
        ServerEvent::Ryukyoku {
            reason,
            tehais,
            delta,
            scores,
        } => {
            if let Some(reason) = reason {
                bounded_string(reason, MAX_REASON_BYTES)?;
            }
            if let Some(tehais) = tehais {
                bounded_vec(tehais, 4)?;
                if enforce_mode && tehais.len() != mode.seat_count() {
                    return Err(ProtocolError::InvalidModeValue);
                }
                for hand in tehais {
                    validate_tile_list(hand, mode, false, 14)?;
                }
            }
            if let Some(delta) = delta {
                bounded_vec(delta, 4)?;
                if enforce_mode && delta.len() != mode.seat_count() {
                    return Err(ProtocolError::InvalidModeValue);
                }
            }
            if let Some(scores) = scores {
                bounded_vec(scores, 4)?;
                if enforce_mode && scores.len() != mode.seat_count() {
                    return Err(ProtocolError::InvalidModeValue);
                }
            }
        }
        ServerEvent::EndKyoku | ServerEvent::EndGame => {}
    }
    Ok(())
}

fn wire_tile(tile: Tile, mode: GameMode) -> Result<WireTile, ProtocolError> {
    if tile.is_valid_for(mode) {
        Ok(WireTile::Known(tile))
    } else {
        Err(ProtocolError::InvalidModeValue)
    }
}

fn wire_tiles(tiles: &[Tile], mode: GameMode) -> Result<Vec<WireTile>, ProtocolError> {
    bounded_vec(tiles, MAX_ACTION_TILES)?;
    tiles
        .iter()
        .copied()
        .map(|tile| wire_tile(tile, mode))
        .collect()
}

/// Convert a canonical event to the projection visible to one Player.
pub fn event_for_player(
    event: &GameEvent,
    viewer: Seat,
    mode: GameMode,
) -> Result<ServerEvent, ProtocolError> {
    validate_seat(viewer.index(), mode)?;
    let event = match event {
        GameEvent::StartGame { names, id } => ServerEvent::StartGame {
            names: names.clone(),
            id: id.clone(),
        },
        GameEvent::StartKyoku {
            bakaze,
            kyoku,
            honba,
            kyotaku,
            oya,
            scores,
            dora_marker,
            tehais,
        } => {
            if scores.len() != mode.seat_count() || tehais.len() != mode.seat_count() {
                return Err(ProtocolError::InvalidModeValue);
            }
            let tehais = tehais
                .iter()
                .enumerate()
                .map(|(seat, hand)| {
                    bounded_vec(hand, 14)?;
                    hand.iter()
                        .copied()
                        .map(|tile| {
                            if seat == usize::from(viewer.index()) {
                                wire_tile(tile, mode)
                            } else if tile.is_valid_for(mode) {
                                Ok(WireTile::Hidden)
                            } else {
                                Err(ProtocolError::InvalidModeValue)
                            }
                        })
                        .collect()
                })
                .collect::<Result<Vec<Vec<_>>, ProtocolError>>()?;
            validate_seat(oya.index(), mode)?;
            ServerEvent::StartKyoku {
                bakaze: wind_name(*bakaze).into(),
                kyoku: *kyoku,
                honba: *honba,
                kyotaku: *kyotaku,
                oya: oya.index(),
                scores: scores.clone(),
                dora_marker: wire_tile(*dora_marker, mode)?,
                tehais,
            }
        }
        GameEvent::Tsumo { actor, tile } => ServerEvent::Tsumo {
            actor: actor.index(),
            pai: if *actor == viewer {
                wire_tile(*tile, mode)?
            } else {
                WireTile::Hidden
            },
        },
        GameEvent::Dahai {
            actor,
            tile,
            tsumogiri,
        } => ServerEvent::Dahai {
            actor: actor.index(),
            pai: wire_tile(*tile, mode)?,
            tsumogiri: *tsumogiri,
        },
        GameEvent::Chi {
            actor,
            target,
            called,
            consumed,
        } => ServerEvent::Chi {
            actor: actor.index(),
            target: target.index(),
            pai: wire_tile(*called, mode)?,
            consumed: wire_tiles(consumed, mode)?,
        },
        GameEvent::Pon {
            actor,
            target,
            called,
            consumed,
        } => ServerEvent::Pon {
            actor: actor.index(),
            target: target.index(),
            pai: wire_tile(*called, mode)?,
            consumed: wire_tiles(consumed, mode)?,
        },
        GameEvent::Daiminkan {
            actor,
            target,
            called,
            consumed,
        } => ServerEvent::Daiminkan {
            actor: actor.index(),
            target: target.index(),
            pai: wire_tile(*called, mode)?,
            consumed: wire_tiles(consumed, mode)?,
        },
        GameEvent::Kakan { actor, called } => ServerEvent::Kakan {
            actor: actor.index(),
            pai: wire_tile(*called, mode)?,
        },
        GameEvent::Ankan { actor, consumed } => ServerEvent::Ankan {
            actor: actor.index(),
            consumed: wire_tiles(consumed, mode)?,
        },
        GameEvent::Dora { dora_marker } => ServerEvent::Dora {
            dora_marker: wire_tile(*dora_marker, mode)?,
        },
        GameEvent::Reach { actor } => ServerEvent::Reach {
            actor: actor.index(),
        },
        GameEvent::ReachAccepted { actor } => ServerEvent::ReachAccepted {
            actor: actor.index(),
        },
        GameEvent::Hora {
            actor,
            target,
            tile,
            ura_markers,
            yaku,
            fu,
            han,
            scores,
            delta,
        } => ServerEvent::Hora {
            actor: actor.index(),
            target: target.index(),
            pai: tile.map(|tile| wire_tile(tile, mode)).transpose()?,
            ura_markers: ura_markers
                .as_deref()
                .map(|tiles| wire_tiles(tiles, mode))
                .transpose()?,
            yaku: yaku.clone(),
            fu: *fu,
            han: *han,
            scores: scores.clone(),
            delta: delta.clone(),
        },
        GameEvent::Ryukyoku {
            reason,
            tehais,
            delta,
            scores,
        } => ServerEvent::Ryukyoku {
            reason: reason.clone(),
            tehais: tehais
                .as_deref()
                .map(|hands| {
                    bounded_vec(hands, mode.seat_count())?;
                    hands
                        .iter()
                        .map(|hand| {
                            bounded_vec(hand, 14)?;
                            hand.iter()
                                .copied()
                                .map(|tile| wire_tile(tile, mode))
                                .collect()
                        })
                        .collect::<Result<Vec<Vec<_>>, ProtocolError>>()
                })
                .transpose()?,
            delta: delta.clone(),
            scores: scores.clone(),
        },
        GameEvent::Kita { actor } => ServerEvent::Kita {
            actor: actor.index(),
        },
        GameEvent::EndKyoku => ServerEvent::EndKyoku,
        GameEvent::EndGame => ServerEvent::EndGame,
    };
    validate_server_event(&event, mode, true)?;
    Ok(event)
}

/// Serialize one projection-filtered server event.
pub fn encode_event(
    event: &GameEvent,
    viewer: Seat,
    mode: GameMode,
) -> Result<String, ProtocolError> {
    let event = event_for_player(event, viewer, mode)?;
    serde_json::to_string(&event).map_err(|_| ProtocolError::Observation)
}

/// Alias used by callers that call MJAI events "messages".
pub fn serialize_event_for_player(
    event: &GameEvent,
    viewer: Seat,
    mode: GameMode,
) -> Result<String, ProtocolError> {
    encode_event(event, viewer, mode)
}

/// One possible action advertised by `request_action`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PossibleAction {
    Dahai {
        pai: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tsumogiri: Option<bool>,
    },
    Chi {
        target: u8,
        pai: String,
        consumed: Vec<String>,
    },
    Pon {
        target: u8,
        pai: String,
        consumed: Vec<String>,
    },
    Daiminkan {
        target: u8,
        pai: String,
        consumed: Vec<String>,
    },
    Ankan {
        consumed: Vec<String>,
    },
    Kakan {
        pai: String,
        consumed: Vec<String>,
    },
    Reach {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<String>,
    },
    Hora,
    Ryukyoku,
    Kita {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<String>,
    },
    None,
}

impl PossibleAction {
    pub fn from_game_action(action: &GameAction) -> Result<Self, ProtocolError> {
        let action = action.clone().canonicalize_for_adapter();
        match action {
            GameAction::Discard { tile, tsumogiri } => Ok(Self::Dahai {
                pai: tile_to_mjai(tile),
                tsumogiri: Some(tsumogiri),
            }),
            GameAction::RiichiDiscard { tile } => Ok(Self::Reach {
                pai: Some(tile_to_mjai(tile)),
            }),
            GameAction::Chi {
                target,
                called,
                consumed,
            } => Ok(Self::Chi {
                target: target.index(),
                pai: tile_to_mjai(called),
                consumed: consumed.iter().copied().map(tile_to_mjai).collect(),
            }),
            GameAction::Pon {
                target,
                called,
                consumed,
            } => Ok(Self::Pon {
                target: target.index(),
                pai: tile_to_mjai(called),
                consumed: consumed.iter().copied().map(tile_to_mjai).collect(),
            }),
            GameAction::Daiminkan {
                target,
                called,
                consumed,
            } => Ok(Self::Daiminkan {
                target: target.index(),
                pai: tile_to_mjai(called),
                consumed: consumed.iter().copied().map(tile_to_mjai).collect(),
            }),
            GameAction::Ankan { consumed } => Ok(Self::Ankan {
                consumed: consumed.iter().copied().map(tile_to_mjai).collect(),
            }),
            GameAction::Kakan { called, consumed } => Ok(Self::Kakan {
                pai: tile_to_mjai(called),
                consumed: consumed.iter().copied().map(tile_to_mjai).collect(),
            }),
            GameAction::Nuki { tile } => Ok(Self::Kita {
                pai: Some(tile_to_mjai(tile)),
            }),
            GameAction::Tsumo | GameAction::Ron(_) => Ok(Self::Hora),
            GameAction::Pass => Ok(Self::None),
            GameAction::AbortiveDraw => Ok(Self::Ryukyoku),
        }
    }
}

/// Strict client action DTO.  Unknown fields are rejected at this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientAction {
    Dahai {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        pai: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tsumogiri: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Chi {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        target: Option<u8>,
        pai: String,
        consumed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Pon {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        target: Option<u8>,
        pai: String,
        consumed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Daiminkan {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        target: Option<u8>,
        pai: String,
        consumed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Ankan {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        consumed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Kakan {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        pai: String,
        consumed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Reach {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Hora {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Ryukyoku {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    Kita {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    None {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
}

impl ClientAction {
    pub fn request_id(&self) -> Option<u64> {
        match self {
            Self::Dahai { request_id, .. }
            | Self::Chi { request_id, .. }
            | Self::Pon { request_id, .. }
            | Self::Daiminkan { request_id, .. }
            | Self::Ankan { request_id, .. }
            | Self::Kakan { request_id, .. }
            | Self::Reach { request_id, .. }
            | Self::Hora { request_id, .. }
            | Self::Ryukyoku { request_id, .. }
            | Self::Kita { request_id, .. }
            | Self::None { request_id } => *request_id,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Dahai { .. } => "dahai",
            Self::Chi { .. } => "chi",
            Self::Pon { .. } => "pon",
            Self::Daiminkan { .. } => "daiminkan",
            Self::Ankan { .. } => "ankan",
            Self::Kakan { .. } => "kakan",
            Self::Reach { .. } => "reach",
            Self::Hora { .. } => "hora",
            Self::Ryukyoku { .. } => "ryukyoku",
            Self::Kita { .. } => "kita",
            Self::None { .. } => "none",
        }
    }
}

fn optional_actor_matches(
    actor: Option<u8>,
    seat: Seat,
    mode: GameMode,
) -> Result<bool, ProtocolError> {
    if let Some(actor) = actor {
        validate_seat(actor, mode)?;
        Ok(actor == seat.index())
    } else {
        Ok(true)
    }
}

fn parse_action_tile(value: &str, mode: GameMode) -> Result<Tile, ProtocolError> {
    parse_tile_for_mode(value, mode)
}

fn parse_action_tiles(values: &[String], mode: GameMode) -> Result<Vec<Tile>, ProtocolError> {
    bounded_vec(values, MAX_ACTION_TILES)?;
    values
        .iter()
        .map(|value| parse_action_tile(value, mode))
        .collect()
}

fn validate_client_action(action: &ClientAction, mode: GameMode) -> Result<(), ProtocolError> {
    let actor = match action {
        ClientAction::Dahai { actor, .. }
        | ClientAction::Chi { actor, .. }
        | ClientAction::Pon { actor, .. }
        | ClientAction::Daiminkan { actor, .. }
        | ClientAction::Ankan { actor, .. }
        | ClientAction::Kakan { actor, .. }
        | ClientAction::Reach { actor, .. }
        | ClientAction::Hora { actor, .. }
        | ClientAction::Ryukyoku { actor, .. }
        | ClientAction::Kita { actor, .. } => *actor,
        ClientAction::None { .. } => None,
    };
    if let Some(actor) = actor {
        validate_seat(actor, mode)?;
    }
    match action {
        ClientAction::Dahai { pai, .. } => {
            parse_action_tile(pai, mode)?;
        }
        ClientAction::Chi {
            target,
            pai,
            consumed,
            ..
        }
        | ClientAction::Pon {
            target,
            pai,
            consumed,
            ..
        }
        | ClientAction::Daiminkan {
            target,
            pai,
            consumed,
            ..
        } => {
            let required = if matches!(action, ClientAction::Daiminkan { .. }) {
                3
            } else {
                2
            };
            if consumed.len() != required {
                return Err(ProtocolError::InvalidField);
            }
            if let Some(target) = target {
                validate_seat(*target, mode)?;
            }
            parse_action_tile(pai, mode)?;
            parse_action_tiles(consumed, mode)?;
        }
        ClientAction::Ankan { consumed, .. } => {
            if consumed.len() != 4 {
                return Err(ProtocolError::InvalidField);
            }
            parse_action_tiles(consumed, mode)?;
        }
        ClientAction::Kakan { pai, consumed, .. } => {
            if consumed.len() != 3 {
                return Err(ProtocolError::InvalidField);
            }
            parse_action_tile(pai, mode)?;
            parse_action_tiles(consumed, mode)?;
        }
        ClientAction::Reach { pai, .. } => {
            if let Some(pai) = pai {
                parse_action_tile(pai, mode)?;
            }
        }
        ClientAction::Hora { target, pai, .. } => {
            if let Some(target) = target {
                validate_seat(*target, mode)?;
            }
            if let Some(pai) = pai {
                parse_action_tile(pai, mode)?;
            }
        }
        ClientAction::Ryukyoku { reason, .. } => {
            if let Some(reason) = reason {
                bounded_string(reason, MAX_REASON_BYTES)?;
            }
        }
        ClientAction::Kita { pai, .. } => {
            if let Some(pai) = pai {
                parse_action_tile(pai, mode)?;
            }
        }
        ClientAction::None { .. } => {}
    }
    Ok(())
}

/// Extract a request ID before parsing the rest of an action.  This lets the
/// adapter discard stale replies without retaining malformed payloads.
pub fn request_id_from_frame(bytes: &[u8]) -> Result<Option<u64>, ProtocolError> {
    let value = parse_json(bytes)?;
    let object = object(&value)?;
    let Some(request_id) = object.get("request_id") else {
        return Ok(None);
    };
    request_id
        .as_u64()
        .map(Some)
        .ok_or(ProtocolError::InvalidField)
}

/// Parse a strict client action for a mode.
pub fn parse_client_action(bytes: &[u8], mode: GameMode) -> Result<ClientAction, ProtocolError> {
    let action: ClientAction = parse_typed(parse_json(bytes)?)?;
    validate_client_action(&action, mode)?;
    Ok(action)
}

/// Parse a strict client action using the four-player tile universe.
pub fn parse_client_action_4p(bytes: &[u8]) -> Result<ClientAction, ProtocolError> {
    parse_client_action(bytes, GameMode::FourPlayerRedEast)
}

/// A complete action matched to one core Decision entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedAction {
    pub action_id: double_riichi_core::ActionId,
    pub action: GameAction,
}

fn sorted_wire_tiles(tiles: &[Tile]) -> Vec<String> {
    let mut names: Vec<_> = tiles.iter().copied().map(tile_to_mjai).collect();
    names.sort();
    names
}

fn candidate_unique(candidates: Vec<&DecisionAction>) -> Result<MatchedAction, ProtocolError> {
    let Some(candidate) = candidates.first() else {
        return Err(ProtocolError::NoMatchingAction);
    };
    // MJAI names a tile type, while the engine keeps physical copies.  Copies
    // with the same wire shape are equivalent; red/ordinary and tsumogiri
    // variants remain distinct and therefore still report ambiguity.
    if candidates.iter().skip(1).any(|other| {
        PossibleAction::from_game_action(&other.action).ok()
            != PossibleAction::from_game_action(&candidate.action).ok()
    }) {
        return Err(ProtocolError::AmbiguousAction);
    }
    Ok(MatchedAction {
        action_id: candidate.id.clone(),
        action: candidate.action.clone(),
    })
}

fn action_target_matches(
    action: &GameAction,
    target: Option<u8>,
    mode: GameMode,
) -> Result<bool, ProtocolError> {
    if let Some(target) = target {
        let target = seat_for_mode(target, mode)?;
        Ok(action.target_seat() == Some(target))
    } else {
        Ok(true)
    }
}

/// Match a wire action only against complete actions in the current Decision.
/// No client-supplied action is applied directly to the MatchMachine.
pub fn match_legal_action(
    mode: GameMode,
    seat: Seat,
    decision: &Decision,
    action: &ClientAction,
) -> Result<MatchedAction, ProtocolError> {
    validate_seat(seat.index(), mode)?;
    validate_client_action(action, mode)?;
    let entries = decision.actions_for(seat);
    if entries.is_empty() {
        return Err(ProtocolError::NoMatchingAction);
    }
    let actor_matches = |actor: Option<u8>| optional_actor_matches(actor, seat, mode);
    let candidates = match action {
        ClientAction::Dahai {
            actor,
            pai,
            tsumogiri,
            ..
        } => {
            if !actor_matches(*actor)? {
                return Err(ProtocolError::NoMatchingAction);
            }
            parse_action_tile(pai, mode)?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.action,
                            GameAction::Discard {
                                tile: candidate,
                                tsumogiri: candidate_tsumogiri,
                            } if tile_to_mjai(candidate) == *pai
                                && tsumogiri.is_none_or(|requested| requested == candidate_tsumogiri)

                    )
                })
                .collect()
        }
        ClientAction::Chi {
            actor,
            target,
            pai,
            consumed,
            ..
        } => {
            if !actor_matches(*actor)? || target.is_none() {
                return Err(ProtocolError::NoMatchingAction);
            }
            parse_action_tile(pai, mode)?;
            let consumed = parse_action_tiles(consumed, mode)?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        &entry.action,
                        GameAction::Chi {
                            called: candidate_called,
                            consumed: candidate_consumed,
                            ..
                        } if action_target_matches(&entry.action, *target, mode).unwrap_or(false)
                            && tile_to_mjai(*candidate_called) == *pai
                            && sorted_wire_tiles(candidate_consumed) == sorted_wire_tiles(&consumed)
                    )
                })
                .collect()
        }
        ClientAction::Pon {
            actor,
            target,
            pai,
            consumed,
            ..
        } => {
            if !actor_matches(*actor)? || target.is_none() {
                return Err(ProtocolError::NoMatchingAction);
            }
            parse_action_tile(pai, mode)?;
            let consumed = parse_action_tiles(consumed, mode)?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        &entry.action,
                        GameAction::Pon {
                            called: candidate_called,
                            consumed: candidate_consumed,
                            ..
                        } if action_target_matches(&entry.action, *target, mode).unwrap_or(false)
                            && tile_to_mjai(*candidate_called) == *pai
                            && sorted_wire_tiles(candidate_consumed) == sorted_wire_tiles(&consumed)
                    )
                })
                .collect()
        }
        ClientAction::Daiminkan {
            actor,
            target,
            pai,
            consumed,
            ..
        } => {
            if !actor_matches(*actor)? || target.is_none() {
                return Err(ProtocolError::NoMatchingAction);
            }
            parse_action_tile(pai, mode)?;
            let consumed = parse_action_tiles(consumed, mode)?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        &entry.action,
                        GameAction::Daiminkan {
                            called: candidate_called,
                            consumed: candidate_consumed,
                            ..
                        } if action_target_matches(&entry.action, *target, mode).unwrap_or(false)
                            && tile_to_mjai(*candidate_called) == *pai
                            && sorted_wire_tiles(candidate_consumed) == sorted_wire_tiles(&consumed)
                    )
                })
                .collect()
        }
        ClientAction::Ankan {
            actor, consumed, ..
        } => {
            if !actor_matches(*actor)? {
                return Err(ProtocolError::NoMatchingAction);
            }
            let consumed = parse_action_tiles(consumed, mode)?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        &entry.action,
                        GameAction::Ankan {
                            consumed: candidate_consumed,
                        } if sorted_wire_tiles(candidate_consumed) == sorted_wire_tiles(&consumed)
                    )
                })
                .collect()
        }
        ClientAction::Kakan {
            actor,
            pai,
            consumed,
            ..
        } => {
            if !actor_matches(*actor)? {
                return Err(ProtocolError::NoMatchingAction);
            }
            parse_action_tile(pai, mode)?;
            let consumed = parse_action_tiles(consumed, mode)?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        &entry.action,
                        GameAction::Kakan {
                            called: candidate_called,
                            consumed: candidate_consumed,
                        } if tile_to_mjai(*candidate_called) == *pai
                            && sorted_wire_tiles(candidate_consumed) == sorted_wire_tiles(&consumed)
                    )
                })
                .collect()
        }
        ClientAction::Reach { actor, pai, .. } => {
            if !actor_matches(*actor)? {
                return Err(ProtocolError::NoMatchingAction);
            }
            let tile = pai
                .as_deref()
                .map(|value| parse_action_tile(value, mode))
                .transpose()?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.action,
                        GameAction::RiichiDiscard { tile: candidate } if tile.is_none_or(|_tile| tile_to_mjai(candidate) == pai.as_deref().unwrap_or_default())
                    )
                })
                .collect()
        }
        ClientAction::Hora {
            actor, target, pai, ..
        } => {
            if !actor_matches(*actor)? {
                return Err(ProtocolError::NoMatchingAction);
            }
            // `pai` is retained for compatibility with conflicting upstream
            // examples, but the neutral core has no winning-tile field for
            // Ron/Tsumo.  It is syntax-checked and only accepted when the
            // remaining legal action is unique.
            if let Some(pai) = pai {
                parse_action_tile(pai, mode)?;
            }
            entries
                .iter()
                .filter(|entry| {
                    matches!(entry.action, GameAction::Tsumo | GameAction::Ron(_))
                        && action_target_matches(&entry.action, *target, mode).unwrap_or(false)
                })
                .collect()
        }
        ClientAction::Ryukyoku { actor, reason, .. } => {
            if !actor_matches(*actor)? || reason.is_some() {
                return Err(ProtocolError::NoMatchingAction);
            }
            entries
                .iter()
                .filter(|entry| matches!(entry.action, GameAction::AbortiveDraw))
                .collect()
        }
        ClientAction::Kita { actor, pai, .. } => {
            if !actor_matches(*actor)? {
                return Err(ProtocolError::NoMatchingAction);
            }
            let tile = pai
                .as_deref()
                .map(|value| parse_action_tile(value, mode))
                .transpose()?;
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.action,
                        GameAction::Nuki { tile: candidate } if tile.is_none_or(|_tile| tile_to_mjai(candidate) == pai.as_deref().unwrap_or_default())
                    )
                })
                .collect()
        }
        ClientAction::None { .. } => entries
            .iter()
            .filter(|entry| matches!(entry.action, GameAction::Pass))
            .collect(),
    };
    candidate_unique(candidates)
}

/// Short alias for [`match_legal_action`].
pub fn match_action(
    mode: GameMode,
    seat: Seat,
    decision: &Decision,
    action: &ClientAction,
) -> Result<MatchedAction, ProtocolError> {
    match_legal_action(mode, seat, decision, action)
}

/// Per-request upstream timing values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestTime {
    pub grace_ms: u64,
    pub bank_ms: u64,
    pub deadline_ms: u64,
}

impl Default for RequestTime {
    fn default() -> Self {
        Self {
            grace_ms: DEFAULT_GRACE_MS,
            bank_ms: DEFAULT_BANK_MS,
            deadline_ms: DEFAULT_GRACE_MS + DEFAULT_BANK_MS,
        }
    }
}

impl RequestTime {
    /// Wire representation for a decision with no deadline, such as a
    /// connected Human under the Unlimited Room time control.
    pub const fn unlimited() -> Self {
        Self {
            grace_ms: 0,
            bank_ms: 0,
            deadline_ms: 0,
        }
    }

    pub fn validate(self) -> Result<(), ProtocolError> {
        if self.grace_ms > MAX_TIME_MS || self.bank_ms > MAX_TIME_MS {
            return Err(ProtocolError::InvalidField);
        }
        if self.deadline_ms == 0 {
            return (self.grace_ms == 0 && self.bank_ms == 0)
                .then_some(())
                .ok_or(ProtocolError::InvalidField);
        }
        if self.deadline_ms > MAX_TIME_MS
            || self.deadline_ms != self.grace_ms.saturating_add(self.bank_ms)
        {
            return Err(ProtocolError::InvalidField);
        }
        Ok(())
    }
}

/// Bounded request DTO sent to an MJAI client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestAction {
    #[serde(rename = "type")]
    pub kind: String,
    pub request_id: u64,
    pub time: RequestTime,
    pub possible_actions: Vec<PossibleAction>,
    pub observation: String,
}

impl RequestAction {
    pub fn new(
        request_id: u64,
        time: RequestTime,
        possible_actions: Vec<PossibleAction>,
        observation: String,
    ) -> Result<Self, ProtocolError> {
        let request = Self {
            kind: "request_action".into(),
            request_id,
            time,
            possible_actions,
            observation,
        };
        request.validate(GameMode::FourPlayerRedEast)?;
        Ok(request)
    }

    pub fn validate(&self, mode: GameMode) -> Result<(), ProtocolError> {
        if self.kind != "request_action" {
            return Err(ProtocolError::UnsupportedType);
        }
        self.time.validate()?;
        bounded_vec(&self.possible_actions, MAX_POSSIBLE_ACTIONS)?;
        for action in &self.possible_actions {
            validate_possible_action(action, mode)?;
        }
        bounded_string(&self.observation, MAX_FRAME_BYTES)?;
        let decoded = BASE64
            .decode(self.observation.as_bytes())
            .map_err(|_| ProtocolError::InvalidField)?;
        check_frame(&decoded)?;
        let value: Value =
            serde_json::from_slice(&decoded).map_err(|_| ProtocolError::InvalidField)?;
        object(&value)?;
        Ok(())
    }
}

fn validate_possible_action(action: &PossibleAction, mode: GameMode) -> Result<(), ProtocolError> {
    match action {
        PossibleAction::Dahai { pai, .. } => {
            parse_action_tile(pai, mode)?;
        }
        PossibleAction::Chi {
            target,
            pai,
            consumed,
        }
        | PossibleAction::Pon {
            target,
            pai,
            consumed,
        }
        | PossibleAction::Daiminkan {
            target,
            pai,
            consumed,
        } => {
            validate_seat(*target, mode)?;
            let required = if matches!(action, PossibleAction::Daiminkan { .. }) {
                3
            } else {
                2
            };
            if consumed.len() != required {
                return Err(ProtocolError::InvalidField);
            }
            parse_action_tile(pai, mode)?;
            parse_action_tiles(consumed, mode)?;
        }
        PossibleAction::Ankan { consumed } => {
            if consumed.len() != 4 {
                return Err(ProtocolError::InvalidField);
            }
            parse_action_tiles(consumed, mode)?;
        }
        PossibleAction::Kakan { pai, consumed } => {
            if consumed.len() != 3 {
                return Err(ProtocolError::InvalidField);
            }
            parse_action_tile(pai, mode)?;
            parse_action_tiles(consumed, mode)?;
        }
        PossibleAction::Reach { pai } | PossibleAction::Kita { pai } => {
            if let Some(pai) = pai {
                parse_action_tile(pai, mode)?;
            }
        }
        PossibleAction::Hora | PossibleAction::Ryukyoku | PossibleAction::None => {}
    }
    Ok(())
}

/// Build possible actions from the core Player projection.
pub fn possible_actions(
    projection: &PlayerProjection,
) -> Result<Vec<PossibleAction>, ProtocolError> {
    let Some(decision) = projection.decision.as_ref() else {
        return Ok(Vec::new());
    };
    bounded_vec(&decision.actions, MAX_POSSIBLE_ACTIONS)?;
    decision
        .actions
        .iter()
        .map(|visible| PossibleAction::from_game_action(&visible.action))
        .collect()
}

/// Decode a base64 RiichiEnv observation generated by this adapter.
#[derive(Debug, Clone)]
pub enum ObservationPayload {
    FourPlayer(Observation),
    ThreePlayer(Observation3P),
}

pub fn decode_observation(
    encoded: &str,
    mode: GameMode,
) -> Result<ObservationPayload, ProtocolError> {
    bounded_string(encoded, MAX_FRAME_BYTES)?;
    let decoded = BASE64
        .decode(encoded.as_bytes())
        .map_err(|_| ProtocolError::InvalidField)?;
    check_frame(&decoded)?;
    match mode.is_three_player() {
        false => Observation::deserialize_from_base64(encoded)
            .map(ObservationPayload::FourPlayer)
            .map_err(|_| ProtocolError::Observation),
        true => Observation3P::deserialize_from_base64(encoded)
            .map(ObservationPayload::ThreePlayer)
            .map_err(|_| ProtocolError::Observation),
    }
}

fn infer_meld_type(meld: &double_riichi_core::VisibleMeld) -> MeldType {
    if !meld.opened {
        return MeldType::Ankan;
    }
    if meld.tiles.len() == 4 {
        return if meld.from_who.is_some() {
            MeldType::Daiminkan
        } else {
            MeldType::Kakan
        };
    }
    let mut types: Vec<_> = meld
        .tiles
        .iter()
        .map(|tile| tile.tile_type().index())
        .collect();
    types.sort_unstable();
    types.dedup();
    if meld.tiles.len() == 3 && types.len() == 3 {
        MeldType::Chi
    } else {
        MeldType::Pon
    }
}

fn projection_meld(meld: &double_riichi_core::VisibleMeld) -> Meld {
    Meld::new(
        infer_meld_type(meld),
        meld.tiles.iter().map(|tile| tile.id()).collect(),
        meld.opened,
        meld.from_who.map_or(-1, |seat| seat.index() as i8),
        meld.called_tile.map(|tile| tile.id()),
    )
}

fn env_action(action: &GameAction, seat: Seat) -> Result<Action, ProtocolError> {
    let actor = Some(seat.index());
    let action = match action {
        GameAction::Discard { tile, .. } => {
            Action::new(ActionType::Discard, Some(tile.id()), Vec::new(), actor)
        }
        GameAction::RiichiDiscard { tile } => {
            Action::new(ActionType::Riichi, Some(tile.id()), Vec::new(), actor)
        }
        GameAction::Chi {
            called, consumed, ..
        } => Action::new(
            ActionType::Chi,
            Some(called.id()),
            consumed.iter().map(|tile| tile.id()).collect(),
            actor,
        ),
        GameAction::Pon {
            called, consumed, ..
        } => Action::new(
            ActionType::Pon,
            Some(called.id()),
            consumed.iter().map(|tile| tile.id()).collect(),
            actor,
        ),
        GameAction::Daiminkan {
            called, consumed, ..
        } => Action::new(
            ActionType::Daiminkan,
            Some(called.id()),
            consumed.iter().map(|tile| tile.id()).collect(),
            actor,
        ),
        GameAction::Ankan { consumed } => Action::new(
            ActionType::Ankan,
            consumed.first().map(|tile| tile.id()),
            consumed.iter().map(|tile| tile.id()).collect(),
            actor,
        ),
        GameAction::Kakan { called, consumed } => Action::new(
            ActionType::Kakan,
            Some(called.id()),
            consumed.iter().map(|tile| tile.id()).collect(),
            actor,
        ),
        GameAction::Nuki { tile } => {
            Action::new(ActionType::Kita, Some(tile.id()), Vec::new(), actor)
        }
        GameAction::Tsumo => Action::new(ActionType::Tsumo, None, Vec::new(), actor),
        GameAction::Ron(_) => Action::new(ActionType::Ron, None, Vec::new(), actor),
        GameAction::Pass => Action::new(ActionType::Pass, None, Vec::new(), actor),
        GameAction::AbortiveDraw => Action::new(ActionType::KyushuKyuhai, None, Vec::new(), actor),
    };
    Ok(action)
}

fn observation_context(events: &[GameEvent], viewer: Seat) -> (Option<Tile>, Option<Tile>) {
    let mut last_discard = None;
    let mut drawn_tile = None;
    for event in events {
        match event {
            GameEvent::StartKyoku { .. } => {
                last_discard = None;
                drawn_tile = None;
            }
            GameEvent::Tsumo { actor, tile } if *actor == viewer => {
                drawn_tile = Some(*tile);
            }
            GameEvent::Dahai { actor, tile, .. } => {
                last_discard = Some(*tile);
                if *actor == viewer {
                    drawn_tile = None;
                }
            }
            GameEvent::Chi { called, .. }
            | GameEvent::Pon { called, .. }
            | GameEvent::Daiminkan { called, .. } => {
                last_discard = Some(*called);
                drawn_tile = None;
            }
            GameEvent::Kakan { called, .. } => {
                last_discard = Some(*called);
                drawn_tile = None;
            }
            _ => {}
        }
    }
    (last_discard, drawn_tile)
}

/// Encode the core's already audience-filtered Player projection as a
/// base64 RiichiEnv `Observation`/`Observation3P`.
pub fn encode_observation(
    projection: &PlayerProjection,
    events: &[GameEvent],
) -> Result<String, ProtocolError> {
    validate_seat(projection.viewer_seat.index(), projection.mode)?;
    if projection.players.len() != projection.mode.seat_count() {
        return Err(ProtocolError::InvalidModeValue);
    }
    bounded_vec(events, MAX_OBSERVATION_EVENTS)?;
    let viewer = projection.viewer_seat;
    let mut hands: Vec<Vec<u8>> = Vec::with_capacity(projection.players.len());
    let mut melds: Vec<Vec<Meld>> = Vec::with_capacity(projection.players.len());
    let mut discards: Vec<Vec<u8>> = Vec::with_capacity(projection.players.len());
    let mut scores = Vec::with_capacity(projection.players.len());
    let mut riichi = Vec::with_capacity(projection.players.len());
    for player in &projection.players {
        if player.seat == viewer {
            let hand = player.hand.as_ref().ok_or(ProtocolError::InvalidField)?;
            hands.push(hand.iter().map(|tile| tile.id()).collect());
        } else {
            // The core projection is the privacy boundary: an opponent hand
            // must be absent, never copied into an MJAI observation.
            if player.hand.is_some() {
                return Err(ProtocolError::InvalidField);
            }
            hands.push(Vec::new());
        }
        melds.push(player.melds.iter().map(projection_meld).collect());
        discards.push(player.discards.iter().map(|tile| tile.id()).collect());
        scores.push(player.score);
        riichi.push(player.riichi);
    }
    let dora: Vec<u8> = projection
        .dora_indicators
        .iter()
        .map(|tile| tile.id())
        .collect();
    let legal_actions = projection
        .decision
        .as_ref()
        .map(|decision| {
            decision
                .actions
                .iter()
                .map(|visible| env_action(&visible.action, viewer))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let event_strings = events
        .iter()
        .map(|event| encode_event(event, viewer, projection.mode))
        .collect::<Result<Vec<_>, _>>()?;
    let (last_discard, drawn_tile) = observation_context(events, viewer);
    let scores4 = scores.clone();
    let riichi4 = riichi.clone();
    let kyoku_index = projection.kyoku.unwrap_or(1).saturating_sub(1);
    let round_wind = projection.round.map_or(0, |wind| wind as u8);
    let oya = projection.dealer.map_or(0, |seat| seat.index());
    let honba = projection.honba.unwrap_or_default();
    let riichi_sticks = projection.kyotaku.unwrap_or_default();
    let encoded = if projection.mode.is_three_player() {
        let hands: [Vec<u8>; 3] = hands
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let melds: [Vec<Meld>; 3] = melds
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let discards: [Vec<u8>; 3] = discards
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let scores: [i32; 3] = scores
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let riichi_declared: [bool; 3] = riichi
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        Observation3P::new(
            viewer.index(),
            hands,
            melds,
            discards,
            dora,
            scores,
            riichi_declared,
            legal_actions,
            event_strings,
            honba,
            riichi_sticks,
            round_wind,
            oya,
            kyoku_index,
            Vec::new(),
            false,
            [None, None, None],
            [None, None, None],
            last_discard.map(|tile| tile.id() as u32),
            drawn_tile.map(|tile| tile.id()),
        )
        .serialize_to_base64()
        .map_err(|_| ProtocolError::Observation)?
    } else {
        let hands: [Vec<u8>; 4] = hands
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let melds: [Vec<Meld>; 4] = melds
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let discards: [Vec<u8>; 4] = discards
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let scores: [i32; 4] = scores4
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        let riichi_declared: [bool; 4] = riichi4
            .try_into()
            .map_err(|_| ProtocolError::InvalidModeValue)?;
        Observation::new(
            viewer.index(),
            hands,
            melds,
            discards,
            dora,
            scores,
            riichi_declared,
            legal_actions,
            event_strings,
            honba,
            riichi_sticks,
            round_wind,
            oya,
            kyoku_index,
            Vec::new(),
            false,
            [None, None, None, None],
            [None, None, None, None],
            last_discard.map(|tile| tile.id() as u32),
            drawn_tile.map(|tile| tile.id()),
        )
        .serialize_to_base64()
        .map_err(|_| ProtocolError::Observation)?
    };
    if encoded.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: encoded.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    Ok(encoded)
}

/// Build a complete request from a Player projection.
pub fn build_request_action(
    request_id: u64,
    time: RequestTime,
    projection: &PlayerProjection,
    events: &[GameEvent],
) -> Result<RequestAction, ProtocolError> {
    let observation = encode_observation(projection, events)?;
    let request = RequestAction {
        kind: "request_action".into(),
        request_id,
        time,
        possible_actions: possible_actions(projection)?,
        observation,
    };
    request.validate(projection.mode)?;
    Ok(request)
}

/// Alias used by transports that call request construction "encoding".
pub fn encode_request_action(
    request_id: u64,
    time: RequestTime,
    projection: &PlayerProjection,
    events: &[GameEvent],
) -> Result<String, ProtocolError> {
    serde_json::to_string(&build_request_action(request_id, time, projection, events)?)
        .map_err(|_| ProtocolError::Observation)
}

/// Parse a bounded `request_action` frame.
pub fn parse_request_action(bytes: &[u8], mode: GameMode) -> Result<RequestAction, ProtocolError> {
    let request: RequestAction = parse_typed(parse_json(bytes)?)?;
    request.validate(mode)?;
    Ok(request)
}

/// Statuses emitted by `action_ack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AckStatus {
    Accepted,
    Rejected,
    Unparseable,
    Stale,
    Defaulted,
}

/// Bounded acknowledgement DTO.  Optional diagnostic fields are emitted only
/// where the provisional evidence describes them; raw malformed frames are
/// never copied into `attempted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionAck {
    #[serde(rename = "type")]
    pub kind: String,
    pub request_id: Option<u64>,
    pub status: AckStatus,
    pub elapsed_ms: u64,
    pub bank_consumed_ms: u64,
    pub bank_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempted: Option<PossibleAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<PossibleAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_types: Option<Vec<String>>,
}

impl ActionAck {
    fn base(
        request_id: Option<u64>,
        status: AckStatus,
        elapsed_ms: u64,
        bank_consumed_ms: u64,
        bank_ms: u64,
    ) -> Self {
        Self {
            kind: "action_ack".into(),
            request_id,
            status,
            elapsed_ms,
            bank_consumed_ms,
            bank_ms,
            reason: None,
            attempted: None,
            action: None,
            legal_types: None,
        }
    }

    pub fn accepted(request_id: u64, timing: TimingOutcome) -> Self {
        Self::base(
            Some(request_id),
            AckStatus::Accepted,
            timing.elapsed_ms,
            timing.bank_consumed_ms,
            timing.bank_ms,
        )
    }

    pub fn rejected(
        request_id: u64,
        timing: TimingOutcome,
        action: &ClientAction,
        legal_types: Vec<String>,
    ) -> Result<Self, ProtocolError> {
        let mut ack = Self::base(
            Some(request_id),
            AckStatus::Rejected,
            timing.elapsed_ms,
            timing.bank_consumed_ms,
            timing.bank_ms,
        );
        ack.reason = Some("action is not legal".into());
        ack.attempted = Some(possible_from_client(action)?);
        bounded_vec(&legal_types, MAX_POSSIBLE_ACTIONS)?;
        ack.legal_types = Some(legal_types);
        Ok(ack)
    }

    pub fn unparseable(
        request_id: Option<u64>,
        timing: TimingOutcome,
        reason: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        let reason = reason.into();
        bounded_string(&reason, MAX_REASON_BYTES)?;
        let mut ack = Self::base(
            request_id,
            AckStatus::Unparseable,
            timing.elapsed_ms,
            timing.bank_consumed_ms,
            timing.bank_ms,
        );
        ack.reason = Some(reason);
        Ok(ack)
    }

    pub fn stale(request_id: Option<u64>, bank_ms: u64) -> Self {
        Self::base(request_id, AckStatus::Stale, 0, 0, bank_ms)
    }

    pub fn defaulted(
        request_id: u64,
        timing: TimingOutcome,
        action: &GameAction,
    ) -> Result<Self, ProtocolError> {
        let mut ack = Self::base(
            Some(request_id),
            AckStatus::Defaulted,
            timing.elapsed_ms,
            timing.bank_consumed_ms,
            timing.bank_ms,
        );
        ack.action = Some(PossibleAction::from_game_action(action)?);
        Ok(ack)
    }
}

fn possible_from_client(action: &ClientAction) -> Result<PossibleAction, ProtocolError> {
    match action {
        ClientAction::Dahai { pai, tsumogiri, .. } => Ok(PossibleAction::Dahai {
            pai: pai.clone(),
            tsumogiri: *tsumogiri,
        }),
        ClientAction::Chi {
            target,
            pai,
            consumed,
            ..
        } => Ok(PossibleAction::Chi {
            target: target.ok_or(ProtocolError::InvalidField)?,
            pai: pai.clone(),
            consumed: consumed.clone(),
        }),
        ClientAction::Pon {
            target,
            pai,
            consumed,
            ..
        } => Ok(PossibleAction::Pon {
            target: target.ok_or(ProtocolError::InvalidField)?,
            pai: pai.clone(),
            consumed: consumed.clone(),
        }),
        ClientAction::Daiminkan {
            target,
            pai,
            consumed,
            ..
        } => Ok(PossibleAction::Daiminkan {
            target: target.ok_or(ProtocolError::InvalidField)?,
            pai: pai.clone(),
            consumed: consumed.clone(),
        }),
        ClientAction::Ankan { consumed, .. } => Ok(PossibleAction::Ankan {
            consumed: consumed.clone(),
        }),
        ClientAction::Kakan { pai, consumed, .. } => Ok(PossibleAction::Kakan {
            pai: pai.clone(),
            consumed: consumed.clone(),
        }),
        ClientAction::Reach { pai, .. } => Ok(PossibleAction::Reach { pai: pai.clone() }),
        ClientAction::Hora { .. } => Ok(PossibleAction::Hora),
        ClientAction::Ryukyoku { .. } => Ok(PossibleAction::Ryukyoku),
        ClientAction::Kita { pai, .. } => Ok(PossibleAction::Kita { pai: pai.clone() }),
        ClientAction::None { .. } => Ok(PossibleAction::None),
    }
}

/// Result of accounting one reply against a timing budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingOutcome {
    pub elapsed_ms: u64,
    pub bank_consumed_ms: u64,
    pub bank_ms: u64,
    pub timed_out: bool,
}

/// Per-kyoku bank accounting.  Only time over grace consumes the bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingBudget {
    grace_ms: u64,
    bank_ms: u64,
}

impl Default for TimingBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl TimingBudget {
    pub const fn new() -> Self {
        Self {
            grace_ms: DEFAULT_GRACE_MS,
            bank_ms: DEFAULT_BANK_MS,
        }
    }

    pub const fn with_bank(bank_ms: u64) -> Self {
        Self {
            grace_ms: DEFAULT_GRACE_MS,
            bank_ms,
        }
    }

    pub const fn bank_ms(self) -> u64 {
        self.bank_ms
    }

    pub const fn grace_ms(self) -> u64 {
        self.grace_ms
    }

    pub fn reset_kyoku(&mut self) {
        self.bank_ms = DEFAULT_BANK_MS;
    }

    pub fn request_time(self) -> RequestTime {
        RequestTime {
            grace_ms: self.grace_ms,
            bank_ms: self.bank_ms,
            deadline_ms: self.grace_ms.saturating_add(self.bank_ms),
        }
    }

    pub fn preview(&self, time: RequestTime, elapsed_ms: u64) -> TimingOutcome {
        if time.deadline_ms == 0 {
            return TimingOutcome {
                elapsed_ms,
                bank_consumed_ms: 0,
                bank_ms: 0,
                timed_out: false,
            };
        }
        let available_bank = self.bank_ms.min(time.bank_ms);
        let over_grace = elapsed_ms.saturating_sub(time.grace_ms);
        let bank_consumed_ms = over_grace.min(available_bank);
        let timed_out = elapsed_ms >= time.deadline_ms || over_grace > available_bank;
        TimingOutcome {
            elapsed_ms,
            bank_consumed_ms,
            bank_ms: if timed_out {
                0
            } else {
                available_bank.saturating_sub(bank_consumed_ms)
            },
            timed_out,
        }
    }

    pub fn account(&mut self, time: RequestTime, elapsed_ms: u64) -> TimingOutcome {
        let outcome = self.preview(time, elapsed_ms);
        self.bank_ms = outcome.bank_ms;
        outcome
    }

    pub fn force_timeout(&mut self, elapsed_ms: u64) -> TimingOutcome {
        let consumed = self.bank_ms;
        self.bank_ms = 0;
        TimingOutcome {
            elapsed_ms,
            bank_consumed_ms: consumed,
            bank_ms: 0,
            timed_out: true,
        }
    }
}

/// Classify a reply against tracked request IDs.  Classifying a current reply
/// consumes that request; timed-out legacy debt is consumed as stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyDisposition {
    Current { request_id: u64, legacy: bool },
    Stale { request_id: Option<u64> },
    Future { request_id: u64 },
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingState {
    Owed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingReply {
    request_id: u64,
    state: PendingState,
    expires_at: Instant,
}

/// Bounded request/reply lifecycle state for one MJAI connection.
#[derive(Debug, Clone)]
pub struct ReplyTracker {
    last_issued: Option<u64>,
    pending: VecDeque<PendingReply>,
}

impl Default for ReplyTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplyTracker {
    pub const fn new() -> Self {
        Self {
            last_issued: None,
            pending: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn last_issued(&self) -> Option<u64> {
        self.last_issued
    }

    pub fn current_request_id(&self) -> Option<u64> {
        self.pending.front().map(|pending| pending.request_id)
    }

    fn owed_request_id(&self) -> Option<u64> {
        self.pending
            .iter()
            .find(|pending| pending.state == PendingState::Owed)
            .map(|pending| pending.request_id)
    }

    pub fn issue(&mut self, request_id: u64) -> Result<(), ProtocolError> {
        self.issue_at(request_id, Instant::now())
    }

    pub fn issue_at(&mut self, request_id: u64, now: Instant) -> Result<(), ProtocolError> {
        self.prune_at(now);
        if self.last_issued.is_some_and(|last| request_id <= last) {
            return Err(ProtocolError::NonMonotonicRequestId);
        }
        if self.pending.len() >= MAX_POSSIBLE_ACTIONS {
            return Err(ProtocolError::RequestQueueFull);
        }
        self.last_issued = Some(request_id);
        self.pending.push_back(PendingReply {
            request_id,
            state: PendingState::Owed,
            expires_at: now + LEGACY_REPLY_TTL,
        });
        Ok(())
    }

    pub fn timeout(&mut self, request_id: u64) -> Result<(), ProtocolError> {
        self.timeout_at(request_id, Instant::now())
    }

    pub fn timeout_at(&mut self, request_id: u64, now: Instant) -> Result<(), ProtocolError> {
        self.prune_at(now);
        let Some(pending) = self
            .pending
            .iter_mut()
            .find(|pending| pending.request_id == request_id)
        else {
            return Err(ProtocolError::StaleRequest);
        };
        if pending.state == PendingState::TimedOut {
            return Ok(());
        }
        pending.state = PendingState::TimedOut;
        pending.expires_at = now + LEGACY_REPLY_TTL;
        Ok(())
    }

    pub fn peek(&self, request_id: Option<u64>) -> ReplyDisposition {
        let mut tracker = self.clone();
        tracker.classify_at(request_id, Instant::now())
    }

    pub fn classify(&mut self, request_id: Option<u64>) -> ReplyDisposition {
        self.classify_at(request_id, Instant::now())
    }

    pub fn classify_at(&mut self, request_id: Option<u64>, now: Instant) -> ReplyDisposition {
        self.prune_at(now);
        match request_id {
            Some(request_id) => {
                if let Some(index) = self
                    .pending
                    .iter()
                    .position(|pending| pending.request_id == request_id)
                {
                    let pending = self.pending.remove(index).expect("pending index exists");
                    if pending.state == PendingState::TimedOut {
                        ReplyDisposition::Stale {
                            request_id: Some(request_id),
                        }
                    } else {
                        ReplyDisposition::Current {
                            request_id,
                            legacy: false,
                        }
                    }
                } else if self.last_issued.is_some_and(|last| request_id <= last) {
                    ReplyDisposition::Stale {
                        request_id: Some(request_id),
                    }
                } else {
                    ReplyDisposition::Future { request_id }
                }
            }
            None => {
                let Some(pending) = self.pending.pop_front() else {
                    return ReplyDisposition::Missing;
                };
                if pending.state == PendingState::TimedOut {
                    ReplyDisposition::Stale {
                        request_id: Some(pending.request_id),
                    }
                } else {
                    ReplyDisposition::Current {
                        request_id: pending.request_id,
                        legacy: true,
                    }
                }
            }
        }
    }

    fn prune_at(&mut self, now: Instant) {
        self.pending.retain(|pending| pending.expires_at > now);
    }
}

/// One adapter reply, with an optional core result when the action was
/// accepted or a timeout default was applied.
#[derive(Debug)]
pub struct ReplyOutcome {
    pub ack: ActionAck,
    pub result: Option<DecisionResult>,
}

/// Transport-neutral adapter that connects wire requests/replies to the core
/// Decision boundary.  It owns no room, socket, task, or duplicate match state.
#[derive(Debug)]
pub struct MjaiAdapter {
    mode: GameMode,
    replies: ReplyTracker,
    timing: TimingBudget,
    next_request_id: u64,
}

impl MjaiAdapter {
    pub fn new(mode: GameMode) -> Self {
        Self {
            mode,
            replies: ReplyTracker::new(),
            timing: TimingBudget::new(),
            next_request_id: 1,
        }
    }

    pub const fn mode(&self) -> GameMode {
        self.mode
    }

    pub fn replies(&self) -> &ReplyTracker {
        &self.replies
    }

    pub fn classify_reply(&mut self, request_id: Option<u64>) -> ReplyDisposition {
        self.replies.classify(request_id)
    }

    pub fn peek_reply(&self, request_id: Option<u64>) -> ReplyDisposition {
        self.replies.peek(request_id)
    }

    pub fn timing(&self) -> TimingBudget {
        self.timing
    }

    pub fn reset_kyoku(&mut self) {
        self.timing.reset_kyoku();
    }

    pub fn open_request(
        &mut self,
        projection: &PlayerProjection,
        events: &[GameEvent],
    ) -> Result<RequestAction, ProtocolError> {
        self.open_request_with_time(projection, self.timing.request_time(), events)
    }

    pub fn open_request_with_time(
        &mut self,
        projection: &PlayerProjection,
        time: RequestTime,
        events: &[GameEvent],
    ) -> Result<RequestAction, ProtocolError> {
        if projection.mode != self.mode {
            return Err(ProtocolError::InvalidModeValue);
        }
        let request_id = self.next_request_id;
        let request = build_request_action(request_id, time, projection, events)?;
        self.replies.issue(request_id)?;
        self.next_request_id = self.next_request_id.saturating_add(1);
        Ok(request)
    }

    pub fn open_request_for_machine(
        &mut self,
        machine: &mut MatchMachine,
        seat: Seat,
        events: &[GameEvent],
    ) -> Result<RequestAction, ProtocolError> {
        let projection = machine
            .project(Audience::Player(seat))
            .map_err(|error| ProtocolError::Core(error.to_string()))?;
        let AudienceProjection::Player(projection) = projection else {
            return Err(ProtocolError::Observation);
        };
        self.open_request(&projection, events)
    }

    fn timing_for_current(&self) -> RequestTime {
        self.timing.request_time()
    }

    fn default_current(
        &mut self,
        machine: &mut MatchMachine,
        seat: Seat,
        request_id: u64,
        elapsed_ms: u64,
    ) -> Result<ReplyOutcome, ProtocolError> {
        let decision = machine
            .current_decision()
            .map_err(|error| ProtocolError::Core(error.to_string()))?
            .ok_or(ProtocolError::NoPendingRequest)?;
        let action_id = decision.default_action_id(seat).clone();
        let action = decision.default_for(seat).clone();
        let result = machine
            .submit_action(seat, decision.id().clone(), action_id)
            .map_err(|error| ProtocolError::Core(error.to_string()))?;
        let timing = self.timing.force_timeout(elapsed_ms);
        let ack = ActionAck::defaulted(request_id, timing, &action)?;
        Ok(ReplyOutcome {
            ack,
            result: Some(result),
        })
    }

    /// Apply one reply through the current core Decision.  Stale/future/
    /// malformed replies never mutate the MatchMachine.
    pub fn submit_reply(
        &mut self,
        machine: &mut MatchMachine,
        seat: Seat,
        bytes: &[u8],
        elapsed_ms: u64,
    ) -> Result<ReplyOutcome, ProtocolError> {
        let request_id = match request_id_from_frame(bytes) {
            Ok(request_id) => request_id,
            Err(_) => {
                return Ok(ReplyOutcome {
                    ack: ActionAck::unparseable(
                        None,
                        TimingOutcome {
                            elapsed_ms: 0,
                            bank_consumed_ms: 0,
                            bank_ms: self.timing.bank_ms(),
                            timed_out: false,
                        },
                        "action frame could not be parsed",
                    )?,
                    result: None,
                });
            }
        };
        let current = self.replies.owed_request_id();
        let request_time = self.timing_for_current();
        let preview = self.timing.preview(request_time, elapsed_ms);
        if preview.timed_out
            && let Some(current_id) = current
        {
            match request_id {
                None => {
                    self.replies.timeout_at(current_id, Instant::now())?;
                    let disposition = self.replies.classify(None);
                    if matches!(disposition, ReplyDisposition::Stale { .. }) {
                        return self.default_current(machine, seat, current_id, elapsed_ms);
                    }
                }
                Some(received) if received == current_id => {
                    self.replies.timeout_at(current_id, Instant::now())?;
                    let _ = self.replies.classify(Some(received));
                    return self.default_current(machine, seat, current_id, elapsed_ms);
                }
                _ => {}
            }
        }
        let disposition = self.replies.peek(request_id);
        match disposition {
            ReplyDisposition::Stale { request_id } => Ok(ReplyOutcome {
                ack: ActionAck::stale(request_id, self.timing.bank_ms()),
                result: None,
            }),
            ReplyDisposition::Future { request_id } => Ok(ReplyOutcome {
                ack: ActionAck::unparseable(
                    Some(request_id),
                    TimingOutcome {
                        elapsed_ms: 0,
                        bank_consumed_ms: 0,
                        bank_ms: self.timing.bank_ms(),
                        timed_out: false,
                    },
                    "unknown request_id",
                )?,
                result: None,
            }),
            ReplyDisposition::Missing => Ok(ReplyOutcome {
                ack: ActionAck::unparseable(
                    None,
                    TimingOutcome {
                        elapsed_ms: 0,
                        bank_consumed_ms: 0,
                        bank_ms: self.timing.bank_ms(),
                        timed_out: false,
                    },
                    "no pending request",
                )?,
                result: None,
            }),
            ReplyDisposition::Current {
                request_id,
                legacy: _,
            } => {
                let timing = self.timing.preview(request_time, elapsed_ms);
                let action = match parse_client_action(bytes, self.mode) {
                    Ok(action) => action,
                    Err(_) => {
                        return Ok(ReplyOutcome {
                            ack: ActionAck::unparseable(
                                Some(request_id),
                                timing,
                                "action could not be parsed",
                            )?,
                            result: None,
                        });
                    }
                };
                let decision = machine
                    .current_decision()
                    .map_err(|error| ProtocolError::Core(error.to_string()))?
                    .ok_or(ProtocolError::NoPendingRequest)?;
                let matched = match match_legal_action(self.mode, seat, &decision, &action) {
                    Ok(matched) => matched,
                    Err(ProtocolError::NoMatchingAction | ProtocolError::AmbiguousAction) => {
                        let legal_types = decision
                            .actions_for(seat)
                            .iter()
                            .map(|entry| PossibleAction::from_game_action(&entry.action))
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .map(|action| match action {
                                PossibleAction::Dahai { .. } => "dahai",
                                PossibleAction::Chi { .. } => "chi",
                                PossibleAction::Pon { .. } => "pon",
                                PossibleAction::Daiminkan { .. } => "daiminkan",
                                PossibleAction::Ankan { .. } => "ankan",
                                PossibleAction::Kakan { .. } => "kakan",
                                PossibleAction::Reach { .. } => "reach",
                                PossibleAction::Hora => "hora",
                                PossibleAction::Ryukyoku => "ryukyoku",
                                PossibleAction::Kita { .. } => "kita",
                                PossibleAction::None => "none",
                            })
                            .map(str::to_owned)
                            .collect();
                        return Ok(ReplyOutcome {
                            ack: ActionAck::rejected(request_id, timing, &action, legal_types)?,
                            result: None,
                        });
                    }
                    Err(_) => {
                        return Ok(ReplyOutcome {
                            ack: ActionAck::unparseable(
                                Some(request_id),
                                timing,
                                "action could not be matched",
                            )?,
                            result: None,
                        });
                    }
                };
                let timing = self.timing.account(request_time, elapsed_ms);
                let _ = self.replies.classify(Some(request_id));
                let result = machine
                    .submit_action(seat, decision.id().clone(), matched.action_id)
                    .map_err(|error| ProtocolError::Core(error.to_string()))?;
                Ok(ReplyOutcome {
                    ack: ActionAck::accepted(request_id, timing),
                    result: Some(result),
                })
            }
        }
    }

    /// Mark the oldest request timed out and apply the core's deterministic
    /// default (tsumogiri for a turn, none for a response).
    pub fn timeout_request(
        &mut self,
        machine: &mut MatchMachine,
        seat: Seat,
        elapsed_ms: u64,
    ) -> Result<ReplyOutcome, ProtocolError> {
        let request_id = self
            .replies
            .owed_request_id()
            .ok_or(ProtocolError::NoPendingRequest)?;
        self.replies.timeout(request_id)?;
        self.default_current(machine, seat, request_id, elapsed_ms)
    }
}

/// Names retained for callers that use the upstream terminology.
pub type MjaiEvent = ServerEvent;
pub type MjaiAction = ClientAction;
pub type ProtocolRequest = RequestAction;
pub type ProtocolAck = ActionAck;
pub type LegacyReplyState = ReplyTracker;
pub type ProtocolAdapter = MjaiAdapter;
pub type Timing = RequestTime;

trait CanonicalizeForAdapter {
    fn canonicalize_for_adapter(self) -> Self;
}

impl CanonicalizeForAdapter for GameAction {
    fn canonicalize_for_adapter(self) -> Self {
        match self {
            Self::Chi {
                target,
                called,
                mut consumed,
            } => {
                consumed.sort_by_key(|tile| tile.canonical_key());
                Self::Chi {
                    target,
                    called,
                    consumed,
                }
            }
            Self::Pon {
                target,
                called,
                mut consumed,
            } => {
                consumed.sort_by_key(|tile| tile.canonical_key());
                Self::Pon {
                    target,
                    called,
                    consumed,
                }
            }
            Self::Daiminkan {
                target,
                called,
                mut consumed,
            } => {
                consumed.sort_by_key(|tile| tile.canonical_key());
                Self::Daiminkan {
                    target,
                    called,
                    consumed,
                }
            }
            Self::Ankan { mut consumed } => {
                consumed.sort_by_key(|tile| tile.canonical_key());
                Self::Ankan { consumed }
            }
            Self::Kakan {
                called,
                mut consumed,
            } => {
                consumed.sort_by_key(|tile| tile.canonical_key());
                Self::Kakan { called, consumed }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn peek_does_not_consume_a_current_reply() {
        let mut tracker = ReplyTracker::new();
        tracker.issue(42).expect("test request should issue");
        assert!(matches!(
            tracker.peek(Some(42)),
            ReplyDisposition::Current { request_id: 42, .. }
        ));
        assert_eq!(tracker.current_request_id(), Some(42));
    }

    use super::*;
    use double_riichi_core::{
        Audience, DecisionId, DecisionKind, Participant, ParticipantKind, TablePlayerState,
        TableState,
    };

    fn seat(index: u8) -> Seat {
        Seat::new(index).expect("valid seat")
    }

    fn tile(id: u8) -> Tile {
        Tile::from_id(id).expect("valid tile")
    }

    fn projection(mode: GameMode) -> PlayerProjection {
        let players = (0..mode.seat_count())
            .map(|index| {
                let participant = Participant::new(
                    format!("p{index}"),
                    format!("Player {index}"),
                    ParticipantKind::Human,
                );
                let mut player = TablePlayerState::new(
                    seat(index as u8),
                    participant,
                    25_000,
                    vec![tile(index as u8), tile(index as u8 + 1)],
                );
                player.discards.push(tile(32 + index as u8));
                player
            })
            .collect();
        let state = TableState::new(mode, players, vec![tile(0)]).expect("table");
        match state.project(Audience::Player(seat(0))) {
            AudienceProjection::Player(projection) => projection,
            _ => unreachable!(),
        }
    }

    fn decision(seat: Seat, mut actions: Vec<GameAction>) -> Decision {
        if !actions
            .iter()
            .any(|action| matches!(action, GameAction::Pass | GameAction::Discard { .. }))
        {
            actions.push(GameAction::Pass);
        }
        Decision::new(
            DecisionId::new("d1"),
            DecisionKind::Turn,
            vec![(seat, actions)],
            Instant::now().into(),
            Some(Duration::from_secs(30)),
            false,
        )
        .expect("decision")
    }

    #[test]
    fn tile_mapping_preserves_red_and_honor_spellings() {
        for (id, spelling) in [
            (16, "5mr"),
            (52, "5pr"),
            (88, "5sr"),
            (108, "E"),
            (132, "C"),
        ] {
            assert_eq!(tile_to_mjai(tile(id)), spelling);
            assert_eq!(parse_tile(spelling).expect("round trip"), tile(id));
        }
        assert_ne!(parse_tile("5m").unwrap(), tile(16));
    }

    #[test]
    fn event_encoding_masks_start_hands_and_opponent_draws() {
        let event = GameEvent::StartKyoku {
            bakaze: double_riichi_core::Wind::East,
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: seat(0),
            scores: vec![25_000; 4],
            dora_marker: tile(8),
            tehais: vec![
                vec![tile(16)],
                vec![tile(52)],
                vec![tile(88)],
                vec![tile(0)],
            ],
        };
        let encoded = encode_event(&event, seat(0), GameMode::FourPlayerRedEast).unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["tehais"][0][0], "5mr");
        assert_eq!(value["tehais"][1][0], "?");
        let draw = encode_event(
            &GameEvent::Tsumo {
                actor: seat(1),
                tile: tile(52),
            },
            seat(0),
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&draw).unwrap()["pai"], "?");
    }

    #[test]
    fn unknown_server_event_is_tolerated_but_unknown_client_field_is_not() {
        let unknown = parse_server_message(
            br#"{"type":"future_event","private_state":{"hand":[1,2,3]}}"#,
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        assert_eq!(
            unknown,
            ServerMessage::Opaque {
                event_type: "future_event".into()
            }
        );
        let bad = parse_client_action(
            br#"{"type":"none","extra":"reject"}"#,
            GameMode::FourPlayerRedEast,
        );
        assert!(bad.is_err());
    }

    #[test]
    fn request_action_is_bounded_and_round_trips_observation() {
        let projection = projection(GameMode::FourPlayerRedEast);
        let encoded = encode_observation(&projection, &[]).unwrap();
        let request = build_request_action(42, RequestTime::default(), &projection, &[]).unwrap();
        assert_eq!(request.request_id, 42);
        let payload = serde_json::to_vec(&request).unwrap();
        let parsed = parse_request_action(&payload, GameMode::FourPlayerRedEast).unwrap();
        assert_eq!(parsed.observation, encoded);
        match decode_observation(&parsed.observation, GameMode::FourPlayerRedEast).unwrap() {
            ObservationPayload::FourPlayer(observation) => {
                assert_eq!(observation.hands[0], vec![0, 1]);
                assert!(observation.hands[1].is_empty());
            }
            ObservationPayload::ThreePlayer(_) => panic!("wrong observation variant"),
        }
    }

    #[test]
    fn observation_does_not_copy_opponent_concealed_hands() {
        let projection = projection(GameMode::ThreePlayerRedEast);
        let encoded = encode_observation(&projection, &[]).unwrap();
        let ObservationPayload::ThreePlayer(observation) =
            decode_observation(&encoded, GameMode::ThreePlayerRedEast).unwrap()
        else {
            panic!("wrong observation variant")
        };
        assert_eq!(observation.hands[0], vec![0, 1]);
        assert!(observation.hands[1].is_empty());
        assert!(observation.hands[2].is_empty());
    }

    #[test]
    fn every_core_action_has_a_wire_offer_and_can_match() {
        let cases = vec![
            (
                GameAction::Discard {
                    tile: tile(17),
                    tsumogiri: false,
                },
                r#"{"type":"dahai","pai":"5m","tsumogiri":false}"#,
            ),
            (
                GameAction::Chi {
                    target: seat(1),
                    called: tile(4),
                    consumed: vec![tile(0), tile(8)],
                },
                r#"{"type":"chi","actor":0,"target":1,"pai":"2m","consumed":["1m","3m"]}"#,
            ),
            (
                GameAction::Pon {
                    target: seat(1),
                    called: tile(108),
                    consumed: vec![tile(109), tile(110)],
                },
                r#"{"type":"pon","target":1,"pai":"E","consumed":["E","E"]}"#,
            ),
            (
                GameAction::Daiminkan {
                    target: seat(1),
                    called: tile(52),
                    consumed: vec![tile(53), tile(54), tile(55)],
                },
                r#"{"type":"daiminkan","target":1,"pai":"5pr","consumed":["5p","5p","5p"]}"#,
            ),
            (
                GameAction::Ankan {
                    consumed: vec![tile(0), tile(1), tile(2), tile(3)],
                },
                r#"{"type":"ankan","consumed":["1m","1m","1m","1m"]}"#,
            ),
            (
                GameAction::Kakan {
                    called: tile(88),
                    consumed: vec![tile(89), tile(90), tile(91)],
                },
                r#"{"type":"kakan","pai":"5sr","consumed":["5s","5s","5s"]}"#,
            ),
            (
                GameAction::RiichiDiscard { tile: tile(12) },
                r#"{"type":"reach","pai":"4m"}"#,
            ),
            (GameAction::Tsumo, r#"{"type":"hora"}"#),
            (GameAction::Ron(seat(1)), r#"{"type":"hora","target":1}"#),
            (GameAction::AbortiveDraw, r#"{"type":"ryukyoku"}"#),
            (GameAction::Pass, r#"{"type":"none"}"#),
        ];
        for (expected, json) in cases {
            let action = parse_client_action(json.as_bytes(), GameMode::FourPlayerRedEast).unwrap();
            let decision = decision(seat(0), vec![expected.clone()]);
            let matched =
                match_legal_action(GameMode::FourPlayerRedEast, seat(0), &decision, &action)
                    .unwrap();
            assert_eq!(matched.action, expected);
        }
    }

    #[test]
    fn discard_matching_keeps_red_and_tsumogiri_distinct() {
        let decision = decision(
            seat(0),
            vec![
                GameAction::Discard {
                    tile: tile(16),
                    tsumogiri: false,
                },
                GameAction::Discard {
                    tile: tile(17),
                    tsumogiri: false,
                },
                GameAction::Discard {
                    tile: tile(17),
                    tsumogiri: true,
                },
            ],
        );
        let red = parse_client_action(
            br#"{"type":"dahai","pai":"5mr","tsumogiri":false}"#,
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        assert_eq!(
            match_legal_action(GameMode::FourPlayerRedEast, seat(0), &decision, &red)
                .unwrap()
                .action,
            GameAction::Discard {
                tile: tile(16),
                tsumogiri: false
            }
        );
        let ambiguous = parse_client_action(
            br#"{"type":"dahai","pai":"5m"}"#,
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        assert_eq!(
            match_legal_action(GameMode::FourPlayerRedEast, seat(0), &decision, &ambiguous),
            Err(ProtocolError::AmbiguousAction)
        );
        let tsumogiri = parse_client_action(
            br#"{"type":"dahai","pai":"5m","tsumogiri":true}"#,
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        assert_eq!(
            match_legal_action(GameMode::FourPlayerRedEast, seat(0), &decision, &tsumogiri)
                .unwrap()
                .action,
            GameAction::Discard {
                tile: tile(17),
                tsumogiri: true
            }
        );
    }

    #[test]
    fn reply_tracker_handles_current_stale_future_and_fifo_legacy_debt() {
        let now = Instant::now();
        let mut tracker = ReplyTracker::new();
        tracker.issue_at(10, now).unwrap();
        assert_eq!(
            tracker.classify_at(Some(10), now),
            ReplyDisposition::Current {
                request_id: 10,
                legacy: false
            }
        );
        assert_eq!(
            tracker.classify_at(Some(10), now),
            ReplyDisposition::Stale {
                request_id: Some(10)
            }
        );
        tracker.issue_at(11, now).unwrap();
        assert_eq!(
            tracker.classify_at(Some(12), now),
            ReplyDisposition::Future { request_id: 12 }
        );
        tracker.timeout_at(11, now).unwrap();
        tracker.issue_at(13, now).unwrap();
        assert_eq!(
            tracker.classify_at(None, now),
            ReplyDisposition::Stale {
                request_id: Some(11)
            }
        );
        assert_eq!(
            tracker.classify_at(None, now),
            ReplyDisposition::Current {
                request_id: 13,
                legacy: true
            }
        );
        tracker.issue_at(14, now).unwrap();
        tracker.timeout_at(14, now).unwrap();
        assert_eq!(
            tracker.classify_at(None, now + LEGACY_REPLY_TTL + Duration::from_millis(1)),
            ReplyDisposition::Missing
        );
    }

    #[test]
    fn timing_defaults_grace_bank_and_timeout_zero() {
        let time = RequestTime::default();
        let mut budget = TimingBudget::new();
        assert_eq!(budget.preview(time, 2_999).bank_consumed_ms, 0);
        let within = budget.account(time, 4_000);
        assert_eq!(within.bank_consumed_ms, 1_000);
        assert_eq!(within.bank_ms, 14_000);
        let timeout = budget.account(
            RequestTime {
                grace_ms: 3_000,
                bank_ms: 14_000,
                deadline_ms: 17_000,
            },
            17_000,
        );
        assert!(timeout.timed_out);
        assert_eq!(timeout.bank_ms, 0);
        assert_eq!(budget.bank_ms(), 0);
    }

    #[test]
    fn unlimited_time_is_valid_and_never_times_out_in_the_adapter_budget() {
        let time = RequestTime::unlimited();
        assert!(time.validate().is_ok());
        let budget = TimingBudget::new();
        assert_eq!(
            budget.preview(time, u64::MAX),
            TimingOutcome {
                elapsed_ms: u64::MAX,
                bank_consumed_ms: 0,
                bank_ms: 0,
                timed_out: false,
            }
        );
    }

    #[test]
    fn rejected_and_unparseable_current_replies_leave_request_owed() {
        let mode = GameMode::FourPlayerRedEast;
        let participants = (0..mode.seat_count())
            .map(|index| {
                Participant::new(
                    format!("p{index}"),
                    format!("Player {index}"),
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect();
        let mut machine = MatchMachine::with_seed(mode, participants, 0xD0).unwrap();
        let seat = Seat::new(0).unwrap();
        let mut adapter = MjaiAdapter::new(mode);
        let request = adapter
            .open_request_for_machine(&mut machine, seat, &[])
            .unwrap();
        let request_id = request.request_id;
        let before_bank = adapter.timing().bank_ms();
        let malformed = format!(r#"{{"type":"not_an_action","request_id":{request_id}}}"#);
        let outcome = adapter
            .submit_reply(&mut machine, seat, malformed.as_bytes(), 4_000)
            .unwrap();
        assert_eq!(outcome.ack.status, AckStatus::Unparseable);
        assert_eq!(adapter.replies().current_request_id(), Some(request_id));
        assert_eq!(adapter.timing().bank_ms(), before_bank);

        let rejected = format!(r#"{{"type":"none","request_id":{request_id}}}"#);
        let outcome = adapter
            .submit_reply(&mut machine, seat, rejected.as_bytes(), 4_000)
            .unwrap();
        assert_eq!(outcome.ack.status, AckStatus::Rejected);
        assert_eq!(adapter.replies().current_request_id(), Some(request_id));
        assert_eq!(adapter.timing().bank_ms(), before_bank);
    }

    #[test]
    fn malformed_and_oversized_frames_are_rejected_before_deserialization() {
        assert_eq!(
            parse_client_action(b"not-json", GameMode::FourPlayerRedEast),
            Err(ProtocolError::MalformedJson)
        );
        let oversized = vec![b' '; MAX_FRAME_BYTES + 1];
        assert_eq!(
            parse_client_action(&oversized, GameMode::FourPlayerRedEast),
            Err(ProtocolError::FrameTooLarge {
                actual: MAX_FRAME_BYTES + 1,
                max: MAX_FRAME_BYTES
            })
        );
        assert!(
            parse_client_action(
                br#"{"type":"ankan","consumed":["1m","1m","1m"]}"#,
                GameMode::FourPlayerRedEast
            )
            .is_err()
        );
        assert!(
            parse_client_action(
                br#"{"type":"dahai","pai":"2m"}"#,
                GameMode::ThreePlayerRedEast
            )
            .is_err()
        );
    }
}
