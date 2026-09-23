use std::{
    collections::HashMap,
    io::{self, Write},
};

use crate::{
    error::{MAX_REPLAY_EVENTS, MAX_REPLAY_FRAME_BYTES, ReplayError},
    mjson::{CanonicalEvent, event_value},
    persistence::{AuxiliaryRecord, ReplayArtifact},
};
use double_riichi_core::{
    Audience, GameEvent, GameMode, MeldState, Participant, ParticipantId, ParticipantKind,
    ReplayAdminProjection, Seat, TablePlayerState, TableState, Tile, project_table_state,
};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct ReplayFrame {
    pub event_index: usize,
    pub visible_event: CanonicalEvent,
    pub visible_state: ReplayAdminProjection,
    pub auxiliary_events: Vec<AuxiliaryRecord>,
}

impl Serialize for ReplayFrame {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut object = serializer.serialize_struct("ReplayFrame", 4)?;
        object.serialize_field("event_index", &self.event_index)?;
        object.serialize_field("visible_event", &event_value(&self.visible_event))?;
        object.serialize_field("visible_state", &self.visible_state)?;
        object.serialize_field("auxiliary_events", &self.auxiliary_events)?;
        object.end()
    }
}

impl ReplayFrame {
    pub fn to_json(&self) -> Result<String, ReplayError> {
        let payload = serialize_bounded(self)?;
        String::from_utf8(payload)
            .map_err(|error| ReplayError::InvalidEvent(format!("frame JSON is not UTF-8: {error}")))
    }
}

#[derive(Debug, Clone)]
struct ReplayState {
    mode: GameMode,
    participants: Vec<Participant>,
    scores: Vec<i32>,
    hands: Vec<Vec<Tile>>,
    discards: Vec<Vec<Tile>>,
    melds: Vec<Vec<MeldState>>,
    riichi: Vec<bool>,
    dora_indicators: Vec<Tile>,
    kyoku_active: bool,
}

