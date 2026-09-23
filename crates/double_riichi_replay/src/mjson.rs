use std::collections::HashSet;

use double_riichi_core::{GameEvent, GameMode, Seat, Tile, Wind};
use serde_json::{Map, Value, json};

use crate::error::{MAX_REPLAY_EVENTS, ReplayError};

pub type CanonicalEvent = GameEvent;
pub type MjsonEvent = CanonicalEvent;
pub type ReplayEvent = CanonicalEvent;

/// Serialize one canonical engine event in the pinned MJAI/MJSON vocabulary.
pub fn serialize_event(event: &CanonicalEvent) -> Result<String, ReplayError> {
    serde_json::to_string(&event_value(event)).map_err(ReplayError::Json)
}

/// Parse and validate a complete newline-delimited canonical replay.
pub fn parse_mjson(input: impl AsRef<str>) -> Result<Vec<CanonicalEvent>, ReplayError> {
    let input = input.as_ref();
    let mut events = Vec::new();
    let mut mode: Option<GameMode> = None;
    for (index, line) in input.lines().enumerate() {
        if events.len() >= MAX_REPLAY_EVENTS {
            return Err(ReplayError::ReplayTooLarge {
                actual: events.len().saturating_add(1),
                limit: MAX_REPLAY_EVENTS,
            });
        }
        if line.trim().is_empty() {
            return Err(ReplayError::Corrupt {
                line: index + 1,
                message: "empty JSON line".into(),
            });
        }
        let value: Value = serde_json::from_str(line).map_err(|error| ReplayError::Corrupt {
            line: index + 1,
            message: error.to_string(),
        })?;
        let event = parse_value(&value).map_err(|message| ReplayError::Corrupt {
            line: index + 1,
            message,
        })?;
        if let GameEvent::StartKyoku { scores, .. } = &event {
            let inferred = match scores.len() {
                3 => GameMode::ThreePlayerRedEast,
                4 => GameMode::FourPlayerRedEast,
                count => {
                    return Err(ReplayError::Corrupt {
                        line: index + 1,
                        message: format!("start_kyoku has unsupported score count {count}"),
                    });
                }
            };
            if let Some(previous) = mode
                && previous.seat_count() != inferred.seat_count()
            {
                return Err(ReplayError::Corrupt {
                    line: index + 1,
                    message: "replay changes player count between kyoku events".into(),
                });
            }
            mode = Some(inferred);
        }
        if let Some(mode) = mode {
            validate_event(&event, mode).map_err(|message| ReplayError::Corrupt {
                line: index + 1,
                message,
            })?;
        }
        events.push(event);
    }
    if events.is_empty() {
        return Err(ReplayError::Corrupt {
            line: 0,
            message: "replay contains no events".into(),
        });
    }
    if mode.is_none() {
        return Err(ReplayError::Corrupt {
            line: 0,
            message: "replay contains no start_kyoku event".into(),
        });
    }
    if !matches!(events.last(), Some(GameEvent::EndGame)) {
        return Err(ReplayError::Corrupt {
            line: events.len(),
            message: "replay does not terminate with end_game".into(),
        });
    }
    if let Some(mode) = mode {
        for (index, event) in events.iter().enumerate() {
            validate_event(event, mode).map_err(|message| ReplayError::Corrupt {
                line: index + 1,
                message,
            })?;
        }
    }
    Ok(events)
}

pub fn validate_canonical_events(
    events: &[CanonicalEvent],
    mode: GameMode,
) -> Result<(), ReplayError> {
    if events.len() > MAX_REPLAY_EVENTS {
        return Err(ReplayError::ReplayTooLarge {
            actual: events.len(),
            limit: MAX_REPLAY_EVENTS,
        });
    }
    for (index, event) in events.iter().enumerate() {
        validate_event(event, mode).map_err(|message| ReplayError::Corrupt {
            line: index + 1,
            message,
        })?;
    }
    Ok(())
}

pub fn parse_event_line(line: &str, mode: Option<GameMode>) -> Result<CanonicalEvent, ReplayError> {
    let value: Value = serde_json::from_str(line)?;
    let event = parse_value(&value).map_err(ReplayError::InvalidEvent)?;
    if let Some(mode) = mode {
        validate_event(&event, mode).map_err(ReplayError::InvalidEvent)?;
    }
    Ok(event)
}

