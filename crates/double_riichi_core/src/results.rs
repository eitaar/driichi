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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedKyokuResults {
    pub kyoku_sequence: u64,
    pub wins: Vec<WinResult>,
    pub settlement: Option<KyokuSettlement>,
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
        let mut winners = HashSet::new();
        let mut previous = None;
        for win in &self.wins {
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
