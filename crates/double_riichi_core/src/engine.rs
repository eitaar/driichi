use std::collections::HashMap;

use riichienv_core::{
    action::{Action, ActionType},
    game_variant::GameStateVariant,
    replay::MjaiEvent,
    rule::GameRule,
};
use thiserror::Error;

use crate::{
    domain::{GameAction, GameEvent, GameMode, Participant, Seat, Tile, Wind},
    projection::{MeldState, TablePlayerState, TableState},
};

#[derive(Debug, Error)]
pub(crate) enum EngineError {
    #[error("invalid engine seat {0}")]
    InvalidSeat(Seat),
    #[error("engine rejected a previously legal action: {0}")]
    Rejected(String),
    #[error("engine adapter divergence: {0}")]
    Divergence(String),
    #[error("engine emitted an unsupported event: {0}")]
    UnsupportedEvent(String),
    #[error("engine emitted an invalid tile: {0}")]
    InvalidTile(String),
    #[error("engine emitted an invalid seat: {0}")]
    InvalidEngineSeat(usize),
}

#[derive(Debug, Clone)]
enum ApplyCommand {
    One(Action),
    RiichiDiscard { tile: u8 },
}

#[derive(Debug, Clone)]
pub(crate) struct EngineAdapter {
    mode: GameMode,
    state: GameStateVariant,
    log_cursor: usize,
}

impl EngineAdapter {
    pub(crate) fn new(
        mode: GameMode,
        seed: Option<u64>,
    ) -> Result<(Self, Vec<GameEvent>), EngineError> {
        let state = GameStateVariant::new(
            engine_mode(mode),
            false,
            seed,
            0,
            GameRule::default_tenhou(),
        );
        let mut adapter = Self {
            mode,
            state,
            log_cursor: 0,
        };
        let events = adapter.drain_events()?;
        Ok((adapter, events))
    }

    pub(crate) fn legal_actions(&mut self, seat: Seat) -> Result<Vec<GameAction>, EngineError> {
        self.validate_seat(seat)?;
        let (raw, drawn_tile) = self.raw_legal_actions(seat)?;
        let mut actions = Vec::new();
        for action in raw {
            if action.action_type == ActionType::Riichi && action.tile.is_none() {
                for tile in self.riichi_candidates(seat)? {
                    actions.push(GameAction::RiichiDiscard {
                        tile: Tile::from_id(tile).ok_or_else(|| {
                            EngineError::InvalidTile(format!("riichi candidate {tile}"))
                        })?,
                    });
                }
                continue;
            }
            actions.push(self.to_neutral_action(&action, drawn_tile)?);
        }
        actions.sort_by_key(action_sort_key);
        Ok(actions)
    }

    pub(crate) fn apply(
        &mut self,
        seat: Seat,
        action: &GameAction,
    ) -> Result<Vec<GameEvent>, EngineError> {
        self.validate_seat(seat)?;
        let action = action.clone().canonicalize();
        let legal = self.legal_actions(seat)?;
        if !legal.contains(&action) {
            return Err(EngineError::Rejected(format!(
                "action is not legal for seat {seat}"
            )));
        }

        let (raw, _) = self.raw_legal_actions(seat)?;
        let command = self.find_command(seat, &action, &raw)?;
        match command {
            ApplyCommand::One(engine_action) => self.step_once(seat, engine_action)?,
            ApplyCommand::RiichiDiscard { tile } => {
                self.step_once(
                    seat,
                    Action::new(ActionType::Riichi, None, Vec::new(), Some(seat.index())),
                )?;
                self.step_once(
                    seat,
                    Action::new(
                        ActionType::Discard,
                        Some(tile),
                        Vec::new(),
                        Some(seat.index()),
                    ),
                )?;
            }
        }

        while self.needs_initialize_next_round() {
            self.step_once_without_action()?;
        }
        self.drain_events()
    }