pub(crate) fn event_value(event: &CanonicalEvent) -> Value {
    let mut object = Map::new();
    match event {
        GameEvent::StartGame { names, id } => {
            object.insert("type".into(), json!("start_game"));
            object.insert("names".into(), json!(names));
            object.insert("id".into(), json!(id));
        }
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
            object.insert("type".into(), json!("start_kyoku"));
            object.insert("bakaze".into(), json!(wind_name(*bakaze)));
            object.insert("kyoku".into(), json!(kyoku));
            object.insert("honba".into(), json!(honba));
            object.insert("kyoutaku".into(), json!(kyotaku));
            object.insert("oya".into(), json!(oya.index()));
            object.insert("scores".into(), json!(scores));
            object.insert("dora_marker".into(), json!(tile_name(*dora_marker)));
            object.insert(
                "tehais".into(),
                json!(
                    tehais
                        .iter()
                        .map(|hand| hand.iter().copied().map(tile_name).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                ),
            );
        }
        GameEvent::Tsumo { actor, tile } => {
            object.insert("type".into(), json!("tsumo"));
            object.insert("actor".into(), json!(actor.index()));
            object.insert("pai".into(), json!(tile_name(*tile)));
        }
        GameEvent::Dahai {
            actor,
            tile,
            tsumogiri,
        } => {
            object.insert("type".into(), json!("dahai"));
            object.insert("actor".into(), json!(actor.index()));
            object.insert("pai".into(), json!(tile_name(*tile)));
            object.insert("tsumogiri".into(), json!(tsumogiri));
        }
        GameEvent::Pon {
            actor,
            target,
            called,
            consumed,
        } => call_value(&mut object, "pon", *actor, *target, *called, consumed),
        GameEvent::Chi {
            actor,
            target,
            called,
            consumed,
        } => call_value(&mut object, "chi", *actor, *target, *called, consumed),
        GameEvent::Daiminkan {
            actor,
            target,
            called,
            consumed,
        } => call_value(&mut object, "kan", *actor, *target, *called, consumed),
        GameEvent::Kakan {
            actor,
            called,
            consumed,
        } => {
            object.insert("type".into(), json!("kakan"));
            object.insert("actor".into(), json!(actor.index()));
            object.insert("pai".into(), json!(tile_name(*called)));
            object.insert(
                "consumed".into(),
                json!(consumed.iter().copied().map(tile_name).collect::<Vec<_>>()),
            );
        }
        GameEvent::Ankan { actor, consumed } => {
            object.insert("type".into(), json!("ankan"));
            object.insert("actor".into(), json!(actor.index()));
            object.insert(
                "consumed".into(),
                json!(consumed.iter().copied().map(tile_name).collect::<Vec<_>>()),
            );
        }
        GameEvent::Dora { dora_marker } => {
            object.insert("type".into(), json!("dora"));
            object.insert("dora_marker".into(), json!(tile_name(*dora_marker)));
        }
        GameEvent::Reach { actor } => {
            object.insert("type".into(), json!("reach"));
            object.insert("actor".into(), json!(actor.index()));
        }
        GameEvent::ReachAccepted { actor } => {
            object.insert("type".into(), json!("reach_accepted"));
            object.insert("actor".into(), json!(actor.index()));
        }
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
        } => {
            object.insert("type".into(), json!("hora"));
            object.insert("actor".into(), json!(actor.index()));
            object.insert("target".into(), json!(target.index()));
            object.insert("pai".into(), json!(tile.map(tile_name)));
            object.insert(
                "uradora_markers".into(),
                json!(
                    ura_markers
                        .as_ref()
                        .map(|tiles| { tiles.iter().copied().map(tile_name).collect::<Vec<_>>() })
                ),
            );
            object.insert("yaku".into(), json!(yaku));
            object.insert("fu".into(), json!(fu));
            object.insert("han".into(), json!(han));
            object.insert("scores".into(), json!(scores));
            object.insert("delta".into(), json!(delta));
        }
        GameEvent::Ryukyoku {
            reason,
            tehais,
            delta,
            scores,
        } => {
            object.insert("type".into(), json!("ryukyoku"));
            object.insert("reason".into(), json!(reason));
            object.insert(
                "tehais".into(),
                json!(tehais.as_ref().map(|hands| {
                    hands
                        .iter()
                        .map(|hand| hand.iter().copied().map(tile_name).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                })),
            );
            object.insert("delta".into(), json!(delta));
            object.insert("scores".into(), json!(scores));
        }
        GameEvent::Kita { actor } => {
            object.insert("type".into(), json!("kita"));
            object.insert("actor".into(), json!(actor.index()));
        }
        GameEvent::EndKyoku => {
            object.insert("type".into(), json!("end_kyoku"));
        }
        GameEvent::EndGame => {
            object.insert("type".into(), json!("end_game"));
        }
    }
    Value::Object(object)
}

