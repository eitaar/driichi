use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{GameMode, MatchResult, Participant, ParticipantId, Seat, Tile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct YakuValue {
    pub id: u32,
    pub name: String,
    pub han: Option<u32>,
    pub yakuman_multiplier: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreLimit {
    Mangan,
    Haneman,
    Baiman,
    Sanbaiman,
    CountedYakuman,
}

impl ScoreLimit {
    pub(crate) fn ordinary(han: u32, fu: u32) -> Option<Self> {
        match han {
            0..=4 => {
                if (if fu == 25 { 25 } else { fu.div_ceil(10) * 10 })
                    .checked_shl(han + 2)
                    .is_some_and(|v| v > 2000)
                {
                    Some(Self::Mangan)
                } else {
                    None
                }
            }
            5 => Some(Self::Mangan),
            6..=7 => Some(Self::Haneman),
            8..=10 => Some(Self::Baiman),
            11..=12 => Some(Self::Sanbaiman),
            _ => Some(Self::CountedYakuman),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WinEvaluation {
    Ordinary {
        han: u32,
        fu: u32,
        limit: Option<ScoreLimit>,
    },
    Yakuman {
        multiplier: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultMeldKind {
    Chi,
    Pon,
    Daiminkan,
    Ankan,
    Kakan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultMeld {
    pub kind: ResultMeldKind,
    pub tiles: Vec<Tile>,
    pub opened: bool,
    pub from_who: Option<Seat>,
    pub called_tile: Option<Tile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinResult {
    pub event_index: usize,
    pub winner: ParticipantId,
    pub seat: Seat,
    pub target: Seat,
    pub tsumo: bool,
    pub concealed: Vec<Tile>,
    pub melds: Vec<ResultMeld>,
    pub winning_tile: Tile,
    pub winning_tile_index: usize,
    pub yaku: Vec<YakuValue>,
    pub evaluation: WinEvaluation,
    pub win_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreChange {
    pub participant_id: ParticipantId,
    pub seat: Seat,
    pub before: i32,
    pub delta: i32,
    pub after: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KyokuSettlement {
    pub changes: Vec<ScoreChange>,
    pub pot_before: u32,
    pub pot_after: u32,
    pub draw_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KyokuResultRecord {
    pub version: u32,
    pub kyoku_sequence: u64,
    pub mode: GameMode,
    pub end_event_index: usize,
    pub wins: Vec<WinResult>,
    pub settlement: KyokuSettlement,
    // Private v1 producer evidence: emitted draw delta minus the actual pre-award movement.
    pub draw_event_delta_adjustment: Option<Vec<i32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedKyokuResults {
    pub kyoku_sequence: u64,
    pub wins: Vec<WinResult>,
    pub settlement: Option<KyokuSettlement>,
}

impl WinResult {
    fn basic_points(&self, mode: GameMode, dealer: bool) -> Result<u32, String> {
        let (han, fu) = match self.evaluation {
            WinEvaluation::Ordinary { han, fu, limit }
                if han > 0
                    && (fu == 20 || fu == 25 || (30..=250).contains(&fu) && fu % 10 == 0)
                    && (limit.is_none() || limit == ScoreLimit::ordinary(han, fu)) =>
            {
                (han.min(13), fu)
            }
            WinEvaluation::Yakuman { multiplier } if multiplier > 0 => (
                multiplier
                    .checked_mul(13)
                    .ok_or("scoring multiplier overflow")?,
                0,
            ),
            _ => return Err("invalid scoring evaluation".into()),
        };
        Ok(riichienv_core::score::calculate_score(
            u8::try_from(han).map_err(|_| "scoring han overflow")?,
            u8::try_from(fu).map_err(|_| "scoring fu overflow")?,
            dealer,
            self.tsumo,
            0,
            mode.seat_count() as u8,
        )
        .total)
    }

    pub fn validate_payment(&self, mode: GameMode, dealer: Seat) -> Result<(), String> {
        if self.basic_points(mode, self.seat == dealer)? != self.win_points {
            return Err("Win Points contradict canonical dealer and evaluation".into());
        }
        Ok(())
    }
}

impl KyokuResultRecord {
    pub fn validate(&self, roster: &[Participant]) -> Result<(), String> {
        if self.version != 1
            || self.kyoku_sequence == 0
            || roster.len() != self.mode.seat_count()
            || self.settlement.changes.len() != roster.len()
        {
            return Err("invalid result version, sequence or roster size".into());
        }
        let mut ids = HashSet::new();
        for (index, participant) in roster.iter().enumerate() {
            if !ids.insert(&participant.id) || participant.id.as_str().is_empty() {
                return Err("duplicate or empty participant ID".into());
            }
            let change = &self.settlement.changes[index];
            if change.seat.index() as usize != index
                || change.participant_id != participant.id
                || change.before.checked_add(change.delta) != Some(change.after)
            {
                return Err("invalid settlement seat, identity or arithmetic".into());
            }
        }
        if self.wins.is_empty() == self.settlement.draw_reason.is_none() {
            return Err("draw reason and wins disagree".into());
        }
        if self.wins.is_empty() {
            if self.draw_event_delta_adjustment.as_ref().map(Vec::len) != Some(roster.len()) {
                return Err("missing or malformed draw delta evidence".into());
            }
        } else if self.draw_event_delta_adjustment.is_some() {
            return Err("win carries draw delta evidence".into());
        }
        let mut winners = HashSet::new();
        let mut previous = None;
        for win in &self.wins {
            if win.melds.len() > 4 || win.concealed.len() + win.melds.len() * 3 != 14 {
                return Err("invalid completed hand size".into());
            }
            // Restored serde values bypass Tile::from_id; validate every private tile.
            if win
                .concealed
                .iter()
                .chain(win.melds.iter().flat_map(|meld| meld.tiles.iter()))
                .chain(
                    win.melds
                        .iter()
                        .filter_map(|meld| meld.called_tile.as_ref()),
                )
                .chain(std::iter::once(&win.winning_tile))
                .any(|tile| Tile::from_id(tile.id()).is_none() || !tile.is_valid_for(self.mode))
            {
                return Err("result contains an invalid tile for its mode".into());
            }
            if win.melds.iter().any(|meld| {
                meld.from_who
                    .is_some_and(|seat| seat.index() as usize >= roster.len())
            }) {
                return Err("result contains an invalid meld source seat for its mode".into());
            }
            let seat = win.seat.index() as usize;
            if seat >= roster.len()
                || win.winner != roster[seat].id
                || !winners.insert(seat)
                || win.target.index() as usize >= roster.len()
                || win.tsumo != (win.seat == win.target)
                || win.event_index >= self.end_event_index
                || previous.is_some_and(|idx| win.event_index <= idx)
                || win.concealed.get(win.winning_tile_index) != Some(&win.winning_tile)
                || win.win_points == 0
            {
                return Err("invalid winner identity, position or order".into());
            }
            previous = Some(win.event_index);
            let mut physical = HashSet::new();
            if !win
                .concealed
                .iter()
                .chain(win.melds.iter().flat_map(|m| &m.tiles))
                .all(|t| physical.insert(t.id()))
            {
                return Err("duplicate physical tile in completed hand".into());
            }
            for meld in &win.melds {
                let kan = matches!(
                    meld.kind,
                    ResultMeldKind::Daiminkan | ResultMeldKind::Ankan | ResultMeldKind::Kakan
                );
                if meld.tiles.len() != if kan { 4 } else { 3 } {
                    return Err("invalid meld size".into());
                }
                let mut kinds = meld.tiles.iter().map(|t| t.id() / 4).collect::<Vec<_>>();
                kinds.sort_unstable();
                let valid_tiles = if meld.kind == ResultMeldKind::Chi {
                    !self.mode.is_three_player()
                        && kinds[0] < 27
                        && kinds[0] / 9 == kinds[2] / 9
                        && kinds[1] == kinds[0] + 1
                        && kinds[2] == kinds[1] + 1
                } else {
                    kinds.iter().all(|k| *k == kinds[0])
                };
                let valid_source = if meld.kind == ResultMeldKind::Ankan {
                    !meld.opened && meld.from_who.is_none() && meld.called_tile.is_none()
                } else {
                    meld.opened
                        && meld.from_who.is_some_and(|s| s != win.seat)
                        && meld.called_tile.is_some_and(|t| meld.tiles.contains(&t))
                };
                if !valid_tiles || !valid_source {
                    return Err("invalid meld structure or source".into());
                }
            }
            let mut hand = riichienv_core::types::Hand::new(Some(
                win.concealed.iter().map(|t| t.id() / 4).collect(),
            ));
            if !riichienv_core::agari::is_agari(&mut hand) {
                return Err("concealed tiles do not complete a winning hand".into());
            }
            let total = win
                .yaku
                .iter()
                .try_fold(0u32, |total, y| {
                    let value = match (&win.evaluation, y.han, y.yakuman_multiplier) {
                        (WinEvaluation::Ordinary { .. }, Some(value), None) if value > 0 => {
                            Some(value)
                        }
                        (WinEvaluation::Yakuman { .. }, None, Some(value)) if value > 0 => {
                            value.checked_mul(13)
                        }
                        _ => None,
                    }?;
                    total.checked_add(value)
                })
                .ok_or("invalid or overflowing yaku value")?;
            if win.yaku.iter().any(|y| {
                riichienv_core::yaku::get_yaku_by_id(y.id).is_none_or(|known| known.name != y.name)
            }) || total
                != match win.evaluation {
                    WinEvaluation::Ordinary { han, fu, .. } if fu > 0 => han,
                    WinEvaluation::Yakuman { multiplier } if multiplier > 0 => multiplier
                        .checked_mul(13)
                        .ok_or("yakuman multiplier overflow")?,
                    _ => 0,
                }
                || total == 0
            {
                return Err("invalid adjudicated yaku values".into());
            }
            if win.basic_points(self.mode, false)? != win.win_points
                && win.basic_points(self.mode, true)? != win.win_points
            {
                return Err("Win Points contradict evaluation".into());
            }
        }
        Ok(())
    }
}

impl MatchResult {
    pub fn validate(&self) -> Result<(), String> {
        let count = self.mode.seat_count();
        if self.players.len() != count || self.final_scores.len() != count {
            return Err("invalid final result count".into());
        }
        let mut ids = HashSet::new();
        let mut ranks = HashSet::new();
        for (seat, player) in self.players.iter().enumerate() {
            if player.seat.index() as usize != seat
                || !ids.insert(&player.participant_id)
                || player.participant_id.as_str().is_empty()
                || player.display_name.trim().is_empty()
                || player.final_score != self.final_scores[seat]
                || !ranks.insert(player.rank)
                || player.rank == 0
                || player.rank as usize > count
            {
                return Err("invalid final seat, rank or identity".into());
            }
            let rank = 1
                + (0..count)
                    .filter(|&other| {
                        self.final_scores[other] > player.final_score
                            || (self.final_scores[other] == player.final_score && other < seat)
                    })
                    .count();
            if usize::from(player.rank) != rank {
                return Err("incorrect final rank".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MatchPlayerResult, ParticipantKind};

    fn roster() -> Vec<Participant> {
        (0..3)
            .map(|index| {
                Participant::new(
                    format!("id-{index}"),
                    "duplicate name",
                    ParticipantKind::Human,
                )
            })
            .collect()
    }

    #[test]
    fn result_rejects_inconsistent_roster() {
        let participants = roster();
        let changes = participants
            .iter()
            .enumerate()
            .map(|(index, participant)| ScoreChange {
                participant_id: participant.id.clone(),
                seat: Seat::new(index as u8).unwrap(),
                before: if index == 0 { -100 } else { 0 },
                delta: if index == 0 { 100 } else { 0 },
                after: 0,
            })
            .collect();
        let record = KyokuResultRecord {
            version: 1,
            kyoku_sequence: 1,
            mode: GameMode::ThreePlayerRedEast,
            end_event_index: 5,
            wins: vec![],
            draw_event_delta_adjustment: Some(vec![0; 3]),
            settlement: KyokuSettlement {
                changes,
                pot_before: 2,
                pot_after: 0,
                draw_reason: Some("nagashi".into()),
            },
        };
        assert!(record.validate(&participants).is_ok());
        let mut bad = record.clone();
        bad.settlement.changes[1].participant_id = participants[0].id.clone();
        assert!(bad.validate(&participants).is_err());
        bad = record.clone();
        bad.settlement.changes[0].before = i32::MAX;
        assert!(bad.validate(&participants).is_err());
        bad = record.clone();
        bad.settlement.changes.pop();
        assert!(bad.validate(&participants).is_err());
        let mut duplicate = participants.clone();
        duplicate[1].id = duplicate[0].id.clone();
        assert!(record.validate(&duplicate).is_err());
    }

    #[test]
    fn result_requires_exact_draw_evidence_and_forbids_win_evidence() {
        let participants = roster();
        let changes = participants
            .iter()
            .enumerate()
            .map(|(index, participant)| ScoreChange {
                participant_id: participant.id.clone(),
                seat: Seat::new(index as u8).unwrap(),
                before: 25_000,
                delta: 0,
                after: 25_000,
            })
            .collect();
        let mut record = KyokuResultRecord {
            version: 1,
            kyoku_sequence: 1,
            mode: GameMode::ThreePlayerRedEast,
            end_event_index: 5,
            wins: vec![],
            draw_event_delta_adjustment: Some(vec![0; 3]),
            settlement: KyokuSettlement {
                changes,
                pot_before: 0,
                pot_after: 0,
                draw_reason: Some("exhaustive_draw".into()),
            },
        };
        assert!(record.validate(&participants).is_ok());
        let mut missing: serde_json::Value = serde_json::to_value(&record).unwrap();
        missing
            .as_object_mut()
            .unwrap()
            .remove("draw_event_delta_adjustment");
        let missing: KyokuResultRecord = serde_json::from_value(missing).unwrap();
        assert!(missing.validate(&participants).is_err());
        record.draw_event_delta_adjustment = Some(vec![0; 2]);
        assert!(record.validate(&participants).is_err());
        record.draw_event_delta_adjustment = Some(vec![0; 4]);
        assert!(record.validate(&participants).is_err());
        record.draw_event_delta_adjustment = Some(vec![0; 3]);
        record.settlement.changes[0].before = i32::MAX;
        record.settlement.changes[0].delta = 1;
        assert!(record.validate(&participants).is_err());
    }

    #[test]
    fn restored_result_rejects_contradictory_limit_metadata() {
        let captures: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        for capture in captures.as_array().unwrap() {
            let record: KyokuResultRecord =
                serde_json::from_value(capture["record"].clone()).unwrap();
            let roster = record
                .settlement
                .changes
                .iter()
                .map(|change| {
                    Participant::new(
                        change.participant_id.clone(),
                        "seat",
                        ParticipantKind::Human,
                    )
                })
                .collect::<Vec<_>>();
            record.validate(&roster).unwrap();
            assert_eq!(
                record.wins[1].evaluation,
                WinEvaluation::Ordinary {
                    han: 2,
                    fu: 40,
                    limit: None
                }
            );
            assert_eq!(record.wins[1].win_points, 2_600);
            let mut payload = capture["record"].clone();
            payload["wins"][1]["evaluation"]["Ordinary"]["limit"] =
                serde_json::json!("CountedYakuman");
            let corrupt: KyokuResultRecord = serde_json::from_value(payload).unwrap();
            assert!(corrupt.validate(&roster).is_err());
            assert!(
                corrupt.wins[1]
                    .validate_payment(record.mode, Seat::new(0).unwrap())
                    .is_err()
            );
        }
    }

    #[test]
    fn basic_payment_checks_limits_at_pinned_scoring_boundaries() {
        let captures: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let record: KyokuResultRecord =
            serde_json::from_value(captures[0]["record"].clone()).unwrap();
        let mut win = record.wins[1].clone();
        let dealer = Seat::new(0).unwrap();
        for (han, fu, limit, points) in [
            (1, 250, None, 8_000),
            (2, 130, Some(ScoreLimit::Mangan), 8_000),
            (3, 60, None, 7_700),
            (3, 70, Some(ScoreLimit::Mangan), 8_000),
            (4, 25, None, 6_400),
            (4, 30, None, 7_700),
            (4, 40, Some(ScoreLimit::Mangan), 8_000),
            (5, 20, Some(ScoreLimit::Mangan), 8_000),
            (6, 130, Some(ScoreLimit::Haneman), 12_000),
            (7, 30, Some(ScoreLimit::Haneman), 12_000),
            (8, 30, Some(ScoreLimit::Baiman), 16_000),
            (10, 30, Some(ScoreLimit::Baiman), 16_000),
            (11, 30, Some(ScoreLimit::Sanbaiman), 24_000),
            (12, 30, Some(ScoreLimit::Sanbaiman), 24_000),
            (13, 30, Some(ScoreLimit::CountedYakuman), 32_000),
            (26, 30, Some(ScoreLimit::CountedYakuman), 32_000),
        ] {
            win.win_points = points;
            win.evaluation = WinEvaluation::Ordinary { han, fu, limit };
            win.validate_payment(record.mode, dealer).unwrap();
            win.evaluation = WinEvaluation::Ordinary {
                han,
                fu,
                limit: None,
            };
            win.validate_payment(record.mode, dealer).unwrap();
            assert_eq!(
                win.evaluation,
                WinEvaluation::Ordinary {
                    han,
                    fu,
                    limit: None
                }
            );
            for wrong in [
                Some(ScoreLimit::Mangan),
                Some(ScoreLimit::Haneman),
                Some(ScoreLimit::Baiman),
                Some(ScoreLimit::Sanbaiman),
                Some(ScoreLimit::CountedYakuman),
            ] {
                if wrong != limit {
                    win.evaluation = WinEvaluation::Ordinary {
                        han,
                        fu,
                        limit: wrong,
                    };
                    assert!(win.validate_payment(record.mode, dealer).is_err());
                }
            }
        }
    }

    #[test]
    fn basic_payment_accepts_high_fu_from_multiple_closed_kans() {
        let captures: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let record: KyokuResultRecord =
            serde_json::from_value(captures[0]["record"].clone()).unwrap();
        use riichienv_core::{
            types::{Hand, Meld, MeldType},
            yaku::{YakuContext, calculate_yaku},
        };
        let melds = vec![
            Meld::new(MeldType::Ankan, vec![0, 1, 2, 3], false, -1, None),
            Meld::new(MeldType::Ankan, vec![32, 33, 34, 35], false, -1, None),
            Meld::new(MeldType::Ankan, vec![36, 37, 38, 39], false, -1, None),
            Meld::new(MeldType::Pon, vec![84, 85, 86], true, 0, Some(84)),
        ];
        let normalized = melds
            .iter()
            .cloned()
            .map(|mut m| {
                m.tiles.iter_mut().for_each(|id| *id /= 4);
                m.called_tile = m.called_tile.map(|id| id / 4);
                m
            })
            .collect::<Vec<_>>();
        let evaluated = calculate_yaku(
            &Hand::new(Some(vec![10, 10])),
            &normalized,
            &YakuContext {
                is_menzen: false,
                is_tsumo: true,
                seat_wind: 28,
                ..Default::default()
            },
            10,
        );
        assert_eq!(
            (evaluated.han, evaluated.fu, evaluated.yakuman_count),
            (6, 130, 0)
        );
        let mut win = record.wins[0].clone();
        win.target = win.seat;
        win.tsumo = true;
        win.concealed = vec![Tile::from_id(40).unwrap(), Tile::from_id(41).unwrap()];
        win.winning_tile = win.concealed[1];
        win.winning_tile_index = 1;
        win.melds = melds
            .iter()
            .map(|m| ResultMeld {
                kind: if m.meld_type == MeldType::Ankan {
                    ResultMeldKind::Ankan
                } else {
                    ResultMeldKind::Pon
                },
                tiles: m
                    .tiles
                    .iter()
                    .map(|&id| Tile::from_id(id).unwrap())
                    .collect(),
                opened: m.opened,
                from_who: if m.from_who < 0 {
                    None
                } else {
                    Seat::new(m.from_who as u8)
                },
                called_tile: m.called_tile.and_then(Tile::from_id),
            })
            .collect();
        win.yaku = evaluated
            .yaku_ids
            .iter()
            .map(|&id| YakuValue {
                id,
                name: riichienv_core::yaku::get_yaku_by_id(id).unwrap().name,
                han: Some(2),
                yakuman_multiplier: None,
            })
            .collect();
        win.evaluation = WinEvaluation::Ordinary {
            han: 6,
            fu: 130,
            limit: Some(ScoreLimit::Haneman),
        };
        win.win_points = 12_000;
        assert!(
            win.validate_payment(record.mode, Seat::new(0).unwrap())
                .is_ok()
        );
        let roster = record
            .settlement
            .changes
            .iter()
            .map(|change| {
                Participant::new(
                    change.participant_id.clone(),
                    "seat",
                    ParticipantKind::Human,
                )
            })
            .collect::<Vec<_>>();
        let high_fu = KyokuResultRecord {
            wins: vec![win],
            ..record
        };
        high_fu.validate(&roster).unwrap();
    }

    #[test]
    fn final_result_preserves_initial_seat_ties() {
        let participants = roster();
        let scores = vec![30_000, 30_000, -1_000];
        let mut result = MatchResult {
            mode: GameMode::ThreePlayerRedEast,
            players: participants
                .iter()
                .enumerate()
                .map(|(index, participant)| MatchPlayerResult {
                    participant_id: participant.id.clone(),
                    display_name: participant.display_name.clone(),
                    kind: participant.kind,
                    seat: Seat::new(index as u8).unwrap(),
                    final_score: scores[index],
                    rank: index as u8 + 1,
                })
                .collect(),
            final_scores: scores,
        };
        assert_eq!(
            result
                .players
                .iter()
                .map(|player| player.rank)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(result.validate().is_ok());
        result.players[0].rank = 2;
        assert!(result.validate().is_err());
        result.players[0].rank = 1;
        result.players[1].seat = Seat::new(0).unwrap();
        assert!(result.validate().is_err());
        result.players[1].seat = Seat::new(1).unwrap();
        result.final_scores[2] = 0;
        assert!(result.validate().is_err());
    }
}