    pub(crate) fn apply_simultaneous(
        &mut self,
        actions: &[(Seat, GameAction)],
    ) -> Result<Vec<GameEvent>, EngineError> {
        let mut commands = HashMap::new();
        let mut seen = HashMap::new();
        for (seat, action) in actions {
            if seen.insert(*seat, ()).is_some() {
                return Err(EngineError::Divergence(format!(
                    "duplicate response for seat {seat}"
                )));
            }
            self.validate_seat(*seat)?;
            let action = action.clone().canonicalize();
            let legal = self.legal_actions(*seat)?;
            if !legal.contains(&action) {
                return Err(EngineError::Rejected(format!(
                    "action is not legal for seat {seat}"
                )));
            }
            let (raw, _) = self.raw_legal_actions(*seat)?;
            match self.find_command(*seat, &action, &raw)? {
                ApplyCommand::One(command) => {
                    commands.insert(seat.index(), command);
                }
                ApplyCommand::RiichiDiscard { .. } => {
                    return Err(EngineError::Divergence(
                        "riichi cannot be resolved in a response window".into(),
                    ));
                }
            }
        }
        step_variant(&mut self.state, &commands);
        if let Some(error) = last_error(&self.state) {
            return Err(EngineError::Rejected(error));
        }
        while self.needs_initialize_next_round() {
            self.step_once_without_action()?;
        }
        self.drain_events()
    }

    pub(crate) fn table_state(
        &self,
        mode: GameMode,
        participants: &[Participant],
    ) -> Result<TableState, EngineError> {
        let mut players = Vec::with_capacity(mode.seat_count());
        let dora_indicators;
        let round_data = self.round_data();
        match &self.state {
            GameStateVariant::FourPlayer(state) => {
                dora_indicators = state
                    .wall
                    .dora_indicators
                    .iter()
                    .copied()
                    .map(|tile| parse_engine_tile(mode, tile))
                    .collect::<Result<Vec<_>, _>>()?;
                for seat in 0..mode.seat_count() {
                    let player = &state.players[seat];
                    players.push(player_snapshot(
                        mode,
                        Seat::new(seat as u8).expect("validated mode seat"),
                        &player.hand,
                        &player.discards,
                        &player.melds,
                        player.score,
                        player.riichi_declared,
                        participants[seat].clone(),
                    )?);
                }
            }
            GameStateVariant::ThreePlayer(state) => {
                dora_indicators = state
                    .wall
                    .dora_indicators
                    .iter()
                    .copied()
                    .map(|tile| parse_engine_tile(mode, tile))
                    .collect::<Result<Vec<_>, _>>()?;
                for seat in 0..mode.seat_count() {
                    let player = &state.players[seat];
                    players.push(player_snapshot(
                        mode,
                        Seat::new(seat as u8).expect("validated mode seat"),
                        &player.hand,
                        &player.discards,
                        &player.melds,
                        player.score,
                        player.riichi_declared,
                        participants[seat].clone(),
                    )?);
                }
            }
        }
        TableState::new(mode, players, dora_indicators)
            .map(|table| {
                table.with_round_data(
                    round_data.0,
                    round_data.1,
                    round_data.2,
                    round_data.3,
                    round_data.4,
                    round_data.5,
                )
            })
            .map_err(|error| EngineError::Divergence(error.to_string()))
    }

    pub(crate) fn is_done(&self) -> bool {
        match &self.state {
            GameStateVariant::FourPlayer(state) => state.is_done,
            GameStateVariant::ThreePlayer(state) => state.is_done,
        }
    }