fn call_value(
    object: &mut Map<String, Value>,
    kind: &str,
    actor: Seat,
    target: Seat,
    called: Tile,
    consumed: &[Tile],
) {
    object.insert("type".into(), json!(kind));
    object.insert("actor".into(), json!(actor.index()));
    object.insert("target".into(), json!(target.index()));
    object.insert("pai".into(), json!(tile_name(called)));
    object.insert(
        "consumed".into(),
        json!(consumed.iter().copied().map(tile_name).collect::<Vec<_>>()),
    );
}

fn parse_value(value: &Value) -> Result<CanonicalEvent, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "event must be a JSON object".to_owned())?;
    let kind = required_string(object, "type")?;
    match kind {
        "start_game" => {
            check_fields(object, &["type", "names", "id"])?;
            Ok(GameEvent::StartGame {
                names: optional_string_vec(object, "names")?,
                id: optional_string(object, "id")?,
            })
        }
        "start_kyoku" => {
            check_fields(
                object,
                &[
                    "type",
                    "bakaze",
                    "kyoku",
                    "honba",
                    "kyoutaku",
                    "oya",
                    "scores",
                    "dora_marker",
                    "tehais",
                ],
            )?;
            let scores = required_i32_vec(object, "scores")?;
            let tehais = required_tile_hands(object, "tehais")?;
            if tehais.len() != scores.len() {
                return Err("start_kyoku hands and scores have different lengths".into());
            }
            Ok(GameEvent::StartKyoku {
                bakaze: parse_wind(required_string(object, "bakaze")?)?,
                kyoku: required_u8(object, "kyoku")?,
                honba: required_u8(object, "honba")?,
                kyotaku: required_u8(object, "kyoutaku")?,
                oya: parse_seat(usize::from(required_u8(object, "oya")?))?,
                scores,
                dora_marker: parse_tile(required_string(object, "dora_marker")?)?,
                tehais,
            })
        }
        "tsumo" => {
            check_fields(object, &["type", "actor", "pai"])?;
            Ok(GameEvent::Tsumo {
                actor: parse_seat(required_usize(object, "actor")?)?,
                tile: parse_tile(required_string(object, "pai")?)?,
            })
        }
        "dahai" => {
            check_fields(object, &["type", "actor", "pai", "tsumogiri"])?;
            Ok(GameEvent::Dahai {
                actor: parse_seat(required_usize(object, "actor")?)?,
                tile: parse_tile(required_string(object, "pai")?)?,
                tsumogiri: required_bool(object, "tsumogiri")?,
            })
        }
        "pon" | "chi" | "kan" | "daiminkan" => {
            check_fields(object, &["type", "actor", "target", "pai", "consumed"])?;
            let actor = parse_seat(required_usize(object, "actor")?)?;
            let target = parse_seat(required_usize(object, "target")?)?;
            let called = parse_tile(required_string(object, "pai")?)?;
            let consumed = required_tiles(object, "consumed")?;
            match kind {
                "pon" => Ok(GameEvent::Pon {
                    actor,
                    target,
                    called,
                    consumed,
                }),
                "chi" => Ok(GameEvent::Chi {
                    actor,
                    target,
                    called,
                    consumed,
                }),
                _ => Ok(GameEvent::Daiminkan {
                    actor,
                    target,
                    called,
                    consumed,
                }),
            }
        }
        "kakan" => {
            check_fields(object, &["type", "actor", "pai", "consumed"])?;
            Ok(GameEvent::Kakan {
                actor: parse_seat(required_usize(object, "actor")?)?,
                called: parse_tile(required_string(object, "pai")?)?,
                consumed: required_tiles(object, "consumed")?,
            })
        }
        "ankan" => {
            check_fields(object, &["type", "actor", "consumed"])?;
            Ok(GameEvent::Ankan {
                actor: parse_seat(required_usize(object, "actor")?)?,
                consumed: required_tiles(object, "consumed")?,
            })
        }
        "dora" => {
            check_fields(object, &["type", "dora_marker"])?;
            Ok(GameEvent::Dora {
                dora_marker: parse_tile(required_string(object, "dora_marker")?)?,
            })
        }
        "reach" => {
            check_fields(object, &["type", "actor"])?;
            Ok(GameEvent::Reach {
                actor: parse_seat(required_usize(object, "actor")?)?,
            })
        }
        "reach_accepted" => {
            check_fields(object, &["type", "actor"])?;
            Ok(GameEvent::ReachAccepted {
                actor: parse_seat(required_usize(object, "actor")?)?,
            })
        }
        "hora" => {
            check_fields(
                object,
                &[
                    "type",
                    "actor",
                    "target",
                    "pai",
                    "uradora_markers",
                    "yaku",
                    "fu",
                    "han",
                    "scores",
                    "delta",
                ],
            )?;
            Ok(GameEvent::Hora {
                actor: parse_seat(required_usize(object, "actor")?)?,
                target: parse_seat(required_usize(object, "target")?)?,
                tile: optional_string(object, "pai")?
                    .map(|tile| parse_tile(&tile))
                    .transpose()?,
                ura_markers: optional_string_vec(object, "uradora_markers")?
                    .map(|tiles| tiles.iter().map(|tile| parse_tile(tile)).collect())
                    .transpose()?,
                yaku: optional_yaku(object, "yaku")?,
                fu: optional_u32(object, "fu")?,
                han: optional_u32(object, "han")?,
                scores: optional_i32_vec(object, "scores")?,
                delta: optional_i32_vec(object, "delta")?,
            })
        }
        "ryukyoku" => {
            check_fields(object, &["type", "reason", "tehais", "delta", "scores"])?;
            Ok(GameEvent::Ryukyoku {
                reason: optional_string(object, "reason")?,
                tehais: optional_tile_hands(object, "tehais")?,
                delta: optional_i32_vec(object, "delta")?,
                scores: optional_i32_vec(object, "scores")?,
            })
        }
        "kita" => {
            check_fields(object, &["type", "actor"])?;
            Ok(GameEvent::Kita {
                actor: parse_seat(required_usize(object, "actor")?)?,
            })
        }
        "end_kyoku" => {
            check_fields(object, &["type"])?;
            Ok(GameEvent::EndKyoku)
        }
        "end_game" => {
            check_fields(object, &["type"])?;
            Ok(GameEvent::EndGame)
        }
        other => Err(format!("unsupported event type {other}")),
    }
}

