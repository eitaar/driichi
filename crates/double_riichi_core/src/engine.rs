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

    /// Finite, opt-in test deal. The installed producer generates all initial logs
    /// and subsequent offers; neither claims nor authoritative results are injected.
    #[cfg(feature = "task8-test-support")]
    pub(crate) fn task8_double_ron(mode: GameMode) -> Result<(Self, Vec<GameEvent>), EngineError> {
        Self::task8_double_ron_deal(mode, false, false)
    }

    #[cfg(feature = "task8-test-support")]
    pub(crate) fn task8_double_ron_final(
        mode: GameMode,
    ) -> Result<(Self, Vec<GameEvent>), EngineError> {
        Self::task8_double_ron_deal(mode, true, false)
    }

    #[cfg(feature = "task8-test-support")]
    pub(crate) fn task8_final_draw(mode: GameMode) -> Result<(Self, Vec<GameEvent>), EngineError> {
        let count = mode.seat_count();
        let dealer = count - 1;
        let physical = (0..136u8)
            .filter(|t| count == 4 || !(4..32).contains(t))
            .collect::<Vec<_>>();
        let mut hands = vec![vec![108, 112], vec![116, 120], vec![124, 128]];
        if count == 4 {
            hands.push(vec![132, 109]);
        }
        let reserved = hands.iter().flatten().copied().collect::<Vec<_>>();
        let mut pool = physical
            .iter()
            .copied()
            .filter(|t| !reserved.contains(t) && *t != 121)
            .collect::<Vec<_>>();
        for hand in &mut hands {
            for _ in 0..11 {
                let i = pool
                    .iter()
                    .position(|t| t / 4 < 27 && !matches!((t / 4) % 9, 0 | 8))
                    .unwrap();
                hand.push(pool.remove(i));
            }
            hand.sort_unstable();
        }
        let mut wall = Vec::with_capacity(physical.len());
        for offset in [0, 4, 8] {
            for index in 0..count {
                let h = &hands[(index + dealer) % count];
                wall.extend_from_slice(&h[offset..offset + 4]);
            }
        }
        for index in 0..count {
            wall.push(hands[(index + dealer) % count][12]);
        }
        // Every first discard is simple, so no player can retain Nagashi eligibility.
        for _ in 0..count {
            let i = pool
                .iter()
                .position(|t| t / 4 < 27 && !matches!((t / 4) % 9, 0 | 8))
                .unwrap();
            wall.push(pool.remove(i));
        }
        wall.extend(pool);
        wall.push(121);
        let last = wall.len() - 1;
        wall.swap(if count == 4 { 131 } else { 99 }, last);
        let mut inventory = wall.clone();
        inventory.sort_unstable();
        if inventory != physical {
            return Err(EngineError::Divergence(
                "Task8 final draw lost physical tiles".into(),
            ));
        }
        let mut state = GameStateVariant::new(
            engine_mode(mode),
            false,
            Some(230),
            0,
            GameRule::default_tenhou(),
        );
        let wind = if matches!(
            mode,
            GameMode::FourPlayerRedHalf | GameMode::ThreePlayerRedHalf
        ) {
            1
        } else {
            0
        };
        match &mut state {
            GameStateVariant::FourPlayer(s) => {
                s.reset();
                s._initialize_round(
                    dealer as u8,
                    wind,
                    2,
                    2,
                    Some(wall),
                    Some(vec![100000; count]),
                );
                for (i, h) in hands.iter().enumerate() {
                    assert_eq!(&s.players[i].hand[..13], h);
                }
            }
            GameStateVariant::ThreePlayer(s) => {
                s.reset();
                s._initialize_round(
                    dealer as u8,
                    wind,
                    2,
                    2,
                    Some(wall),
                    Some(vec![100000; count]),
                );
                for (i, h) in hands.iter().enumerate() {
                    assert_eq!(&s.players[i].hand[..13], h);
                }
            }
        }
        let mut adapter = Self {
            mode,
            state,
            log_cursor: 0,
        };
        let events = adapter.drain_events(&[])?;
        Ok((adapter, events))
    }

    #[cfg(feature = "task8-test-support")]
    pub(crate) fn task8_dealer_tenhou(
        mode: GameMode,
    ) -> Result<(Self, Vec<GameEvent>), EngineError> {
        if mode != GameMode::FourPlayerRedEast {
            return Err(EngineError::Rejected(
                "Task8 dealer-tenhou requires 4p-red-east".into(),
            ));
        }
        Self::task8_double_ron_deal(mode, false, true)
    }

    #[cfg(feature = "task8-test-support")]
    fn task8_double_ron_deal(
        mode: GameMode,
        final_match: bool,
        dealer_tenhou: bool,
    ) -> Result<(Self, Vec<GameEvent>), EngineError> {
        const FOUR: [u8; 136] = [
            0, 1, 2, 3, 36, 40, 44, 48, 37, 41, 45, 49, 4, 8, 12, 17, 32, 33, 34, 35, 53, 56, 60,
            64, 54, 57, 61, 65, 20, 24, 28, 39, 38, 46, 55, 62, 68, 72, 73, 74, 69, 80, 81, 82, 43,
            47, 51, 58, 70, 76, 78, 66, 77, 5, 6, 7, 9, 10, 11, 13, 14, 15, 16, 18, 19, 21, 22, 23,
            25, 26, 27, 29, 30, 31, 42, 50, 52, 59, 63, 67, 71, 75, 79, 83, 84, 85, 86, 87, 88, 89,
            90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108,
            109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 131, 121, 122, 123, 124, 125,
            126, 127, 128, 129, 130, 120, 132, 133, 134, 135,
        ];
        const THREE: [u8; 108] = [
            0, 1, 2, 3, 36, 40, 44, 48, 37, 41, 45, 49, 32, 33, 34, 35, 53, 56, 60, 64, 54, 57, 61,
            65, 38, 46, 55, 62, 68, 72, 73, 74, 69, 80, 81, 82, 70, 76, 78, 77, 39, 42, 43, 47, 50,
            51, 52, 58, 59, 63, 66, 67, 71, 75, 79, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94,
            95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112,
            113, 114, 115, 116, 117, 118, 119, 127, 121, 122, 123, 124, 125, 126, 120, 128, 129,
            130, 131, 132, 133, 134, 135,
        ];
        let mut wall = match mode {
            GameMode::FourPlayerRedEast | GameMode::FourPlayerRedHalf => FOUR.to_vec(),
            GameMode::ThreePlayerRedEast => THREE.to_vec(),
            _ => {
                return Err(EngineError::Rejected(
                    "Task8 double-ron requires a supported red mode".into(),
                ));
            }
        };
        if dealer_tenhou {
            for offset in [0, 16, 32] {
                for tile in 0..4 {
                    wall.swap(offset + tile, offset + 4 + tile);
                }
            }
            wall.swap(48, 49);
        }
        let expected = (0..136u8)
            .filter(|&tile| mode.seat_count() == 4 || !(4..32).contains(&tile))
            .collect::<Vec<_>>();
        let mut sorted = wall.clone();
        sorted.sort_unstable();
        if sorted != expected {
            return Err(EngineError::Divergence(
                "Task8 full-wall permutation is invalid".into(),
            ));
        }
        let mut scores = vec![100_000; mode.seat_count()];
        if final_match {
            scores[0] = 1_000;
        }
        let mut state = GameStateVariant::new(
            engine_mode(mode),
            false,
            Some(230),
            0,
            GameRule::default_tenhou(),
        );
        match &mut state {
            GameStateVariant::FourPlayer(s) => {
                s.reset();
                s._initialize_round(0, 0, 2, 2, Some(wall), Some(scores));
                let mut physical = s.wall.tiles.clone();
                physical.extend(s.players.iter().flat_map(|p| p.hand.iter().copied()));
                physical.sort_unstable();
                if physical != expected {
                    return Err(EngineError::Divergence(
                        "Task8 deal lost physical tiles".into(),
                    ));
                }
            }
            GameStateVariant::ThreePlayer(s) => {
                s.reset();
                s._initialize_round(0, 0, 2, 2, Some(wall), Some(scores));
                let mut physical = s.wall.tiles.clone();
                physical.extend(s.players.iter().flat_map(|p| p.hand.iter().copied()));
                physical.sort_unstable();
                if physical != expected {
                    return Err(EngineError::Divergence(
                        "Task8 deal lost physical tiles".into(),
                    ));
                }
            }
        }
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
                let award = i32::try_from(copy_pot)
                    .ok()
                    .and_then(|sticks| sticks.checked_mul(1_000))
                    .ok_or_else(|| EngineError::Divergence("captured pot award overflow".into()))?;
                expected[top] = expected[top].checked_add(award).ok_or_else(|| {
                    EngineError::Divergence("captured score award overflow".into())
                })?;
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
                                let kind = next_kind(marker);
                                full.iter().filter(|&&id| id / 4 == kind).count() as u32
                                    + u32::from(self.mode.is_three_player() && kind == 30)
                                        * kita as u32
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
                        let limit = ScoreLimit::ordinary(han, fu);
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
        let draw_event_delta_adjustment = if draw_reason.is_some() {
            let deltas = events
                .iter()
                .filter_map(|event| match event {
                    GameEvent::Ryukyoku { delta, .. } => Some(delta.as_ref()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let delta = match deltas.as_slice() {
                [Some(delta)] if delta.len() == roster.len() => *delta,
                _ => return Err(invalid("missing or malformed canonical draw delta")),
            };
            let ended_scores = match &ended.ended_copy {
                GameStateVariant::FourPlayer(state) => {
                    state.players.iter().map(|p| p.score).collect::<Vec<_>>()
                }
                GameStateVariant::ThreePlayer(state) => {
                    state.players.iter().map(|p| p.score).collect::<Vec<_>>()
                }
            };
            if ended_scores.len() != scores.len() {
                return Err(invalid("captured draw score count differs from roster"));
            }
            Some(
                delta
                    .iter()
                    .zip(&scores)
                    .zip(ended_scores)
                    .map(|((&emitted, &before), after)| {
                        let movement = after
                            .checked_sub(before)
                            .ok_or_else(|| invalid("draw movement overflow"))?;
                        emitted
                            .checked_sub(movement)
                            .ok_or_else(|| invalid("draw adjustment overflow"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };
        let result = KyokuResultRecord {
            version: 1,
            kyoku_sequence,
            mode: self.mode,
            end_event_index: end_event_index.ok_or_else(|| invalid("missing end kyoku event"))?,
            wins,
            draw_event_delta_adjustment,
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

    pub(crate) fn ended_table_state(
        &self,
        ended: &EngineKyokuEnd,
        participants: &[Participant],
        before_scores: &[i32],
        pot_before: u32,
    ) -> Result<TableState, EngineError> {
        if participants.len() != self.mode.seat_count() || before_scores.len() != participants.len()
        {
            return Err(EngineError::Divergence(
                "ended table roster/score count mismatch".into(),
            ));
        }
        let mut snapshot = self.clone();
        snapshot.state = ended.ended_copy.clone();
        let table = snapshot.table_state(self.mode, participants)?;
        if table
            .players()
            .iter()
            .zip(participants)
            .any(|(player, participant)| player.participant.id != participant.id)
        {
            return Err(EngineError::Divergence(
                "ended table roster identity mismatch".into(),
            ));
        }
        Ok(table.with_settlement_state(before_scores, pot_before))
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
                        player.riichi_declaration_index,
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
                        player.riichi_declaration_index,
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
            // Stop at this Kyoku: a fourth accepted riichi otherwise resets in the next deal.
            match &mut clone {
                GameStateVariant::FourPlayer(state) => state.game_mode = 0,
                GameStateVariant::ThreePlayer(state) => state.game_mode = 3,
            }
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
    riichi_discard_index: Option<usize>,
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
        riichi_discard_index,
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
pub(crate) mod tests {
    use super::*;

    pub(crate) fn prepared_capture(mode: GameMode, ron: bool, kokushi: bool) -> EngineAdapter {
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

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_seeded_snapshot_evaluation_delta_and_observed_max_match_log() {
        use std::{hint::black_box, time::Instant};
        fn stats(mut ns: Vec<u128>) -> serde_json::Value {
            assert_eq!(ns.len(), 100);
            ns.sort_unstable();
            serde_json::json!({"samples":100,"median_ns":ns[50],"p90_ns":ns[90],"max_ns":ns[99]})
        }
        let mut timing = Vec::new();
        for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
            let (mut pre, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            let seat0 = Seat::new(0).unwrap();
            let discard = GameAction::Discard {
                tile: Tile::from_id(77).unwrap(),
                tsumogiri: true,
            };
            assert!(pre.legal_actions(seat0).unwrap().contains(&discard));
            pre.apply(seat0, &discard).unwrap();
            let actions = vec![
                (Seat::new(1).unwrap(), GameAction::Ron(seat0)),
                (Seat::new(2).unwrap(), GameAction::Ron(seat0)),
            ];
            let mut commands = HashMap::new();
            for (seat, action) in &actions {
                assert!(pre.legal_actions(*seat).unwrap().contains(action));
                let (raw, _) = pre.raw_legal_actions(*seat).unwrap();
                let ApplyCommand::One(command) = pre.find_command(*seat, action, &raw).unwrap()
                else {
                    panic!("actual Ron command");
                };
                commands.insert(seat.index(), command);
            }
            let roster = (0..mode.seat_count())
                .map(|i| {
                    Participant::new(format!("timing-{i}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let (mut clone_ns, mut baseline_ns, mut captured_ns) =
                (Vec::new(), Vec::new(), Vec::new());
            for run in 0..110 {
                let start = Instant::now();
                let snapshot = black_box(pre.state.clone());
                let elapsed = start.elapsed().as_nanos();
                black_box(snapshot);
                if run >= 10 {
                    clone_ns.push(elapsed);
                }
                let start = Instant::now();
                let mut baseline = black_box(pre.clone());
                step_variant(&mut baseline.state, &commands);
                assert!(last_error(&baseline.state).is_none());
                while baseline.needs_initialize_next_round() {
                    baseline.step_once_without_action().unwrap();
                }
                let baseline_events = baseline.drain_events(&actions).unwrap();
                let baseline_time = start.elapsed().as_nanos();
                let start = Instant::now();
                let mut measured = black_box(pre.clone());
                let before = measured.state.clone();
                step_variant(&mut measured.state, &commands);
                assert!(last_error(&measured.state).is_none());
                let outcome = measured
                    .finish_apply(before, std::slice::from_ref(&commands), &actions)
                    .unwrap();
                let capture = outcome.ended.as_ref().unwrap();
                let record = measured
                    .result_record(capture, &roster, 1, pre.log_cursor, &outcome.events)
                    .unwrap();
                let captured_time = start.elapsed().as_nanos();
                assert_eq!(record.wins.len(), 2);
                assert_eq!(baseline_events, outcome.events);
                assert_eq!(
                    capture.real_after_scores,
                    if mode.seat_count() == 3 {
                        vec![94400, 105000, 102600]
                    } else {
                        vec![94200, 105200, 102600, 100000]
                    }
                );
                black_box(record);
                if run >= 10 {
                    baseline_ns.push(baseline_time);
                    captured_ns.push(captured_time);
                }
            }
            let baseline = stats(baseline_ns);
            let captured = stats(captured_ns);
            let delta = captured["median_ns"].as_u64().unwrap() as i128
                - baseline["median_ns"].as_u64().unwrap() as i128;
            timing.push(serde_json::json!({"mode":mode,"seed":230,"warmups":10,"snapshot_clone":stats(clone_ns),"baseline_clone_raw_producer_drain":baseline,"authoritative_capture_evaluation":captured,"median_delta_ns":delta,"baseline_scope":"same seeded preterminal producer commands; no historical binary comparison"}));
        }
        let mut corpus = Vec::new();
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            for seed in [230, 231, 232] {
                let (mut adapter, mut events) = EngineAdapter::new(mode, Some(seed)).unwrap();
                let mut submitted = 0;
                for _ in 0..20000 {
                    if adapter.is_done() {
                        break;
                    }
                    let offered = (0..mode.seat_count())
                        .flat_map(|i| {
                            let seat = Seat::new(i as u8).unwrap();
                            adapter
                                .legal_actions(seat)
                                .unwrap()
                                .into_iter()
                                .map(move |a| (seat, a))
                        })
                        .collect::<Vec<_>>();
                    let responses = offered
                        .iter()
                        .filter(|(_, a)| matches!(a, GameAction::Ron(_) | GameAction::Pass))
                        .cloned()
                        .collect::<Vec<_>>();
                    let outcome = if responses.is_empty() {
                        let choice = offered
                            .iter()
                            .find(|(_, a)| *a == GameAction::Tsumo)
                            .or_else(|| {
                                offered.iter().find(|(_, a)| {
                                    matches!(
                                        a,
                                        GameAction::Discard {
                                            tsumogiri: true,
                                            ..
                                        }
                                    )
                                })
                            })
                            .unwrap();
                        submitted += 1;
                        adapter.apply(choice.0, &choice.1).unwrap()
                    } else {
                        let mut selected = Vec::new();
                        for seat in Seat::all(mode) {
                            if let Some(pair) = responses
                                .iter()
                                .find(|(s, a)| *s == seat && matches!(a, GameAction::Ron(_)))
                                .or_else(|| {
                                    responses
                                        .iter()
                                        .find(|(s, a)| *s == seat && *a == GameAction::Pass)
                                })
                            {
                                selected.push(pair.clone());
                            }
                        }
                        submitted += selected.len();
                        adapter.apply_simultaneous(&selected).unwrap()
                    };
                    events.extend(outcome.events);
                    assert!(submitted <= 20000);
                }
                assert!(adapter.is_done(), "bounded seeded match must end");
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, GameEvent::EndGame))
                        .count(),
                    1
                );
                let logs = state_logs(&adapter.state);
                let bytes = logs.iter().map(|s| s.len() + 1).sum::<usize>();
                assert!(bytes > 0);
                assert!(logs.iter().all(|s| !s.as_bytes().contains(&0)));
                corpus.push(serde_json::json!({"mode":mode,"seed":seed,"submitted":submitted,"canonical_log_bytes":bytes,"canonical_log_events":logs.len(),"kyokus":events.iter().filter(|e|matches!(e,GameEvent::StartKyoku{..})).count()}));
            }
        }
        let maximum = corpus
            .iter()
            .max_by_key(|c| c["canonical_log_bytes"].as_u64().unwrap())
            .unwrap();
        println!(
            "TASK8_TIMING_JSON:{}",
            serde_json::json!({"timing":timing,"maximum_observed_match_log":maximum,"corpus":corpus,"ceiling":"maximum observed in 12 fixed seeded natural matches, not an absolute match-size bound; debug native core timing, not renderer/GPU acceptance"})
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_all_last_dealer_win_has_literal_agari_yame_and_copy_in_four_modes() {
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            let count = mode.seat_count();
            let dealer = count - 1;
            let source_mode = if count == 4 {
                GameMode::FourPlayerRedEast
            } else {
                GameMode::ThreePlayerRedEast
            };
            let (source, _) = EngineAdapter::task8_double_ron(source_mode).unwrap();
            let (mut hands, tail) = match &source.state {
                GameStateVariant::FourPlayer(s) => (
                    s.players
                        .iter()
                        .map(|p| p.hand[..13].to_vec())
                        .collect::<Vec<_>>(),
                    s.wall.tiles.iter().rev().copied().collect::<Vec<_>>(),
                ),
                GameStateVariant::ThreePlayer(s) => (
                    s.players
                        .iter()
                        .map(|p| p.hand[..13].to_vec())
                        .collect::<Vec<_>>(),
                    s.wall.tiles.iter().rev().copied().collect::<Vec<_>>(),
                ),
            };
            hands.swap(1, dealer);
            let mut wall = Vec::new();
            for offset in [0, 4, 8] {
                for i in 0..count {
                    let h = &hands[(i + dealer) % count];
                    wall.extend_from_slice(&h[offset..offset + 4]);
                }
            }
            for i in 0..count {
                wall.push(hands[(i + dealer) % count][12]);
            }
            wall.push(77);
            wall.extend(tail);
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(
                inventory,
                (0..136u8)
                    .filter(|t| count == 4 || !(4..32).contains(t))
                    .collect::<Vec<_>>()
            );
            let mut state = GameStateVariant::new(
                engine_mode(mode),
                false,
                Some(230),
                0,
                GameRule::default_tenhou(),
            );
            let wind = if matches!(
                mode,
                GameMode::FourPlayerRedHalf | GameMode::ThreePlayerRedHalf
            ) {
                1
            } else {
                0
            };
            match &mut state {
                GameStateVariant::FourPlayer(s) => {
                    s.reset();
                    s._initialize_round(
                        dealer as u8,
                        wind,
                        2,
                        2,
                        Some(wall),
                        Some(vec![100000; count]),
                    );
                    for (i, h) in hands.iter().enumerate() {
                        assert_eq!(&s.players[i].hand[..13], h);
                    }
                    assert!(s.is_first_turn);
                }
                GameStateVariant::ThreePlayer(s) => {
                    s.reset();
                    s._initialize_round(
                        dealer as u8,
                        wind,
                        2,
                        2,
                        Some(wall),
                        Some(vec![100000; count]),
                    );
                    for (i, h) in hands.iter().enumerate() {
                        assert_eq!(&s.players[i].hand[..13], h);
                    }
                    assert!(s.is_first_turn);
                }
            }
            let mut adapter = EngineAdapter {
                mode,
                state,
                log_cursor: 0,
            };
            let mut events = adapter.drain_events(&[]).unwrap();
            let seat = Seat::new(dealer as u8).unwrap();
            assert!(
                adapter
                    .legal_actions(seat)
                    .unwrap()
                    .contains(&GameAction::Tsumo)
            );
            let outcome = adapter.apply(seat, &GameAction::Tsumo).unwrap();
            events.extend(outcome.events);
            let capture = outcome.ended.unwrap();
            assert!(adapter.is_done());
            let mut after = vec![83800; count];
            after[dealer] = if count == 4 { 150600 } else { 134400 };
            let mut delta = vec![-16200; count];
            delta[dealer] = if count == 4 { 50600 } else { 34400 };
            assert_eq!(capture.real_after_scores, after);
            assert_eq!(capture.real_after_pot, 0);
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::EndGame))
                    .count(),
                1
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|l| parse_event(l, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Hora { .. } | GameEvent::EndKyoku))
                    .cloned()
                    .collect::<Vec<_>>(),
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Hora { .. } | GameEvent::EndKyoku))
                    .cloned()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            let roster = (0..count)
                .map(|i| {
                    Participant::new(format!("yame-{i}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert_eq!(record.wins.len(), 1);
            let win = &record.wins[0];
            assert_eq!(win.seat, seat);
            assert_eq!(win.winning_tile.id(), 77);
            assert!(win.yaku.iter().any(|y| y.id == 35));
            assert_eq!(win.win_points, if count == 4 { 48000 } else { 32000 });
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                vec![100000; count]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                delta
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                after
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_final_exhaustive_draw_awards_tied_pot_once_and_preserves_copy_in_four_modes() {
        for mode in [
            GameMode::FourPlayerRedEast,
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            let count = mode.seat_count();
            let (mut adapter, mut events) = EngineAdapter::task8_final_draw(mode).unwrap();
            let mut terminal = None;
            let mut submitted = 0;
            for _ in 0..280 {
                let offered = (0..count)
                    .flat_map(|i| {
                        let seat = Seat::new(i as u8).unwrap();
                        adapter
                            .legal_actions(seat)
                            .unwrap()
                            .into_iter()
                            .map(move |a| (seat, a))
                    })
                    .collect::<Vec<_>>();
                let passes = offered
                    .iter()
                    .filter(|(_, a)| *a == GameAction::Pass)
                    .cloned()
                    .collect::<Vec<_>>();
                let outcome = if passes.is_empty() {
                    let (seat, action) = offered
                        .iter()
                        .find(|(_, a)| {
                            matches!(
                                a,
                                GameAction::Discard {
                                    tsumogiri: true,
                                    ..
                                }
                            )
                        })
                        .expect("offered final-draw discard");
                    submitted += 1;
                    adapter.apply(*seat, action).unwrap()
                } else {
                    submitted += passes.len();
                    adapter.apply_simultaneous(&passes).unwrap()
                };
                assert!(submitted <= 280);
                events.extend(outcome.events);
                if let Some(capture) = outcome.ended {
                    terminal = Some(capture);
                    break;
                }
            }
            let capture = terminal.expect("genuine final exhaustive draw");
            let mut after = vec![100000; count];
            after[0] = 102000;
            assert!(adapter.is_done());
            assert_eq!(capture.real_after_scores, after);
            assert_eq!(capture.real_after_pot, 0);
            assert!(
                matches!(events.iter().find(|e|matches!(e,GameEvent::Ryukyoku{..})),Some(GameEvent::Ryukyoku{reason:Some(reason),delta:Some(delta),..}) if reason=="exhaustive_draw" && delta==&vec![0;count])
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::EndKyoku))
                    .count(),
                1
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::EndGame { .. }))
                    .count(),
                1
            );
            assert!(!events.iter().any(|e| matches!(e, GameEvent::Hora { .. })));
            let (copy_scores, copy_pot, copy_done, hands) = match &capture.ended_copy {
                GameStateVariant::FourPlayer(s) => (
                    s.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                    s.riichi_sticks,
                    s.is_done,
                    s.players.iter().map(|p| p.hand.clone()).collect::<Vec<_>>(),
                ),
                GameStateVariant::ThreePlayer(s) => (
                    s.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                    s.riichi_sticks,
                    s.is_done,
                    s.players.iter().map(|p| p.hand.clone()).collect::<Vec<_>>(),
                ),
            };
            assert!(copy_done);
            assert_eq!(copy_scores, vec![100000; count]);
            assert_eq!(copy_pot, 2);
            for hand in hands {
                assert_eq!(hand.len(), 13);
                let honors = hand
                    .iter()
                    .filter(|t| **t / 4 >= 27)
                    .copied()
                    .collect::<Vec<_>>();
                assert_eq!(honors.len(), 2);
                assert_ne!(honors[0] / 4, honors[1] / 4);
                assert!(
                    hand.iter()
                        .any(|t| t / 4 < 27 && !matches!((t / 4) % 9, 0 | 8))
                );
            }
            // Two distinct single honors cannot both form groups/pair with one new tile; no orphan/7-pair shape.
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|l| parse_event(l, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku))
                    .cloned()
                    .collect::<Vec<_>>(),
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku))
                    .cloned()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            let roster = (0..count)
                .map(|i| {
                    Participant::new(
                        format!("final-draw-{i}"),
                        "same",
                        crate::ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert!(record.wins.is_empty());
            assert_eq!(
                record.settlement.draw_reason.as_deref(),
                Some("exhaustive_draw")
            );
            assert_eq!(record.draw_event_delta_adjustment, Some(vec![0; count]));
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| (c.before, c.delta, c.after))
                    .collect::<Vec<_>>(),
                (0..count)
                    .map(|i| (
                        100000,
                        if i == 0 { 2000 } else { 0 },
                        if i == 0 { 102000 } else { 100000 }
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_unclaimed_terminal_river_nagashi_has_literal_payments_and_copy_in_both_modes() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let count = mode.seat_count();
            let physical = (0..136u8)
                .filter(|t| count == 4 || !(4..32).contains(t))
                .collect::<Vec<_>>();
            let dealer_hand = vec![0, 32, 36, 68, 72, 104, 108, 112, 116, 120, 124, 128, 132];
            let is_terminal = |tile: u8| tile / 4 >= 27 || matches!((tile / 4) % 9, 0 | 8);
            let mut pool = physical
                .iter()
                .copied()
                .filter(|t| !dealer_hand.contains(t) && *t != 121)
                .collect::<Vec<_>>();
            let mut hands = vec![dealer_hand.clone()];
            for _ in 1..count {
                let hand = (0..13)
                    .map(|_| {
                        let i = pool.iter().position(|t| !is_terminal(*t)).unwrap();
                        pool.remove(i)
                    })
                    .collect::<Vec<_>>();
                hands.push(hand);
            }
            let mut wall = Vec::with_capacity(physical.len());
            for offset in [0, 4, 8] {
                for h in &hands {
                    wall.extend_from_slice(&h[offset..offset + 4]);
                }
            }
            for h in &hands {
                wall.push(h[12]);
            }
            for live in 0..physical.len() - 14 - count * 13 {
                let i = if live % count == 0 {
                    pool.iter().position(|t| is_terminal(*t)).unwrap()
                } else {
                    pool.iter().position(|t| !is_terminal(*t)).unwrap_or(0)
                };
                wall.push(pool.remove(i));
            }
            wall.extend(pool);
            wall.push(121);
            let marker_slot = if count == 4 { 131 } else { 99 };
            let last = wall.len() - 1;
            wall.swap(marker_slot, last);
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(inventory, physical);
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            match &mut adapter.state {
                GameStateVariant::FourPlayer(s) => {
                    s.reset();
                    s._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; count]));
                    for (i, h) in hands.iter().enumerate() {
                        assert_eq!(&s.players[i].hand[..13], h);
                    }
                    assert_eq!(s.wall.dora_indicators, vec![121]);
                }
                GameStateVariant::ThreePlayer(s) => {
                    s.reset();
                    s._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; count]));
                    for (i, h) in hands.iter().enumerate() {
                        assert_eq!(&s.players[i].hand[..13], h);
                    }
                    assert_eq!(s.wall.dora_indicators, vec![121]);
                }
            }
            adapter.log_cursor = 0;
            let mut events = adapter.drain_events(&[]).unwrap();
            let mut terminal = None;
            let mut submitted = 0;
            for _ in 0..280 {
                let offered = (0..count)
                    .flat_map(|i| {
                        let seat = Seat::new(i as u8).unwrap();
                        adapter
                            .legal_actions(seat)
                            .unwrap()
                            .into_iter()
                            .map(move |a| (seat, a))
                    })
                    .collect::<Vec<_>>();
                let passes = offered
                    .iter()
                    .filter(|(_, a)| *a == GameAction::Pass)
                    .cloned()
                    .collect::<Vec<_>>();
                let outcome = if passes.is_empty() {
                    let (seat, action) = offered
                        .iter()
                        .find(|(_, a)| {
                            matches!(
                                a,
                                GameAction::Discard {
                                    tsumogiri: true,
                                    ..
                                }
                            )
                        })
                        .expect("actual offered tsumogiri");
                    submitted += 1;
                    adapter.apply(*seat, action).unwrap()
                } else {
                    submitted += passes.len();
                    adapter.apply_simultaneous(&passes).unwrap()
                };
                assert!(submitted <= 280);
                events.extend(outcome.events);
                if let Some(capture) = outcome.ended {
                    terminal = Some(capture);
                    break;
                }
            }
            let capture = terminal.expect("natural wall exhaustion with unclaimed terminal river");
            let expected_delta = if count == 4 {
                vec![12000, -4000, -4000, -4000]
            } else {
                vec![8000, -4000, -4000]
            };
            let expected_after = if count == 4 {
                vec![112000, 96000, 96000, 96000]
            } else {
                vec![108000, 96000, 96000]
            };
            let terminals = events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(terminals.len(), 2);
            assert!(
                matches!(&terminals[0],GameEvent::Ryukyoku{reason:Some(r),delta:Some(v),..} if r=="nagashimangan"&&v==&expected_delta)
            );
            assert_eq!(terminals[1], GameEvent::EndKyoku);
            let (rivers, eligible, hand, done, drawable) = match &capture.ended_copy {
                GameStateVariant::FourPlayer(s) => (
                    s.players
                        .iter()
                        .map(|p| p.discards.clone())
                        .collect::<Vec<_>>(),
                    s.players
                        .iter()
                        .map(|p| p.nagashi_eligible)
                        .collect::<Vec<_>>(),
                    s.players[0].hand.clone(),
                    s.is_done,
                    s.wall.drawable_count,
                ),
                GameStateVariant::ThreePlayer(s) => (
                    s.players
                        .iter()
                        .map(|p| p.discards.clone())
                        .collect::<Vec<_>>(),
                    s.players
                        .iter()
                        .map(|p| p.nagashi_eligible)
                        .collect::<Vec<_>>(),
                    s.players[0].hand.clone(),
                    s.is_done,
                    s.wall.drawable_count,
                ),
            };
            assert!(done);
            assert_eq!(drawable, 0);
            assert_eq!(hand, dealer_hand);
            assert_eq!(eligible, (0..count).map(|i| i == 0).collect::<Vec<_>>());
            assert!(rivers[0].len() >= 18 && rivers[0].iter().all(|t| is_terminal(*t)));
            assert!(rivers[1..].iter().all(|river| !is_terminal(river[0])));
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|l| parse_event(l, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    ))
                    .cloned()
                    .collect::<Vec<_>>(),
                terminals
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            assert_eq!(capture.real_after_scores, expected_after);
            assert_eq!(capture.real_after_pot, 2);
            let next = match &adapter.state {
                GameStateVariant::FourPlayer(s) => {
                    (s.round_wind, s.oya, s.honba, s.riichi_sticks, s.is_done)
                }
                GameStateVariant::ThreePlayer(s) => {
                    (s.round_wind, s.oya, s.honba, s.riichi_sticks, s.is_done)
                }
            };
            assert_eq!(next, (0, 0, 3, 2, false));
            let roster = (0..count)
                .map(|i| {
                    Participant::new(
                        format!("nagashi-{i}"),
                        "same",
                        crate::ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert!(record.wins.is_empty());
            assert_eq!(
                record.settlement.draw_reason.as_deref(),
                Some("nagashimangan")
            );
            assert_eq!(record.draw_event_delta_adjustment, Some(vec![0; count]));
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 2)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                vec![100000; count]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                expected_delta
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                expected_after
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_four_closed_kans_by_two_players_have_literal_draw_and_copy_in_both_modes() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let count = mode.seat_count();
            let physical = (0..136u8)
                .filter(|t| count == 4 || !(4..32).contains(t))
                .collect::<Vec<_>>();
            let mut hands = vec![
                vec![0, 1, 2, 3, 36, 37, 38, 39],
                vec![72, 73, 74, 75, 124, 125, 126, 127],
            ];
            hands.resize(count, Vec::new());
            let mut reserved = hands.iter().flatten().copied().collect::<Vec<_>>();
            reserved.extend([84, 85, 96, 97, 98, 99, 120]);
            let mut remaining = physical.iter().copied().filter(|t| !reserved.contains(t));
            for hand in &mut hands {
                hand.extend(remaining.by_ref().take(13 - hand.len()));
                hand.sort_unstable();
            }
            let mut wall = Vec::with_capacity(physical.len());
            for offset in [0, 4, 8] {
                for hand in &hands {
                    wall.extend_from_slice(&hand[offset..offset + 4]);
                }
            }
            for hand in &hands {
                wall.push(hand[12]);
            }
            wall.extend([84, 85]);
            wall.extend(remaining);
            wall.extend([120, 99, 98, 97, 96]);
            if count == 3 {
                wall.swap(99, 103); // Sanma has eight rinshan slots; its first marker is at 99.
            }
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(inventory, physical);
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            match &mut adapter.state {
                GameStateVariant::FourPlayer(s) => {
                    s.reset();
                    s._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; count]));
                    for (i, h) in hands.iter().enumerate() {
                        assert_eq!(&s.players[i].hand[..13], h);
                    }
                    assert_eq!(s.wall.dora_indicators, vec![120]);
                }
                GameStateVariant::ThreePlayer(s) => {
                    s.reset();
                    s._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; count]));
                    for (i, h) in hands.iter().enumerate() {
                        assert_eq!(&s.players[i].hand[..13], h);
                    }
                    assert_eq!(s.wall.dora_indicators, vec![120]);
                }
            }
            assert_eq!(adapter.drawn_tile(), Some(84));
            adapter.log_cursor = 0;
            let mut events = adapter.drain_events(&[]).unwrap();
            let mut terminal = None;
            for (index, (player, quad)) in [
                (0, Some(0)),
                (0, Some(36)),
                (0, None),
                (1, Some(72)),
                (1, Some(124)),
                (1, None),
            ]
            .into_iter()
            .enumerate()
            {
                let seat = Seat::new(player).unwrap();
                let action = if let Some(first) = quad {
                    GameAction::Ankan {
                        consumed: (first..first + 4)
                            .map(|t| Tile::from_id(t).unwrap())
                            .collect(),
                    }
                } else {
                    GameAction::Discard {
                        tile: Tile::from_id(adapter.drawn_tile().unwrap()).unwrap(),
                        tsumogiri: true,
                    }
                };
                assert!(
                    adapter.legal_actions(seat).unwrap().contains(&action),
                    "{mode:?} script{index} {action:?}"
                );
                let outcome = adapter.apply(seat, &action).unwrap();
                events.extend(outcome.events);
                if let Some(capture) = outcome.ended {
                    terminal = Some(capture);
                    break;
                }
                let passes = (0..count)
                    .filter_map(|i| {
                        let responder = Seat::new(i as u8).unwrap();
                        adapter
                            .legal_actions(responder)
                            .unwrap()
                            .contains(&GameAction::Pass)
                            .then_some((responder, GameAction::Pass))
                    })
                    .collect::<Vec<_>>();
                if !passes.is_empty() {
                    let outcome = adapter.apply_simultaneous(&passes).unwrap();
                    events.extend(outcome.events);
                    if let Some(capture) = outcome.ended {
                        terminal = Some(capture);
                        break;
                    }
                }
            }
            let capture = terminal.expect("actual four-Kan abortive boundary");
            let terminals = events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(terminals.len(), 2);
            assert!(
                matches!(&terminals[0],GameEvent::Ryukyoku {reason:Some(r),delta:Some(v),..} if r=="suukansansen" && v==&vec![0;count])
            );
            assert_eq!(terminals[1], GameEvent::EndKyoku);
            let (melds, markers, done) = match &capture.ended_copy {
                GameStateVariant::FourPlayer(s) => (
                    s.players
                        .iter()
                        .map(|p| p.melds.clone())
                        .collect::<Vec<_>>(),
                    s.wall.dora_indicators.len(),
                    s.is_done,
                ),
                GameStateVariant::ThreePlayer(s) => (
                    s.players
                        .iter()
                        .map(|p| p.melds.clone())
                        .collect::<Vec<_>>(),
                    s.wall.dora_indicators.len(),
                    s.is_done,
                ),
            };
            assert!(done);
            assert_eq!(markers, 5);
            assert_eq!(
                melds.iter().map(Vec::len).collect::<Vec<_>>(),
                if count == 4 {
                    vec![2, 2, 0, 0]
                } else {
                    vec![2, 2, 0]
                }
            );
            for (seat, expected) in [
                (0, vec![vec![0, 1, 2, 3], vec![36, 37, 38, 39]]),
                (1, vec![vec![72, 73, 74, 75], vec![124, 125, 126, 127]]),
            ] {
                assert_eq!(
                    melds[seat]
                        .iter()
                        .map(|m| m.tiles.clone())
                        .collect::<Vec<_>>(),
                    expected
                );
                assert!(melds[seat].iter().all(|m| !m.opened && m.meld_type==riichienv_core::types::MeldType::Ankan));
            }
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|l| parse_event(l, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    ))
                    .cloned()
                    .collect::<Vec<_>>(),
                terminals
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            assert_eq!(capture.real_after_scores, vec![100000; count]);
            assert_eq!(capture.real_after_pot, 2);
            let next = match &adapter.state {
                GameStateVariant::FourPlayer(s) => {
                    (s.round_wind, s.oya, s.honba, s.riichi_sticks, s.is_done)
                }
                GameStateVariant::ThreePlayer(s) => {
                    (s.round_wind, s.oya, s.honba, s.riichi_sticks, s.is_done)
                }
            };
            assert_eq!(next, (0, 0, 3, 2, false));
            let roster = (0..count)
                .map(|i| {
                    Participant::new(
                        format!("four-kan-{i}"),
                        "same",
                        crate::ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert!(record.wins.is_empty());
            assert_eq!(
                record.settlement.draw_reason.as_deref(),
                Some("suukansansen")
            );
            assert_eq!(record.draw_event_delta_adjustment, Some(vec![0; count]));
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 2)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| (c.before, c.delta, c.after))
                    .collect::<Vec<_>>(),
                vec![(100000, 0, 100000); count]
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_four_wind_and_four_paid_riichi_preserve_literal_draw_and_copy() {
        let mode = GameMode::FourPlayerRedEast;
        for riichi in [false, true] {
            let hands = (0..4u8)
                .map(|seat| {
                    [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 88]
                        .map(|tile| tile + seat)
                        .to_vec()
                })
                .collect::<Vec<_>>();
            let draws = if riichi {
                [96, 97, 98, 99]
            } else {
                [108, 109, 110, 111]
            };
            let mut reserved = hands.iter().flatten().copied().collect::<Vec<_>>();
            reserved.extend(draws);
            reserved.push(120);
            let mut wall = Vec::with_capacity(136);
            for offset in [0, 4, 8] {
                for hand in &hands {
                    wall.extend_from_slice(&hand[offset..offset + 4]);
                }
            }
            for hand in &hands {
                wall.push(hand[12]);
            }
            wall.extend(draws);
            wall.extend((0..136u8).filter(|tile| !reserved.contains(tile)));
            wall.push(120);
            wall.swap(131, 135);
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
                unreachable!()
            };
            initial.reset();
            initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 4]));
            for (seat, hand) in hands.iter().enumerate() {
                assert_eq!(&initial.players[seat].hand[..13], hand);
                assert!(initial.players[seat].melds.is_empty());
                assert!(!initial.players[seat].riichi_declared);
            }
            assert_eq!(initial.drawn_tile, Some(draws[0]));
            assert_eq!(initial.wall.dora_indicators, vec![120]);
            adapter.log_cursor = 0;
            let mut events = adapter.drain_events(&[]).unwrap();
            let mut ended = None;
            for (index, tile) in draws.into_iter().enumerate() {
                let seat = Seat::new(index as u8).unwrap();
                let GameStateVariant::FourPlayer(current) = &adapter.state else {
                    unreachable!()
                };
                assert_eq!(current.current_player, index as u8);
                assert_eq!(current.drawn_tile, Some(tile));
                assert!(current.players.iter().all(|p| p.melds.is_empty()));
                let action = if riichi {
                    GameAction::RiichiDiscard {
                        tile: Tile::from_id(tile).unwrap(),
                    }
                } else {
                    GameAction::Discard {
                        tile: Tile::from_id(tile).unwrap(),
                        tsumogiri: true,
                    }
                };
                let offered = adapter.legal_actions(seat).unwrap();
                if riichi && index == 3 {
                    assert!(
                        !offered.contains(&GameAction::RiichiDiscard {
                            tile: Tile::from_id(3).unwrap()
                        }),
                        "non-tenpai discard remains rejected"
                    );
                }
                assert!(
                    offered.contains(&action),
                    "riichi={riichi} index={index} action={action:?}"
                );
                let outcome = adapter.apply(seat, &action).unwrap();
                events.extend(outcome.events);
                if index < 3 {
                    assert!(outcome.ended.is_none());
                    let GameStateVariant::FourPlayer(current) = &adapter.state else {
                        unreachable!()
                    };
                    assert_eq!(
                        current.riichi_sticks,
                        if riichi { 3 + index as u32 } else { 2 }
                    );
                    for (i, player) in current.players.iter().enumerate() {
                        assert_eq!(
                            player.score,
                            if riichi && i <= index { 99000 } else { 100000 }
                        );
                    }
                } else {
                    ended = outcome.ended;
                }
            }
            let capture = ended.expect("four genuine offered discards end the actual hand");
            let reason = if riichi {
                "suucha_riichi"
            } else {
                "sufuurenta"
            };
            let terminal = events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(terminal.len(), 2);
            assert!(
                matches!(&terminal[0], GameEvent::Ryukyoku { reason: Some(r), delta: Some(delta), .. } if r == reason && delta == &vec![0; 4])
            );
            assert_eq!(terminal[1], GameEvent::EndKyoku);
            for index in 0..4u8 {
                assert_eq!(events.iter().filter(|e| matches!(e, GameEvent::ReachAccepted { actor } if actor.index() == index)).count(), usize::from(riichi));
            }
            let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
                unreachable!()
            };
            assert!(copy.is_done);
            assert!(copy.players.iter().all(|p| p.melds.is_empty()));
            for (index, p) in copy.players.iter().enumerate() {
                assert_eq!(p.hand, hands[index]);
                assert_eq!(p.discards, vec![draws[index]]);
                assert_eq!(p.riichi_declared, riichi);
            }
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    ))
                    .cloned()
                    .collect::<Vec<_>>(),
                terminal
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            assert_eq!(
                capture.real_after_scores,
                vec![if riichi { 99000 } else { 100000 }; 4]
            );
            assert_eq!(capture.real_after_pot, if riichi { 6 } else { 2 });
            let GameStateVariant::FourPlayer(after) = &adapter.state else {
                unreachable!()
            };
            assert!(!after.is_done);
            assert_eq!(
                (
                    after.round_wind,
                    after.oya,
                    after.honba,
                    after.riichi_sticks
                ),
                (0, 0, 3, if riichi { 6 } else { 2 })
            );
            let roster = (0..4)
                .map(|i| {
                    Participant::new(
                        format!("four-draw-{i}"),
                        "same",
                        crate::ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert!(record.wins.is_empty());
            assert_eq!(record.settlement.draw_reason.as_deref(), Some(reason));
            assert_eq!(
                record.draw_event_delta_adjustment,
                Some(if riichi {
                    vec![0, 0, 0, 1000]
                } else {
                    vec![0; 4]
                })
            );
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                if riichi { (5, 6) } else { (2, 2) }
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                if riichi {
                    vec![99000, 99000, 99000, 100000]
                } else {
                    vec![100000; 4]
                }
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                if riichi {
                    vec![0, 0, 0, -1000]
                } else {
                    vec![0; 4]
                }
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                vec![if riichi { 99000 } else { 100000 }; 4]
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_kyushu_and_sanchaho_keep_literal_pot_scores_and_copy_boundaries() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        for sanchaho in [false, true] {
            let mut hands = if sanchaho {
                vec![
                    Vec::new(),
                    vec![36, 40, 44, 48, 52, 56, 60, 64, 68, 108, 109, 110, 76],
                    vec![37, 41, 45, 49, 53, 57, 61, 65, 69, 112, 113, 114, 78],
                    vec![38, 42, 46, 50, 54, 58, 62, 66, 70, 116, 117, 118, 79],
                ]
            } else {
                vec![
                    vec![0, 32, 36, 68, 72, 104, 108, 112, 116, 120, 124, 128, 132],
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ]
            };
            let drawn = if sanchaho { 77 } else { 52 };
            let marker = if sanchaho { 120 } else { 121 };
            let mut reserved = hands.iter().flatten().copied().collect::<Vec<_>>();
            reserved.extend([drawn, marker]);
            let mut remaining = (0..136u8).filter(|t| !reserved.contains(t));
            for hand in &mut hands {
                if hand.is_empty() {
                    hand.extend(remaining.by_ref().take(13));
                }
            }
            let mut wall = Vec::with_capacity(136);
            for offset in [0, 4, 8] {
                for hand in &hands {
                    wall.extend_from_slice(&hand[offset..offset + 4]);
                }
            }
            for hand in &hands {
                wall.push(hand[12]);
            }
            // Complete deal before offers: no terminal/log/claim/first-turn flags are set.
            wall.push(drawn);
            wall.extend(remaining);
            wall.push(marker);
            wall.swap(131, 135);
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
                unreachable!()
            };
            initial.reset();
            initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 4]));
            for (i, hand) in hands.iter().enumerate() {
                let mut sorted = hand.clone();
                sorted.sort_unstable();
                if i == 0 {
                    assert_eq!(initial.players[i].hand[..13], sorted);
                } else {
                    assert_eq!(initial.players[i].hand, sorted);
                }
            }
            assert_eq!(initial.drawn_tile, Some(drawn));
            assert!(initial.rule.sanchaho_is_draw && initial.is_first_turn);
            adapter.log_cursor = 0;
            let mut events = adapter.drain_events(&[]).unwrap();
            let outcome = if sanchaho {
                let discard = GameAction::Discard {
                    tile: Tile::from_id(77).unwrap(),
                    tsumogiri: true,
                };
                assert!(adapter.legal_actions(dealer).unwrap().contains(&discard));
                let first = adapter.apply(dealer, &discard).unwrap();
                assert!(first.ended.is_none());
                events.extend(first.events);
                let responses = (1..4)
                    .map(|i| {
                        let seat = Seat::new(i).unwrap();
                        let ron = GameAction::Ron(dealer);
                        assert!(adapter.legal_actions(seat).unwrap().contains(&ron));
                        (seat, ron)
                    })
                    .collect::<Vec<_>>();
                adapter.apply_simultaneous(&responses).unwrap()
            } else {
                assert!(
                    adapter
                        .legal_actions(dealer)
                        .unwrap()
                        .contains(&GameAction::AbortiveDraw)
                );
                adapter.apply(dealer, &GameAction::AbortiveDraw).unwrap()
            };
            let reason = if sanchaho {
                "sanchaho"
            } else {
                "kyushu_kyuhai"
            };
            let terminal_events = outcome
                .events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(terminal_events.len(), 2);
            assert!(
                matches!(&terminal_events[0], GameEvent::Ryukyoku { reason: Some(r), delta: Some(delta), .. } if r == reason && delta == &vec![0; 4])
            );
            assert_eq!(terminal_events[1], GameEvent::EndKyoku);
            events.extend(outcome.events);
            let capture = outcome.ended.expect("actual offered abortive terminal");
            let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
                unreachable!()
            };
            assert!(copy.is_done);
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::Ryukyoku { .. } | GameEvent::EndKyoku
                    ))
                    .cloned()
                    .collect::<Vec<_>>(),
                terminal_events
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            assert_eq!(capture.real_after_scores, vec![100000; 4]);
            assert_eq!(capture.real_after_pot, 2);
            let GameStateVariant::FourPlayer(after) = &adapter.state else {
                unreachable!()
            };
            assert!(!after.is_done);
            assert_eq!(
                (
                    after.round_wind,
                    after.oya,
                    after.honba,
                    after.riichi_sticks
                ),
                (0, 0, 3, 2)
            );
            let roster = (0..4)
                .map(|i| {
                    Participant::new(
                        format!("abortive-{i}"),
                        "same",
                        crate::ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert!(record.wins.is_empty());
            assert_eq!(record.settlement.draw_reason.as_deref(), Some(reason));
            assert_eq!(record.draw_event_delta_adjustment, Some(vec![0; 4]));
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 2)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                vec![100000; 4]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                vec![0; 4]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                vec![100000; 4]
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_open_kan_reveals_second_dora_before_discard_and_later_literal_win() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let feeder = Seat::new(1).unwrap();
        let dealer_hand = vec![72, 73, 74, 36, 40, 44, 48, 53, 56, 60, 64, 68, 76];
        let mut reserved = dealer_hand.clone();
        reserved.extend([75, 24, 26, 27, 28, 29, 79, 84, 120, 71]);
        let mut remaining = (0..136u8).filter(|t| !reserved.contains(t));
        let mut hands = vec![dealer_hand, vec![75]];
        hands[1].extend(remaining.by_ref().take(12));
        hands.push(remaining.by_ref().take(13).collect());
        hands.push(remaining.by_ref().take(13).collect());
        let mut wall = Vec::with_capacity(136);
        for offset in [0, 4, 8] {
            for hand in &hands {
                wall.extend_from_slice(&hand[offset..offset + 4]);
            }
        }
        for hand in &hands {
            wall.push(hand[12]);
        }
        // Complete finite wall before any offer; actions generate Kan and delayed Dora.
        wall.extend([24, 26, 27, 28, 29, 79]);
        wall.extend(remaining);
        wall.extend([71, 120, 84]);
        wall.swap(129, 133);
        wall.swap(131, 134);
        let mut inventory = wall.clone();
        inventory.sort_unstable();
        assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
        let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
        let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
            unreachable!()
        };
        initial.reset();
        initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 4]));
        for (i, hand) in hands.iter().enumerate() {
            let mut sorted = hand.clone();
            sorted.sort_unstable();
            if i == 0 {
                assert_eq!(initial.players[i].hand[..13], sorted);
            } else {
                assert_eq!(initial.players[i].hand, sorted);
            }
        }
        assert_eq!(
            (initial.drawn_tile, initial.wall.dora_indicators.clone()),
            (Some(24), vec![120])
        );
        adapter.log_cursor = 0;
        let mut events = adapter.drain_events(&[]).unwrap();
        let script = [
            (
                dealer,
                GameAction::Discard {
                    tile: Tile::from_id(24).unwrap(),
                    tsumogiri: true,
                },
            ),
            (
                feeder,
                GameAction::Discard {
                    tile: Tile::from_id(75).unwrap(),
                    tsumogiri: false,
                },
            ),
            (
                dealer,
                GameAction::Daiminkan {
                    target: feeder,
                    called: Tile::from_id(75).unwrap(),
                    consumed: vec![
                        Tile::from_id(72).unwrap(),
                        Tile::from_id(73).unwrap(),
                        Tile::from_id(74).unwrap(),
                    ],
                },
            ),
            (
                dealer,
                GameAction::Discard {
                    tile: Tile::from_id(84).unwrap(),
                    tsumogiri: true,
                },
            ),
            (dealer, GameAction::Tsumo),
        ];
        let mut stage = 0;
        let mut terminal = None;
        for _ in 0..32 {
            let offered = (0..4)
                .flat_map(|i| {
                    let seat = Seat::new(i).unwrap();
                    adapter
                        .legal_actions(seat)
                        .unwrap()
                        .into_iter()
                        .map(move |a| (seat, a))
                })
                .collect::<Vec<_>>();
            let wanted = script.get(stage).expect("finite offered open Kan script");
            let choice = offered
                .iter()
                .find(|a| *a == wanted)
                .or_else(|| offered.iter().find(|(_, a)| *a == GameAction::Pass))
                .or_else(|| {
                    offered.iter().find(|(_, a)| {
                        matches!(
                            a,
                            GameAction::Discard {
                                tsumogiri: true,
                                ..
                            }
                        )
                    })
                })
                .cloned()
                .expect("actual offered open Kan continuation");
            if choice == script[3] {
                let GameStateVariant::FourPlayer(state) = &adapter.state else {
                    unreachable!()
                };
                assert!(state.is_rinshan_flag);
                assert_eq!(state.drawn_tile, Some(84));
                assert_eq!(state.wall.dora_indicators, vec![120]);
                assert_eq!(state.wall.pending_kan_dora_count, 1);
            }
            if choice == *wanted {
                stage += 1;
            }
            let outcome = adapter.apply(choice.0, &choice.1).unwrap();
            if choice == script[3] {
                assert!(
                    matches!(outcome.events.first(), Some(GameEvent::Dora { dora_marker }) if dora_marker.id() == 68)
                );
                assert!(
                    matches!(outcome.events.get(1), Some(GameEvent::Dahai { actor, tile, .. }) if *actor == dealer && tile.id() == 84)
                );
            }
            events.extend(outcome.events);
            if let Some(capture) = outcome.ended {
                terminal = Some(capture);
                break;
            }
        }
        assert_eq!(stage, 5);
        let capture = terminal.expect("actual later win after delayed open Kan Dora");
        let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
            unreachable!()
        };
        assert!(!before.is_first_turn && !before.is_rinshan_flag);
        assert_eq!(
            (
                before.drawn_tile,
                before.wall.rinshan_draw_count,
                before.wall.pending_kan_dora_count
            ),
            (Some(79), 1, 0)
        );
        assert_eq!(before.wall.dora_indicators, vec![120, 71]);
        let horas = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(horas.len(), 1);
        let GameEvent::Hora {
            actor,
            target,
            delta,
            ..
        } = &horas[0]
        else {
            unreachable!()
        };
        assert_eq!(
            (*actor, *target, delta.as_ref().unwrap()),
            (dealer, dealer, &vec![6500, -1500, -1500, -1500])
        );
        let copied = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| parse_event(line, mode, &[]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            horas
        );
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
        let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(capture.real_after_scores, vec![106500, 98500, 98500, 98500]);
        assert_eq!(capture.real_after_pot, 0);
        let roster = (0..4)
            .map(|i| Participant::new(format!("open-{i}"), "same", crate::ParticipantKind::Human))
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &events)
            .unwrap();
        assert_eq!(record.wins.len(), 1);
        let win = &record.wins[0];
        assert_eq!((win.seat, win.target, win.tsumo), (dealer, dealer, true));
        assert_eq!(
            (
                win.concealed.len(),
                win.winning_tile.id(),
                win.winning_tile_index
            ),
            (11, 79, 10)
        );
        assert_eq!(win.win_points, 3900);
        assert_eq!(
            win.evaluation,
            WinEvaluation::Ordinary {
                han: 2,
                fu: 40,
                limit: None
            }
        );
        let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
        yaku.sort_unstable();
        assert_eq!(yaku, vec![(16, Some(1)), (31, Some(1))]);
        assert_eq!(win.melds.len(), 1);
        let meld = &win.melds[0];
        assert_eq!(meld.kind, crate::results::ResultMeldKind::Daiminkan);
        assert!(meld.opened);
        assert_eq!(meld.from_who, Some(feeder));
        assert_eq!(meld.called_tile.unwrap().id(), 75);
        assert_eq!(
            meld.tiles.iter().map(|t| t.id()).collect::<Vec<_>>(),
            vec![72, 73, 74, 75]
        );
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (2, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.before)
                .collect::<Vec<_>>(),
            vec![100000; 4]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.delta)
                .collect::<Vec<_>>(),
            vec![6500, -1500, -1500, -1500]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>(),
            vec![106500, 98500, 98500, 98500]
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_red_counted_yakuman_preserves_paid_riichi_and_ordinary_classification() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let dealer_hand = vec![36, 40, 44, 48, 52, 56, 60, 64, 68, 53, 54, 55, 57];
        let mut reserved = dealer_hand.clone();
        reserved.extend([24, 26, 27, 28, 58, 120, 122]);
        let mut remaining = (0..136u8).filter(|t| !reserved.contains(t));
        let hands = [
            dealer_hand,
            remaining.by_ref().take(13).collect(),
            remaining.by_ref().take(13).collect(),
            remaining.by_ref().take(13).collect(),
        ];
        let mut wall = Vec::with_capacity(136);
        for offset in [0, 4, 8] {
            for hand in &hands {
                wall.extend_from_slice(&hand[offset..offset + 4]);
            }
        }
        for hand in &hands {
            wall.push(hand[12]);
        }
        // Complete physical wall before offers; actual paid Riichi unlocks Ippatsu/ura.
        wall.extend([24, 26, 27, 28, 58]);
        wall.extend(remaining);
        wall.extend([120, 122]);
        wall.swap(131, 134);
        wall.swap(130, 135);
        let mut inventory = wall.clone();
        inventory.sort_unstable();
        assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
        let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
        let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
            unreachable!()
        };
        initial.reset();
        initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 4]));
        for (i, hand) in hands.iter().enumerate() {
            let mut sorted = hand.clone();
            sorted.sort_unstable();
            if i == 0 {
                assert_eq!(initial.players[i].hand[..13], sorted);
            } else {
                assert_eq!(initial.players[i].hand, sorted);
            }
        }
        assert_eq!(
            (
                initial.drawn_tile,
                initial.wall.dora_indicators.clone(),
                initial.wall.tiles[5]
            ),
            (Some(24), vec![120], 122)
        );
        adapter.log_cursor = 0;
        let mut events = adapter.drain_events(&[]).unwrap();
        let riichi = GameAction::RiichiDiscard {
            tile: Tile::from_id(24).unwrap(),
        };
        assert!(adapter.legal_actions(dealer).unwrap().contains(&riichi));
        let outcome = adapter.apply(dealer, &riichi).unwrap();
        assert!(outcome.ended.is_none());
        events.extend(outcome.events);
        let mut terminal = None;
        for _ in 0..24 {
            let offered = (0..4)
                .flat_map(|i| {
                    let seat = Seat::new(i).unwrap();
                    adapter
                        .legal_actions(seat)
                        .unwrap()
                        .into_iter()
                        .map(move |a| (seat, a))
                })
                .collect::<Vec<_>>();
            let choice = offered
                .iter()
                .find(|(seat, a)| *seat == dealer && *a == GameAction::Tsumo)
                .or_else(|| offered.iter().find(|(_, a)| *a == GameAction::Pass))
                .or_else(|| {
                    offered.iter().find(|(_, a)| {
                        matches!(
                            a,
                            GameAction::Discard {
                                tsumogiri: true,
                                ..
                            }
                        )
                    })
                })
                .cloned()
                .expect("actual offered next-cycle counted win");
            let outcome = adapter.apply(choice.0, &choice.1).unwrap();
            events.extend(outcome.events);
            if let Some(capture) = outcome.ended {
                terminal = Some(capture);
                break;
            }
        }
        let capture = terminal.expect("actual offered counted Tsumo");
        let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
            unreachable!()
        };
        assert!(
            before.players[0].riichi_declared
                && before.players[0].double_riichi_declared
                && before.players[0].ippatsu_cycle
        );
        assert!(!before.is_first_turn && !before.is_rinshan_flag);
        assert_eq!(
            (
                before.drawn_tile,
                before.riichi_sticks,
                before.players[0].score
            ),
            (Some(58), 3, 99000)
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, GameEvent::ReachAccepted { actor } if *actor == dealer))
                .count(),
            1
        );
        let horas = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(horas.len(), 1);
        let GameEvent::Hora {
            actor,
            target,
            delta,
            ..
        } = &horas[0]
        else {
            unreachable!()
        };
        assert_eq!(
            (*actor, *target, delta.as_ref().unwrap()),
            (dealer, dealer, &vec![51600, -16200, -16200, -16200])
        );
        let copied = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| parse_event(line, mode, &[]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            horas
        );
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
        let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(capture.real_after_scores, vec![150600, 83800, 83800, 83800]);
        assert_eq!(capture.real_after_pot, 0);
        let roster = (0..4)
            .map(|i| {
                Participant::new(
                    format!("counted-{i}"),
                    "same",
                    crate::ParticipantKind::Human,
                )
            })
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &events)
            .unwrap();
        assert_eq!(record.wins.len(), 1);
        let win = &record.wins[0];
        assert_eq!((win.seat, win.target, win.tsumo), (dealer, dealer, true));
        assert_eq!(
            (
                win.concealed.len(),
                win.winning_tile.id(),
                win.winning_tile_index
            ),
            (14, 58, 13)
        );
        assert_eq!(win.concealed.iter().filter(|t| t.id() == 52).count(), 1);
        assert_eq!(win.win_points, 48000);
        assert_eq!(
            win.evaluation,
            WinEvaluation::Ordinary {
                han: 13,
                fu: 30,
                limit: Some(ScoreLimit::CountedYakuman)
            }
        );
        let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
        yaku.sort_unstable();
        assert_eq!(
            yaku,
            vec![
                (1, Some(1)),
                (16, Some(2)),
                (18, Some(2)),
                (29, Some(6)),
                (30, Some(1)),
                (32, Some(1))
            ]
        );
        assert!(win.yaku.iter().all(|y| y.yakuman_multiplier.is_none()));
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (3, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.before)
                .collect::<Vec<_>>(),
            vec![99000, 100000, 100000, 100000]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.delta)
                .collect::<Vec<_>>(),
            vec![51600, -16200, -16200, -16200]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>(),
            vec![150600, 83800, 83800, 83800]
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_robbed_north_has_literal_payment_without_chankan_yaku() {
        let mode = GameMode::ThreePlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let robber = Seat::new(1).unwrap();
        let dealer_hand = vec![36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76];
        let robber_hand = vec![37, 41, 45, 49, 54, 57, 61, 65, 69, 80, 81, 82, 122];
        let mut reserved = dealer_hand.clone();
        reserved.extend(&robber_hand);
        reserved.extend([120, 121]);
        let mut remaining = (0..136u8).filter(|t| !(4..32).contains(t) && !reserved.contains(t));
        let hands = [
            dealer_hand,
            robber_hand,
            remaining.by_ref().take(13).collect(),
        ];
        let mut wall = Vec::with_capacity(108);
        for offset in [0, 4, 8] {
            for hand in &hands {
                wall.extend_from_slice(&hand[offset..offset + 4]);
            }
        }
        for hand in &hands {
            wall.push(hand[12]);
        }
        // Fixed complete deal before offers; only the actual Nuki creates a pending Ron.
        wall.push(121);
        wall.extend(remaining);
        wall.push(120);
        wall.swap(99, 107);
        let mut inventory = wall.clone();
        inventory.sort_unstable();
        assert_eq!(
            inventory,
            (0..136u8)
                .filter(|t| !(4..32).contains(t))
                .collect::<Vec<_>>()
        );
        let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
        let GameStateVariant::ThreePlayer(initial) = &mut adapter.state else {
            unreachable!()
        };
        initial.reset();
        initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 3]));
        for (i, hand) in hands.iter().enumerate() {
            let mut sorted = hand.clone();
            sorted.sort_unstable();
            if i == 0 {
                assert_eq!(initial.players[i].hand[..13], sorted);
            } else {
                assert_eq!(initial.players[i].hand, sorted);
            }
        }
        assert_eq!(
            (initial.drawn_tile, initial.wall.dora_indicators.clone()),
            (Some(121), vec![120])
        );
        adapter.log_cursor = 0;
        let mut events = adapter.drain_events(&[]).unwrap();
        let nuki = GameAction::Nuki {
            tile: Tile::from_id(121).unwrap(),
        };
        assert!(adapter.legal_actions(dealer).unwrap().contains(&nuki));
        let outcome = adapter.apply(dealer, &nuki).unwrap();
        assert!(outcome.ended.is_none());
        events.extend(outcome.events);
        let ron = GameAction::Ron(dealer);
        assert!(adapter.legal_actions(robber).unwrap().contains(&ron));
        let outcome = adapter.apply(robber, &ron).unwrap();
        events.extend(outcome.events);
        let capture = outcome
            .ended
            .expect("actual offered Ron on extracted North");
        let GameStateVariant::ThreePlayer(before) = &capture.pre_action else {
            unreachable!()
        };
        assert_eq!(before.players[0].kita_tiles, vec![121]);
        assert!(!before.players[0].hand.contains(&121));
        assert!(!before.is_first_turn && !before.is_rinshan_flag);
        assert_eq!(before.last_discard, Some((0, 121)));
        assert_eq!(
            before.pending_kan.as_ref().unwrap().1.action_type,
            ActionType::Kita
        );
        assert_eq!(before.wall.rinshan_draw_count, 0);
        assert_eq!(before.wall.dora_indicators, vec![120]);
        let horas = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(horas.len(), 1);
        let GameEvent::Hora {
            actor,
            target,
            delta,
            ..
        } = &horas[0]
        else {
            unreachable!()
        };
        assert_eq!(
            (*actor, *target, delta.as_ref().unwrap()),
            (robber, dealer, &vec![-3000, 5000, 0])
        );
        let copied = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| parse_event(line, mode, &[]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            horas
        );
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
        let GameStateVariant::ThreePlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(capture.real_after_scores, vec![97000, 105000, 100000]);
        assert_eq!(capture.real_after_pot, 0);
        let roster = (0..3)
            .map(|i| {
                Participant::new(
                    format!("north-ron-{i}"),
                    "same",
                    crate::ParticipantKind::Human,
                )
            })
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &events)
            .unwrap();
        assert_eq!(record.wins.len(), 1);
        let win = &record.wins[0];
        assert_eq!((win.seat, win.target, win.tsumo), (robber, dealer, false));
        assert_eq!(
            (
                win.concealed.len(),
                win.winning_tile.id(),
                win.winning_tile_index
            ),
            (14, 121, 13)
        );
        assert!(win.melds.is_empty());
        assert_eq!(win.win_points, 2600);
        assert_eq!(
            win.evaluation,
            WinEvaluation::Ordinary {
                han: 2,
                fu: 40,
                limit: None
            }
        );
        assert_eq!(
            win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>(),
            vec![(16, Some(2))]
        );
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (2, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.before)
                .collect::<Vec<_>>(),
            vec![100000; 3]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.delta)
                .collect::<Vec<_>>(),
            vec![-3000, 5000, 0]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>(),
            vec![97000, 105000, 100000]
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_added_kan_can_be_robbed_or_win_on_its_replacement_with_literal_payments() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let feeder = Seat::new(1).unwrap();
        let robber = Seat::new(2).unwrap();
        for robbed in [true, false] {
            let dealer_hand = vec![72, 73, 74, 36, 40, 44, 48, 53, 56, 60, 64, 68, 76];
            let robber_hand = vec![37, 41, 45, 49, 54, 57, 61, 65, 69, 77, 80, 112, 113];
            let mut reserved = dealer_hand.clone();
            reserved.extend(&robber_hand);
            reserved.extend([75, 24, 26, 27, 28, 29, 79, 78, 120]);
            let mut remaining = (0..136u8).filter(|t| !reserved.contains(t));
            let mut hands = vec![dealer_hand, vec![75], robber_hand];
            hands[1].extend(remaining.by_ref().take(12));
            hands.push(remaining.by_ref().take(13).collect());
            let mut wall = Vec::with_capacity(136);
            for offset in [0, 4, 8] {
                for hand in &hands {
                    wall.extend_from_slice(&hand[offset..offset + 4]);
                }
            }
            for hand in &hands {
                wall.push(hand[12]);
            }
            // Every tile is allocated before offers; no Kan/Chankan context is injected.
            wall.extend([24, 26, 27, 28, 29, 79]);
            wall.extend(remaining);
            wall.extend([120, 78]);
            wall.swap(131, 134);
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
                unreachable!()
            };
            initial.reset();
            initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 4]));
            for (i, hand) in hands.iter_mut().enumerate() {
                hand.sort_unstable();
                if i == 0 {
                    assert_eq!(initial.players[i].hand[..13], hand[..]);
                } else {
                    assert_eq!(initial.players[i].hand, *hand);
                }
            }
            assert_eq!(
                (initial.drawn_tile, initial.wall.dora_indicators.clone()),
                (Some(24), vec![120])
            );
            adapter.log_cursor = 0;
            let mut events = adapter.drain_events(&[]).unwrap();
            let script = [
                (
                    dealer,
                    GameAction::Discard {
                        tile: Tile::from_id(24).unwrap(),
                        tsumogiri: true,
                    },
                ),
                (
                    feeder,
                    GameAction::Discard {
                        tile: Tile::from_id(75).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    dealer,
                    GameAction::Pon {
                        target: feeder,
                        called: Tile::from_id(75).unwrap(),
                        consumed: vec![Tile::from_id(72).unwrap(), Tile::from_id(73).unwrap()],
                    },
                ),
                (
                    dealer,
                    GameAction::Discard {
                        tile: Tile::from_id(76).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    dealer,
                    GameAction::Kakan {
                        called: Tile::from_id(74).unwrap(),
                        consumed: vec![
                            Tile::from_id(72).unwrap(),
                            Tile::from_id(73).unwrap(),
                            Tile::from_id(75).unwrap(),
                        ],
                    },
                ),
                (
                    robber,
                    if robbed {
                        GameAction::Ron(dealer)
                    } else {
                        GameAction::Pass
                    },
                ),
                (dealer, GameAction::Tsumo),
            ];
            let mut stage = 0;
            let mut terminal = None;
            for _ in 0..48 {
                let offered = (0..4)
                    .flat_map(|i| {
                        let seat = Seat::new(i).unwrap();
                        adapter
                            .legal_actions(seat)
                            .unwrap()
                            .into_iter()
                            .map(move |a| (seat, a))
                    })
                    .collect::<Vec<_>>();
                let wanted = script.get(stage).expect("finite offered Kan script");
                let choice = offered
                    .iter()
                    .find(|a| *a == wanted)
                    .or_else(|| offered.iter().find(|(_, a)| *a == GameAction::Pass))
                    .or_else(|| {
                        offered.iter().find(|(_, a)| {
                            matches!(
                                a,
                                GameAction::Discard {
                                    tsumogiri: true,
                                    ..
                                }
                            )
                        })
                    })
                    .cloned()
                    .expect("actual offered Kan continuation");
                if choice == *wanted {
                    stage += 1;
                }
                let outcome = adapter.apply(choice.0, &choice.1).unwrap();
                events.extend(outcome.events);
                if let Some(capture) = outcome.ended {
                    terminal = Some(capture);
                    break;
                }
            }
            assert_eq!(stage, if robbed { 6 } else { 7 });
            let capture = terminal.expect("actual Kan terminal");
            let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
                unreachable!()
            };
            assert!(!before.is_first_turn);
            assert_eq!(before.pending_kan.is_some(), robbed);
            assert_eq!(before.is_rinshan_flag, !robbed);
            assert_eq!(before.wall.rinshan_draw_count, if robbed { 0 } else { 1 });
            assert_eq!(before.wall.dora_indicators, vec![120]);
            assert_eq!(
                before.wall.pending_kan_dora_count,
                if robbed { 0 } else { 1 }
            );
            assert!(
                before.players[2]
                    .discards
                    .iter()
                    .all(|t| ![18, 21].contains(&(t / 4)))
            );
            assert_eq!(before.players[2].missed_agari_doujun, !robbed);
            assert_eq!(before.players[0].melds[0].tiles, vec![72, 73, 74, 75]);
            let winner = if robbed { robber } else { dealer };
            let expected_delta = if robbed {
                vec![-8300, 0, 10300, 0]
            } else {
                vec![6500, -1500, -1500, -1500]
            };
            let expected_after = if robbed {
                vec![91700, 100000, 110300, 100000]
            } else {
                vec![106500, 98500, 98500, 98500]
            };
            let horas = events
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(horas.len(), 1);
            let GameEvent::Hora {
                actor,
                target,
                delta,
                ..
            } = &horas[0]
            else {
                unreachable!()
            };
            assert_eq!(
                (*actor, *target, delta.as_ref().unwrap()),
                (winner, dealer, &expected_delta)
            );
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &[script[4].clone()]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Hora { .. }))
                    .cloned()
                    .collect::<Vec<_>>(),
                horas
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
                unreachable!()
            };
            assert!(copy.is_done);
            assert_eq!(capture.real_after_scores, expected_after);
            assert_eq!(capture.real_after_pot, 0);
            let roster = (0..4)
                .map(|i| {
                    Participant::new(format!("added-{i}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert_eq!(record.wins.len(), 1);
            let win = &record.wins[0];
            assert_eq!((win.seat, win.target, win.tsumo), (winner, dealer, !robbed));
            assert_eq!(
                (
                    win.concealed.len(),
                    win.winning_tile.id(),
                    win.winning_tile_index
                ),
                if robbed { (14, 74, 13) } else { (11, 78, 10) }
            );
            assert_eq!(win.win_points, if robbed { 7700 } else { 3900 });
            assert_eq!(
                win.evaluation,
                WinEvaluation::Ordinary {
                    han: if robbed { 4 } else { 2 },
                    fu: if robbed { 30 } else { 40 },
                    limit: None
                }
            );
            let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
            yaku.sort_unstable();
            assert_eq!(
                yaku,
                if robbed {
                    vec![(3, Some(1)), (14, Some(1)), (16, Some(2))]
                } else {
                    vec![(4, Some(1)), (16, Some(1))]
                }
            );
            if robbed {
                assert!(win.melds.is_empty());
            } else {
                assert_eq!(win.melds.len(), 1);
                let meld = &win.melds[0];
                assert_eq!(meld.kind, crate::results::ResultMeldKind::Kakan);
                assert!(meld.opened);
                assert_eq!(meld.from_who, Some(feeder));
                assert_eq!(meld.called_tile.unwrap().id(), 75);
                assert_eq!(
                    meld.tiles.iter().map(|t| t.id()).collect::<Vec<_>>(),
                    vec![72, 73, 74, 75]
                );
            }
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                vec![100000; 4]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                expected_delta
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                expected_after
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_three_dragon_pons_charge_responsibility_for_single_and_stacked_yakuman() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let feeder = Seat::new(1).unwrap();
        for stacked in [false, true] {
            let dealer_hand = if stacked {
                [
                    124, 125, 128, 129, 132, 133, 108, 109, 110, 112, 113, 16, 20,
                ]
            } else {
                [124, 125, 128, 129, 132, 133, 0, 4, 8, 108, 109, 16, 20]
            };
            let final_discard = if stacked { 110 } else { 8 };
            let win_copy = if stacked { 111 } else { 10 };
            // Finite full deal before offers; actual Pons establish the liability map.
            let live = [26, 27, 28, 29, 30, 31, win_copy];
            let mut reserved = dealer_hand.to_vec();
            reserved.extend([126, 130, 134, 24, 120]);
            reserved.extend(live);
            let mut remaining = (0..136u8).filter(|t| !reserved.contains(t));
            let mut hands = vec![dealer_hand.to_vec(), vec![126, 130, 134]];
            hands[1].extend(remaining.by_ref().take(10));
            hands.push(remaining.by_ref().take(13).collect());
            hands.push(remaining.by_ref().take(13).collect());
            let mut wall = Vec::with_capacity(136);
            for offset in [0, 4, 8] {
                for hand in &hands {
                    wall.extend_from_slice(&hand[offset..offset + 4]);
                }
            }
            for hand in &hands {
                wall.push(hand[12]);
            }
            wall.push(24);
            wall.extend(live);
            wall.extend(remaining);
            wall.push(120);
            let marker = wall.len() - 5;
            wall.swap(marker, 135);
            let mut inventory = wall.clone();
            inventory.sort_unstable();
            assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
                unreachable!()
            };
            initial.reset();
            initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 4]));
            for (i, hand) in hands.iter_mut().enumerate() {
                hand.sort_unstable();
                if i == 0 {
                    assert_eq!(initial.players[i].hand[..13], hand[..]);
                } else {
                    assert_eq!(initial.players[i].hand, *hand);
                }
            }
            assert_eq!(
                (initial.drawn_tile, initial.wall.dora_indicators.clone()),
                (Some(24), vec![120])
            );
            adapter.log_cursor = 0;
            let mut events = adapter.drain_events(&[]).unwrap();
            let script = [
                (
                    dealer,
                    GameAction::Discard {
                        tile: Tile::from_id(24).unwrap(),
                        tsumogiri: true,
                    },
                ),
                (
                    feeder,
                    GameAction::Discard {
                        tile: Tile::from_id(126).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    dealer,
                    GameAction::Pon {
                        target: feeder,
                        called: Tile::from_id(126).unwrap(),
                        consumed: vec![Tile::from_id(124).unwrap(), Tile::from_id(125).unwrap()],
                    },
                ),
                (
                    dealer,
                    GameAction::Discard {
                        tile: Tile::from_id(16).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    feeder,
                    GameAction::Discard {
                        tile: Tile::from_id(130).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    dealer,
                    GameAction::Pon {
                        target: feeder,
                        called: Tile::from_id(130).unwrap(),
                        consumed: vec![Tile::from_id(128).unwrap(), Tile::from_id(129).unwrap()],
                    },
                ),
                (
                    dealer,
                    GameAction::Discard {
                        tile: Tile::from_id(20).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    feeder,
                    GameAction::Discard {
                        tile: Tile::from_id(134).unwrap(),
                        tsumogiri: false,
                    },
                ),
                (
                    dealer,
                    GameAction::Pon {
                        target: feeder,
                        called: Tile::from_id(134).unwrap(),
                        consumed: vec![Tile::from_id(132).unwrap(), Tile::from_id(133).unwrap()],
                    },
                ),
                (
                    dealer,
                    GameAction::Discard {
                        tile: Tile::from_id(final_discard).unwrap(),
                        tsumogiri: false,
                    },
                ),
            ];
            let mut stage = 0;
            let mut terminal = None;
            for _ in 0..48 {
                let offered = (0..4)
                    .flat_map(|i| {
                        let seat = Seat::new(i).unwrap();
                        adapter
                            .legal_actions(seat)
                            .unwrap()
                            .into_iter()
                            .map(move |a| (seat, a))
                    })
                    .collect::<Vec<_>>();
                let wanted = script
                    .get(stage)
                    .cloned()
                    .unwrap_or((dealer, GameAction::Tsumo));
                let choice = offered
                    .iter()
                    .find(|a| **a == wanted)
                    .or_else(|| offered.iter().find(|(_, a)| *a == GameAction::Pass))
                    .or_else(|| {
                        offered.iter().find(|(_, a)| {
                            matches!(
                                a,
                                GameAction::Discard {
                                    tsumogiri: true,
                                    ..
                                }
                            )
                        })
                    })
                    .cloned()
                    .expect("actual offered responsibility continuation");
                if choice == wanted {
                    stage += 1;
                }
                let outcome = adapter.apply(choice.0, &choice.1).unwrap();
                events.extend(outcome.events);
                if let Some(capture) = outcome.ended {
                    terminal = Some(capture);
                    break;
                }
            }
            assert_eq!(stage, 11, "all ten calls/discards and actual Tsumo");
            let capture = terminal.expect("actual responsibility win");
            let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
                unreachable!()
            };
            assert_eq!(before.players[0].pao.get(&37), Some(&1));
            assert!(
                !before.rule.yakuman_pao_is_liability_only
                    && !before.is_first_turn
                    && !before.is_rinshan_flag
            );
            assert_eq!(before.drawn_tile, Some(win_copy));
            let expected_delta = if stacked {
                vec![98600, -96600, 0, 0]
            } else {
                vec![50600, -48600, 0, 0]
            };
            let expected_after = if stacked {
                vec![198600, 3400, 100000, 100000]
            } else {
                vec![150600, 51400, 100000, 100000]
            };
            let horas = events
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(horas.len(), 1);
            let GameEvent::Hora {
                actor,
                target,
                delta,
                ..
            } = &horas[0]
            else {
                unreachable!()
            };
            assert_eq!(
                (*actor, *target, delta.as_ref().unwrap()),
                (dealer, dealer, &expected_delta)
            );
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Hora { .. }))
                    .cloned()
                    .collect::<Vec<_>>(),
                horas
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
                unreachable!()
            };
            assert!(copy.is_done);
            assert_eq!(capture.real_after_scores, expected_after);
            assert_eq!(capture.real_after_pot, 0);
            let roster = (0..4)
                .map(|i| {
                    Participant::new(format!("pao-{i}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            assert_eq!(record.wins.len(), 1);
            let win = &record.wins[0];
            assert_eq!((win.seat, win.target, win.tsumo), (dealer, dealer, true));
            assert_eq!(
                (
                    win.concealed.len(),
                    win.winning_tile.id(),
                    win.winning_tile_index
                ),
                (5, win_copy, 4)
            );
            assert_eq!(win.win_points, if stacked { 96000 } else { 48000 });
            assert_eq!(
                win.evaluation,
                WinEvaluation::Yakuman {
                    multiplier: if stacked { 2 } else { 1 }
                }
            );
            let mut yaku = win
                .yaku
                .iter()
                .map(|y| (y.id, y.han, y.yakuman_multiplier))
                .collect::<Vec<_>>();
            yaku.sort_unstable();
            assert_eq!(
                yaku,
                if stacked {
                    vec![(37, None, Some(1)), (39, None, Some(1))]
                } else {
                    vec![(37, None, Some(1))]
                }
            );
            assert_eq!(win.melds.len(), 3);
            for (i, meld) in win.melds.iter().enumerate() {
                assert_eq!(meld.kind, crate::results::ResultMeldKind::Pon);
                assert!(meld.opened);
                assert_eq!(meld.from_who, Some(feeder));
                assert_eq!(meld.called_tile.unwrap().id(), [126, 130, 134][i]);
                assert_eq!(
                    meld.tiles.iter().map(|t| t.id()).collect::<Vec<_>>(),
                    [
                        vec![124, 125, 126],
                        vec![128, 129, 130],
                        vec![132, 133, 134]
                    ][i]
                );
            }
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                vec![100000; 4]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                expected_delta
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                expected_after
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_offered_nuki_has_literal_three_player_payments_and_copy() {
        let mode = GameMode::ThreePlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
        let GameStateVariant::ThreePlayer(initial) = &mut adapter.state else {
            unreachable!()
        };
        assert_eq!(initial.players[0].hand.last(), Some(&77));
        let hands = [
            initial.players[1].hand.clone(),
            initial.players[0].hand[..13].to_vec(),
            initial.players[2].hand.clone(),
        ];
        let mut wall = Vec::with_capacity(108);
        for offset in [0, 4, 8] {
            for hand in &hands {
                wall.extend_from_slice(&hand[offset..offset + 4]);
            }
        }
        for hand in &hands {
            wall.push(hand[12]);
        }
        wall.push(77);
        wall.extend(initial.wall.tiles.iter().rev().copied());
        // Fixed private deal: actual North extraction and replacement, not flags/results.
        let north = wall.iter().position(|&t| t == 121).unwrap();
        wall.swap(39, north);
        let replacement = wall.iter().position(|&t| t == 79).unwrap();
        wall.swap(107, replacement);
        let mut inventory = wall.clone();
        inventory.sort_unstable();
        assert_eq!(
            inventory,
            (0..136u8)
                .filter(|t| !(4..32).contains(t))
                .collect::<Vec<_>>()
        );
        initial.reset();
        initial._initialize_round(0, 0, 2, 2, Some(wall), Some(vec![100000; 3]));
        assert_eq!(
            (initial.drawn_tile, initial.wall.dora_indicators.clone()),
            (Some(121), vec![120])
        );
        assert_eq!(initial.players[0].hand[..13], hands[0]);
        adapter.log_cursor = 0;
        let mut events = adapter.drain_events(&[]).unwrap();
        let mut extracted = false;
        let mut terminal = None;
        for _ in 0..8 {
            let legal = adapter.legal_actions(dealer).unwrap();
            let action = legal
                .into_iter()
                .find(|a| {
                    if extracted {
                        *a == GameAction::Tsumo
                    } else {
                        matches!(a,GameAction::Nuki {tile} if tile.id()==121)
                    }
                })
                .expect("actual offered North then replacement Tsumo");
            if matches!(action, GameAction::Nuki { .. }) {
                assert!(!extracted);
                extracted = true;
            }
            let outcome = adapter.apply(dealer, &action).unwrap();
            events.extend(outcome.events);
            if let Some(capture) = outcome.ended {
                terminal = Some(capture);
                break;
            }
        }
        assert!(extracted);
        let capture = terminal.expect("actual Kita replacement win");
        let GameStateVariant::ThreePlayer(before) = &capture.pre_action else {
            unreachable!()
        };
        assert!(!before.is_first_turn && before.is_rinshan_flag);
        assert_eq!(before.players[0].kita_tiles, vec![121]);
        assert!(!before.players[0].hand.contains(&121));
        assert!(before.players[0].melds.is_empty());
        assert_eq!(
            (before.drawn_tile, before.wall.rinshan_draw_count),
            (Some(79), 1)
        );
        assert_eq!(before.wall.dora_indicators, vec![120]);
        let horas = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(horas.len(), 1);
        let copied = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| parse_event(line, mode, &[]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            horas
        );
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
        let GameStateVariant::ThreePlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(capture.real_after_scores, vec![110400, 95800, 95800]);
        assert_eq!(capture.real_after_pot, 0);
        let roster = (0..3)
            .map(|i| Participant::new(format!("nuki-{i}"), "same", crate::ParticipantKind::Human))
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &events)
            .unwrap();
        assert_eq!(record.wins.len(), 1);
        let win = &record.wins[0];
        assert_eq!((win.seat, win.target, win.tsumo), (dealer, dealer, true));
        assert_eq!(
            (
                win.concealed.len(),
                win.winning_tile.id(),
                win.winning_tile_index
            ),
            (14, 79, 13)
        );
        assert_eq!(win.win_points, 8000);
        assert_eq!(
            win.evaluation,
            WinEvaluation::Ordinary {
                han: 5,
                fu: 40,
                limit: Some(ScoreLimit::Mangan)
            }
        );
        let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
        yaku.sort_unstable();
        assert_eq!(
            yaku,
            vec![(1, Some(1)), (4, Some(1)), (16, Some(2)), (34, Some(1))]
        );
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (2, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.before)
                .collect::<Vec<_>>(),
            vec![100000; 3]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.delta)
                .collect::<Vec<_>>(),
            vec![10400, -4200, -4200]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>(),
            vec![110400, 95800, 95800]
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_offered_ankan_rinshan_preserves_physical_meld_and_copy() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let (mut adapter, mut events) = EngineAdapter::task8_dealer_tenhou(mode).unwrap();
        let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
            unreachable!()
        };
        // Finite undealt permutation before offers; actions generate every Kan flag/log.
        let live = initial.wall.tiles.len() - 4;
        let fourth = initial.wall.tiles.iter().position(|&t| t == 75).unwrap();
        assert!(fourth >= 14);
        initial.wall.tiles.swap(live, fourth);
        let replacement = initial.wall.tiles.iter().position(|&t| t == 79).unwrap();
        assert!(replacement >= 14);
        initial.wall.tiles.swap(0, replacement);
        let mut inventory = initial.wall.tiles.clone();
        inventory.extend(initial.players.iter().flat_map(|p| p.hand.iter().copied()));
        inventory.sort_unstable();
        assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
        assert_eq!(
            (initial.drawn_tile, initial.wall.dora_indicators.clone()),
            (Some(77), vec![120])
        );
        let mut kan_played = false;
        let mut terminal = None;
        for _ in 0..32 {
            let offered = (0..4)
                .flat_map(|i| {
                    let seat = Seat::new(i).unwrap();
                    adapter
                        .legal_actions(seat)
                        .unwrap()
                        .into_iter()
                        .map(move |a| (seat, a))
                })
                .collect::<Vec<_>>();
            let choice = offered.iter().find(|(seat,a)| *seat==dealer && if kan_played {*a==GameAction::Tsumo} else {matches!(a,GameAction::Ankan {consumed} if consumed.iter().map(|t|t.id()).collect::<Vec<_>>()==vec![72,73,74,75])})
                .or_else(||offered.iter().find(|(_,a)|*a==GameAction::Pass))
                .or_else(||offered.iter().find(|(_,a)|matches!(a,GameAction::Discard {tsumogiri:true,..})))
                .cloned().expect("actual offered bounded Kan continuation");
            if matches!(choice.1, GameAction::Ankan { .. }) {
                assert!(!kan_played);
                kan_played = true;
            }
            let outcome = adapter.apply(choice.0, &choice.1).unwrap();
            events.extend(outcome.events);
            if let Some(capture) = outcome.ended {
                terminal = Some(capture);
                break;
            }
        }
        assert!(kan_played);
        let capture = terminal.expect("actual Rinshan Tsumo");
        let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
            unreachable!()
        };
        assert!(before.is_rinshan_flag && !before.is_first_turn);
        assert_eq!(
            (before.drawn_tile, before.wall.rinshan_draw_count),
            (Some(79), 1)
        );
        assert_eq!(before.wall.dora_indicators, vec![120, 129]);
        assert_eq!(before.players[0].melds.len(), 1);
        assert!(!before.players[0].melds[0].opened);
        assert_eq!(before.players[0].melds[0].tiles, vec![72, 73, 74, 75]);
        let horas = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(horas.len(), 1);
        let copied = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| parse_event(line, mode, &[]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            horas
        );
        assert_eq!(
            copied
                .iter()
                .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                .count(),
            1
        );
        let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(capture.real_after_scores, vec![114600, 95800, 95800, 95800]);
        assert_eq!(capture.real_after_pot, 0);
        let roster = (0..4)
            .map(|i| Participant::new(format!("kan-{i}"), "same", crate::ParticipantKind::Human))
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &events)
            .unwrap();
        assert_eq!(record.wins.len(), 1);
        let win = &record.wins[0];
        assert_eq!((win.seat, win.target, win.tsumo), (dealer, dealer, true));
        assert_eq!(
            (
                win.concealed.len(),
                win.winning_tile.id(),
                win.winning_tile_index,
                win.melds.len()
            ),
            (11, 79, 10, 1)
        );
        assert_eq!(win.win_points, 12000);
        assert_eq!(
            win.evaluation,
            WinEvaluation::Ordinary {
                han: 4,
                fu: 60,
                limit: Some(ScoreLimit::Mangan)
            }
        );
        let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
        yaku.sort_unstable();
        assert_eq!(yaku, vec![(1, Some(1)), (4, Some(1)), (16, Some(2))]);
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (2, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.before)
                .collect::<Vec<_>>(),
            vec![100000; 4]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.delta)
                .collect::<Vec<_>>(),
            vec![14600, -4200, -4200, -4200]
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| c.after)
                .collect::<Vec<_>>(),
            vec![114600, 95800, 95800, 95800]
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_haitei_and_houtei_have_literal_context_payments_and_copy() {
        let mode = GameMode::FourPlayerRedEast;
        for haitei in [true, false] {
            let (mut adapter, mut events) = EngineAdapter::task8_double_ron(mode).unwrap();
            let GameStateVariant::FourPlayer(initial) = &mut adapter.state else {
                unreachable!()
            };
            // Only undealt live tiles change; 14 dead tiles and all initial logs stay intact.
            let last_copy = initial.wall.tiles.iter().position(|&t| t == 79).unwrap();
            assert!(last_copy >= 14);
            initial.wall.tiles.swap(14, last_copy);
            if !haitei {
                // Seat 2 also waits on 1s/4s: don't deal it a self-discarded 4s.
                assert_eq!((initial.wall.tiles[49], initial.wall.tiles[52]), (86, 83));
                initial.wall.tiles.swap(49, 52);
            }
            let mut inventory = initial.wall.tiles.clone();
            inventory.extend(initial.players.iter().flat_map(|p| p.hand.iter().copied()));
            inventory.sort_unstable();
            assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
            assert_eq!(initial.wall.dora_indicators, vec![120]);
            let winner = Seat::new(if haitei { 1 } else { 2 }).unwrap();
            let wanted = if haitei {
                GameAction::Tsumo
            } else {
                GameAction::Ron(Seat::new(1).unwrap())
            };
            let mut terminal = None;
            for _ in 0..280 {
                // 70 discards plus at most three responses per discard.
                let GameStateVariant::FourPlayer(state) = &adapter.state else {
                    unreachable!()
                };
                let last_live = state.wall.drawable_count == 0;
                let offered = (0..4)
                    .flat_map(|i| {
                        let seat = Seat::new(i).unwrap();
                        adapter
                            .legal_actions(seat)
                            .unwrap()
                            .into_iter()
                            .map(move |a| (seat, a))
                    })
                    .collect::<Vec<_>>();
                let choice = offered
                    .iter()
                    .find(|(seat, a)| last_live && *seat == winner && *a == wanted)
                    .or_else(|| offered.iter().find(|(_, a)| *a == GameAction::Pass))
                    .or_else(|| {
                        offered.iter().find(|(_, a)| {
                            matches!(
                                a,
                                GameAction::Discard {
                                    tsumogiri: true,
                                    ..
                                }
                            )
                        })
                    })
                    .cloned()
                    .expect("actual offered bounded continuation");
                let outcome = adapter.apply(choice.0, &choice.1).unwrap();
                events.extend(outcome.events);
                if let Some(ended) = outcome.ended {
                    terminal = Some(ended);
                    break;
                }
            }
            let capture = terminal.expect("actual terminal within finite physical wall");
            let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
                unreachable!()
            };
            assert_eq!(before.wall.drawable_count, 0);
            assert!(!before.is_first_turn && !before.is_rinshan_flag);
            if !haitei {
                assert!(
                    before.players[2]
                        .discards
                        .iter()
                        .all(|t| ![18, 19, 21].contains(&(t / 4)))
                );
            }
            let horas = events
                .iter()
                .filter(|e| matches!(e, GameEvent::Hora { .. }))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                horas.len(),
                1,
                "haitei={haitei} player={} draw={:?} discard={:?} hands={:?} tail={:?} river2={:?}",
                before.current_player,
                before.drawn_tile,
                before.last_discard,
                before.players.iter().map(|p| &p.hand).collect::<Vec<_>>(),
                events.iter().rev().take(8).collect::<Vec<_>>(),
                before.players[2].discards
            );
            let GameEvent::Hora {
                actor,
                target,
                delta,
                ..
            } = &horas[0]
            else {
                unreachable!()
            };
            assert_eq!((*actor, *target), (winner, Seat::new(1).unwrap()));
            let expected_delta = if haitei {
                vec![-4200, 10600, -2200, -2200]
            } else {
                vec![0, -5800, 7800, 0]
            };
            let expected_after = if haitei {
                vec![95800, 110600, 97800, 97800]
            } else {
                vec![100000, 94200, 107800, 100000]
            };
            assert_eq!(delta.as_ref().unwrap(), &expected_delta);
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &[]).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::Hora { .. }))
                    .cloned()
                    .collect::<Vec<_>>(),
                horas
            );
            assert_eq!(
                copied
                    .iter()
                    .filter(|e| matches!(e, GameEvent::StartKyoku { .. }))
                    .count(),
                1
            );
            let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
                unreachable!()
            };
            assert!(copy.is_done);
            assert_eq!(capture.real_after_scores, expected_after);
            assert_eq!(capture.real_after_pot, 0);
            let roster = (0..4)
                .map(|i| {
                    Participant::new(format!("last-{i}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &events)
                .unwrap();
            let win = &record.wins[0];
            assert_eq!(
                (win.seat, win.target, win.tsumo),
                (winner, Seat::new(1).unwrap(), haitei)
            );
            assert_eq!((win.winning_tile.id(), win.winning_tile_index), (79, 13));
            assert_eq!(win.win_points, if haitei { 8000 } else { 5200 });
            assert_eq!(
                win.evaluation,
                WinEvaluation::Ordinary {
                    han: if haitei { 4 } else { 3 },
                    fu: 40,
                    limit: if haitei {
                        Some(ScoreLimit::Mangan)
                    } else {
                        None
                    }
                }
            );
            let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
            yaku.sort_unstable();
            assert_eq!(
                yaku,
                if haitei {
                    vec![(1, Some(1)), (5, Some(1)), (16, Some(2))]
                } else {
                    vec![(6, Some(1)), (16, Some(2))]
                }
            );
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.before)
                    .collect::<Vec<_>>(),
                vec![100000; 4]
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.delta)
                    .collect::<Vec<_>>(),
                expected_delta
            );
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                expected_after
            );
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_actual_compound_riichi_ippatsu_and_ura_preserve_paid_deposit_and_copy() {
        let mode = GameMode::FourPlayerRedEast;
        let dealer = Seat::new(0).unwrap();
        let (mut adapter, mut events) = EngineAdapter::task8_dealer_tenhou(mode).unwrap();
        // Private initial-wall fixture only: no context flags or adjudication are injected.
        let GameStateVariant::FourPlayer(state) = &mut adapter.state else {
            unreachable!()
        };
        let next_dealer_draw = state.wall.tiles.len() - 4;
        let winning_copy = state
            .wall
            .tiles
            .iter()
            .position(|&tile| tile == 79)
            .unwrap();
        assert!(winning_copy >= 14 && next_dealer_draw >= 14);
        state.wall.tiles.swap(next_dealer_draw, winning_copy);
        let ura_copy = state
            .wall
            .tiles
            .iter()
            .position(|&tile| tile == 75)
            .unwrap();
        assert!(ura_copy >= 14);
        state.wall.tiles.swap(5, ura_copy);
        let mut inventory = state.wall.tiles.clone();
        inventory.extend(state.players.iter().flat_map(|p| p.hand.iter().copied()));
        inventory.sort_unstable();
        assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
        assert_eq!(state.wall.dora_indicators, vec![120]);
        let declaration = GameAction::RiichiDiscard {
            tile: Tile::from_id(77).unwrap(),
        };
        assert!(
            adapter
                .legal_actions(dealer)
                .unwrap()
                .contains(&declaration)
        );
        let declared = adapter.apply(dealer, &declaration).unwrap();
        assert!(declared.ended.is_none());
        assert!(matches!(&declared.events[0], GameEvent::Reach { actor } if *actor == dealer));
        assert!(
            matches!(&declared.events[1], GameEvent::Dahai { actor, tile, .. } if *actor == dealer && tile.id() == 76)
        );
        let GameStateVariant::FourPlayer(declared_state) = &adapter.state else {
            unreachable!()
        };
        assert_eq!(declared_state.players[0].discards.last(), Some(&77));
        // MJAI "2s" represents kind19 with canonicalcopy76; the accepted physical copy is77.
        events.extend(declared.events);
        let mut terminal = None;
        // Three other turns, declining every offered response before the next draw.
        for _ in 0..24 {
            if adapter
                .legal_actions(dealer)
                .unwrap()
                .contains(&GameAction::Tsumo)
            {
                terminal = Some(adapter.apply(dealer, &GameAction::Tsumo).unwrap());
                break;
            }
            let mut choice = None;
            for index in 0..4 {
                let seat = Seat::new(index).unwrap();
                let actions = adapter.legal_actions(seat).unwrap();
                if let Some(action) =
                    actions
                        .iter()
                        .find(|a| **a == GameAction::Pass)
                        .or_else(|| {
                            actions.iter().find(|a| {
                                matches!(
                                    a,
                                    GameAction::Discard {
                                        tsumogiri: true,
                                        ..
                                    }
                                )
                            })
                        })
                {
                    choice = Some((seat, action.clone()));
                    break;
                }
            }
            let (seat, action) = choice.expect("genuine intervening offered action");
            let outcome = adapter.apply(seat, &action).unwrap();
            assert!(outcome.ended.is_none(), "no earlier terminal");
            events.extend(outcome.events);
        }
        let terminal = terminal.expect("dealer's first draw after actual riichi acceptance");
        let capture = terminal.ended.unwrap();
        let GameStateVariant::FourPlayer(before) = &capture.pre_action else {
            unreachable!()
        };
        assert_eq!(before.drawn_tile, Some(79));
        assert_eq!(
            before.players.iter().map(|p| p.score).collect::<Vec<_>>(),
            vec![99000, 100000, 100000, 100000]
        );
        assert_eq!(before.riichi_sticks, 3);
        assert!(
            before.players[0].riichi_declared
                && before.players[0].double_riichi_declared
                && before.players[0].ippatsu_cycle
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, GameEvent::ReachAccepted { actor } if *actor == dealer))
                .count(),
            1
        );
        let hora = terminal
            .events
            .iter()
            .find(|e| matches!(e, GameEvent::Hora { .. }))
            .unwrap();
        let GameEvent::Hora {
            delta, ura_markers, ..
        } = hora
        else {
            unreachable!()
        };
        assert_eq!(delta.as_ref().unwrap(), &vec![27600, -8200, -8200, -8200]);
        assert_eq!(
            ura_markers
                .as_ref()
                .unwrap()
                .iter()
                .map(|t| t.id())
                .collect::<Vec<_>>(),
            vec![72]
        );
        let copy_hora = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| parse_event(line, mode, &[(dealer, GameAction::Tsumo)]).unwrap())
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .collect::<Vec<_>>();
        assert_eq!(copy_hora, vec![hora.clone()]);
        let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(capture.real_after_scores, vec![126600, 91800, 91800, 91800]);
        assert_eq!(capture.real_after_pot, 0);
        events.extend(terminal.events);
        let roster = (0..4)
            .map(|i| Participant::new(format!("riichi-{i}"), "same", crate::ParticipantKind::Human))
            .collect::<Vec<_>>();
        let table = adapter
            .ended_table_state(&capture, &roster, &[99000, 100000, 100000, 100000], 3)
            .unwrap();
        for audience in [
            crate::Audience::Public,
            crate::Audience::Player(dealer),
            crate::Audience::ReplayAdmin,
        ] {
            let json = serde_json::to_value(crate::project_table_state(&table, audience)).unwrap();
            assert_eq!(
                json["players"][0]["riichi_discard_index"], 0,
                "actual declaration survives terminal snapshot"
            );
            assert!(json["players"][1].get("riichi_discard_index").is_none());
            if audience == crate::Audience::Public {
                assert!(
                    json["players"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|p| p.get("hand").is_none())
                );
            }
        }
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &events)
            .unwrap();
        let win = &record.wins[0];
        assert_eq!(win.win_points, 24000); // Baiman basic total; excludes honba/deposits.
        assert_eq!(
            win.evaluation,
            WinEvaluation::Ordinary {
                han: 8,
                fu: 40,
                limit: Some(ScoreLimit::Baiman)
            }
        );
        assert_eq!((win.winning_tile.id(), win.winning_tile_index), (79, 13));
        let mut yaku = win.yaku.iter().map(|y| (y.id, y.han)).collect::<Vec<_>>();
        yaku.sort_unstable();
        assert_eq!(
            yaku,
            vec![
                (1, Some(1)),
                (16, Some(2)),
                (18, Some(2)),
                (30, Some(1)),
                (33, Some(2))
            ]
        );
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (3, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| (c.before, c.delta, c.after))
                .collect::<Vec<_>>(),
            vec![
                (99000, 27600, 126600),
                (100000, -8200, 91800),
                (100000, -8200, 91800),
                (100000, -8200, 91800)
            ]
        );
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_dealer_tenhou_has_literal_yakuman_and_first_natural_renchan() {
        let mode = GameMode::FourPlayerRedEast;
        let (mut adapter, initial) = EngineAdapter::task8_dealer_tenhou(mode).unwrap();
        assert_eq!(
            initial.iter().map(GameEvent::kind).collect::<Vec<_>>(),
            ["start_game", "start_kyoku", "tsumo"]
        );
        let expected_hand = vec![36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76, 77];
        let GameStateVariant::FourPlayer(pre) = &adapter.state else {
            unreachable!()
        };
        assert!(pre.is_first_turn);
        assert_eq!(pre.turn_count, 0);
        assert_eq!(pre.drawn_tile, Some(77));
        assert_eq!(pre.players[0].hand, expected_hand);
        assert!(
            pre.players
                .iter()
                .all(|p| p.melds.is_empty() && p.discards.is_empty())
        );
        let mut inventory = pre
            .wall
            .tiles
            .iter()
            .copied()
            .chain(pre.players.iter().flat_map(|p| p.hand.iter().copied()))
            .collect::<Vec<_>>();
        inventory.sort_unstable();
        assert_eq!(inventory, (0..136u8).collect::<Vec<_>>());
        assert_eq!(
            &pre.wall.tiles[..14],
            &[
                135, 134, 133, 132, 120, 130, 129, 128, 127, 126, 125, 124, 123, 122
            ]
        );
        assert_eq!(
            (pre.oya, pre.round_wind, pre.honba, pre.riichi_sticks),
            (0, 0, 2, 2)
        );
        assert!(
            adapter
                .legal_actions(Seat::new(0).unwrap())
                .unwrap()
                .contains(&GameAction::Tsumo)
        );
        let outcome = adapter
            .apply(Seat::new(0).unwrap(), &GameAction::Tsumo)
            .unwrap();
        assert_eq!(
            outcome
                .events
                .iter()
                .map(GameEvent::kind)
                .collect::<Vec<_>>(),
            ["hora", "end_kyoku", "start_kyoku", "tsumo"]
        );
        let GameEvent::Hora {
            actor,
            target,
            delta,
            ..
        } = &outcome.events[0]
        else {
            panic!("actual Hora")
        };
        assert_eq!((actor.index(), target.index()), (0, 0));
        assert_eq!(delta.as_ref(), Some(&vec![50600, -16200, -16200, -16200]));
        let GameEvent::StartKyoku {
            oya,
            honba,
            kyotaku,
            kyoku,
            scores,
            ..
        } = &outcome.events[2]
        else {
            panic!("natural next hand")
        };
        assert_eq!((oya.index(), *honba, *kyotaku, *kyoku), (0, 3, 0, 1));
        assert_eq!(scores, &vec![150600, 83800, 83800, 83800]);
        let capture = outcome.ended.unwrap();
        assert_eq!(capture.real_after_scores, vec![150600, 83800, 83800, 83800]);
        assert_eq!(capture.real_after_pot, 0);
        let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        assert!(copy.is_done);
        assert_eq!(
            copy.players.iter().map(|p| p.score).collect::<Vec<_>>(),
            vec![150600, 83800, 83800, 83800]
        );
        assert_eq!(copy.riichi_sticks, 0);
        let copied = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| {
                parse_event(line, mode, &[(Seat::new(0).unwrap(), GameAction::Tsumo)]).unwrap()
            })
            .filter(|e| matches!(e, GameEvent::Hora { .. }))
            .collect::<Vec<_>>();
        assert_eq!(copied, vec![outcome.events[0].clone()]);
        let copied_kinds = state_logs(&capture.ended_copy)
            .iter()
            .map(|line| {
                parse_event(line, mode, &[(Seat::new(0).unwrap(), GameAction::Tsumo)])
                    .unwrap()
                    .kind()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            copied_kinds,
            [
                "start_game",
                "start_kyoku",
                "tsumo",
                "hora",
                "end_kyoku",
                "end_game"
            ]
        );
        let GameStateVariant::FourPlayer(next) = &adapter.state else {
            unreachable!()
        };
        assert!(!next.is_done);
        assert_eq!(
            (next.oya, next.round_wind, next.honba, next.riichi_sticks),
            (0, 0, 3, 0)
        );
        assert_ne!(
            next.players[0].hand, expected_hand,
            "next hand is real seeded shuffle, not fixture reapplication"
        );
        let mut next_inventory = next
            .wall
            .tiles
            .iter()
            .copied()
            .chain(next.players.iter().flat_map(|p| p.hand.iter().copied()))
            .collect::<Vec<_>>();
        next_inventory.sort_unstable();
        assert_eq!(next_inventory, (0..136u8).collect::<Vec<_>>());
        let roster = (0..4)
            .map(|seat| {
                Participant::new(
                    format!("tenhou-{seat}"),
                    "same",
                    crate::ParticipantKind::Human,
                )
            })
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, initial.len(), &outcome.events)
            .unwrap();
        assert_eq!(record.wins.len(), 1);
        let win = &record.wins[0];
        assert_eq!(win.seat.index(), 0);
        assert_eq!(win.win_points, 48000);
        assert_eq!(win.evaluation, WinEvaluation::Yakuman { multiplier: 1 });
        assert_eq!((win.winning_tile.id(), win.winning_tile_index), (77, 13));
        assert_eq!(
            win.concealed.iter().map(|t| t.id()).collect::<Vec<_>>(),
            expected_hand
        );
        assert_eq!(win.yaku.len(), 1);
        assert_eq!(
            (
                win.yaku[0].id,
                win.yaku[0].han,
                win.yaku[0].yakuman_multiplier
            ),
            (35, None, Some(1))
        );
        assert_eq!(
            (record.settlement.pot_before, record.settlement.pot_after),
            (2, 0)
        );
        assert_eq!(
            record
                .settlement
                .changes
                .iter()
                .map(|c| (c.before, c.delta, c.after))
                .collect::<Vec<_>>(),
            [
                (100000, 50600, 150600),
                (100000, -16200, 83800),
                (100000, -16200, 83800),
                (100000, -16200, 83800)
            ]
        );
        for unsupported in [
            GameMode::FourPlayerRedHalf,
            GameMode::ThreePlayerRedEast,
            GameMode::ThreePlayerRedHalf,
        ] {
            assert!(EngineAdapter::task8_dealer_tenhou(unsupported).is_err());
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_final_double_ron_has_physical_deal_literal_payments_and_canonical_bankruptcy_end() {
        for (mode, before, after, deltas) in [
            (
                GameMode::ThreePlayerRedEast,
                vec![1_000, 100_000, 100_000],
                vec![-4_600, 105_000, 102_600],
                vec![vec![-3_000, 5_000, 0], vec![-2_600, 0, 2_600]],
            ),
            (
                GameMode::FourPlayerRedEast,
                vec![1_000, 100_000, 100_000, 100_000],
                vec![-4_800, 105_200, 102_600, 100_000],
                vec![vec![-3_200, 5_200, 0, 0], vec![-2_600, 0, 2_600, 0]],
            ),
        ] {
            let (mut adapter, initial) = EngineAdapter::task8_double_ron_final(mode).unwrap();
            assert_eq!(
                initial.iter().map(GameEvent::kind).collect::<Vec<_>>(),
                ["start_game", "start_kyoku", "tsumo"]
            );
            let GameEvent::StartKyoku {
                scores,
                kyotaku,
                honba,
                ..
            } = &initial[1]
            else {
                panic!("actual deal prefix")
            };
            assert_eq!(scores, &before);
            assert_eq!((*kyotaku, *honba), (2, 2));
            let mut inventory = match &adapter.state {
                GameStateVariant::FourPlayer(s) => s
                    .wall
                    .tiles
                    .iter()
                    .copied()
                    .chain(s.players.iter().flat_map(|p| p.hand.iter().copied()))
                    .collect::<Vec<_>>(),
                GameStateVariant::ThreePlayer(s) => s
                    .wall
                    .tiles
                    .iter()
                    .copied()
                    .chain(s.players.iter().flat_map(|p| p.hand.iter().copied()))
                    .collect::<Vec<_>>(),
            };
            inventory.sort_unstable();
            assert_eq!(
                inventory,
                (0..136u8)
                    .filter(|t| mode.seat_count() == 4 || !(4..32).contains(t))
                    .collect::<Vec<_>>()
            );
            let discard = GameAction::Discard {
                tile: Tile::from_id(77).unwrap(),
                tsumogiri: true,
            };
            assert!(
                adapter
                    .legal_actions(Seat::new(0).unwrap())
                    .unwrap()
                    .contains(&discard)
            );
            let discarded = adapter.apply(Seat::new(0).unwrap(), &discard).unwrap();
            assert!(discarded.ended.is_none());
            let mut responses = Vec::new();
            for seat in Seat::all(mode).into_iter().skip(1) {
                let offers = adapter.legal_actions(seat).unwrap();
                let action = if seat.index() <= 2 {
                    GameAction::Ron(Seat::new(0).unwrap())
                } else {
                    GameAction::Pass
                };
                if seat.index() <= 2 || !offers.is_empty() {
                    assert!(offers.contains(&action));
                    responses.push((seat, action));
                }
            }
            let outcome = adapter.apply_simultaneous(&responses).unwrap();
            assert_eq!(
                outcome
                    .events
                    .iter()
                    .map(GameEvent::kind)
                    .collect::<Vec<_>>(),
                ["hora", "hora", "end_kyoku", "end_game"]
            );
            for (i, expected_delta) in deltas.iter().enumerate() {
                let GameEvent::Hora {
                    actor,
                    target,
                    delta,
                    ..
                } = &outcome.events[i]
                else {
                    panic!("actual ordered Hora")
                };
                assert_eq!((actor.index(), target.index()), ((i + 1) as u8, 0));
                assert_eq!(delta.as_ref(), Some(expected_delta));
            }
            let capture = outcome.ended.unwrap();
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &responses).unwrap())
                .filter(|e| {
                    matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::EndKyoku | GameEvent::EndGame
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(copied, outcome.events);
            assert_eq!(capture.real_after_scores, after);
            assert_eq!(capture.real_after_pot, 0);
            let roster = (0..mode.seat_count())
                .map(|i| Participant::new(format!("p{i}"), "same", crate::ParticipantKind::Human))
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(
                    &capture,
                    &roster,
                    1,
                    initial.len() + discarded.events.len(),
                    &outcome.events,
                )
                .unwrap();
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| (c.before, c.after))
                    .collect::<Vec<_>>(),
                before.into_iter().zip(after).collect::<Vec<_>>()
            );
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .wins
                    .iter()
                    .map(|w| w.seat.index())
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            for win in &record.wins {
                assert_eq!(
                    win.evaluation,
                    WinEvaluation::Ordinary {
                        han: 2,
                        fu: 40,
                        limit: None
                    }
                );
                assert_eq!(win.win_points, 2_600);
                assert_eq!((win.winning_tile.id(), win.winning_tile_index), (77, 13));
                assert_eq!(win.concealed[13].id(), 77);
                assert_eq!(win.concealed.iter().filter(|t| t.id() / 4 == 19).count(), 2);
            }
            record.validate(&roster).unwrap();
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_half_final_double_ron_has_same_physical_wall_offers_literal_payments_and_genuine_end()
    {
        for (mode, before, after, deltas) in [(
            GameMode::FourPlayerRedHalf,
            vec![1_000, 100_000, 100_000, 100_000],
            vec![-4_800, 105_200, 102_600, 100_000],
            vec![vec![-3_200, 5_200, 0, 0], vec![-2_600, 0, 2_600, 0]],
        )] {
            let (mut adapter, initial) = EngineAdapter::task8_double_ron_final(mode).unwrap();
            assert_eq!(adapter.mode, GameMode::FourPlayerRedHalf);
            let GameStateVariant::FourPlayer(producer) = &adapter.state else {
                panic!("Half must use four-player producer");
            };
            assert_eq!(
                producer.game_mode, 2,
                "pinned Half mode, not an East override"
            );
            assert_eq!(
                initial.iter().map(GameEvent::kind).collect::<Vec<_>>(),
                ["start_game", "start_kyoku", "tsumo"]
            );
            let GameEvent::StartKyoku {
                scores,
                kyotaku,
                honba,
                ..
            } = &initial[1]
            else {
                panic!("actual deal prefix")
            };
            assert_eq!(scores, &before);
            assert_eq!((*kyotaku, *honba), (2, 2));
            let mut inventory = match &adapter.state {
                GameStateVariant::FourPlayer(s) => s
                    .wall
                    .tiles
                    .iter()
                    .copied()
                    .chain(s.players.iter().flat_map(|p| p.hand.iter().copied()))
                    .collect::<Vec<_>>(),
                GameStateVariant::ThreePlayer(s) => s
                    .wall
                    .tiles
                    .iter()
                    .copied()
                    .chain(s.players.iter().flat_map(|p| p.hand.iter().copied()))
                    .collect::<Vec<_>>(),
            };
            inventory.sort_unstable();
            assert_eq!(
                inventory,
                (0..136u8)
                    .filter(|t| mode.seat_count() == 4 || !(4..32).contains(t))
                    .collect::<Vec<_>>()
            );
            let discard = GameAction::Discard {
                tile: Tile::from_id(77).unwrap(),
                tsumogiri: true,
            };
            assert!(
                adapter
                    .legal_actions(Seat::new(0).unwrap())
                    .unwrap()
                    .contains(&discard)
            );
            let discarded = adapter.apply(Seat::new(0).unwrap(), &discard).unwrap();
            assert!(discarded.ended.is_none());
            let mut responses = Vec::new();
            for seat in Seat::all(mode).into_iter().skip(1) {
                let offers = adapter.legal_actions(seat).unwrap();
                let action = if seat.index() <= 2 {
                    GameAction::Ron(Seat::new(0).unwrap())
                } else {
                    GameAction::Pass
                };
                if seat.index() <= 2 || !offers.is_empty() {
                    assert!(offers.contains(&action));
                    responses.push((seat, action));
                }
            }
            let outcome = adapter.apply_simultaneous(&responses).unwrap();
            assert_eq!(
                outcome
                    .events
                    .iter()
                    .map(GameEvent::kind)
                    .collect::<Vec<_>>(),
                ["hora", "hora", "end_kyoku", "end_game"]
            );
            for (i, expected_delta) in deltas.iter().enumerate() {
                let GameEvent::Hora {
                    actor,
                    target,
                    delta,
                    ..
                } = &outcome.events[i]
                else {
                    panic!("actual ordered Hora")
                };
                assert_eq!((actor.index(), target.index()), ((i + 1) as u8, 0));
                assert_eq!(delta.as_ref(), Some(expected_delta));
            }
            let capture = outcome.ended.unwrap();
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &responses).unwrap())
                .filter(|e| {
                    matches!(
                        e,
                        GameEvent::Hora { .. } | GameEvent::EndKyoku | GameEvent::EndGame
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(copied, outcome.events);
            assert_eq!(capture.real_after_scores, after);
            assert_eq!(capture.real_after_pot, 0);
            let roster = (0..mode.seat_count())
                .map(|i| Participant::new(format!("p{i}"), "same", crate::ParticipantKind::Human))
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(
                    &capture,
                    &roster,
                    1,
                    initial.len() + discarded.events.len(),
                    &outcome.events,
                )
                .unwrap();
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| (c.before, c.after))
                    .collect::<Vec<_>>(),
                before.into_iter().zip(after).collect::<Vec<_>>()
            );
            assert_eq!(
                (record.settlement.pot_before, record.settlement.pot_after),
                (2, 0)
            );
            assert_eq!(
                record
                    .wins
                    .iter()
                    .map(|w| w.seat.index())
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            for win in &record.wins {
                assert_eq!(
                    win.evaluation,
                    WinEvaluation::Ordinary {
                        han: 2,
                        fu: 40,
                        limit: None
                    }
                );
                assert_eq!(win.win_points, 2_600);
                assert_eq!((win.winning_tile.id(), win.winning_tile_index), (77, 13));
                assert_eq!(win.concealed[13].id(), 77);
                assert_eq!(win.concealed.iter().filter(|t| t.id() / 4 == 19).count(), 2);
            }
            record.validate(&roster).unwrap();
        }
    }

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task8_full_wall_offers_double_ron_and_captures_literal_authority() {
        for (mode, expected_after, first_delta, second_delta) in [
            (
                GameMode::ThreePlayerRedEast,
                vec![94_400, 105_000, 102_600],
                vec![-3_000, 5_000, 0],
                vec![-2_600, 0, 2_600],
            ),
            (
                GameMode::FourPlayerRedEast,
                vec![94_200, 105_200, 102_600, 100_000],
                vec![-3_200, 5_200, 0, 0],
                vec![-2_600, 0, 2_600, 0],
            ),
            (
                GameMode::FourPlayerRedHalf,
                vec![94_200, 105_200, 102_600, 100_000],
                vec![-3_200, 5_200, 0, 0],
                vec![-2_600, 0, 2_600, 0],
            ),
        ] {
            let (mut adapter, initial) = EngineAdapter::task8_double_ron(mode).unwrap();
            assert_eq!(adapter.mode, mode);
            assert_eq!(
                initial.iter().map(GameEvent::kind).collect::<Vec<_>>(),
                ["start_game", "start_kyoku", "tsumo"]
            );
            let GameEvent::StartKyoku {
                scores,
                kyotaku,
                honba,
                dora_marker,
                ..
            } = &initial[1]
            else {
                panic!("initial deal")
            };
            assert_eq!(scores, &vec![100_000; mode.seat_count()]);
            assert_eq!((*kyotaku, *honba, dora_marker.id()), (2, 2, 120));
            match &adapter.state {
                GameStateVariant::FourPlayer(s) => {
                    assert_eq!(s.wall.tiles.len(), 83);
                    assert_eq!(
                        &s.wall.tiles[..14],
                        &[
                            135, 134, 133, 132, 120, 130, 129, 128, 127, 126, 125, 124, 123, 122
                        ]
                    );
                    assert_eq!(
                        s.players[1].hand,
                        [36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76]
                    );
                    assert_eq!(
                        s.players[2].hand,
                        [37, 41, 45, 49, 54, 57, 61, 65, 69, 78, 80, 81, 82]
                    );
                    assert_eq!(s.drawn_tile, Some(77));
                }
                GameStateVariant::ThreePlayer(s) => {
                    assert_eq!(s.wall.tiles.len(), 68);
                    assert_eq!(
                        &s.wall.tiles[..18],
                        &[
                            135, 134, 133, 132, 131, 130, 129, 128, 120, 126, 125, 124, 123, 122,
                            121, 127, 119, 118
                        ]
                    );
                    assert_eq!(
                        s.players[1].hand,
                        [36, 40, 44, 48, 53, 56, 60, 64, 68, 72, 73, 74, 76]
                    );
                    assert_eq!(
                        s.players[2].hand,
                        [37, 41, 45, 49, 54, 57, 61, 65, 69, 78, 80, 81, 82]
                    );
                    assert_eq!(s.drawn_tile, Some(77));
                }
            }
            let discard = GameAction::Discard {
                tile: Tile::from_id(77).unwrap(),
                tsumogiri: true,
            };
            assert!(
                adapter
                    .legal_actions(Seat::new(0).unwrap())
                    .unwrap()
                    .contains(&discard)
            );
            let discarded = adapter.apply(Seat::new(0).unwrap(), &discard).unwrap();
            assert!(discarded.ended.is_none());
            let mut offered = Vec::new();
            for seat in Seat::all(mode).into_iter().skip(1) {
                let legal = adapter.legal_actions(seat).unwrap();
                if seat.index() <= 2 {
                    let ron = GameAction::Ron(Seat::new(0).unwrap());
                    assert!(legal.contains(&ron));
                    offered.push((seat, ron));
                } else if !legal.is_empty() {
                    assert!(!legal.iter().any(|a| matches!(a, GameAction::Ron(_))));
                    assert!(legal.contains(&GameAction::Pass));
                    offered.push((seat, GameAction::Pass));
                }
            }
            let outcome = adapter.apply_simultaneous(&offered).unwrap();
            let actual = outcome
                .events
                .iter()
                .filter_map(|e| match e {
                    GameEvent::Hora {
                        actor,
                        target,
                        delta,
                        ..
                    } => Some((actor.index(), target.index(), delta.clone().unwrap())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, [(1, 0, first_delta), (2, 0, second_delta)]);
            let capture = outcome.ended.unwrap();
            let copied = state_logs(&capture.ended_copy)
                .iter()
                .map(|line| parse_event(line, mode, &offered).unwrap())
                .filter_map(|e| match e {
                    GameEvent::Hora {
                        actor,
                        target,
                        delta,
                        ..
                    } => Some((actor.index(), target.index(), delta.unwrap())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(copied, actual);
            assert_eq!(capture.real_after_scores, expected_after);
            assert_eq!(capture.real_after_pot, 0);
            let roster = (0..mode.seat_count())
                .map(|seat| {
                    Participant::new(format!("p{seat}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(
                    &capture,
                    &roster,
                    1,
                    initial.len() + discarded.events.len(),
                    &outcome.events,
                )
                .unwrap();
            assert_eq!(
                record
                    .wins
                    .iter()
                    .map(|w| w.seat.index())
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            assert_eq!(record.settlement.pot_before, 2);
            assert_eq!(record.settlement.pot_after, 0);
            assert_eq!(
                record
                    .settlement
                    .changes
                    .iter()
                    .map(|c| c.after)
                    .collect::<Vec<_>>(),
                expected_after
            );
            for win in &record.wins {
                assert_eq!(
                    win.evaluation,
                    WinEvaluation::Ordinary {
                        han: 2,
                        fu: 40,
                        limit: None
                    }
                );
                assert_eq!(win.win_points, 2_600);
                assert_eq!((win.winning_tile.id(), win.winning_tile_index), (77, 13));
                assert_eq!(win.concealed[13].id(), 77);
                assert_eq!(win.concealed.iter().filter(|t| t.id() / 4 == 19).count(), 2);
                assert!(win.melds.is_empty());
            }
            record.validate(&roster).unwrap();
        }
        assert!(EngineAdapter::task8_double_ron(GameMode::ThreePlayerRedHalf).is_err());
    }

    #[test]
    fn draw_event_delta_adjustment_actual_all_tenpai_paid_reach() {
        use riichienv_core::action::Phase;
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let (mut adapter, _) = EngineAdapter::new(mode, Some(230)).unwrap();
            let count = mode.seat_count();
            macro_rules! prepare {
                ($state:expr) => {{
                    let state = $state;
                    state.mjai_log.clear();
                    state.phase = Phase::WaitResponse;
                    state.active_players = (1..count as u8).collect();
                    state.current_claims.clear();
                    state.current_player = 0;
                    state.last_discard = Some((0, 36));
                    state.drawn_tile = None;
                    state.needs_tsumo = false;
                    state.is_first_turn = false;
                    state.wall.drawable_count = 0;
                    state.riichi_sticks = 1;
                    for (seat, player) in state.players.iter_mut().enumerate() {
                        player.hand = (9..=21).map(|kind| kind * 4 + seat as u8).collect();
                        player.nagashi_eligible = false;
                        player.score = if seat == 0 { 24_000 } else { 25_000 };
                        player.score_delta = if seat == 0 { -1_000 } else { 0 };
                    }
                }};
            }
            match &mut adapter.state {
                GameStateVariant::FourPlayer(state) => prepare!(state),
                GameStateVariant::ThreePlayer(state) => prepare!(state),
            }
            adapter.log_cursor = 0;
            let actions = (1..count as u8)
                .map(|seat| (Seat::new(seat).unwrap(), GameAction::Pass))
                .collect::<Vec<_>>();
            let outcome = adapter.apply_simultaneous(&actions).unwrap();
            let capture = outcome.ended.unwrap();
            let roster = (0..count)
                .map(|seat| {
                    Participant::new(format!("p{seat}"), "same", crate::ParticipantKind::Human)
                })
                .collect::<Vec<_>>();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &outcome.events)
                .unwrap();
            let emitted = outcome
                .events
                .iter()
                .find_map(|event| match event {
                    GameEvent::Ryukyoku { delta, .. } => delta.as_ref(),
                    _ => None,
                })
                .unwrap();
            assert_eq!(emitted[0], -1_000);
            assert!(
                record
                    .settlement
                    .changes
                    .iter()
                    .all(|change| change.delta == 0)
            );
            assert_eq!(
                record.draw_event_delta_adjustment,
                Some([vec![-1_000], vec![0; count - 1]].concat())
            );
            let mut corrupt = EngineKyokuEnd {
                pre_action: capture.pre_action.clone(),
                ended_copy: capture.ended_copy.clone(),
                win_results: capture.win_results.clone(),
                real_after_scores: capture.real_after_scores.clone(),
                real_after_pot: capture.real_after_pot,
            };
            match &mut corrupt.pre_action {
                GameStateVariant::FourPlayer(state) => state.players[0].score = 0,
                GameStateVariant::ThreePlayer(state) => state.players[0].score = 0,
            }
            match &mut corrupt.ended_copy {
                GameStateVariant::FourPlayer(state) => state.players[0].score = i32::MAX,
                GameStateVariant::ThreePlayer(state) => state.players[0].score = i32::MAX,
            }
            assert!(
                adapter
                    .result_record(&corrupt, &roster, 1, 0, &outcome.events)
                    .is_err()
            );
        }
    }

    // The prefix is a controlled fixture (not a recorded deal); every terminal
    // event and result below comes from the pinned engine's real resolution.
    fn draw_fixture(mode: GameMode, scenario: &str) -> serde_json::Value {
        use riichienv_core::action::Phase;
        let (mut adapter, mut prefix) = EngineAdapter::new(mode, Some(230)).unwrap();
        let count = mode.seat_count();
        let final_round = scenario == "final";
        macro_rules! prepare {
            ($state:expr) => {{
                let state = $state;
                state.mjai_log.clear();
                state.phase = Phase::WaitResponse;
                state.active_players = (1..count as u8).collect();
                state.current_claims.clear();
                state.current_player = 0;
                state.last_discard = Some((0, 36));
                state.drawn_tile = None;
                state.needs_tsumo = false;
                state.is_first_turn = false;
                state.wall.drawable_count = 0;
                state.riichi_sticks = 1;
                if final_round {
                    state.kyoku_idx = (count - 1) as u8;
                    state.oya = (count - 1) as u8;
                }
                for (seat, player) in state.players.iter_mut().enumerate() {
                    let tenpai = scenario == "all_tenpai" || (scenario == "mixed" && seat == 0);
                    let kinds: &[u8] = if tenpai {
                        &[9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21]
                    } else {
                        &[9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 27]
                    };
                    player.hand = kinds.iter().map(|kind| kind * 4 + seat as u8).collect();
                    player.nagashi_eligible = scenario == "nagashi" && seat == 1;
                    player.score = if seat == 0 {
                        24_000
                    } else if final_round && seat == 1 {
                        if count == 3 { 40_000 } else { 30_000 }
                    } else {
                        25_000
                    };
                    player.score_delta = if seat == 0 { -1_000 } else { 0 };
                }
            }};
        }
        match &mut adapter.state {
            GameStateVariant::FourPlayer(state) => prepare!(state),
            GameStateVariant::ThreePlayer(state) => prepare!(state),
        }
        adapter.log_cursor = 0;
        let (scores, pot, kyoku, oya) = match &adapter.state {
            GameStateVariant::FourPlayer(state) => (
                state.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                state.riichi_sticks,
                state.kyoku_idx,
                state.oya,
            ),
            GameStateVariant::ThreePlayer(state) => (
                state.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                state.riichi_sticks,
                state.kyoku_idx,
                state.oya,
            ),
        };
        let GameEvent::StartKyoku {
            scores: start_scores,
            kyotaku,
            kyoku: start_kyoku,
            oya: start_oya,
            ..
        } = &mut prefix[1]
        else {
            panic!("missing start");
        };
        *start_scores = scores;
        *kyotaku = pot as u8;
        *start_kyoku = kyoku + 1;
        *start_oya = Seat::new(oya).unwrap();
        // The paid deposit preceded this terminal action, so canonical prefix
        // records its acceptance while the captured pre-action already paid it.
        prefix.push(GameEvent::ReachAccepted {
            actor: Seat::new(0).unwrap(),
        });
        prefix.push(GameEvent::Tsumo {
            actor: Seat::new(1).unwrap(),
            tile: parse_engine_tile(mode, 40).unwrap(),
        });
        prefix.push(GameEvent::Dahai {
            actor: Seat::new(1).unwrap(),
            tile: parse_engine_tile(mode, 40).unwrap(),
            tsumogiri: true,
        });
        // Prefix StartKyoku must precede the earlier, paid ReachAccepted.
        if let GameEvent::StartKyoku {
            scores, kyotaku, ..
        } = &mut prefix[1]
        {
            scores[0] = 25_000;
            *kyotaku = 0;
        }
        let actions = (1..count as u8)
            .map(|seat| (Seat::new(seat).unwrap(), GameAction::Pass))
            .collect::<Vec<_>>();
        let outcome = adapter.apply_simultaneous(&actions).unwrap();
        let capture = outcome.ended.unwrap();
        let end = outcome
            .events
            .iter()
            .position(|event| matches!(event, GameEvent::EndKyoku))
            .unwrap();
        let roster = (0..count)
            .map(|seat| {
                Participant::new(format!("seat{seat}"), "same", crate::ParticipantKind::Human)
            })
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, prefix.len(), &outcome.events)
            .unwrap();
        let original_terminal = outcome
            .events
            .iter()
            .find(|event| matches!(event, GameEvent::Ryukyoku { .. }))
            .unwrap();
        let GameEvent::Ryukyoku {
            delta: Some(delta), ..
        } = original_terminal
        else {
            panic!("missing canonical delta");
        };
        let delta = delta.clone();
        let before = record
            .settlement
            .changes
            .iter()
            .map(|change| change.before)
            .collect::<Vec<_>>();
        let copy = match &capture.ended_copy {
            GameStateVariant::FourPlayer(state) => {
                state.players.iter().map(|p| p.score).collect::<Vec<_>>()
            }
            GameStateVariant::ThreePlayer(state) => {
                state.players.iter().map(|p| p.score).collect::<Vec<_>>()
            }
        };
        let mut events = prefix;
        events.extend(
            outcome
                .events
                .into_iter()
                .take(end + 1 + usize::from(final_round)),
        );
        serde_json::json!({"scenario": scenario, "mode": mode, "events": events, "record": record,
            "captured_before": before, "ended_copy_scores": copy, "actual_after_scores": capture.real_after_scores,
            "raw_draw_delta": delta, "raw_engine_log": state_logs(&capture.ended_copy),
            "actual_after_pot": capture.real_after_pot})
    }

    fn fourth_riichi_fixture() -> serde_json::Value {
        use riichienv_core::action::Phase;
        let mode = GameMode::FourPlayerRedEast;
        let (mut adapter, mut events) = EngineAdapter::new(mode, Some(230)).unwrap();
        let GameStateVariant::FourPlayer(state) = &mut adapter.state else {
            unreachable!()
        };
        state.mjai_log.clear();
        state.phase = Phase::WaitResponse;
        state.active_players = vec![0, 1, 2];
        state.current_claims.clear();
        state.current_player = 3;
        state.last_discard = Some((3, 111));
        state.drawn_tile = None;
        state.needs_tsumo = false;
        state.is_first_turn = false;
        state.turn_count = 11;
        state.riichi_sticks = 3;
        state.riichi_pending_acceptance = Some(3);
        for (seat, player) in state.players.iter_mut().enumerate() {
            player.hand = (9..=21).map(|kind| kind * 4 + seat as u8).collect();
            player.nagashi_eligible = false;
            player.score = if seat == 3 { 25_000 } else { 24_000 };
            player.score_delta = if seat == 3 { 0 } else { -1_000 };
            player.riichi_declared = seat != 3;
        }
        adapter.log_cursor = 0;
        for seat in 0..3 {
            events.push(GameEvent::ReachAccepted {
                actor: Seat::new(seat).unwrap(),
            });
            events.push(GameEvent::Tsumo {
                actor: Seat::new(seat + 1).unwrap(),
                tile: parse_engine_tile(mode, 40).unwrap(),
            });
            events.push(GameEvent::Dahai {
                actor: Seat::new(seat + 1).unwrap(),
                tile: parse_engine_tile(mode, 40).unwrap(),
                tsumogiri: true,
            });
        }
        // The last ReachAccepted is not fabricated; it is emitted by this
        // response resolution, immediately before the real abortive terminal.
        let actions = (0..3)
            .map(|seat| (Seat::new(seat).unwrap(), GameAction::Pass))
            .collect::<Vec<_>>();
        let outcome = adapter.apply_simultaneous(&actions).unwrap();
        let capture = outcome.ended.unwrap();
        let end = outcome
            .events
            .iter()
            .position(|event| matches!(event, GameEvent::EndKyoku))
            .unwrap();
        let roster = (0..4)
            .map(|seat| {
                Participant::new(format!("seat{seat}"), "same", crate::ParticipantKind::Human)
            })
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, events.len(), &outcome.events)
            .unwrap();
        let GameEvent::Ryukyoku {
            delta: Some(delta), ..
        } = outcome
            .events
            .iter()
            .find(|event| matches!(event, GameEvent::Ryukyoku { .. }))
            .unwrap()
        else {
            panic!("missing draw");
        };
        let delta = delta.clone();
        let before = record
            .settlement
            .changes
            .iter()
            .map(|change| change.before)
            .collect::<Vec<_>>();
        let GameStateVariant::FourPlayer(copy) = &capture.ended_copy else {
            unreachable!()
        };
        let copy_scores = copy.players.iter().map(|p| p.score).collect::<Vec<_>>();
        events.extend(outcome.events.into_iter().take(end + 1));
        serde_json::json!({"scenario": "same_resolution_fourth_riichi", "mode": mode, "events": events,
            "record": record, "captured_before": before, "ended_copy_scores": copy_scores,
            "actual_after_scores": capture.real_after_scores, "raw_draw_delta": delta,
            "raw_engine_log": state_logs(&capture.ended_copy), "actual_after_pot": capture.real_after_pot})
    }

    // Only StartGame/StartKyoku and the preceding discard are test-controlled;
    // both Hora, EndKyoku, raw MJAI logs and the settlement are pinned output.
    fn multi_ron_fixture(mode: GameMode) -> serde_json::Value {
        let mut adapter = prepared_capture(mode, true, false);
        let (_, mut prefix) = EngineAdapter::new(mode, Some(230)).unwrap();
        let count = mode.seat_count();
        let GameEvent::StartKyoku {
            scores,
            kyotaku,
            honba,
            tehais,
            ..
        } = &mut prefix[1]
        else {
            panic!("missing fixture start");
        };
        *scores = vec![100_000; count];
        *kyotaku = 2;
        *honba = 2;
        tehais[0][0] = parse_engine_tile(mode, 77).unwrap();
        let GameEvent::Tsumo { actor, tile } = &mut prefix[2] else {
            panic!("missing controlled pre-discard draw");
        };
        *actor = Seat::new(0).unwrap();
        *tile = parse_engine_tile(mode, 77).unwrap();
        prefix.push(GameEvent::Dahai {
            actor: Seat::new(0).unwrap(),
            tile: parse_engine_tile(mode, 77).unwrap(),
            tsumogiri: false,
        });
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
        let end = outcome
            .events
            .iter()
            .position(|event| matches!(event, GameEvent::EndKyoku))
            .unwrap();
        let roster = (0..count)
            .map(|seat| {
                Participant::new(format!("seat{seat}"), "same", crate::ParticipantKind::Human)
            })
            .collect::<Vec<_>>();
        let record = adapter
            .result_record(&capture, &roster, 1, prefix.len(), &outcome.events)
            .unwrap();
        assert_eq!(record.draw_event_delta_adjustment, None);
        assert_eq!(record.wins.len(), 2);
        let terminal_indices = outcome
            .events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                matches!(event, GameEvent::Hora { .. }).then_some(prefix.len() + index)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            record
                .wins
                .iter()
                .map(|win| win.event_index)
                .collect::<Vec<_>>(),
            terminal_indices
        );
        assert!(record.wins[0].event_index < record.wins[1].event_index);
        let captured_before = match &capture.pre_action {
            GameStateVariant::FourPlayer(state) => {
                state.players.iter().map(|p| p.score).collect::<Vec<_>>()
            }
            GameStateVariant::ThreePlayer(state) => {
                state.players.iter().map(|p| p.score).collect::<Vec<_>>()
            }
        };
        let (ended_copy_scores, copy_pot) = match &capture.ended_copy {
            GameStateVariant::FourPlayer(state) => (
                state.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                state.riichi_sticks,
            ),
            GameStateVariant::ThreePlayer(state) => (
                state.players.iter().map(|p| p.score).collect::<Vec<_>>(),
                state.riichi_sticks,
            ),
        };
        assert_eq!(record.settlement.pot_before, 2);
        assert_eq!(record.settlement.pot_after, capture.real_after_pot);
        assert_eq!(ended_copy_scores, capture.real_after_scores);
        assert_eq!(copy_pot, capture.real_after_pot);
        for (seat, change) in record.settlement.changes.iter().enumerate() {
            assert_eq!(change.before, captured_before[seat]);
            assert_eq!(change.after, capture.real_after_scores[seat]);
            assert_eq!(change.before.checked_add(change.delta), Some(change.after));
        }
        let raw_hora_deltas = outcome
            .events
            .iter()
            .filter_map(|event| match event {
                GameEvent::Hora { delta, .. } => delta.clone(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(raw_hora_deltas.len(), 2);
        let include_end_game = matches!(outcome.events.get(end + 1), Some(GameEvent::EndGame));
        let mut events = prefix;
        events.extend(
            outcome
                .events
                .into_iter()
                .take(end + 1 + usize::from(include_end_game)),
        );
        serde_json::json!({"scenario": "ordered_multi_ron", "mode": mode, "events": events,
            "record": record, "captured_before": captured_before,
            "ended_copy_scores": ended_copy_scores, "actual_after_scores": capture.real_after_scores,
            "raw_engine_log": state_logs(&capture.ended_copy), "raw_hora_deltas": raw_hora_deltas,
            "actual_after_pot": capture.real_after_pot})
    }

    fn capture_fixture_with_parsed_logs(mut fixture: serde_json::Value) -> serde_json::Value {
        // JSON key order varies with unified features; retain frozen bytes and event order.
        for case in fixture.as_array_mut().unwrap() {
            for line in case["raw_engine_log"].as_array_mut().unwrap() {
                *line = serde_json::from_str(line.as_str().unwrap()).unwrap();
            }
        }
        fixture
    }

    #[test]
    fn capture_log_comparison_ignores_keys_but_preserves_payload_and_event_order() {
        let capture =
            |logs: &[&str]| serde_json::json!([{"raw_engine_log": logs, "scores": [1000, 2000]}]);
        let original = capture(&[r#"{"type":"hora","actor":1}"#, r#"{"type":"end_kyoku"}"#]);
        let expected = capture_fixture_with_parsed_logs(original.clone());
        assert_eq!(
            expected,
            capture_fixture_with_parsed_logs(capture(&[
                r#"{"actor":1,"type":"hora"}"#,
                r#"{"type":"end_kyoku"}"#,
            ]))
        );
        assert_ne!(
            expected,
            capture_fixture_with_parsed_logs(capture(&[
                r#"{"type":"hora","actor":2}"#,
                r#"{"type":"end_kyoku"}"#,
            ]))
        );
        assert_ne!(
            expected,
            capture_fixture_with_parsed_logs(capture(&[
                r#"{"type":"end_kyoku"}"#,
                r#"{"type":"hora","actor":1}"#,
            ]))
        );
        let mut changed = original;
        changed[0]["scores"][0] = serde_json::json!(999);
        assert_ne!(expected, capture_fixture_with_parsed_logs(changed));
    }

    #[test]
    fn multi_ron_fixture_matches_pinned_engine_capture() {
        // Immutable test-only original terminal payload; normal tests read,
        // never regenerate it. Replay joins these same records and events.
        let frozen: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let cases = [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast]
            .into_iter()
            .map(multi_ron_fixture)
            .collect::<Vec<_>>();
        assert_eq!(
            capture_fixture_with_parsed_logs(serde_json::Value::Array(cases)),
            capture_fixture_with_parsed_logs(frozen)
        );
    }

    #[test]
    fn draw_fixture_matches_pinned_engine_capture() {
        // Frozen once from the pinned EngineAdapter resolution. Never regenerate
        // during normal tests; Replay consumes these same original terminal events.
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/authoritative-draw-captures.json"
        ))
        .unwrap();
        let cases = [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast]
            .into_iter()
            .flat_map(|mode| {
                ["all_tenpai", "mixed", "nagashi", "final"]
                    .map(move |scenario| draw_fixture(mode, scenario))
            })
            .chain(std::iter::once(fourth_riichi_fixture()))
            .collect::<Vec<_>>();
        assert_eq!(
            capture_fixture_with_parsed_logs(serde_json::Value::Array(cases)),
            capture_fixture_with_parsed_logs(expected)
        );
    }

    #[test]
    fn draw_evidence_absent_for_pinned_wins_and_checked_on_capture() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
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
            assert_eq!(record.draw_event_delta_adjustment, None);
            assert!(record.validate(&roster).is_ok());
            let mut corrupt = record;
            corrupt.draw_event_delta_adjustment = Some(vec![0; mode.seat_count()]);
            assert!(corrupt.validate(&roster).is_err());
        }
        let value = draw_fixture(GameMode::FourPlayerRedEast, "all_tenpai");
        let mut corrupt: KyokuResultRecord =
            serde_json::from_value(value["record"].clone()).unwrap();
        corrupt.draw_event_delta_adjustment = None;
        let roster = (0..4)
            .map(|seat| {
                Participant::new(format!("seat{seat}"), "same", crate::ParticipantKind::Human)
            })
            .collect::<Vec<_>>();
        assert!(corrupt.validate(&roster).is_err());
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
            let capture = outcome.ended.unwrap();
            let record = adapter
                .result_record(&capture, &roster, 1, 0, &outcome.events)
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
                let (hand, drawn, discarded) = match &capture.pre_action {
                    GameStateVariant::FourPlayer(s) => (
                        &s.players[win.seat.index() as usize].hand,
                        s.drawn_tile,
                        s.last_discard,
                    ),
                    GameStateVariant::ThreePlayer(s) => (
                        &s.players[win.seat.index() as usize].hand,
                        s.drawn_tile,
                        s.last_discard,
                    ),
                };
                let expected_id = if ron {
                    discarded.unwrap().1
                } else {
                    drawn.unwrap()
                };
                let expected_index = if ron {
                    hand.len()
                } else {
                    hand.iter().position(|&id| id == expected_id).unwrap()
                };
                assert_eq!(expected_id, 77);
                assert_eq!(win.winning_tile.id(), expected_id);
                assert_eq!(win.winning_tile_index, expected_index);
                assert_eq!(win.concealed[expected_index].id(), expected_id);
                assert_eq!(
                    win.concealed
                        .iter()
                        .filter(|tile| tile.id() / 4 == expected_id / 4)
                        .count(),
                    2
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
    fn result_preserves_red_winning_physical_id() {
        let mode = GameMode::FourPlayerRedEast;
        let mut adapter = prepared_capture(mode, false, false);
        let GameStateVariant::FourPlayer(state) = &mut adapter.state else {
            unreachable!()
        };
        // Four complete groups after drawing red 5p; no pre-existing 5p physical ID.
        let kinds = [9, 10, 11, 18, 19, 20, 22, 23, 24, 12, 14, 27, 27];
        let mut copies = [0u8; 34];
        state.players[1].hand = kinds
            .iter()
            .map(|&kind| {
                let id = kind * 4 + copies[kind as usize];
                copies[kind as usize] += 1;
                id
            })
            .collect();
        state.players[1].hand.push(52);
        state.players[1].riichi_declared = true;
        state.drawn_tile = Some(52);
        let roster = (0..4)
            .map(|seat| Participant::new(format!("p{seat}"), "same", crate::ParticipantKind::Human))
            .collect::<Vec<_>>();
        let outcome = adapter
            .apply(Seat::new(1).unwrap(), &GameAction::Tsumo)
            .unwrap();
        let capture = outcome.ended.unwrap();
        let GameStateVariant::FourPlayer(pre) = &capture.pre_action else {
            unreachable!()
        };
        let expected_index = pre.players[1]
            .hand
            .iter()
            .position(|&id| id == pre.drawn_tile.unwrap())
            .unwrap();
        assert_eq!(pre.drawn_tile, Some(52));
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &outcome.events)
            .unwrap();
        let win = &record.wins[0];
        assert_eq!(win.winning_tile.id(), 52);
        assert_eq!(win.winning_tile_index, expected_index);
        assert_eq!(win.concealed[expected_index].id(), 52);
        assert_eq!(
            win.concealed
                .iter()
                .filter(|tile| tile.id() / 4 == 13)
                .count(),
            1
        );
    }

    #[test]
    fn result_counts_extracted_north_for_dora_and_ura() {
        let mode = GameMode::ThreePlayerRedEast;
        let mut adapter = prepared_capture(mode, false, false);
        let GameStateVariant::ThreePlayer(state) = &mut adapter.state else {
            unreachable!()
        };
        state.players[1].kita_tiles.push(120); // North removed from the winning hand.
        state.players[1].riichi_declared = true;
        state.wall.dora_indicators.push(116); // West -> North.
        state.wall.ura_indicator_tiles[0] = 117; // Second West marker -> North ura.
        let roster = (0..3)
            .map(|seat| Participant::new(format!("p{seat}"), "same", crate::ParticipantKind::Human))
            .collect::<Vec<_>>();
        let outcome = adapter
            .apply(Seat::new(1).unwrap(), &GameAction::Tsumo)
            .unwrap();
        let capture = outcome.ended.unwrap();
        let record = adapter
            .result_record(&capture, &roster, 1, 0, &outcome.events)
            .unwrap();
        for id in [31, 33, 34] {
            assert_eq!(
                record.wins[0]
                    .yaku
                    .iter()
                    .find(|y| y.id == id)
                    .and_then(|y| y.han),
                Some(1),
                "bonus ID {id}"
            );
        }
        assert_eq!(
            record.wins[0]
                .concealed
                .iter()
                .filter(|tile| tile.id() / 4 == 30)
                .count(),
            0
        );
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

    #[cfg(feature = "task8-test-support")]
    #[test]
    fn task9_actual_riichi_index_survives_later_discard_in_both_modes() {
        for mode in [GameMode::ThreePlayerRedEast, GameMode::FourPlayerRedEast] {
            let (mut adapter, _) = EngineAdapter::task8_double_ron(mode).unwrap();
            let declarer = Seat::new(1).unwrap();
            let roster = (0..mode.seat_count())
                .map(|seat| {
                    Participant::new(
                        format!("seat-{seat}"),
                        "same",
                        crate::ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            let initial = adapter.table_state(mode, &roster).unwrap();
            assert!(
                initial
                    .players()
                    .iter()
                    .all(|p| p.riichi_discard_index.is_none())
            );
            // The dealer's genuine safe discard gives the existing tenpai seat its first draw.
            adapter
                .apply(
                    Seat::new(0).unwrap(),
                    &GameAction::Discard {
                        tile: Tile::from_id(0).unwrap(),
                        tsumogiri: false,
                    },
                )
                .unwrap();
            for _ in 0..10 {
                if adapter
                    .legal_actions(declarer)
                    .unwrap()
                    .iter()
                    .any(|a| matches!(a, GameAction::RiichiDiscard { .. }))
                {
                    break;
                }
                let seat = Seat::all(mode)
                    .into_iter()
                    .find(|&seat| {
                        adapter
                            .legal_actions(seat)
                            .unwrap()
                            .contains(&GameAction::Pass)
                    })
                    .expect("actual response before first draw");
                adapter.apply(seat, &GameAction::Pass).unwrap();
            }
            let declaration = adapter
                .legal_actions(declarer)
                .unwrap()
                .into_iter()
                .find(|a| matches!(a, GameAction::RiichiDiscard { .. }))
                .expect("actual offered riichi on first draw");
            let outcome = adapter.apply(declarer, &declaration).unwrap();
            assert!(
                matches!(outcome.events.first(), Some(GameEvent::Reach { actor }) if *actor == declarer)
            );
            assert!(outcome.ended.is_none());
            for _ in 0..40 {
                let table = adapter.table_state(mode, &roster).unwrap();
                assert_eq!(table.players()[1].riichi_discard_index, Some(0));
                if table.players()[1].discards.len() == 2 {
                    break;
                }
                let (seat, action) = Seat::all(mode)
                    .into_iter()
                    .find_map(|seat| {
                        let legal = adapter.legal_actions(seat).unwrap();
                        legal
                            .iter()
                            .find(|a| matches!(a, GameAction::Pass))
                            .or_else(|| {
                                legal.iter().find(|a| {
                                    matches!(
                                        a,
                                        GameAction::Discard {
                                            tsumogiri: true,
                                            ..
                                        }
                                    )
                                })
                            })
                            .map(|action| (seat, action.clone()))
                    })
                    .expect("actual offered response or discard");
                assert!(adapter.apply(seat, &action).unwrap().ended.is_none());
            }
            let table = adapter.table_state(mode, &roster).unwrap();
            assert_eq!(
                table.players()[1].discards.len(),
                2,
                "bounded actual second discard"
            );
            for audience in [
                crate::Audience::Public,
                crate::Audience::Player(declarer),
                crate::Audience::ReplayAdmin,
            ] {
                let json =
                    serde_json::to_value(crate::project_table_state(&table, audience)).unwrap();
                assert_eq!(json["players"][1]["riichi_discard_index"], 0);
            }
        }
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
