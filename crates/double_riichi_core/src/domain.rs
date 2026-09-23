use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameMode {
    FourPlayerRedEast,
    FourPlayerRedHalf,
    ThreePlayerRedEast,
    ThreePlayerRedHalf,
}

impl GameMode {
    pub const FOUR_PLAYER_RED_EAST: Self = Self::FourPlayerRedEast;
    pub const FOUR_PLAYER_RED_HALF: Self = Self::FourPlayerRedHalf;
    pub const THREE_PLAYER_RED_EAST: Self = Self::ThreePlayerRedEast;
    pub const THREE_PLAYER_RED_HALF: Self = Self::ThreePlayerRedHalf;

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FourPlayerRedEast => "4p-red-east",
            Self::FourPlayerRedHalf => "4p-red-half",
            Self::ThreePlayerRedEast => "3p-red-east",
            Self::ThreePlayerRedHalf => "3p-red-half",
        }
    }

    pub const fn seat_count(self) -> usize {
        match self {
            Self::FourPlayerRedEast | Self::FourPlayerRedHalf => 4,
            Self::ThreePlayerRedEast | Self::ThreePlayerRedHalf => 3,
        }
    }

    pub const fn is_three_player(self) -> bool {
        self.seat_count() == 3
    }

    pub const fn all() -> [Self; 4] {
        [
            Self::FourPlayerRedEast,
            Self::FourPlayerRedHalf,
            Self::ThreePlayerRedEast,
            Self::ThreePlayerRedHalf,
        ]
    }
}

impl fmt::Display for GameMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidGameMode;

impl fmt::Display for InvalidGameMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unsupported match mode")
    }
}

impl std::error::Error for InvalidGameMode {}

impl FromStr for GameMode {
    type Err = InvalidGameMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "4p-red-east" => Ok(Self::FourPlayerRedEast),
            "4p-red-half" => Ok(Self::FourPlayerRedHalf),
            "3p-red-east" => Ok(Self::ThreePlayerRedEast),
            "3p-red-half" => Ok(Self::ThreePlayerRedHalf),
            _ => Err(InvalidGameMode),
        }
    }
}

pub type Mode = GameMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Wind {
    East = 0,
    South = 1,
    West = 2,
    North = 3,
}

impl Wind {
    pub const fn from_index(index: u8) -> Self {
        match index % 4 {
            0 => Self::East,
            1 => Self::South,
            2 => Self::West,
            _ => Self::North,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Seat(u8);

impl Seat {
    pub const fn new(index: u8) -> Option<Self> {
        if index < 4 { Some(Self(index)) } else { None }
    }

    pub const fn from_index(index: u8) -> Option<Self> {
        Self::new(index)
    }

    pub const fn index(self) -> u8 {
        self.0
    }

    pub fn all(mode: GameMode) -> Vec<Self> {
        (0..mode.seat_count() as u8).map(Self).collect()
    }
}

impl TryFrom<u8> for Seat {
    type Error = InvalidSeat;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidSeat(value))
    }
}

impl fmt::Display for Seat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidSeat(pub u8);

impl fmt::Display for InvalidSeat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid seat {}", self.0)
    }
}

impl std::error::Error for InvalidSeat {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TileType(u8);

impl TileType {
    pub const ONE_MAN: Self = Self(0);
    pub const TWO_MAN: Self = Self(1);
    pub const THREE_MAN: Self = Self(2);
    pub const FOUR_MAN: Self = Self(3);
    pub const FIVE_MAN: Self = Self(4);
    pub const SIX_MAN: Self = Self(5);
    pub const SEVEN_MAN: Self = Self(6);
    pub const EIGHT_MAN: Self = Self(7);
    pub const NINE_MAN: Self = Self(8);
    pub const ONE_PIN: Self = Self(9);
    pub const TWO_PIN: Self = Self(10);
    pub const THREE_PIN: Self = Self(11);
    pub const FOUR_PIN: Self = Self(12);
    pub const FIVE_PIN: Self = Self(13);
    pub const SIX_PIN: Self = Self(14);
    pub const SEVEN_PIN: Self = Self(15);
    pub const EIGHT_PIN: Self = Self(16);
    pub const NINE_PIN: Self = Self(17);
    pub const ONE_SOU: Self = Self(18);
    pub const TWO_SOU: Self = Self(19);
    pub const THREE_SOU: Self = Self(20);
    pub const FOUR_SOU: Self = Self(21);
    pub const FIVE_SOU: Self = Self(22);
    pub const SIX_SOU: Self = Self(23);
    pub const SEVEN_SOU: Self = Self(24);
    pub const EIGHT_SOU: Self = Self(25);
    pub const NINE_SOU: Self = Self(26);
    pub const EAST: Self = Self(27);
    pub const SOUTH: Self = Self(28);
    pub const WEST: Self = Self(29);
    pub const NORTH: Self = Self(30);
    pub const HAKU: Self = Self(31);
    pub const HATSU: Self = Self(32);
    pub const CHUN: Self = Self(33);