pub(crate) fn validate_event(event: &CanonicalEvent, mode: GameMode) -> Result<(), String> {
    let seat = |seat: Seat| {
        if usize::from(seat.index()) < mode.seat_count() {
            Ok(())
        } else {
            Err(format!("seat {} is invalid for {mode}", seat.index()))
        }
    };
    let tile = |tile: Tile| {
        if tile.is_valid_for(mode) {
            Ok(())
        } else {
            Err(format!("tile {} is invalid for {mode}", tile.id()))
        }
    };
    let tiles = |tiles: &[Tile]| tiles.iter().copied().try_for_each(tile);
    match event {
        GameEvent::StartGame { .. } | GameEvent::EndKyoku | GameEvent::EndGame => Ok(()),
        GameEvent::StartKyoku {
            oya,
            scores,
            dora_marker,
            tehais,
            ..
        } => {
            if scores.len() != mode.seat_count() || tehais.len() != mode.seat_count() {
                return Err(format!("start_kyoku player count does not match {mode}"));
            }
            seat(*oya)?;
            tile(*dora_marker)?;
            for hand in tehais {
                if hand.len() != 13 {
                    return Err("start_kyoku hand must contain 13 tiles".into());
                }
                tiles(hand)?;
            }
            Ok(())
        }
        GameEvent::Tsumo { actor, tile: value }
        | GameEvent::Dahai {
            actor, tile: value, ..
        } => {
            seat(*actor)?;
            tile(*value)
        }
        GameEvent::Pon {
            actor,
            target,
            called,
            consumed,
        }
        | GameEvent::Chi {
            actor,
            target,
            called,
            consumed,
        }
        | GameEvent::Daiminkan {
            actor,
            target,
            called,
            consumed,
        } => {
            seat(*actor)?;
            seat(*target)?;
            tile(*called)?;
            tiles(consumed)?;
            let expected = if matches!(event, GameEvent::Daiminkan { .. }) {
                3
            } else {
                2
            };
            if consumed.len() != expected {
                return Err(format!(
                    "call consumed {} tiles, expected {expected}",
                    consumed.len()
                ));
            }
            Ok(())
        }
        GameEvent::Kakan {
            actor,
            called,
            consumed,
        } => {
            seat(*actor)?;
            tile(*called)?;
            tiles(consumed)?;
            if consumed.len() != 3 {
                return Err("kakan consumed must contain three tiles".into());
            }
            Ok(())
        }
        GameEvent::Ankan { actor, consumed } => {
            seat(*actor)?;
            tiles(consumed)?;
            if consumed.len() != 4 {
                return Err("ankan must consume four tiles".into());
            }
            Ok(())
        }
        GameEvent::Dora { dora_marker } => tile(*dora_marker),
        GameEvent::Reach { actor } | GameEvent::ReachAccepted { actor } => seat(*actor),
        GameEvent::Kita { actor } => {
            if !mode.is_three_player() {
                return Err("kita is only valid in three-player mode".into());
            }
            seat(*actor)
        }
        GameEvent::Hora {
            actor,
            target,
            tile: winning,
            ura_markers,
            scores,
            delta,
            ..
        } => {
            seat(*actor)?;
            seat(*target)?;
            if let Some(winning) = winning {
                tile(*winning)?;
            }
            if let Some(ura_markers) = ura_markers {
                tiles(ura_markers)?;
            }
            if let Some(scores) = scores
                && scores.len() != mode.seat_count()
            {
                return Err("hora scores have the wrong player count".into());
            }
            if let Some(delta) = delta
                && delta.len() != mode.seat_count()
            {
                return Err("hora delta has the wrong player count".into());
            }
            Ok(())
        }
        GameEvent::Ryukyoku {
            tehais,
            delta,
            scores,
            ..
        } => {
            if let Some(hands) = tehais {
                if hands.len() != mode.seat_count() {
                    return Err("ryukyoku hands have the wrong player count".into());
                }
                for hand in hands {
                    tiles(hand)?;
                }
            }
            if let Some(delta) = delta
                && delta.len() != mode.seat_count()
            {
                return Err("ryukyoku delta has the wrong player count".into());
            }
            if let Some(scores) = scores
                && scores.len() != mode.seat_count()
            {
                return Err("ryukyoku scores have the wrong player count".into());
            }
            Ok(())
        }
    }
}

