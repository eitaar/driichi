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
    Audience, GameEvent, GameMode, KyokuResultRecord, KyokuSettlement, MeldState, Participant,
    ParticipantId, ParticipantKind, ReplayAdminProjection, Seat, TablePlayerState, TableState,
    Tile, WinResult, Wind, project_table_state,
};
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct ReplayFrame {
    pub event_index: usize,
    pub visible_event: CanonicalEvent,
    pub visible_state: ReplayAdminProjection,
    pub auxiliary_events: Vec<AuxiliaryRecord>,
    pub result: Option<WinResult>,
    pub settlement: Option<KyokuSettlement>,
}

impl Serialize for ReplayFrame {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ReplayFrameWithResults {
            frame: self,
            result: self.result.as_ref(),
            settlement: self.settlement.as_ref(),
        }
        .serialize(serializer)
    }
}

struct ReplayFrameWithResults<'a> {
    frame: &'a ReplayFrame,
    result: Option<&'a WinResult>,
    settlement: Option<&'a KyokuSettlement>,
}

impl Serialize for ReplayFrameWithResults<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut object = serializer.serialize_struct("ReplayFrame", 6)?;
        object.serialize_field("event_index", &self.frame.event_index)?;
        object.serialize_field("visible_event", &event_value(&self.frame.visible_event))?;
        object.serialize_field("visible_state", &self.frame.visible_state)?;
        object.serialize_field("auxiliary_events", &self.frame.auxiliary_events)?;
        object.serialize_field("result", &self.result)?;
        object.serialize_field("settlement", &self.settlement)?;
        object.end()
    }
}

struct JoinedFrames<'a> {
    frames: &'a [ReplayFrame],
    results: HashMap<usize, &'a WinResult>,
    settlements: HashMap<usize, &'a KyokuSettlement>,
}