    pub const fn from_index(index: u8) -> Option<Self> {
        if index < 34 { Some(Self(index)) } else { None }
    }

    pub const fn index(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tile(u8);

impl Tile {
    pub const RED_FIVE_MAN: Self = Self(16);
    pub const RED_FIVE_PIN: Self = Self(52);
    pub const RED_FIVE_SOU: Self = Self(88);

    pub const ONE_MAN: TileType = TileType::ONE_MAN;
    pub const TWO_MAN: TileType = TileType::TWO_MAN;
    pub const THREE_MAN: TileType = TileType::THREE_MAN;
    pub const FOUR_MAN: TileType = TileType::FOUR_MAN;
    pub const FIVE_MAN: TileType = TileType::FIVE_MAN;
    pub const SIX_MAN: TileType = TileType::SIX_MAN;
    pub const SEVEN_MAN: TileType = TileType::SEVEN_MAN;
    pub const EIGHT_MAN: TileType = TileType::EIGHT_MAN;
    pub const NINE_MAN: TileType = TileType::NINE_MAN;
    pub const ONE_PIN: TileType = TileType::ONE_PIN;
    pub const TWO_PIN: TileType = TileType::TWO_PIN;
    pub const THREE_PIN: TileType = TileType::THREE_PIN;
    pub const FOUR_PIN: TileType = TileType::FOUR_PIN;
    pub const FIVE_PIN: TileType = TileType::FIVE_PIN;
    pub const SIX_PIN: TileType = TileType::SIX_PIN;
    pub const SEVEN_PIN: TileType = TileType::SEVEN_PIN;
    pub const EIGHT_PIN: TileType = TileType::EIGHT_PIN;
    pub const NINE_PIN: TileType = TileType::NINE_PIN;
    pub const ONE_SOU: TileType = TileType::ONE_SOU;
    pub const TWO_SOU: TileType = TileType::TWO_SOU;
    pub const THREE_SOU: TileType = TileType::THREE_SOU;
    pub const FOUR_SOU: TileType = TileType::FOUR_SOU;
    pub const FIVE_SOU: TileType = TileType::FIVE_SOU;
    pub const SIX_SOU: TileType = TileType::SIX_SOU;
    pub const SEVEN_SOU: TileType = TileType::SEVEN_SOU;
    pub const EIGHT_SOU: TileType = TileType::EIGHT_SOU;
    pub const NINE_SOU: TileType = TileType::NINE_SOU;
    pub const EAST: TileType = TileType::EAST;
    pub const SOUTH: TileType = TileType::SOUTH;
    pub const WEST: TileType = TileType::WEST;
    pub const NORTH: TileType = TileType::NORTH;
    pub const HAKU: TileType = TileType::HAKU;
    pub const HATSU: TileType = TileType::HATSU;
    pub const CHUN: TileType = TileType::CHUN;

    pub const fn from_id(id: u8) -> Option<Self> {
        if id < 136 { Some(Self(id)) } else { None }
    }

    pub const fn id(self) -> u8 {
        self.0
    }

    pub const fn tile_type(self) -> TileType {
        TileType(self.0 / 4)
    }

    pub const fn is_red(self) -> bool {
        matches!(self.0, 16 | 52 | 88)
    }

    pub const fn is_valid_for(self, mode: GameMode) -> bool {
        if !mode.is_three_player() {
            return true;
        }
        let tile_type = self.tile_type().0;
        !(tile_type >= 1 && tile_type <= 7)
    }

    /// Return every physical tile in the stable application order.
    /// Regular copies precede the red copy of a five, matching action fallback order.
    pub fn canonical_order(mode: GameMode) -> Vec<Self> {
        let mut result = Vec::with_capacity(if mode.is_three_player() { 108 } else { 136 });
        for tile_type in 0..34u8 {
            let base = tile_type * 4;
            let red = matches!(base, 16 | 52 | 88);
            let ids = if red {
                [base + 1, base + 2, base + 3, base]
            } else {
                [base, base + 1, base + 2, base + 3]
            };
            for id in ids {
                let tile = Self(id);
                if tile.is_valid_for(mode) {
                    result.push(tile);
                }
            }
        }
        result
    }

    pub fn canonical_key(self) -> (TileType, bool, u8) {
        (self.tile_type(), self.is_red(), self.id())
    }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParticipantId(String);

impl ParticipantId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ParticipantId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ParticipantId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Display for ParticipantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParticipantKind {
    Human,
    MJAI,
    MCP,
    BuiltInBot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub id: ParticipantId,
    pub display_name: String,
    pub kind: ParticipantKind,
}

impl Participant {
    pub fn new(
        id: impl Into<ParticipantId>,
        display_name: impl Into<String>,
        kind: ParticipantKind,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameAction {
    Discard {
        tile: Tile,
        tsumogiri: bool,
    },
    RiichiDiscard {
        tile: Tile,
    },
    Chi {
        target: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Pon {
        target: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Daiminkan {
        target: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Ankan {
        consumed: Vec<Tile>,
    },
    Kakan {
        called: Tile,
        consumed: Vec<Tile>,
    },
    Nuki {
        tile: Tile,
    },
    Tsumo,
    Ron(Seat),
    Pass,
    AbortiveDraw,
}

impl GameAction {
    pub fn discard(tile: Tile, tsumogiri: bool) -> Self {
        Self::Discard { tile, tsumogiri }
    }

    pub fn riichi_discard(tile: Tile) -> Self {
        Self::RiichiDiscard { tile }
    }

    pub fn target_seat(&self) -> Option<Seat> {
        match self {
            Self::Chi { target, .. }
            | Self::Pon { target, .. }
            | Self::Daiminkan { target, .. }
            | Self::Ron(target) => Some(*target),
            _ => None,
        }
    }

    pub(crate) fn canonicalize(self) -> Self {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    StartGame {
        names: Option<Vec<String>>,
        id: Option<String>,
    },
    StartKyoku {
        bakaze: Wind,
        kyoku: u8,
        honba: u8,
        kyotaku: u8,
        oya: Seat,
        scores: Vec<i32>,
        dora_marker: Tile,
        tehais: Vec<Vec<Tile>>,
    },
    Tsumo {
        actor: Seat,
        tile: Tile,
    },
    Dahai {
        actor: Seat,
        tile: Tile,
        tsumogiri: bool,
    },
    Pon {
        actor: Seat,
        target: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Chi {
        actor: Seat,
        target: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Daiminkan {
        actor: Seat,
        target: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Kakan {
        actor: Seat,
        called: Tile,
        consumed: Vec<Tile>,
    },
    Ankan {
        actor: Seat,
        consumed: Vec<Tile>,
    },
    Dora {
        dora_marker: Tile,
    },
    Reach {
        actor: Seat,
    },
    ReachAccepted {
        actor: Seat,
    },
    Hora {
        actor: Seat,
        target: Seat,
        tile: Option<Tile>,
        ura_markers: Option<Vec<Tile>>,
        yaku: Option<Vec<(String, u32)>>,
        fu: Option<u32>,
        han: Option<u32>,
        scores: Option<Vec<i32>>,
        delta: Option<Vec<i32>>,
    },
    Ryukyoku {
        reason: Option<String>,
        tehais: Option<Vec<Vec<Tile>>>,
        delta: Option<Vec<i32>>,
        scores: Option<Vec<i32>>,
    },
    Kita {
        actor: Seat,
    },
    EndKyoku,
    EndGame,
}

impl GameEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::StartGame { .. } => "start_game",
            Self::StartKyoku { .. } => "start_kyoku",
            Self::Tsumo { .. } => "tsumo",
            Self::Dahai { .. } => "dahai",
            Self::Pon { .. } => "pon",
            Self::Chi { .. } => "chi",
            Self::Daiminkan { .. } => "kan",
            Self::Kakan { .. } => "kakan",
            Self::Ankan { .. } => "ankan",
            Self::Dora { .. } => "dora",
            Self::Reach { .. } => "reach",
            Self::ReachAccepted { .. } => "reach_accepted",
            Self::Hora { .. } => "hora",
            Self::Ryukyoku { .. } => "ryukyoku",
            Self::Kita { .. } => "kita",
            Self::EndKyoku => "end_kyoku",
            Self::EndGame => "end_game",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchPlayerResult {
    pub participant_id: ParticipantId,
    pub display_name: String,
    pub kind: ParticipantKind,
    pub seat: Seat,
    pub final_score: i32,
    pub rank: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchResult {
    pub mode: GameMode,
    pub players: Vec<MatchPlayerResult>,
    pub final_scores: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchAbort {
    pub reason: String,
}

impl MatchAbort {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchStatus {
    Running,
    Completed(MatchResult),
    Aborted(MatchAbort),
}