fn tile_name(tile: Tile) -> String {
    let id = tile.id();
    if id == 16 {
        return "5mr".into();
    }
    if id == 52 {
        return "5pr".into();
    }
    if id == 88 {
        return "5sr".into();
    }
    if id < 108 {
        let suit = match id / 36 {
            0 => 'm',
            1 => 'p',
            _ => 's',
        };
        let number = (id % 36) / 4 + 1;
        return format!("{number}{suit}");
    }
    let honor = ["E", "S", "W", "N", "P", "F", "C"];
    honor[((id - 108) / 4) as usize].to_owned()
}

fn parse_tile(value: &str) -> Result<Tile, String> {
    let id = match value {
        "5mr" => 16,
        "5pr" => 52,
        "5sr" => 88,
        "E" => 108,
        "S" => 112,
        "W" => 116,
        "N" => 120,
        "P" => 124,
        "F" => 128,
        "C" => 132,
        _ => {
            let mut chars = value.chars();
            let number = chars
                .next()
                .and_then(|character| character.to_digit(10))
                .ok_or_else(|| format!("invalid tile {value}"))? as u8;
            let suit = chars
                .next()
                .ok_or_else(|| format!("invalid tile {value}"))?;
            if chars.next().is_some() || !(1..=9).contains(&number) {
                return Err(format!("invalid tile {value}"));
            }
            if suit == 'z' {
                if number > 7 {
                    return Err(format!("invalid tile {value}"));
                }
                return Tile::from_id(108 + (number - 1) * 4)
                    .ok_or_else(|| format!("invalid tile {value}"));
            }
            let suit_index = match suit {
                'm' => 0,
                'p' => 1,
                's' => 2,
                _ => return Err(format!("invalid tile {value}")),
            };
            let base = suit_index * 36 + (number - 1) * 4;
            if number == 5 { base + 1 } else { base }
        }
    };
    Tile::from_id(id).ok_or_else(|| format!("invalid tile {value}"))
}