impl ReplayState {
    fn new(mode: GameMode) -> Self {
        let participants = (0..mode.seat_count())
            .map(|seat| {
                Participant::new(
                    format!("seat{seat}"),
                    format!("Seat {seat}"),
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect::<Vec<_>>();
        Self {
            mode,
            participants,
            scores: vec![25_000; mode.seat_count()],
            hands: vec![Vec::new(); mode.seat_count()],
            discards: vec![Vec::new(); mode.seat_count()],
            melds: vec![Vec::new(); mode.seat_count()],
            riichi: vec![false; mode.seat_count()],
            dora_indicators: Vec::new(),
            kyoku_active: false,
        }
    }

    fn apply(&mut self, event: &CanonicalEvent) -> Result<(), ReplayError> {
        if !self.kyoku_active
            && !matches!(
                event,
                GameEvent::StartGame { .. } | GameEvent::StartKyoku { .. } | GameEvent::EndGame
            )
        {
            return Err(ReplayError::InvalidEvent(
                "event appears outside an active kyoku".into(),
            ));
        }
        match event {
            GameEvent::StartGame { names, .. } => {
                if let Some(names) = names {
                    for (seat, name) in names.iter().take(self.mode.seat_count()).enumerate() {
                        self.participants[seat].display_name = name.clone();
                    }
                }
            }
            GameEvent::StartKyoku {
                scores,
                tehais,
                dora_marker,
                ..
            } => {
                if self.kyoku_active {
                    return Err(ReplayError::InvalidEvent(
                        "start_kyoku appeared before the previous kyoku ended".into(),
                    ));
                }
                if scores.len() != self.mode.seat_count() || tehais.len() != self.mode.seat_count()
                {
                    return Err(ReplayError::InvalidEvent(
                        "start_kyoku does not match replay mode".into(),
                    ));
                }
                self.scores.clone_from(scores);
                self.hands.clone_from(tehais);
                self.discards.iter_mut().for_each(Vec::clear);
                self.melds.iter_mut().for_each(Vec::clear);
                self.riichi.fill(false);
                self.dora_indicators.clear();
                self.dora_indicators.push(*dora_marker);
                self.kyoku_active = true;
            }
            GameEvent::Tsumo { actor, tile } => {
                self.player_mut(*actor)?.hands_mut().push(*tile);
            }
            GameEvent::Dahai { actor, tile, .. } => {
                let player = self.player_mut(*actor)?;
                remove_required_tile(player.hand, *tile)?;
                player.discards.push(*tile);
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
                if actor == target {
                    return Err(ReplayError::InvalidEvent(
                        "meld target cannot be the calling player".into(),
                    ));
                }
                validate_open_meld(*called, consumed, matches!(event, GameEvent::Chi { .. }))?;
                let player = self.player_mut(*actor)?;
                for tile in consumed {
                    remove_required_tile(player.hand, *tile)?;
                }
                let mut tiles = consumed.clone();
                tiles.push(*called);
                player.melds.push(MeldState {
                    tiles,
                    opened: true,
                    from_who: Some(*target),
                    called_tile: Some(*called),
                });
            }
            GameEvent::Kakan { actor, called, .. } => {
                let player = self.player_mut(*actor)?;
                remove_required_tile(player.hand, *called)?;
                let Some(meld) = player.melds.iter_mut().find(|meld| {
                    meld.opened
                        && meld.tiles.len() == 3
                        && meld
                            .tiles
                            .iter()
                            .all(|tile| tile.tile_type() == called.tile_type())
                        && meld
                            .tiles
                            .iter()
                            .any(|tile| tile.tile_type() == called.tile_type())
                }) else {
                    return Err(ReplayError::InvalidEvent(
                        "kakan requires an existing open pon meld".into(),
                    ));
                };
                meld.tiles.push(*called);
                meld.called_tile = Some(*called);
            }
            GameEvent::Ankan { actor, consumed } => {
                if consumed.is_empty()
                    || consumed
                        .iter()
                        .any(|tile| tile.tile_type() != consumed[0].tile_type())
                {
                    return Err(ReplayError::InvalidEvent(
                        "ankan tiles must have one tile type".into(),
                    ));
                }
                let player = self.player_mut(*actor)?;
                for tile in consumed {
                    remove_required_tile(player.hand, *tile)?;
                }
                player.melds.push(MeldState {
                    tiles: consumed.clone(),
                    opened: false,
                    from_who: None,
                    called_tile: None,
                });
            }
            GameEvent::Dora { dora_marker } => self.dora_indicators.push(*dora_marker),
            GameEvent::Reach { actor } | GameEvent::ReachAccepted { actor } => {
                *self.player_mut(*actor)?.riichi = true;
            }
            GameEvent::Hora { scores, .. } => {
                if let Some(scores) = scores {
                    self.set_scores(scores)?;
                }
            }
            GameEvent::Ryukyoku { tehais, scores, .. } => {
                if let Some(tehais) = tehais {
                    if tehais.len() != self.mode.seat_count() {
                        return Err(ReplayError::InvalidEvent(
                            "ryukyoku hand count mismatch".into(),
                        ));
                    }
                    self.hands.clone_from(tehais);
                }
                if let Some(scores) = scores {
                    self.set_scores(scores)?;
                }
            }
            GameEvent::Kita { actor } => {
                // Sanma nuki removes one North from the concealed hand. The
                // core projection has no separate nuki collection, so the
                // concealed count is represented by the reduced hand.
                let player = self.player_mut(*actor)?;
                let Some(index) = player
                    .hand
                    .iter()
                    .position(|tile| tile.tile_type() == Tile::NORTH)
                else {
                    return Err(ReplayError::InvalidEvent(
                        "kita requires a North tile in the concealed hand".into(),
                    ));
                };
                player.hand.remove(index);
            }
            GameEvent::EndKyoku => {
                if !self.kyoku_active {
                    return Err(ReplayError::InvalidEvent(
                        "end_kyoku appeared outside an active kyoku".into(),
                    ));
                }
                self.kyoku_active = false;
            }
            GameEvent::EndGame => {}
        }
        Ok(())
    }

    fn set_scores(&mut self, scores: &[i32]) -> Result<(), ReplayError> {
        if scores.len() != self.mode.seat_count() {
            return Err(ReplayError::InvalidEvent("score count mismatch".into()));
        }
        self.scores.clone_from_slice(scores);
        Ok(())
    }

    fn player_mut(&mut self, seat: Seat) -> Result<PlayerStateMut<'_>, ReplayError> {
        let index = usize::from(seat.index());
        if index >= self.mode.seat_count() {
            return Err(ReplayError::InvalidEvent(format!(
                "seat {} is outside replay mode",
                seat.index()
            )));
        }
        Ok(PlayerStateMut {
            hand: &mut self.hands[index],
            discards: &mut self.discards[index],
            melds: &mut self.melds[index],
            riichi: &mut self.riichi[index],
        })
    }

    fn projection(&self) -> Result<ReplayAdminProjection, ReplayError> {
        let players = self
            .participants
            .iter()
            .enumerate()
            .map(|(index, participant)| TablePlayerState {
                seat: Seat::new(index as u8).expect("mode seats are less than four"),
                participant: participant.clone(),
                score: self.scores[index],
                hand: self.hands[index].clone(),
                discards: self.discards[index].clone(),
                melds: self.melds[index].clone(),
                riichi: self.riichi[index],
            })
            .collect();
        let table = TableState::new(self.mode, players, self.dora_indicators.clone())
            .map_err(|error| ReplayError::InvalidEvent(error.to_string()))?;
        match project_table_state(&table, Audience::ReplayAdmin) {
            double_riichi_core::AudienceProjection::ReplayAdmin(projection) => Ok(projection),
            _ => unreachable!("ReplayAdmin projection policy returned another audience"),
        }
    }
}

struct PlayerStateMut<'a> {
    hand: &'a mut Vec<Tile>,
    discards: &'a mut Vec<Tile>,
    melds: &'a mut Vec<MeldState>,
    riichi: &'a mut bool,
}

impl PlayerStateMut<'_> {
    fn hands_mut(&mut self) -> &mut Vec<Tile> {
        self.hand
    }
}

