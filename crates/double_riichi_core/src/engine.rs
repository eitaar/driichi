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
    results::{
        KyokuResultRecord, KyokuSettlement, ResultMeld, ResultMeldKind, ScoreChange, ScoreLimit,
        WinEvaluation, WinResult, YakuValue,
    },
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

pub(crate) struct EngineApplyOutcome {
    pub events: Vec<GameEvent>,
    #[allow(dead_code)] // Read by the result adapter in Task 2.
    pub ended: Option<EngineKyokuEnd>,
}

#[allow(dead_code)] // Consumed by result projection in Task 2.
pub(crate) struct EngineKyokuEnd {
    pub pre_action: GameStateVariant,
    pub ended_copy: GameStateVariant,
    pub win_results: HashMap<u8, riichienv_core::types::WinResult>,
    pub real_after_scores: Vec<i32>,
    pub real_after_pot: u32,
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
        let events = adapter.drain_events(&[])?;
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
    ) -> Result<EngineApplyOutcome, EngineError> {
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
        // ponytail: one whole-state/log clone per batch and a second engine evaluation
        // only at a terminal boundary; measure cost before optimizing in Task 8.
        let before = self.state.clone();
        let mut batch = Vec::new();
        match command {
            ApplyCommand::One(engine_action) => {
                let mut commands = HashMap::new();
                commands.insert(seat.index(), engine_action);
                batch.push(commands);
            }
            ApplyCommand::RiichiDiscard { tile } => {
                let mut first = HashMap::new();
                first.insert(
                    seat.index(),
                    Action::new(ActionType::Riichi, None, Vec::new(), Some(seat.index())),
                );
                batch.push(first);
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
                batch.push(second);
            }
        }
        for commands in &batch {
            step_variant(&mut self.state, commands);
            if let Some(error) = last_error(&self.state) {
                return Err(EngineError::Rejected(error));
            }
        }
        self.finish_apply(before, &batch, &[(seat, action)])
    }

    pub(crate) fn apply_simultaneous(
        &mut self,
        actions: &[(Seat, GameAction)],
    ) -> Result<EngineApplyOutcome, EngineError> {
        let mut commands = HashMap::new();
        let mut applied_actions = Vec::with_capacity(actions.len());
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
            applied_actions.push((*seat, action));
        }
        let before = self.state.clone();
        step_variant(&mut self.state, &commands);
        if let Some(error) = last_error(&self.state) {
            return Err(EngineError::Rejected(error));
        }
        self.finish_apply(before, &[commands], &applied_actions)
    }

    fn finish_apply(
        &mut self,
        before: GameStateVariant,
        batch: &[HashMap<u8, Action>],
        actions: &[(Seat, GameAction)],
    ) -> Result<EngineApplyOutcome, EngineError> {
        while self.needs_initialize_next_round() {
            self.step_once_without_action()?;
        }
        let events = self.drain_events(actions)?;
        let ended = if events
            .iter()
            .any(|event| matches!(event, GameEvent::EndKyoku))
        {
            let mut copy = before.clone();
            let cursor = state_logs(&copy).len();
            match &mut copy {
                GameStateVariant::FourPlayer(state) => state.game_mode = 0,
                GameStateVariant::ThreePlayer(state) => state.game_mode = 3,
            }
            for commands in batch {
                step_variant(&mut copy, commands);
                if let Some(error) = last_error(&copy) {
                    return Err(EngineError::Divergence(format!(
                        "capture rejected command: {error}"
                    )));
                }
            }
            let actual_terminals: Vec<_> = events
                .iter()
                .filter(|event| {
                    matches!(event, GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. })
                })
                .cloned()
                .collect();
            let copied_events = state_logs(&copy)[cursor..]
                .iter()
                .map(|line| parse_event(line, self.mode, actions))
                .collect::<Result<Vec<_>, _>>()?;
            if !match &copy {
                GameStateVariant::FourPlayer(state) => state.is_done,
                GameStateVariant::ThreePlayer(state) => state.is_done,
            } || copied_events
                .iter()
                .any(|event| matches!(event, GameEvent::StartKyoku { .. }))
            {
                return Err(EngineError::Divergence(
                    "capture did not stop at the ended kyoku".into(),
                ));
            }
            let copied_terminals = copied_events
                .into_iter()
                .filter(|event| {
                    matches!(event, GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. })
                })
                .collect::<Vec<_>>();
            if actual_terminals != copied_terminals {
                return Err(EngineError::Divergence(
                    "captured terminal events differ from real resolution".into(),
                ));
            }
            let (win_results, copy_scores, copy_pot) = match &copy {
                GameStateVariant::FourPlayer(state) => (
                    state.win_results.clone(),
                    state.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                    state.riichi_sticks,
                ),
                GameStateVariant::ThreePlayer(state) => (
                    state.win_results.clone(),
                    state.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                    state.riichi_sticks,
                ),
            };
            let winner_count = actual_terminals
                .iter()
                .filter(|event| matches!(event, GameEvent::Hora { .. }))
                .count();
            if win_results.len() != winner_count {
                return Err(EngineError::Divergence(format!(
                    "capture has {} results for {winner_count} winners",
                    win_results.len()
                )));
            }
            let scores = self.scores();
            let pot = self.riichi_sticks();
            let mut expected = copy_scores;
            if self.is_done() && copy_pot > 0 && pot == 0 {
                let top = (0..expected.len())
                    .max_by_key(|&seat| (expected[seat], std::cmp::Reverse(seat)))
                    .unwrap();
                expected[top] += (copy_pot as i32) * 1000;
            } else if pot != copy_pot {
                return Err(EngineError::Divergence(
                    "captured pot differs from real resolution".into(),
                ));
            }
            if scores != expected {
                return Err(EngineError::Divergence(format!(
                    "captured scores {expected:?} differ from real {scores:?}"
                )));
            }
            Some(EngineKyokuEnd {
                pre_action: before,
                ended_copy: copy,
                win_results,
                real_after_scores: scores,
                real_after_pot: pot,
            })
        } else {
            None
        };
        Ok(EngineApplyOutcome { events, ended })
    }

    pub(crate) fn result_record(
        &self,
        ended: &EngineKyokuEnd,
        roster: &[Participant],
        kyoku_sequence: u64,
        first_event_index: usize,
        events: &[GameEvent],
    ) -> Result<KyokuResultRecord, EngineError> {
        let invalid = |reason: &str| EngineError::Divergence(reason.into());
        if roster.len() != self.mode.seat_count() || ended.real_after_scores.len() != roster.len() {
            return Err(invalid("result roster size differs from engine"));
        }
        let (scores, pot_before) = match &ended.pre_action {
            GameStateVariant::FourPlayer(s) => (
                s.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                s.riichi_sticks,
            ),
            GameStateVariant::ThreePlayer(s) => (
                s.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                s.riichi_sticks,
            ),
        };
        let changes = roster
            .iter()
            .enumerate()
            .map(|(i, player)| {
                Ok(ScoreChange {
                    participant_id: player.id.clone(),
                    seat: Seat::new(i as u8).ok_or_else(|| invalid("invalid seat"))?,
                    before: scores[i],
                    delta: ended.real_after_scores[i]
                        .checked_sub(scores[i])
                        .ok_or_else(|| invalid("settlement overflow"))?,
                    after: ended.real_after_scores[i],
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let mut wins = Vec::new();
        let mut draw_reason = None;
        let mut end_event_index = None;
        for (index, event) in events.iter().enumerate() {
            let event_index = first_event_index
                .checked_add(index)
                .ok_or_else(|| invalid("event index overflow"))?;
            match event {
                GameEvent::Hora {
                    actor,
                    target,
                    tile,
                    ura_markers,
                    ..
                } => {
                    let seat = actor.index() as usize;
                    let adjudicated = ended
                        .win_results
                        .get(&actor.index())
                        .ok_or_else(|| invalid("missing adjudicated winner"))?;
                    let (hand, melds, drawn, indicators, kita, dealer, rule) =
                        match &ended.pre_action {
                            GameStateVariant::FourPlayer(s) => (
                                &s.players[seat].hand,
                                &s.players[seat].melds,
                                s.drawn_tile,
                                &s.wall.dora_indicators,
                                s.players[seat].kita_tiles.len(),
                                s.oya,
                                &s.rule,
                            ),
                            GameStateVariant::ThreePlayer(s) => (
                                &s.players[seat].hand,
                                &s.players[seat].melds,
                                s.drawn_tile,
                                &s.wall.dora_indicators,
                                s.players[seat].kita_tiles.len(),
                                s.oya,
                                &s.rule,
                            ),
                        };
                    let tsumo = actor == target;
                    let winning_id = if tsumo {
                        drawn.ok_or_else(|| invalid("missing drawn winning tile"))?
                    } else {
                        let discard = match &ended.pre_action {
                            GameStateVariant::FourPlayer(s) => s.last_discard,
                            GameStateVariant::ThreePlayer(s) => s.last_discard,
                        }
                        .ok_or_else(|| invalid("missing accepted ron physical tile"))?;
                        if discard.0 != target.index() {
                            return Err(invalid("ron target differs from captured discard"));
                        }
                        discard.1
                    };
                    if tile.is_some_and(|t| t.id() != winning_id) {
                        return Err(invalid("winning event tile differs from draw"));
                    }
                    let mut concealed = hand
                        .iter()
                        .map(|&id| parse_engine_tile(self.mode, id))
                        .collect::<Result<Vec<_>, _>>()?;
                    let winning_tile_index = if tsumo {
                        hand.iter()
                            .position(|&id| id == winning_id)
                            .ok_or_else(|| invalid("drawn physical tile missing from hand"))?
                    } else {
                        concealed.push(parse_engine_tile(self.mode, winning_id)?);
                        concealed.len() - 1
                    };
                    let winning_tile = parse_engine_tile(self.mode, winning_id)?;
                    let melds = melds
                        .iter()
                        .map(|m| {
                            Ok(ResultMeld {
                                kind: match m.meld_type {
                                    riichienv_core::types::MeldType::Chi => ResultMeldKind::Chi,
                                    riichienv_core::types::MeldType::Pon => ResultMeldKind::Pon,
                                    riichienv_core::types::MeldType::Daiminkan => {
                                        ResultMeldKind::Daiminkan
                                    }
                                    riichienv_core::types::MeldType::Ankan => ResultMeldKind::Ankan,
                                    riichienv_core::types::MeldType::Kakan => ResultMeldKind::Kakan,
                                },
                                tiles: m
                                    .tiles
                                    .iter()
                                    .map(|&t| parse_engine_tile(self.mode, t))
                                    .collect::<Result<Vec<_>, _>>()?,
                                opened: m.opened,
                                from_who: if m.from_who < 0 {
                                    None
                                } else {
                                    Some(parse_seat(m.from_who as usize, self.mode)?)
                                },
                                called_tile: m
                                    .called_tile
                                    .map(|t| parse_engine_tile(self.mode, t))
                                    .transpose()?,
                            })
                        })
                        .collect::<Result<Vec<_>, EngineError>>()?;
                    let full = concealed
                        .iter()
                        .chain(melds.iter().flat_map(|m: &ResultMeld| m.tiles.iter()))
                        .map(|tile| tile.id())
                        .collect::<Vec<_>>();
                    let next_kind = |id: u8| -> u8 {
                        let kind = id / 4;
                        if self.mode.is_three_player() && kind == 0 {
                            return 8;
                        }
                        if self.mode.is_three_player() && kind == 8 {
                            return 0;
                        }
                        match kind {
                            8 => 0,
                            17 => 9,
                            26 => 18,
                            30 => 27,
                            33 => 31,
                            _ => kind + 1,
                        }
                    };
                    let count_dora = |markers: &[u8]| -> u32 {
                        markers
                            .iter()
                            .map(|&marker| {
                                full.iter()
                                    .filter(|&&id| id / 4 == next_kind(marker))
                                    .count() as u32
                            })
                            .sum()
                    };
                    let ura = ura_markers
                        .as_ref()
                        .map(|markers| markers.iter().map(|t| t.id()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    let bonus = [
                        count_dora(indicators),
                        full.iter()
                            .filter(|&&id| [16, 52, 88].contains(&id))
                            .count() as u32,
                        count_dora(&ura),
                        kita as u32,
                    ];
                    let is_open = melds.iter().any(|m: &ResultMeld| m.opened);
                    let mut yaku = Vec::new();
                    for &id in &adjudicated.yaku {
                        let name = riichienv_core::yaku::get_yaku_by_id(id)
                            .ok_or_else(|| invalid("unknown adjudicated yaku ID"))?
                            .name;
                        let value = if adjudicated.yakuman {
                            let multiplier = match id {
                                47 if rule.is_junsei_chuurenpoutou_double => 2,
                                48 if rule.is_suuankou_tanki_double => 2,
                                49 if rule.is_kokushi_musou_13machi_double => 2,
                                50 if rule.is_daisuushii_double => 2,
                                35..=50 => 1,
                                _ => return Err(invalid("ordinary ID in yakuman result")),
                            };
                            YakuValue {
                                id,
                                name,
                                han: None,
                                yakuman_multiplier: Some(multiplier),
                            }
                        } else {
                            let han = match id {
                                31..=34 => bonus[(id - 31) as usize],
                                15..=17 | 26..=27 => {
                                    if is_open {
                                        if id >= 26 { 2 } else { 1 }
                                    } else if id >= 26 {
                                        3
                                    } else {
                                        2
                                    }
                                }
                                29 => {
                                    if is_open {
                                        5
                                    } else {
                                        6
                                    }
                                }
                                18..=25 => 2,
                                28 => 3,
                                1..=14 | 30 => 1,
                                _ => return Err(invalid("invalid ordinary yaku ID")),
                            };
                            if han == 0 {
                                return Err(invalid("zero-value adjudicated yaku"));
                            }
                            YakuValue {
                                id,
                                name,
                                han: Some(han),
                                yakuman_multiplier: None,
                            }
                        };
                        yaku.push(value);
                    }
                    let total: u32 = yaku
                        .iter()
                        .map(|y| y.han.unwrap_or_else(|| y.yakuman_multiplier.unwrap() * 13))
                        .sum();
                    if total != adjudicated.han || adjudicated.han == 0 {
                        return Err(invalid("yaku values differ from adjudicated han"));
                    }
                    let evaluation = if adjudicated.yakuman {
                        WinEvaluation::Yakuman {
                            multiplier: total / 13,
                        }
                    } else {
                        let han = adjudicated.han;
                        let fu = adjudicated.fu;
                        if fu == 0 {
                            return Err(invalid("ordinary win has no fu"));
                        }
                        let limit = match han {
                            0..=4 => {
                                if (if fu == 25 { 25 } else { fu.div_ceil(10) * 10 })
                                    .checked_shl(han + 2)
                                    .is_some_and(|v| v > 2000)
                                {
                                    Some(ScoreLimit::Mangan)
                                } else {
                                    None
                                }
                            }
                            5 => Some(ScoreLimit::Mangan),
                            6..=7 => Some(ScoreLimit::Haneman),
                            8..=10 => Some(ScoreLimit::Baiman),
                            11..=12 => Some(ScoreLimit::Sanbaiman),
                            _ => Some(ScoreLimit::CountedYakuman),
                        };
                        WinEvaluation::Ordinary { han, fu, limit }
                    };
                    let scoring_han = if !adjudicated.yakuman && total >= 13 {
                        13
                    } else {
                        total
                    };
                    let score = riichienv_core::score::calculate_score(
                        u8::try_from(scoring_han).map_err(|_| invalid("scoring han overflow"))?,
                        u8::try_from(adjudicated.fu).map_err(|_| invalid("fu overflow"))?,
                        actor.index() == dealer,
                        tsumo,
                        0,
                        self.mode.seat_count() as u8,
                    );
                    let honba = match &ended.pre_action {
                        GameStateVariant::FourPlayer(s) => u32::from(s.honba),
                        GameStateVariant::ThreePlayer(s) => u32::from(s.honba),
                    };
                    let applied_honba = if tsumo || wins.is_empty() { honba } else { 0 };
                    let adjudicated_base = riichienv_core::score::calculate_score(
                        u8::try_from(scoring_han).map_err(|_| invalid("scoring han overflow"))?,
                        u8::try_from(adjudicated.fu).map_err(|_| invalid("fu overflow"))?,
                        actor.index() == dealer,
                        tsumo,
                        applied_honba,
                        self.mode.seat_count() as u8,
                    );
                    if !adjudicated.is_win
                        || adjudicated.ron_agari != adjudicated_base.pay_ron
                        || adjudicated.tsumo_agari_oya != adjudicated_base.pay_tsumo_oya
                        || adjudicated.tsumo_agari_ko != adjudicated_base.pay_tsumo_ko
                    {
                        return Err(invalid("adjudicated payment differs from calculated score"));
                    }
                    wins.push(WinResult {
                        event_index,
                        winner: roster[seat].id.clone(),
                        seat: *actor,
                        target: *target,
                        tsumo,
                        concealed,
                        melds,
                        winning_tile,
                        winning_tile_index,
                        yaku,
                        evaluation,
                        win_points: score.total,
                    });
                }
                GameEvent::Ryukyoku { reason, .. } => {
                    draw_reason = Some(
                        reason
                            .clone()
                            .ok_or_else(|| invalid("missing engine draw reason"))?,
                    );
                }
                GameEvent::EndKyoku => {
                    end_event_index = Some(event_index);
                }
                _ => {}
            }
        }
        let result = KyokuResultRecord {
            version: 1,
            kyoku_sequence,
            mode: self.mode,
            end_event_index: end_event_index.ok_or_else(|| invalid("missing end kyoku event"))?,
            wins,
            settlement: KyokuSettlement {
                changes,
                pot_before,
                pot_after: ended.real_after_pot,
                draw_reason,
            },
        };
        result.validate(roster).map_err(EngineError::Divergence)?;
        Ok(result)
    }

    fn riichi_sticks(&self) -> u32 {
        match &self.state {
            GameStateVariant::FourPlayer(state) => state.riichi_sticks,
            GameStateVariant::ThreePlayer(state) => state.riichi_sticks,
        }
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

    fn drain_events(
        &mut self,
        applied_actions: &[(Seat, GameAction)],
    ) -> Result<Vec<GameEvent>, EngineError> {
        let logs = match &self.state {
            GameStateVariant::FourPlayer(state) => &state.mjai_log,
            GameStateVariant::ThreePlayer(state) => &state.mjai_log,
        };
        let new_logs = logs.get(self.log_cursor..).unwrap_or_default();
        let mut events = Vec::with_capacity(new_logs.len());
        for log in new_logs {
            events.push(parse_event(log, self.mode, applied_actions)?);
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

fn state_logs(state: &GameStateVariant) -> &[String] {
    match state {
        GameStateVariant::FourPlayer(state) => &state.mjai_log,
        GameStateVariant::ThreePlayer(state) => &state.mjai_log,
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

fn parse_event(
    line: &str,
    mode: GameMode,
    applied_actions: &[(Seat, GameAction)],
) -> Result<GameEvent, EngineError> {
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
        MjaiEvent::Kakan { actor, pai } => {
            let actor = parse_seat_for_mode(actor)?;
            let consumed = applied_actions
                .iter()
                .find_map(|(seat, action)| match action {
                    GameAction::Kakan { consumed, .. } if *seat == actor => Some(consumed.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    EngineError::Divergence("kakan event has no matching applied action".into())
                })?;
            Ok(GameEvent::Kakan {
                actor,
                called: parse_tile_for_mode(&pai)?,
                consumed,
            })
        }
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

    fn prepared_capture(mode: GameMode, ron: bool, kokushi: bool) -> EngineAdapter {
        use riichienv_core::action::Phase;
        let (mut adapter, _) = EngineAdapter::new(mode, Some(230)).unwrap();
        macro_rules! prepare {
            ($state:expr) => {{
                let state = $state;
                for player in state.players.iter_mut() {
                    player.score = 100_000;
                }
                state.is_first_turn = false;
                state.honba = 2;
                state.riichi_sticks = 2;
                state.wall.dora_indicators.clear();
                state.mjai_log.clear();
                state.current_player = if ron { 0 } else { 1 };
                state.needs_tsumo = false;
                state.phase = if ron {
                    Phase::WaitResponse
                } else {
                    Phase::WaitAct
                };
                state.active_players = if ron { vec![1, 2] } else { vec![1] };
                state.current_claims.clear();
                let win = if kokushi { 1 } else { 77 };
                for seat in 1..if ron { 3 } else { 2 } {
                    let player = &mut state.players[seat];
                    let kinds: &[u8] = if kokushi {
                        &[0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33]
                    } else {
                        &[9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 18, 18, 19]
                    };
                    let mut copies = [0u8; 34];
                    player.hand = kinds
                        .iter()
                        .map(|&kind| {
                            let id = kind * 4
                                + ((if seat == 1 { 0 } else { 2 }) + copies[kind as usize]) % 4;
                            copies[kind as usize] += 1;
                            id
                        })
                        .collect();
                    player.melds.clear();
                    player.riichi_declared = false;
                    player.double_riichi_declared = false;
                    player.ippatsu_cycle = false;
                    player.missed_agari_doujun = false;
                    player.missed_agari_riichi = false;
                    if !ron {
                        player.hand.push(win);
                    }
                    if ron {
                        state.current_claims.insert(
                            seat as u8,
                            vec![Action::new(ActionType::Ron, None, vec![], Some(seat as u8))],
                        );
                    }
                }
                if ron {
                    state.last_discard = Some((0, win));
                    state.drawn_tile = None;
                } else {
                    state.drawn_tile = Some(win);
                }
            }};
        }
        match &mut adapter.state {
            GameStateVariant::FourPlayer(state) => prepare!(state),
            GameStateVariant::ThreePlayer(state) => prepare!(state),
        }
        adapter.log_cursor = 0;
        adapter
    }

    #[test]
    fn result_preserves_physical_winning_position() {
        let mode = GameMode::FourPlayerRedEast;
        let roster = (0..4)
            .map(|seat| {
                Participant::new(
                    format!("p{seat}"),
                    "same name",
                    crate::ParticipantKind::Human,
                )
            })
            .collect::<Vec<_>>();
        for ron in [false, true] {
            let mut adapter = prepared_capture(mode, ron, false);
            let outcome = if ron {
                adapter
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
                    .unwrap()
            } else {
                adapter
                    .apply(Seat::new(1).unwrap(), &GameAction::Tsumo)
                    .unwrap()
            };
            let record = adapter
                .result_record(&outcome.ended.unwrap(), &roster, 1, 0, &outcome.events)
                .unwrap();
            assert_eq!(record.wins.len(), if ron { 2 } else { 1 });
            assert!(record.wins[0].concealed.iter().any(|tile| tile.id() == 52));
            assert!(
                record.wins[0]
                    .yaku
                    .iter()
                    .any(|y| y.id == 32 && y.han == Some(1))
            );
            for win in &record.wins {
                assert_eq!(win.concealed[win.winning_tile_index], win.winning_tile);
                assert_eq!(
                    win.concealed
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| *index == win.winning_tile_index)
                        .count(),
                    1
                );
                if win.seat.index() == 1 {
                    assert_ne!(win.win_points as i32, record.settlement.changes[1].delta);
                }
            }
            assert_eq!(
                record
                    .wins
                    .iter()
                    .map(|w| w.seat.index())
                    .collect::<Vec<_>>(),
                if ron { vec![1, 2] } else { vec![1] }
            );
        }
    }

    #[test]
    fn settlement_uses_real_scores_and_final_pot() {
        for (mode, expected_points) in [
            (GameMode::ThreePlayerRedEast, 6_000),
            (GameMode::FourPlayerRedEast, 8_000),
        ] {
            let mut adapter = prepared_capture(mode, false, false);
            let roster = (0..mode.seat_count())
                .map(|seat| {
                    Participant::new(format!("p{seat}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let outcome = adapter
                .apply(Seat::new(1).unwrap(), &GameAction::Tsumo)
                .unwrap();
            let record = adapter
                .result_record(&outcome.ended.unwrap(), &roster, 1, 0, &outcome.events)
                .unwrap();
            assert!(
                record
                    .settlement
                    .changes
                    .iter()
                    .all(|c| c.before.checked_add(c.delta) == Some(c.after))
            );
            assert_eq!(record.settlement.pot_before, 2);
            assert_eq!(record.settlement.pot_after, 0);
            assert_eq!(
                record.wins[0].evaluation,
                WinEvaluation::Ordinary {
                    han: 4,
                    fu: 40,
                    limit: Some(ScoreLimit::Mangan)
                }
            );
            assert_eq!(record.wins[0].win_points, expected_points); // Excludes honba/deposits.
        }
    }

    #[test]
    fn pre_action_capture_survives_next_round() {
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            for kokushi in [false, true] {
                let mut adapter = prepared_capture(mode, false, kokushi);
                let outcome = adapter
                    .apply(Seat::new(1).unwrap(), &GameAction::Tsumo)
                    .unwrap();
                let capture = outcome.ended.unwrap();
                assert_eq!(capture.real_after_scores, adapter.scores());
                assert_eq!(capture.win_results.len(), 1);
                if kokushi {
                    assert_eq!(capture.win_results[&1].han, 13);
                }
                assert!(
                    outcome
                        .events
                        .iter()
                        .any(|event| matches!(event, GameEvent::EndKyoku))
                );
                assert!(match &adapter.state {
                    GameStateVariant::FourPlayer(s) => s.win_results.is_empty(),
                    GameStateVariant::ThreePlayer(s) => s.win_results.is_empty(),
                });
            }
        }
    }

    #[test]
    fn simultaneous_capture_uses_one_resolution() {
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            for kokushi in [false, true] {
                let mut adapter = prepared_capture(mode, true, kokushi);
                let outcome = adapter
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
                let capture = outcome.ended.unwrap();
                assert_eq!(capture.win_results.len(), 2);
                assert_eq!(capture.real_after_scores, adapter.scores());
                if kokushi {
                    assert_eq!(capture.win_results[&1].han, 13);
                }
                assert_eq!(
                    outcome
                        .events
                        .iter()
                        .filter(|e| matches!(e, GameEvent::Hora { .. }))
                        .count(),
                    2
                );
            }
        }
    }

    #[test]
    fn capture_rejection_is_not_adopted() {
        let (mut adapter, _) = EngineAdapter::new(GameMode::FourPlayerRedEast, Some(0)).unwrap();
        let seat = Seat::new(0).unwrap();
        assert!(matches!(
            adapter.apply(seat, &GameAction::Tsumo),
            Err(EngineError::Rejected(_))
        ));
        assert!(adapter.scores().iter().all(|&score| score == 25_000));
    }

    #[test]
    fn seeded_legal_action_lifecycle_reaches_capture() {
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            let (mut adapter, _) = EngineAdapter::new(mode, Some(230)).unwrap();
            let mut ended = 0;
            for _ in 0..2000 {
                if adapter.is_done() || ended > 0 {
                    break;
                }
                let eligible = Seat::all(mode)
                    .into_iter()
                    .filter_map(|seat| {
                        let legal = adapter.legal_actions(seat).unwrap();
                        (!legal.is_empty()).then_some((seat, legal))
                    })
                    .collect::<Vec<_>>();
                assert!(!eligible.is_empty());
                let actions = eligible
                    .iter()
                    .map(|(seat, legal)| {
                        let action = legal
                            .iter()
                            .find(|action| matches!(action, GameAction::Tsumo | GameAction::Ron(_)))
                            .or_else(|| {
                                legal
                                    .iter()
                                    .find(|action| matches!(action, GameAction::Pass))
                            })
                            .or_else(|| {
                                legal
                                    .iter()
                                    .find(|action| matches!(action, GameAction::Discard { .. }))
                            })
                            .unwrap()
                            .clone();
                        (*seat, action)
                    })
                    .collect::<Vec<_>>();
                let outcome = if actions.len() == 1 {
                    adapter.apply(actions[0].0, &actions[0].1).unwrap()
                } else {
                    adapter.apply_simultaneous(&actions).unwrap()
                };
                if let Some(capture) = outcome.ended {
                    ended += 1;
                    let roster = (0..mode.seat_count())
                        .map(|seat| {
                            Participant::new(
                                format!("seeded-{seat}"),
                                "same",
                                crate::ParticipantKind::Human,
                            )
                        })
                        .collect::<Vec<_>>();
                    let record = adapter
                        .result_record(&capture, &roster, 1, 0, &outcome.events)
                        .unwrap();
                    assert_eq!(record.settlement.changes.len(), mode.seat_count());
                    assert_eq!(capture.real_after_scores, adapter.scores());
                    assert_eq!(capture.real_after_pot, adapter.riichi_sticks());
                    assert!(
                        outcome
                            .events
                            .iter()
                            .any(|event| matches!(event, GameEvent::EndKyoku))
                    );
                }
            }
            assert_eq!(ended, 1, "{mode:?} never terminated a kyoku");
        }
    }

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
    fn kakan_parser_restores_consumed_tiles_from_the_applied_action() {
        let actor = Seat::new(3).unwrap();
        let consumed = vec![
            Tile::from_id(33).unwrap(),
            Tile::from_id(34).unwrap(),
            Tile::from_id(35).unwrap(),
        ];

        let event = parse_event(
            r#"{"type":"kakan","actor":3,"pai":"9m"}"#,
            GameMode::FourPlayerRedEast,
            &[(
                actor,
                GameAction::Kakan {
                    called: Tile::from_id(32).unwrap(),
                    consumed: consumed.clone(),
                },
            )],
        )
        .unwrap();

        assert!(matches!(
            event,
            GameEvent::Kakan {
                actor: event_actor,
                consumed: event_consumed,
                ..
            } if event_actor == actor && event_consumed == consumed
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
