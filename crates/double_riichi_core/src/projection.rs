use std::time::Duration;

use serde::Serialize;
use thiserror::Error;
use tokio::time::Instant;

use crate::{
    ActionId, Decision, DecisionAction, DecisionId, DecisionKind, GameAction, GameMode,
    Participant, ParticipantId, ParticipantKind, Seat, Tile,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeldState {
    pub tiles: Vec<Tile>,
    pub opened: bool,
    pub from_who: Option<Seat>,
    pub called_tile: Option<Tile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePlayerState {
    pub seat: Seat,
    pub participant: Participant,
    pub score: i32,
    pub hand: Vec<Tile>,
    pub discards: Vec<Tile>,
    pub melds: Vec<MeldState>,
    pub riichi: bool,
}

impl TablePlayerState {
    pub fn new(seat: Seat, participant: Participant, score: i32, hand: Vec<Tile>) -> Self {
        Self {
            seat,
            participant,
            score,
            hand,
            discards: Vec::new(),
            melds: Vec::new(),
            riichi: false,
        }
    }
}

/// Complete server-side table state. It intentionally does not implement
/// `Serialize`; callers must first choose one of the three audience views.
#[derive(Debug, Clone)]
pub struct TableState {
    mode: GameMode,
    players: Vec<TablePlayerState>,
    dora_indicators: Vec<Tile>,
    decision: Option<Decision>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    #[error("table state has {actual} players but {mode} requires {expected}")]
    InvalidPlayerCount {
        mode: GameMode,
        expected: usize,
        actual: usize,
    },
    #[error("table state contains duplicate or non-contiguous seat {0}")]
    InvalidSeat(Seat),
    #[error("table state contains tile {tile} that is invalid for {mode}")]
    InvalidTile { mode: GameMode, tile: Tile },
}

impl TableState {
    pub fn new(
        mode: GameMode,
        players: Vec<TablePlayerState>,
        dora_indicators: Vec<Tile>,
    ) -> Result<Self, ProjectionError> {
        if players.len() != mode.seat_count() {
            return Err(ProjectionError::InvalidPlayerCount {
                mode,
                expected: mode.seat_count(),
                actual: players.len(),
            });
        }
        for (index, player) in players.iter().enumerate() {
            if player.seat.index() as usize != index {
                return Err(ProjectionError::InvalidSeat(player.seat));
            }
            for tile in player
                .hand
                .iter()
                .chain(player.discards.iter())
                .chain(player.melds.iter().flat_map(|meld| meld.tiles.iter()))
            {
                if !tile.is_valid_for(mode) {
                    return Err(ProjectionError::InvalidTile { mode, tile: *tile });
                }
            }
        }
        if let Some(tile) = dora_indicators
            .iter()
            .copied()
            .find(|tile| !tile.is_valid_for(mode))
        {
            return Err(ProjectionError::InvalidTile { mode, tile });
        }
        Ok(Self {
            mode,
            players,
            dora_indicators,
            decision: None,
        })
    }

    pub fn from_hands(
        mode: GameMode,
        players: Vec<(Seat, Participant, i32)>,
        hands: Vec<Vec<Tile>>,
    ) -> Result<Self, ProjectionError> {
        let table_players = players
            .into_iter()
            .zip(hands)
            .map(|((seat, participant, score), hand)| {
                TablePlayerState::new(seat, participant, score, hand)
            })
            .collect();
        Self::new(mode, table_players, Vec::new())
    }

    pub fn with_decision(mut self, decision: Decision) -> Self {
        self.decision = Some(decision);
        self
    }

    pub fn mode(&self) -> GameMode {
        self.mode
    }

    pub fn players(&self) -> &[TablePlayerState] {
        &self.players
    }

    pub fn dora_indicators(&self) -> &[Tile] {
        &self.dora_indicators
    }

    pub fn decision(&self) -> Option<&Decision> {
        self.decision.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Player(Seat),
    Public,
    ReplayAdmin,
}

impl Audience {
    pub const fn player(seat: Seat) -> Self {
        Self::Player(seat)
    }
}

impl TableState {
    pub fn project(&self, audience: Audience) -> AudienceProjection {
        project_table_state(self, audience)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "audience", rename_all = "snake_case")]
pub enum AudienceProjection {
    Player(PlayerProjection),
    Public(PublicProjection),
    ReplayAdmin(ReplayAdminProjection),
}

impl AudienceProjection {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serialize_projection(self)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerProjection {
    pub viewer_seat: Seat,
    pub mode: GameMode,
    pub players: Vec<VisiblePlayer>,
    pub dora_indicators: Vec<Tile>,
    pub decision: Option<PlayerDecisionProjection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicProjection {
    pub mode: GameMode,
    pub players: Vec<VisiblePlayer>,
    pub dora_indicators: Vec<Tile>,
    pub decision: Option<PublicDecisionProjection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayAdminProjection {
    pub mode: GameMode,
    pub players: Vec<VisiblePlayer>,
    pub dora_indicators: Vec<Tile>,
    pub decision: Option<ReplayDecisionProjection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisiblePlayer {
    pub seat: Seat,
    pub participant_id: ParticipantId,
    pub display_name: String,
    pub kind: ParticipantKind,
    pub score: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hand: Option<Vec<Tile>>,
    pub concealed_count: usize,
    pub discards: Vec<Tile>,
    pub melds: Vec<VisibleMeld>,
    pub riichi: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisibleMeld {
    pub tiles: Vec<Tile>,
    pub opened: bool,
    pub from_who: Option<Seat>,
    pub called_tile: Option<Tile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisibleAction {
    pub action_id: ActionId,
    pub action: GameAction,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerDecisionProjection {
    pub decision_id: DecisionId,
    pub kind: DecisionKind,
    pub actions: Vec<VisibleAction>,
    pub default_action_id: ActionId,
    pub duration_ms: Option<u64>,
    pub remaining_ms: Option<u64>,
    pub watchdog: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicDecisionProjection {
    pub decision_id: DecisionId,
    pub kind: DecisionKind,
    pub duration_ms: Option<u64>,
    pub remaining_ms: Option<u64>,
    pub watchdog: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayDecisionEntry {
    pub seat: Seat,
    pub actions: Vec<VisibleAction>,
    pub default_action_id: ActionId,
    pub submitted_action_id: Option<ActionId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayDecisionProjection {
    pub decision_id: DecisionId,
    pub kind: DecisionKind,
    pub eligible: Vec<Seat>,
    pub entries: Vec<ReplayDecisionEntry>,
    pub duration_ms: Option<u64>,
    pub remaining_ms: Option<u64>,
    pub watchdog: bool,
}

pub fn project_table_state(state: &TableState, audience: Audience) -> AudienceProjection {
    match audience {
        Audience::Player(viewer_seat) => AudienceProjection::Player(PlayerProjection {
            viewer_seat,
            mode: state.mode,
            players: state
                .players
                .iter()
                .map(|player| visible_player(player, HandVisibility::Own(viewer_seat)))
                .collect(),
            dora_indicators: state.dora_indicators.clone(),
            decision: state
                .decision
                .as_ref()
                .and_then(|decision| player_decision(decision, viewer_seat)),
        }),
        Audience::Public => AudienceProjection::Public(PublicProjection {
            mode: state.mode,
            players: state
                .players
                .iter()
                .map(|player| visible_player(player, HandVisibility::None))
                .collect(),
            dora_indicators: state.dora_indicators.clone(),
            decision: state.decision.as_ref().map(public_decision),
        }),
        Audience::ReplayAdmin => AudienceProjection::ReplayAdmin(ReplayAdminProjection {
            mode: state.mode,
            players: state
                .players
                .iter()
                .map(|player| visible_player(player, HandVisibility::All))
                .collect(),
            dora_indicators: state.dora_indicators.clone(),
            decision: state.decision.as_ref().map(replay_decision),
        }),
    }
}

pub fn serialize_projection(projection: &AudienceProjection) -> Result<String, serde_json::Error> {
    serde_json::to_string(projection)
}

#[derive(Clone, Copy)]
enum HandVisibility {
    Own(Seat),
    None,
    All,
}

fn visible_player(player: &TablePlayerState, visibility: HandVisibility) -> VisiblePlayer {
    let hand = match visibility {
        HandVisibility::Own(seat) if seat == player.seat => Some(player.hand.clone()),
        HandVisibility::All => Some(player.hand.clone()),
        HandVisibility::Own(_) | HandVisibility::None => None,
    };
    let reveal_closed_melds = match visibility {
        HandVisibility::Own(seat) => seat == player.seat,
        HandVisibility::None => false,
        HandVisibility::All => true,
    };
    VisiblePlayer {
        seat: player.seat,
        participant_id: player.participant.id.clone(),
        display_name: player.participant.display_name.clone(),
        kind: player.participant.kind,
        score: player.score,
        concealed_count: player.hand.len(),
        hand,
        discards: player.discards.clone(),
        melds: player
            .melds
            .iter()
            .map(|meld| visible_meld(meld, reveal_closed_melds))
            .collect(),
        riichi: player.riichi,
    }
}

fn visible_meld(meld: &MeldState, reveal_closed: bool) -> VisibleMeld {
    let reveal = meld.opened || reveal_closed;
    VisibleMeld {
        tiles: reveal.then(|| meld.tiles.clone()).unwrap_or_default(),
        opened: meld.opened,
        from_who: reveal.then_some(meld.from_who).flatten(),
        called_tile: reveal.then_some(meld.called_tile).flatten(),
    }
}

fn player_decision(decision: &Decision, seat: Seat) -> Option<PlayerDecisionProjection> {
    if decision.actions_for(seat).is_empty() {
        return None;
    }
    Some(PlayerDecisionProjection {
        decision_id: decision.id().clone(),
        kind: decision.kind(),
        actions: decision
            .actions_for(seat)
            .iter()
            .map(visible_action)
            .collect(),
        default_action_id: decision.default_action_id(seat).clone(),
        duration_ms: duration_ms(decision.duration_for(seat)),
        remaining_ms: duration_ms(decision.remaining_for(seat, Instant::now())),
        watchdog: decision.is_watchdog_for(seat),
    })
}

fn public_decision(decision: &Decision) -> PublicDecisionProjection {
    PublicDecisionProjection {
        decision_id: decision.id().clone(),
        kind: decision.kind(),
        duration_ms: duration_ms(decision.duration()),
        remaining_ms: duration_ms(decision.remaining(Instant::now())),
        watchdog: decision.is_watchdog(),
    }
}

fn replay_decision(decision: &Decision) -> ReplayDecisionProjection {
    ReplayDecisionProjection {
        decision_id: decision.id().clone(),
        kind: decision.kind(),
        eligible: decision.eligible().collect(),
        entries: decision
            .entries()
            .iter()
            .map(|entry| ReplayDecisionEntry {
                seat: entry.seat,
                actions: entry.actions.iter().map(visible_action).collect(),
                default_action_id: decision.default_action_id(entry.seat).clone(),
                submitted_action_id: decision.submitted_action_id(entry.seat).cloned(),
            })
            .collect(),
        duration_ms: duration_ms(decision.duration()),
        remaining_ms: duration_ms(decision.remaining(Instant::now())),
        watchdog: decision.is_watchdog(),
    }
}

fn visible_action(action: &DecisionAction) -> VisibleAction {
    VisibleAction {
        action_id: action.id.clone(),
        action: action.action.clone(),
    }
}

fn duration_ms(duration: Option<Duration>) -> Option<u64> {
    duration.map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
}