    pub(crate) fn scores(&self) -> Vec<i32> {
        match &self.state {
            GameStateVariant::FourPlayer(state) => state.players.iter().map(|p| p.score).collect(),
            GameStateVariant::ThreePlayer(state) => state.players.iter().map(|p| p.score).collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn force_event_divergence_for_test(&mut self) {
        match &mut self.state {
            GameStateVariant::FourPlayer(state) => {
                state.mjai_log.push(r#"{"type":"future_event"}"#.into())
            }
            GameStateVariant::ThreePlayer(state) => {
                state.mjai_log.push(r#"{"type":"future_event"}"#.into())
            }
        }
    }

    fn validate_seat(&self, seat: Seat) -> Result<(), EngineError> {
        if usize::from(seat.index()) < self.mode.seat_count() {
            Ok(())
        } else {
            Err(EngineError::InvalidSeat(seat))
        }
    }

    fn raw_legal_actions(&mut self, seat: Seat) -> Result<(Vec<Action>, Option<u8>), EngineError> {
        let index = seat.index();
        match &mut self.state {
            GameStateVariant::FourPlayer(state) => {
                let observation = state.get_observation(index);
                Ok((observation.legal_actions_method(), observation.drawn_tile))
            }
            GameStateVariant::ThreePlayer(state) => {
                let observation = state.get_observation(index);
                Ok((
                    observation
                        .legal_actions_method()
                        .into_iter()
                        .map(|action| action.0)
                        .collect(),
                    observation.drawn_tile,
                ))
            }
        }
    }

    fn to_neutral_action(
        &self,
        action: &Action,
        drawn_tile: Option<u8>,
    ) -> Result<GameAction, EngineError> {
        let tile = |value: Option<u8>| {
            value
                .ok_or_else(|| EngineError::InvalidTile(format!("{:?}", value)))
                .and_then(|value| parse_engine_tile(self.mode, value))
        };
        let target = || {
            self.last_discard()
                .map(|(seat, _)| seat)
                .ok_or_else(|| EngineError::Divergence("claim without a last discard".into()))
        };
        let neutral = match action.action_type {
            ActionType::Discard => {
                let value = tile(action.tile)?;
                GameAction::Discard {
                    tile: value,
                    tsumogiri: drawn_tile == action.tile,
                }
            }
            ActionType::Chi => GameAction::Chi {
                target: target()?,
                called: tile(action.tile)?,
                consumed: action
                    .consume_tiles
                    .iter()
                    .copied()
                    .map(|value| parse_engine_tile(self.mode, value))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ActionType::Pon => GameAction::Pon {
                target: target()?,
                called: tile(action.tile)?,
                consumed: action
                    .consume_tiles
                    .iter()
                    .copied()
                    .map(|value| parse_engine_tile(self.mode, value))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ActionType::Daiminkan => GameAction::Daiminkan {
                target: target()?,
                called: tile(action.tile)?,
                consumed: action
                    .consume_tiles
                    .iter()
                    .copied()
                    .map(|value| parse_engine_tile(self.mode, value))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ActionType::Ankan => GameAction::Ankan {
                consumed: action
                    .consume_tiles
                    .iter()
                    .copied()
                    .map(|value| parse_engine_tile(self.mode, value))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ActionType::Kakan => GameAction::Kakan {
                called: tile(action.tile)?,
                consumed: action
                    .consume_tiles
                    .iter()
                    .copied()
                    .map(|value| parse_engine_tile(self.mode, value))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            ActionType::Kita => GameAction::Nuki {
                tile: tile(action.tile)?,
            },
            ActionType::Tsumo => GameAction::Tsumo,
            ActionType::Ron => GameAction::Ron(target()?),
            ActionType::Riichi => {
                return Err(EngineError::Divergence(
                    "unexpanded riichi action reached neutral conversion".into(),
                ));
            }
            ActionType::Pass => GameAction::Pass,
            ActionType::KyushuKyuhai => GameAction::AbortiveDraw,
        };
        Ok(neutral.canonicalize())
    }

    fn find_command(
        &self,
        seat: Seat,
        desired: &GameAction,
        raw: &[Action],
    ) -> Result<ApplyCommand, EngineError> {
        for action in raw {
            if action.action_type == ActionType::Riichi && action.tile.is_none() {
                if let GameAction::RiichiDiscard { tile } = desired
                    && self
                        .riichi_candidates_for_raw(seat, action)?
                        .contains(&tile.id())
                {
                    return Ok(ApplyCommand::RiichiDiscard { tile: tile.id() });
                }
                continue;
            }
            let drawn_tile = self.drawn_tile();
            if self.to_neutral_action(action, drawn_tile)? == *desired {
                return Ok(ApplyCommand::One(action.clone()));
            }
        }
        Err(EngineError::Divergence(format!(
            "legal action {:?} had no engine command",
            desired
        )))
    }

    fn riichi_candidates(&self, seat: Seat) -> Result<Vec<u8>, EngineError> {
        let (raw, _) = self.raw_legal_actions_clone(seat)?;
        let marker = raw
            .iter()
            .find(|action| action.action_type == ActionType::Riichi && action.tile.is_none())
            .ok_or_else(|| {
                EngineError::Divergence("riichi candidates requested when unavailable".into())
            })?;
        self.riichi_candidates_for_raw(seat, marker)
    }

    fn riichi_candidates_for_raw(
        &self,
        seat: Seat,
        _marker: &Action,
    ) -> Result<Vec<u8>, EngineError> {
        let hand = match &self.state {
            GameStateVariant::FourPlayer(state) => {
                state.players[seat.index() as usize].hand.clone()
            }
            GameStateVariant::ThreePlayer(state) => {
                state.players[seat.index() as usize].hand.clone()
            }
        };
        let mut candidates = hand;
        candidates.sort_unstable();
        candidates.dedup();
        candidates.retain(|&tile| {
            let mut clone = self.state.clone();
            let mut first = HashMap::new();
            first.insert(
                seat.index(),
                Action::new(ActionType::Riichi, None, Vec::new(), Some(seat.index())),
            );
            step_variant(&mut clone, &first);
            if has_last_error(&clone) {
                return false;
            }
            let mut second = HashMap::new();
            second.insert(
                seat.index(),
                Action::new(
                    ActionType::Discard,
                    Some(tile),
                    Vec::new(),
                    Some(seat.index()),
                ),
            );
            step_variant(&mut clone, &second);
            if has_last_error(&clone) {
                return false;
            }
            match clone {
                GameStateVariant::FourPlayer(state) => {
                    state.players[seat.index() as usize].riichi_declared
                }
                GameStateVariant::ThreePlayer(state) => {
                    state.players[seat.index() as usize].riichi_declared
                }
            }
        });
        Ok(candidates)
    }

    fn raw_legal_actions_clone(
        &self,
        seat: Seat,
    ) -> Result<(Vec<Action>, Option<u8>), EngineError> {
        let mut clone = self.clone();
        clone.raw_legal_actions(seat)
    }

    fn drawn_tile(&self) -> Option<u8> {
        match &self.state {
            GameStateVariant::FourPlayer(state) => state.drawn_tile,
            GameStateVariant::ThreePlayer(state) => state.drawn_tile,
        }
    }

    fn last_discard(&self) -> Option<(Seat, Tile)> {
        let raw = match &self.state {
            GameStateVariant::FourPlayer(state) => state.last_discard,
            GameStateVariant::ThreePlayer(state) => state.last_discard,
        }?;
        if usize::from(raw.0) >= self.mode.seat_count() {
            return None;
        }
        Some((Seat::new(raw.0)?, parse_engine_tile(self.mode, raw.1).ok()?))
    }

    fn step_once(&mut self, seat: Seat, action: Action) -> Result<(), EngineError> {
        let mut actions = HashMap::new();
        actions.insert(seat.index(), action);
        step_variant(&mut self.state, &actions);
        if let Some(error) = last_error(&self.state) {
            return Err(EngineError::Rejected(error));
        }
        Ok(())
    }

    fn step_once_without_action(&mut self) -> Result<(), EngineError> {
        step_variant(&mut self.state, &HashMap::new());
        if let Some(error) = last_error(&self.state) {
            return Err(EngineError::Rejected(error));
        }
        Ok(())
    }

    fn round_data(&self) -> (Wind, u8, Seat, u8, u32, u8) {
        match &self.state {
            GameStateVariant::FourPlayer(state) => (
                Wind::from_index(state.round_wind),
                state.kyoku_idx.saturating_add(1),
                Seat::new(state.oya).expect("engine dealer is a valid seat"),
                state.honba,
                state.riichi_sticks,
                state.wall.drawable_count,
            ),
            GameStateVariant::ThreePlayer(state) => (
                Wind::from_index(state.round_wind),
                state.kyoku_idx.saturating_add(1),
                Seat::new(state.oya).expect("engine dealer is a valid seat"),
                state.honba,
                state.riichi_sticks,
                state.wall.drawable_count,
            ),
        }
    }

    fn needs_initialize_next_round(&self) -> bool {
        match &self.state {
            GameStateVariant::FourPlayer(state) => state.needs_initialize_next_round,
            GameStateVariant::ThreePlayer(state) => state.needs_initialize_next_round,
        }
    }

    fn drain_events(&mut self) -> Result<Vec<GameEvent>, EngineError> {
        let logs = match &self.state {
            GameStateVariant::FourPlayer(state) => &state.mjai_log,
            GameStateVariant::ThreePlayer(state) => &state.mjai_log,
        };
        let new_logs = logs.get(self.log_cursor..).unwrap_or_default();
        let mut events = Vec::with_capacity(new_logs.len());
        for log in new_logs {
            events.push(parse_event(log, self.mode)?);
        }
        self.log_cursor = logs.len();
        Ok(events)
    }
}

fn player_snapshot(
    mode: GameMode,
    seat: Seat,
    hand: &[u8],
    discards: &[u8],
    melds: &[riichienv_core::types::Meld],
    score: i32,
    riichi: bool,
    participant: Participant,
) -> Result<TablePlayerState, EngineError> {
    let parse = |tile: &u8| parse_engine_tile(mode, *tile);
    Ok(TablePlayerState {
        seat,
        participant,
        score,
        hand: hand.iter().map(parse).collect::<Result<Vec<_>, _>>()?,
        discards: discards.iter().map(parse).collect::<Result<Vec<_>, _>>()?,
        melds: melds
            .iter()
            .map(|meld| {
                Ok(MeldState {
                    tiles: meld
                        .tiles
                        .iter()
                        .map(parse)
                        .collect::<Result<Vec<_>, _>>()?,
                    opened: meld.opened,
                    from_who: if meld.from_who < 0 {
                        None
                    } else {
                        Seat::new(meld.from_who as u8)
                    },
                    called_tile: meld.called_tile.as_ref().map(&parse).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?,
        riichi,
    })
}

fn engine_mode(mode: GameMode) -> u8 {
    match mode {
        GameMode::FourPlayerRedEast => 1,
        GameMode::FourPlayerRedHalf => 2,
        GameMode::ThreePlayerRedEast => 4,
        GameMode::ThreePlayerRedHalf => 5,
    }
}

fn parse_engine_tile(mode: GameMode, id: u8) -> Result<Tile, EngineError> {
    let tile = Tile::from_id(id).ok_or_else(|| EngineError::InvalidTile(format!("{id}")))?;
    if Tile::canonical_order(mode).contains(&tile) {
        Ok(tile)
    } else {
        Err(EngineError::InvalidTile(format!(
            "tile {id} is not valid for {mode}"
        )))
    }
}

fn step_variant(state: &mut GameStateVariant, actions: &HashMap<u8, Action>) {
    match state {
        GameStateVariant::FourPlayer(state) => state.step(actions),
        GameStateVariant::ThreePlayer(state) => state.step(actions),
    }
}

fn has_last_error(state: &GameStateVariant) -> bool {
    last_error(state).is_some()
}

fn last_error(state: &GameStateVariant) -> Option<String> {
    match state {
        GameStateVariant::FourPlayer(state) => state.last_error.clone(),
        GameStateVariant::ThreePlayer(state) => state.last_error.clone(),
    }
}

fn action_sort_key(action: &GameAction) -> (u8, u8, u8, Vec<u8>) {
    let tile_key = |tile: Tile| (tile.tile_type().index(), u8::from(tile.is_red()), tile.id());
    match action {
        GameAction::Discard { tile, tsumogiri } => {
            let (tile_type, red, id) = tile_key(*tile);
            (0, tile_type, red, vec![u8::from(*tsumogiri), id])
        }
        GameAction::RiichiDiscard { tile } => {
            let (tile_type, red, id) = tile_key(*tile);
            (1, tile_type, red, vec![id])
        }
        GameAction::Chi {
            called, consumed, ..
        } => (
            2,
            tile_key(*called).0,
            tile_key(*called).1,
            consumed.iter().map(|t| t.id()).collect(),
        ),
        GameAction::Pon {
            called, consumed, ..
        } => (
            3,
            tile_key(*called).0,
            tile_key(*called).1,
            consumed.iter().map(|t| t.id()).collect(),
        ),
        GameAction::Daiminkan {
            called, consumed, ..
        } => (
            4,
            tile_key(*called).0,
            tile_key(*called).1,
            consumed.iter().map(|t| t.id()).collect(),
        ),
        GameAction::Ankan { consumed } => (5, 0, 0, consumed.iter().map(|t| t.id()).collect()),
        GameAction::Kakan { called, consumed } => (
            6,
            tile_key(*called).0,
            tile_key(*called).1,
            consumed.iter().map(|t| t.id()).collect(),
        ),
        GameAction::Nuki { tile } => (7, tile_key(*tile).0, tile_key(*tile).1, vec![tile.id()]),
        GameAction::Tsumo => (8, 0, 0, Vec::new()),
        GameAction::Ron(target) => (9, target.index(), 0, Vec::new()),
        GameAction::Pass => (10, 0, 0, Vec::new()),
        GameAction::AbortiveDraw => (11, 0, 0, Vec::new()),
    }
}

fn parse_event(line: &str, mode: GameMode) -> Result<GameEvent, EngineError> {
    let parse_seat_for_mode = |value: usize| parse_seat(value, mode);
    let parse_tile_for_mode = |value: &str| parse_tile(value, mode);
    let raw: MjaiEvent = serde_json::from_str(line)
        .map_err(|error| EngineError::UnsupportedEvent(format!("{error}: {line}")))?;
    match raw {
        MjaiEvent::StartGame { names, id } => Ok(GameEvent::StartGame { names, id }),
        MjaiEvent::StartKyoku {
            bakaze,
            kyoku,
            honba,
            kyoutaku,
            oya,
            scores,
            dora_marker,
            tehais,
        } => Ok(GameEvent::StartKyoku {
            bakaze: parse_wind(&bakaze)?,
            kyoku,
            honba,
            kyotaku: kyoutaku,
            oya: parse_seat_for_mode(usize::from(oya))?,
            scores,
            dora_marker: parse_tile_for_mode(&dora_marker)?,
            tehais: tehais
                .iter()
                .map(|hand| hand.iter().map(|tile| parse_tile_for_mode(tile)).collect())
                .collect::<Result<Vec<_>, _>>()?,
        }),
        MjaiEvent::Tsumo { actor, pai } => Ok(GameEvent::Tsumo {
            actor: parse_seat_for_mode(actor)?,
            tile: parse_tile_for_mode(&pai)?,
        }),
        MjaiEvent::Dahai {
            actor,
            pai,
            tsumogiri,
        } => Ok(GameEvent::Dahai {
            actor: parse_seat_for_mode(actor)?,
            tile: parse_tile_for_mode(&pai)?,
            tsumogiri,
        }),
        MjaiEvent::Pon {
            actor,
            target,
            pai,
            consumed,
        } => Ok(GameEvent::Pon {
            actor: parse_seat_for_mode(actor)?,
            target: parse_seat_for_mode(target)?,
            called: parse_tile_for_mode(&pai)?,
            consumed: consumed
                .iter()
                .map(|tile| parse_tile_for_mode(tile))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        MjaiEvent::Chi {
            actor,
            target,
            pai,
            consumed,
        } => Ok(GameEvent::Chi {
            actor: parse_seat_for_mode(actor)?,
            target: parse_seat_for_mode(target)?,
            called: parse_tile_for_mode(&pai)?,
            consumed: consumed
                .iter()
                .map(|tile| parse_tile_for_mode(tile))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        MjaiEvent::Kan {
            actor,
            target,
            pai,
            consumed,
        } => Ok(GameEvent::Daiminkan {
            actor: parse_seat_for_mode(actor)?,
            target: parse_seat_for_mode(target)?,
            called: parse_tile_for_mode(&pai)?,
            consumed: consumed
                .iter()
                .map(|tile| parse_tile_for_mode(tile))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        MjaiEvent::Kakan { actor, pai } => Ok(GameEvent::Kakan {
            actor: parse_seat_for_mode(actor)?,
            called: parse_tile_for_mode(&pai)?,
        }),
        MjaiEvent::Ankan { actor, consumed } => Ok(GameEvent::Ankan {
            actor: parse_seat_for_mode(actor)?,
            consumed: consumed
                .iter()
                .map(|tile| parse_tile_for_mode(tile))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        MjaiEvent::Dora { dora_marker } => Ok(GameEvent::Dora {
            dora_marker: parse_tile_for_mode(&dora_marker)?,
        }),
        MjaiEvent::Reach { actor } => Ok(GameEvent::Reach {
            actor: parse_seat_for_mode(actor)?,
        }),
        MjaiEvent::ReachAccepted { actor } => Ok(GameEvent::ReachAccepted {
            actor: parse_seat_for_mode(actor)?,
        }),
        MjaiEvent::Hora {
            actor,
            target,
            pai,
            uradora_markers,
            yaku,
            fu,
            han,
            scores,
            delta,
        } => Ok(GameEvent::Hora {
            actor: parse_seat_for_mode(actor)?,
            target: parse_seat_for_mode(target)?,
            tile: pai.as_deref().map(&parse_tile_for_mode).transpose()?,
            ura_markers: uradora_markers
                .as_ref()
                .map(|tiles| tiles.iter().map(|tile| parse_tile_for_mode(tile)).collect())
                .transpose()?,
            yaku,
            fu,
            han,
            scores,
            delta,
        }),
        MjaiEvent::Ryukyoku {
            reason,
            tehais,
            delta,
            scores,
        } => Ok(GameEvent::Ryukyoku {
            reason,
            tehais: tehais
                .as_ref()
                .map(|hands| {
                    hands
                        .iter()
                        .map(|hand| hand.iter().map(|tile| parse_tile_for_mode(tile)).collect())
                        .collect()
                })
                .transpose()?,
            delta,
            scores,
        }),
        MjaiEvent::Kita { actor } => Ok(GameEvent::Kita {
            actor: parse_seat_for_mode(actor)?,
        }),
        MjaiEvent::EndGame => Ok(GameEvent::EndGame),
        MjaiEvent::EndKyoku => Ok(GameEvent::EndKyoku),
        MjaiEvent::Other => Err(EngineError::UnsupportedEvent(line.to_owned())),
    }
}

fn parse_tile(value: &str, mode: GameMode) -> Result<Tile, EngineError> {
    let id = riichienv_core::parser::mjai_to_tid(value)
        .ok_or_else(|| EngineError::InvalidTile(value.to_owned()))?;
    parse_engine_tile(mode, id)
}

fn parse_seat(value: usize, mode: GameMode) -> Result<Seat, EngineError> {
    if value >= mode.seat_count() {
        return Err(EngineError::InvalidEngineSeat(value));
    }
    Seat::new(value as u8).ok_or(EngineError::InvalidEngineSeat(value))
}

fn parse_wind(value: &str) -> Result<Wind, EngineError> {
    match value {
        "E" => Ok(Wind::East),
        "S" => Ok(Wind::South),
        "W" => Ok(Wind::West),
        "N" => Ok(Wind::North),
        other => Err(EngineError::UnsupportedEvent(format!(
            "invalid wind {other}"
        ))),
    }
}

// Keep the engine's private action/event vocabulary below this module boundary.
#[allow(dead_code)]
fn _engine_type_names(_: ActionType) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_consumed_tile_is_adapter_divergence() {
        let (mut adapter, _) = EngineAdapter::new(GameMode::FourPlayerRedEast, Some(0)).unwrap();
        if let GameStateVariant::FourPlayer(state) = &mut adapter.state {
            state.last_discard = Some((1, 8));
        }
        let action = Action::new(ActionType::Pon, Some(8), vec![8, 255], Some(0));

        assert!(matches!(
            adapter.to_neutral_action(&action, None),
            Err(EngineError::InvalidTile(_))
        ));
    }

    #[test]
    fn three_player_parser_rejects_removed_tile() {
        assert!(parse_tile("2m", GameMode::ThreePlayerRedEast).is_err());
    }

    #[test]
    fn parser_rejects_engine_seats_before_narrowing() {
        assert!(parse_seat(256, GameMode::FourPlayerRedEast).is_err());
        assert!(parse_seat(3, GameMode::ThreePlayerRedEast).is_err());
    }
}