fn wind_name(wind: Wind) -> &'static str {
    match wind {
        Wind::East => "E",
        Wind::South => "S",
        Wind::West => "W",
        Wind::North => "N",
    }
}

fn parse_wind(value: &str) -> Result<Wind, String> {
    match value {
        "E" => Ok(Wind::East),
        "S" => Ok(Wind::South),
        "W" => Ok(Wind::West),
        "N" => Ok(Wind::North),
        _ => Err(format!("invalid wind {value}")),
    }
}

fn check_fields(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    let allowed: HashSet<_> = allowed.iter().copied().collect();
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!("unknown event field {field}"));
    }
    Ok(())
}

fn required_string<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} must be a string"))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("{field} must be a string or null"))
            .map(Some),
    }
}

fn optional_string_vec(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<Vec<String>>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_array()
            .ok_or_else(|| format!("{field} must be an array or null"))?
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{field} contains a non-string"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
    }
}

fn required_bool(object: &Map<String, Value>, field: &str) -> Result<bool, String> {
    object
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{field} must be a boolean"))
}

fn required_u8(object: &Map<String, Value>, field: &str) -> Result<u8, String> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{field} must be an unsigned integer"))?;
    u8::try_from(value).map_err(|_| format!("{field} is out of range"))
}

fn required_u32(object: &Map<String, Value>, field: &str) -> Result<u32, String> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{field} must be an unsigned integer"))?;
    u32::try_from(value).map_err(|_| format!("{field} is out of range"))
}

fn optional_u32(object: &Map<String, Value>, field: &str) -> Result<Option<u32>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => required_u32(object, field).map(Some),
    }
}

fn required_usize(object: &Map<String, Value>, field: &str) -> Result<usize, String> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("{field} must be an unsigned integer"))
}

fn parse_seat(value: usize) -> Result<Seat, String> {
    let index = u8::try_from(value).map_err(|_| format!("invalid seat {value}"))?;
    Seat::new(index).ok_or_else(|| format!("invalid seat {value}"))
}

fn required_i32_vec(object: &Map<String, Value>, field: &str) -> Result<Vec<i32>, String> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field} must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| format!("{field} contains an invalid score"))
        })
        .collect()
}

fn optional_i32_vec(object: &Map<String, Value>, field: &str) -> Result<Option<Vec<i32>>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => required_i32_vec(object, field).map(Some),
    }
}

fn required_tiles(object: &Map<String, Value>, field: &str) -> Result<Vec<Tile>, String> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field} must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{field} contains a non-string"))
                .and_then(parse_tile)
        })
        .collect()
}

fn required_tile_hands(object: &Map<String, Value>, field: &str) -> Result<Vec<Vec<Tile>>, String> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field} must be an array"))?
        .iter()
        .map(|hand| {
            hand.as_array()
                .ok_or_else(|| format!("{field} contains a non-array hand"))?
                .iter()
                .map(|tile| {
                    tile.as_str()
                        .ok_or_else(|| format!("{field} contains a non-string tile"))
                        .and_then(parse_tile)
                })
                .collect()
        })
        .collect()
}

fn optional_tile_hands(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<Vec<Vec<Tile>>>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => required_tile_hands(object, field).map(Some),
    }
}

fn optional_yaku(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<Vec<(String, u32)>>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| format!("{field} is invalid: {error}")),
    }
}