impl Serialize for JoinedFrames<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.frames.len()))?;
        for (index, frame) in self.frames.iter().enumerate() {
            sequence.serialize_element(&ReplayFrameWithResults {
                frame,
                result: self.results.get(&index).copied().or(frame.result.as_ref()),
                settlement: self
                    .settlements
                    .get(&index)
                    .copied()
                    .or(frame.settlement.as_ref()),
            })?;
        }
        sequence.end()
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
    round_data: Option<(Wind, u8, Seat, u8, u32)>,
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
            round_data: None,
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
                bakaze,
                kyoku,
                oya,
                honba,
                kyotaku,
                scores,
                tehais,
                dora_marker,
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
                self.round_data = Some((*bakaze, *kyoku, *oya, *honba, u32::from(*kyotaku)));
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
            GameEvent::Reach { actor } => {
                *self.player_mut(*actor)?.riichi = true;
            }
            GameEvent::ReachAccepted { actor } => {
                *self.player_mut(*actor)?.riichi = true;
                if let Some((_, _, _, _, kyotaku)) = &mut self.round_data {
                    // The timeline is bounded by MAX_REPLAY_EVENTS, well below u32::MAX.
                    *kyotaku += 1;
                }
            }
            GameEvent::Hora { scores, .. } => {
                if let Some(scores) = scores {
                    self.set_scores(scores)?;
                }
                if let Some((_, _, _, _, kyotaku)) = &mut self.round_data {
                    *kyotaku = 0;
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
            double_riichi_core::AudienceProjection::ReplayAdmin(mut projection) => {
                if let Some((round, kyoku, dealer, honba, kyotaku)) = self.round_data {
                    projection.round = Some(round);
                    projection.kyoku = Some(kyoku);
                    projection.dealer = Some(dealer);
                    projection.honba = Some(honba);
                    projection.kyotaku = Some(kyotaku);
                }
                Ok(projection)
            }
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
            result: None,
            settlement: None,
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

#[cfg(test)]
thread_local! {
    static ATTACH_CLONE_BOUNDARIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ROSTER_ID_CLONE_BOUNDARIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn observe_attach_clone_boundary() {
    ATTACH_CLONE_BOUNDARIES.with(|count| count.set(count.get() + 1));
}

/// Join private authoritative metadata only at the corresponding canonical terminal events.
/// Validate the entire join before mutating any frame, including the Kyoku order.
pub fn attach_result_records(
    frames: &mut [ReplayFrame],
    records: &[KyokuResultRecord],
) -> Result<(), ReplayError> {
    if frames.len() > MAX_REPLAY_EVENTS || records.len() > MAX_REPLAY_EVENTS {
        return Err(replay_too_large());
    }
    let starts = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            matches!(frame.visible_event, GameEvent::StartKyoku { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    if records.len() != starts.len() || records.is_empty() {
        return Err(ReplayError::InvalidEvent(
            "result record count does not match kyoku count".into(),
        ));
    }
    for (position, record) in records.iter().enumerate() {
        let start = starts[position];
        let end = record.end_event_index;
        if record.kyoku_sequence != position as u64 + 1
            || end <= start
            || end >= frames.len()
            || starts.get(position + 1).is_some_and(|next| end >= *next)
            || !matches!(frames[end].visible_event, GameEvent::EndKyoku)
            || record.mode != frames[start].visible_state.mode
        {
            return Err(ReplayError::InvalidEvent(
                "invalid result kyoku boundary or mode".into(),
            ));
        }
        if record.wins.len() > end - start - 1
            || record.wins.iter().any(|win| {
                win.event_index <= start
                    || win.event_index >= end
                    || !matches!(
                        frames[win.event_index].visible_event,
                        GameEvent::Hora { .. }
                    )
            })
            || record
                .wins
                .windows(2)
                .any(|wins| wins[0].event_index >= wins[1].event_index)
        {
            return Err(ReplayError::InvalidEvent(
                "result terminals contradict canonical events".into(),
            ));
        }
    }
    let mut joined = JoinedFrames {
        frames,
        results: HashMap::new(),
        settlements: HashMap::new(),
    };
    for record in records {
        for win in &record.wins {
            joined.results.insert(win.event_index, win);
        }
        joined
            .settlements
            .insert(record.end_event_index, &record.settlement);
    }
    serialized_len(&joined, MAX_REPLAY_FRAME_BYTES)?;

    for (position, record) in records.iter().enumerate() {
        let start = starts[position];
        let end = record.end_event_index;
        let roster = record
            .settlement
            .changes
            .iter()
            .map(|change| {
                #[cfg(test)]
                ROSTER_ID_CLONE_BOUNDARIES.with(|count| count.set(count.get() + 1));
                Participant::new(
                    change.participant_id.clone(),
                    "Replay seat",
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect::<Vec<_>>();
        record
            .validate(&roster)
            .map_err(ReplayError::InvalidEvent)?;
        let terminal = (start + 1..end)
            .filter(|&index| {
                matches!(
                    frames[index].visible_event,
                    GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. }
                )
            })
            .collect::<Vec<_>>();
        if terminal.is_empty()
            || terminal
                .iter()
                .any(|&index| match &frames[index].visible_event {
                    GameEvent::Hora {
                        actor,
                        target,
                        tile,
                        ..
                    } => !record.wins.iter().any(|win| {
                        win.event_index == index
                            && win.seat == *actor
                            && win.target == *target
                            && tile
                                .is_none_or(|tile| tile.tile_type() == win.winning_tile.tile_type())
                    }),
                    GameEvent::Ryukyoku { reason, .. } => {
                        !record.wins.is_empty()
                            || reason.as_deref() != record.settlement.draw_reason.as_deref()
                    }
                    _ => true,
                })
            || terminal.len() != record.wins.len() + usize::from(record.wins.is_empty())
            || record
                .wins
                .iter()
                .any(|win| win.event_index <= start || win.event_index >= end)
        {
            return Err(ReplayError::InvalidEvent(
                "result terminals contradict canonical events".into(),
            ));
        }
        validate_canonical_settlement(
            frames,
            start,
            end,
            &terminal,
            record,
            position + 1 == starts.len(),
        )?;
    }
    for record in records {
        for win in &record.wins {
            #[cfg(test)]
            observe_attach_clone_boundary();
            frames[win.event_index].result = Some(win.clone());
        }
        #[cfg(test)]
        observe_attach_clone_boundary();
        frames[record.end_event_index].settlement = Some(record.settlement.clone());
    }
    Ok(())
}

fn validate_canonical_settlement(
    frames: &[ReplayFrame],
    start: usize,
    end: usize,
    terminal: &[usize],
    record: &KyokuResultRecord,
    final_kyoku: bool,
) -> Result<(), ReplayError> {
    let invalid = || {
        ReplayError::InvalidEvent("result scores, deltas or pot contradict canonical events".into())
    };
    let GameEvent::StartKyoku {
        scores, kyotaku, ..
    } = &frames[start].visible_event
    else {
        return Err(invalid());
    };
    if scores.len() != record.settlement.changes.len() {
        return Err(invalid());
    }
    let mut reached_scores = scores.clone();
    let mut pot = u32::from(*kyotaku);
    let matches_before = |scores: &[i32], pot: u32| {
        pot == record.settlement.pot_before
            && scores
                .iter()
                .zip(&record.settlement.changes)
                .all(|(score, change)| *score == change.before)
    };
    // The record captures the state before the terminal action batch, not
    // necessarily just before the terminal MJAI event. The pinned engine may
    // accept one pending declaration immediately before a terminal draw; an
    // earlier acceptance followed by another action is already paid.
    let mut before_boundary = matches_before(&reached_scores, pot).then_some(start);
    for index in start + 1..terminal[0] {
        if let GameEvent::ReachAccepted { actor } = frames[index].visible_event {
            let score = reached_scores
                .get_mut(usize::from(actor.index()))
                .ok_or_else(invalid)?;
            *score = score.checked_sub(1_000).ok_or_else(invalid)?;
            pot = pot.checked_add(1).ok_or_else(invalid)?;
            if matches_before(&reached_scores, pot) {
                before_boundary = Some(index);
            }
        }
    }
    let later_reaches = before_boundary.map(|boundary| {
        frames[boundary + 1..terminal[0]]
            .iter()
            .filter(|frame| matches!(frame.visible_event, GameEvent::ReachAccepted { .. }))
            .count()
    });
    // A terminal apply resolves one action (or a single response batch); its
    // pending riichi acceptance, if any, directly precedes the terminal draw
    // in both pinned 3p/4p control flows. Earlier or multiple acceptances
    // cannot be attributed to this result's pre-action capture.
    if !matches!(later_reaches, Some(0))
        && !(later_reaches == Some(1)
            && terminal[0] > start + 1
            && matches!(
                frames[terminal[0] - 1].visible_event,
                GameEvent::ReachAccepted { .. }
            ))
        || frames[terminal[0]..end]
            .iter()
            .any(|frame| matches!(frame.visible_event, GameEvent::ReachAccepted { .. }))
    {
        return Err(invalid());
    }
    let final_draw_award = final_kyoku
        && record.wins.is_empty()
        && pot > 0
        && frames.get(end + 1..).is_some_and(|rest| {
            rest.iter()
                .any(|frame| matches!(frame.visible_event, GameEvent::EndGame))
        });
    let expected_pot_after = if !record.wins.is_empty() || final_draw_award {
        0
    } else {
        pot
    };
    if record.settlement.pot_after != expected_pot_after {
        return Err(invalid());
    }
    // A draw's emitted delta can retain an earlier paid reach deposit. Compare
    // it to independently captured pre-award movement plus private producer
    // evidence, never apply it a second time to canonical scores.
    if record.wins.is_empty() {
        if terminal.len() != 1 {
            return Err(invalid());
        }
        let GameEvent::Ryukyoku {
            delta: Some(delta),
            scores: supplied_scores,
            ..
        } = &frames[terminal[0]].visible_event
        else {
            return Err(invalid());
        };
        let evidence = record
            .draw_event_delta_adjustment
            .as_ref()
            .ok_or_else(invalid)?;
        if delta.len() != reached_scores.len() || evidence.len() != delta.len() {
            return Err(invalid());
        }
        let mut pre_award = record
            .settlement
            .changes
            .iter()
            .map(|change| change.after)
            .collect::<Vec<_>>();
        if final_draw_award {
            let top = (0..pre_award.len())
                .max_by_key(|&seat| (pre_award[seat], std::cmp::Reverse(seat)))
                .ok_or_else(invalid)?;
            let award = i32::try_from(pot)
                .ok()
                .and_then(|sticks| sticks.checked_mul(1_000))
                .ok_or_else(invalid)?;
            pre_award[top] = pre_award[top].checked_sub(award).ok_or_else(invalid)?;
            if (0..pre_award.len()).max_by_key(|&seat| (pre_award[seat], std::cmp::Reverse(seat)))
                != Some(top)
            {
                return Err(invalid());
            }
        }
        for ((change, &after), (&emitted, &adjustment)) in record
            .settlement
            .changes
            .iter()
            .zip(&pre_award)
            .zip(delta.iter().zip(evidence))
        {
            let movement = after.checked_sub(change.before).ok_or_else(invalid)?;
            if movement.checked_add(adjustment) != Some(emitted) {
                return Err(invalid());
            }
        }
        if supplied_scores
            .as_ref()
            .is_some_and(|scores| scores != &pre_award)
        {
            return Err(invalid());
        }
        if let Some(next) = frames.get(end + 1..).and_then(|rest| {
            rest.iter().find_map(|frame| {
                if let GameEvent::StartKyoku {
                    scores, kyotaku, ..
                } = &frame.visible_event
                {
                    Some((scores, *kyotaku))
                } else {
                    None
                }
            })
        }) && (next.0
            != &record
                .settlement
                .changes
                .iter()
                .map(|change| change.after)
                .collect::<Vec<_>>()
            || u32::from(next.1) != record.settlement.pot_after)
        {
            return Err(invalid());
        }
        return Ok(());
    }
    // Every acceptance is reflected once in the actual score before applying
    // terminal deltas; already-paid acceptances were included at the matched
    // pre-action boundary, later ones were not.
    let mut running = reached_scores;
    let mut known = true;
    for &index in terminal {
        let (delta, scores) = match &frames[index].visible_event {
            GameEvent::Hora { delta, scores, .. } | GameEvent::Ryukyoku { delta, scores, .. } => {
                (delta, scores)
            }
            _ => return Err(invalid()),
        };
        if let Some(delta) = delta {
            if delta.len() != running.len() {
                return Err(invalid());
            }
            if known {
                for (score, change) in running.iter_mut().zip(delta) {
                    *score = score.checked_add(*change).ok_or_else(invalid)?;
                }
            }
        } else {
            known = false;
        }
        if let Some(scores) = scores {
            if scores.len() != running.len() || (known && scores != &running) {
                return Err(invalid());
            }
            running.clone_from(scores);
            known = true;
        }
    }
    let mut after = running;
    if final_draw_award {
        // The pinned engine emits the draw before awarding a remaining pot at
        // EndGame; only a fully reconstructed canonical score can identify its
        // actual top-ranked recipient (ties use the initial seat index).
        if !known {
            return Err(invalid());
        }
        let top = (0..after.len())
            .max_by_key(|&seat| (after[seat], std::cmp::Reverse(seat)))
            .ok_or_else(invalid)?;
        let award = i32::try_from(pot)
            .ok()
            .and_then(|sticks| sticks.checked_mul(1_000))
            .ok_or_else(invalid)?;
        after[top] = after[top].checked_add(award).ok_or_else(invalid)?;
    }
    if known
        && after
            .iter()
            .zip(&record.settlement.changes)
            .any(|(score, change)| *score != change.after)
    {
        return Err(invalid());
    }
    if let Some(next) = frames.get(end + 1..).and_then(|rest| {
        rest.iter().find_map(|frame| {
            if let GameEvent::StartKyoku {
                scores, kyotaku, ..
            } = &frame.visible_event
            {
                Some((scores, *kyotaku))
            } else {
                None
            }
        })
    }) && (next
        .0
        .iter()
        .zip(&record.settlement.changes)
        .any(|(score, change)| *score != change.after)
        || next.0.len() != record.settlement.changes.len()
        || u32::from(next.1) != record.settlement.pot_after)
    {
        return Err(invalid());
    }
    Ok(())
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

#[cfg(test)]
mod attachment_order_tests {
    use super::*;

    #[test]
    fn oversized_borrowed_identity_is_rejected_before_roster_id_clone() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let case = &cases.as_array().unwrap()[0];
        let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
        let mut record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
        let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();

        ROSTER_ID_CLONE_BOUNDARIES.with(|count| count.set(0));
        attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
        assert!(ROSTER_ID_CLONE_BOUNDARIES.with(|count| count.get()) > 0);
        assert_eq!(
            frames[record.wins[0].event_index].result.as_ref(),
            Some(&record.wins[0])
        );
        let before = encode_replay_frames(&frames).unwrap();
        let seat = record.wins[0].seat.index() as usize;
        let identity = ParticipantId::new("q".repeat(MAX_REPLAY_FRAME_BYTES + 1));
        record.wins[0].winner = identity.clone();
        record.settlement.changes[seat].participant_id = identity;
        let borrowed_len = record.wins[0].winner.as_str().len();

        ROSTER_ID_CLONE_BOUNDARIES.with(|count| count.set(0));
        let outcome = attach_result_records(&mut frames, std::slice::from_ref(&record));
        assert!(
            matches!(&outcome, Err(ReplayError::ReplayTooLarge { .. })),
            "{outcome:?}"
        );
        assert_eq!(
            ROSTER_ID_CLONE_BOUNDARIES.with(|count| count.get()),
            0,
            "oversized borrowed ID must be rejected before validation-roster clone"
        );
        assert_eq!(encode_replay_frames(&frames).unwrap(), before);
        assert_eq!(record.wins[0].winner.as_str().len(), borrowed_len);
        assert_eq!(
            record.wins[0].winner,
            record.settlement.changes[seat].participant_id
        );
    }

    #[test]
    fn oversized_borrowed_result_is_rejected_before_attachment_clone() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let case = &cases.as_array().unwrap()[0];
        let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
        let mut record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
        let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();

        ATTACH_CLONE_BOUNDARIES.with(|count| count.set(0));
        attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
        assert!(ATTACH_CLONE_BOUNDARIES.with(|count| count.get()) > 0);
        assert_eq!(
            frames[record.wins[0].event_index].result.as_ref(),
            Some(&record.wins[0])
        );
        let mut oversized_frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
        let first_win_index = record.wins[0].event_index;
        oversized_frames[first_win_index].result = frames[first_win_index].result.clone();
        oversized_frames[record.end_event_index].settlement =
            frames[record.end_event_index].settlement.clone();
        let before = encode_replay_frames(&oversized_frames).unwrap();
        let winning_tile = record.wins[0].winning_tile;
        let tile_bytes = serde_json::to_vec(&winning_tile).unwrap().len();
        let repeated_tile_count = MAX_REPLAY_FRAME_BYTES / (tile_bytes + 1) + 2;
        record.wins[0]
            .concealed
            .resize(repeated_tile_count, winning_tile);
        let before_len = record.wins[0].concealed.len();

        ATTACH_CLONE_BOUNDARIES.with(|count| count.set(0));
        let outcome = attach_result_records(&mut oversized_frames, std::slice::from_ref(&record));
        assert!(
            matches!(&outcome, Err(ReplayError::ReplayTooLarge { .. })),
            "{outcome:?}; tile_bytes={tile_bytes}"
        );
        assert_eq!(
            ATTACH_CLONE_BOUNDARIES.with(|count| count.get()),
            0,
            "oversized borrowed winner must be rejected before any attachment clone"
        );
        assert_eq!(encode_replay_frames(&oversized_frames).unwrap(), before);
        assert_eq!(record.wins[0].concealed.len(), before_len);
        assert_eq!(record.wins[0].winning_tile, winning_tile);
    }

    #[test]
    fn joined_metadata_accepts_exact_aggregate_limit_and_rejects_next_byte() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let case = &cases.as_array().unwrap()[0];
        let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
        let record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
        let mut frames = build_replay_frames_for_mode(&events, record.mode).unwrap();
        attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
        let base = encode_replay_frames(&frames).unwrap().len();
        frames[0].visible_state.players[0]
            .display_name
            .push_str(&"x".repeat(MAX_REPLAY_FRAME_BYTES - base));
        // Replacing existing Some metadata must count the replacement, not both copies.
        attach_result_records(&mut frames, std::slice::from_ref(&record)).unwrap();
        assert_eq!(
            encode_replay_frames(&frames).unwrap().len(),
            MAX_REPLAY_FRAME_BYTES
        );
        let before_result = frames[record.wins[0].event_index].result.clone();
        let before_settlement = frames[record.end_event_index].settlement.clone();
        frames[0].visible_state.players[0].display_name.push('x');
        assert!(matches!(
            attach_result_records(&mut frames, std::slice::from_ref(&record)),
            Err(ReplayError::ReplayTooLarge { .. })
        ));
        assert_eq!(frames[record.wins[0].event_index].result, before_result);
        assert_eq!(frames[record.end_event_index].settlement, before_settlement);
    }
}

// Keep this import visible in generated docs: these are the domain types that
// make ReplayAdmin reconstruction explicit rather than exposing engine values.
#[allow(dead_code)]
fn _domain_type_names(_: ParticipantId) {}