fn remove_required_tile(hand: &mut Vec<Tile>, wanted: Tile) -> Result<(), ReplayError> {
    let Some(index) = hand.iter().position(|tile| *tile == wanted) else {
        return Err(ReplayError::InvalidEvent(format!(
            "tile {} is not in concealed hand",
            wanted.id()
        )));
    };
    hand.remove(index);
    Ok(())
}

fn validate_open_meld(called: Tile, consumed: &[Tile], chi: bool) -> Result<(), ReplayError> {
    let mut tiles = consumed.to_vec();
    tiles.push(called);
    if chi {
        let mut types: Vec<_> = tiles.iter().map(|tile| tile.tile_type().index()).collect();
        types.sort_unstable();
        let same_suit = types.iter().all(|index| *index < 27)
            && types.windows(2).all(|window| window[1] == window[0] + 1)
            && types.first().is_some_and(|index| index / 9 == types[0] / 9);
        if types.len() != 3 || !same_suit {
            return Err(ReplayError::InvalidEvent(
                "chi tiles must form a consecutive suited sequence".into(),
            ));
        }
    } else if tiles.len() != 3 && tiles.len() != 4 {
        return Err(ReplayError::InvalidEvent(
            "open meld has an invalid tile count".into(),
        ));
    } else if tiles
        .iter()
        .any(|tile| tile.tile_type() != called.tile_type())
    {
        return Err(ReplayError::InvalidEvent(
            "pon or daiminkan tiles must have one tile type".into(),
        ));
    }
    Ok(())
}

/// Build the complete Admin-only timeline from validated canonical events.
pub fn build_replay_frames(events: &[CanonicalEvent]) -> Result<Vec<ReplayFrame>, ReplayError> {
    let mode = infer_mode(events)?;
    build_replay_frames_with_auxiliary_for_mode(events, &[], mode)
}

/// Build frames using the persisted match mode rather than inferring East mode
/// from the number of seats. East and half-game replays share MJSON events.
pub fn build_replay_frames_for_mode(
    events: &[CanonicalEvent],
    mode: GameMode,
) -> Result<Vec<ReplayFrame>, ReplayError> {
    build_replay_frames_with_auxiliary_for_mode(events, &[], mode)
}

pub fn build_replay_frames_with_auxiliary(
    events: &[CanonicalEvent],
    auxiliary_events: &[AuxiliaryRecord],
) -> Result<Vec<ReplayFrame>, ReplayError> {
    let mode = infer_mode(events)?;
    build_replay_frames_with_auxiliary_for_mode(events, auxiliary_events, mode)
}

pub fn build_replay_frames_with_auxiliary_for_mode(
    events: &[CanonicalEvent],
    auxiliary_events: &[AuxiliaryRecord],
    mode: GameMode,
) -> Result<Vec<ReplayFrame>, ReplayError> {
    if events.len() > MAX_REPLAY_EVENTS || auxiliary_events.len() > MAX_REPLAY_EVENTS {
        return Err(replay_too_large());
    }
    let inferred = infer_mode(events)?;
    if inferred.seat_count() != mode.seat_count() {
        return Err(ReplayError::InvalidEvent(format!(
            "stored replay mode {mode} does not match event player count"
        )));
    }
    for event in events {
        crate::mjson::validate_event(event, mode).map_err(ReplayError::InvalidEvent)?;
    }
    let mut state = ReplayState::new(mode);
    let mut aux_by_line: HashMap<usize, Vec<AuxiliaryRecord>> = HashMap::new();
    for auxiliary in auxiliary_events {
        if auxiliary.line_index >= events.len() {
            return Err(ReplayError::InvalidEvent(format!(
                "auxiliary line index {} is outside the replay timeline",
                auxiliary.line_index
            )));
        }
        aux_by_line
            .entry(auxiliary.line_index)
            .or_default()
            .push(auxiliary.clone());
    }
    for records in aux_by_line.values_mut() {
        records.sort_by_key(|record| (record.phase.sort_key(), record.sequence));
    }

    let mut frames = Vec::new();
    let mut serialized_size = 2usize;
    for (event_index, event) in events.iter().enumerate() {
        state.apply(event)?;
        let frame = ReplayFrame {
            event_index,
            visible_event: event.clone(),
            visible_state: state.projection()?,
            auxiliary_events: aux_by_line.remove(&event_index).unwrap_or_default(),
        };
        let separator = usize::from(!frames.is_empty());
        serialized_size = serialized_size
            .checked_add(separator)
            .ok_or_else(replay_too_large)?;
        let remaining = MAX_REPLAY_FRAME_BYTES.saturating_sub(serialized_size);
        let frame_size = serialized_len(&frame, remaining)?;
        serialized_size = serialized_size
            .checked_add(frame_size)
            .ok_or_else(replay_too_large)?;
        frames.push(frame);
    }
    Ok(frames)
}

pub fn frames_from_artifact(
    events: &[CanonicalEvent],
    artifact: &ReplayArtifact,
) -> Result<Vec<ReplayFrame>, ReplayError> {
    build_replay_frames_with_auxiliary(events, &artifact.auxiliary_events)
}

pub fn frames_from_artifact_for_mode(
    events: &[CanonicalEvent],
    artifact: &ReplayArtifact,
    mode: GameMode,
) -> Result<Vec<ReplayFrame>, ReplayError> {
    build_replay_frames_with_auxiliary_for_mode(events, &artifact.auxiliary_events, mode)
}

fn infer_mode(events: &[CanonicalEvent]) -> Result<GameMode, ReplayError> {
    events
        .iter()
        .find_map(|event| match event {
            GameEvent::StartKyoku { scores, .. } => match scores.len() {
                3 => Some(GameMode::ThreePlayerRedEast),
                4 => Some(GameMode::FourPlayerRedEast),
                _ => None,
            },
            _ => None,
        })
        .ok_or_else(|| ReplayError::InvalidEvent("replay has no start_kyoku event".into()))
}

pub fn encode_replay_frames(frames: &[ReplayFrame]) -> Result<Vec<u8>, ReplayError> {
    if frames.len() > MAX_REPLAY_EVENTS {
        return Err(replay_too_large());
    }
    serialize_bounded(frames)
}

fn replay_too_large() -> ReplayError {
    ReplayError::ReplayTooLarge {
        actual: MAX_REPLAY_FRAME_BYTES.saturating_add(1),
        limit: MAX_REPLAY_FRAME_BYTES,
    }
}

fn serialized_len<T: serde::Serialize>(value: &T, limit: usize) -> Result<usize, ReplayError> {
    let mut writer = CountingWriter {
        bytes: 0,
        limit,
        overflowed: false,
    };
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(writer.bytes),
        Err(_error) if writer.overflowed => Err(replay_too_large()),
        Err(error) => Err(ReplayError::Json(error)),
    }
}

fn serialize_bounded<T: serde::Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, ReplayError> {
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        limit: MAX_REPLAY_FRAME_BYTES,
        overflowed: false,
    };
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(writer.bytes),
        Err(_error) if writer.overflowed => Err(replay_too_large()),
        Err(error) => Err(ReplayError::Json(error)),
    }
}

struct CountingWriter {
    bytes: usize,
    limit: usize,
    overflowed: bool,
}

impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(total) = self.bytes.checked_add(bytes.len()) else {
            self.overflowed = true;
            return Err(io::Error::other("replay frame size overflow"));
        };
        if total > self.limit {
            self.overflowed = true;
            return Err(io::Error::other("replay frame size limit exceeded"));
        }
        self.bytes = total;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct LimitedWriter {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(total) = self.bytes.len().checked_add(bytes.len()) else {
            self.overflowed = true;
            return Err(io::Error::other("replay frame size overflow"));
        };
        if total > self.limit {
            self.overflowed = true;
            return Err(io::Error::other("replay frame size limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub const fn replay_frame_limit() -> usize {
    MAX_REPLAY_FRAME_BYTES
}

// Keep this import visible in generated docs: these are the domain types that
// make ReplayAdmin reconstruction explicit rather than exposing engine values.
#[allow(dead_code)]
fn _domain_type_names(_: ParticipantId) {}
