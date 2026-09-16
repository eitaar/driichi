use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::Arc,
    time::Duration,
};

use rand::random;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    sync::{RwLock, mpsc, oneshot},
    time::{self, Instant},
};

use crate::{
    MatchMachine,
    decision::{ActionId, ControllerState, Decision, DecisionId, Presence, TimeControl},
    domain::{
        GameEvent, GameMode, MatchPlayerResult, MatchResult, Participant, ParticipantId,
        ParticipantKind, Seat,
    },
    match_machine::DecisionResult,
    projection::{Audience, AudienceProjection},
};

pub const ROOM_COMMAND_CAPACITY: usize = 256;
pub const CONNECTION_OUTBOUND_CAPACITY: usize = 64;
pub const ROOM_EFFECT_CAPACITY: usize = 256;
const DEFAULT_MAX_ROOMS: usize = 32;
const DEFAULT_MAX_PARTICIPANTS: usize = 32;
const ROOM_CODE_COOLDOWN: Duration = Duration::from_secs(24 * 60 * 60);
const PERSISTENCE_ACK_TIMEOUT: Duration = Duration::from_secs(1);

fn normalize_name(value: &str) -> Option<String> {
    let value = value.trim_matches(char::is_whitespace);
    (!value.is_empty() && value.chars().count() <= 64).then(|| value.to_owned())
}

fn generate_ulid() -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u128;
    let value = ((millis & ((1u128 << 48) - 1)) << 80) | (random::<u128>() & ((1u128 << 80) - 1));
    let mut result = String::with_capacity(26);
    for shift in (0..26).rev().map(|index| index * 5) {
        result.push(ALPHABET[((value >> shift) & 31) as usize] as char);
    }
    result
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, RoomError> {
                let value = value.into();
                if value.is_empty()
                    || !value
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
                {
                    return Err(RoomError::InvalidIdentifier);
                }
                Ok(Self(value))
            }

            pub fn generate() -> Self {
                Self(generate_ulid())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

id_type!(RoomId);
id_type!(MatchId);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoomJoinCode(String);

impl RoomJoinCode {
    pub fn new(value: impl AsRef<str>) -> Result<Self, RoomError> {
        let value = value.as_ref();
        if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_digit()) || value == "000000"
        {
            return Err(RoomError::InvalidJoinCode);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn generate() -> Self {
        Self(format!("{:06}", 100_000 + random::<u32>() % 900_000))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomJoinCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for RoomJoinCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterUsage {
    Human,
    Mjai,
    Mcp,
    BuiltInBot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterCatalog {
    entries: BTreeMap<String, CharacterUsage>,
}

impl CharacterCatalog {
    pub fn starter() -> Self {
        let mut catalog = Self {
            entries: BTreeMap::new(),
        };
        catalog.insert("player-red", CharacterUsage::Human);
        catalog.insert("player-blue", CharacterUsage::Human);
        catalog.insert("mjai-bot", CharacterUsage::Mjai);
        catalog.insert("mcp-bot", CharacterUsage::Mcp);
        catalog.insert("tsumogiri-bot", CharacterUsage::BuiltInBot);
        catalog
    }

    pub fn insert(&mut self, id: impl Into<String>, usage: CharacterUsage) {
        self.entries.insert(id.into(), usage);
    }

    pub fn usage(&self, id: &str) -> Option<CharacterUsage> {
        self.entries.get(id).copied()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    fn default_for(&self, kind: ParticipantKind) -> Option<String> {
        let usage = match kind {
            ParticipantKind::Human => CharacterUsage::Human,
            ParticipantKind::MJAI => CharacterUsage::Mjai,
            ParticipantKind::MCP => CharacterUsage::Mcp,
            ParticipantKind::BuiltInBot => CharacterUsage::BuiltInBot,
        };
        self.entries
            .iter()
            .find_map(|(id, candidate)| (*candidate == usage).then(|| id.clone()))
    }

    fn valid_for(&self, id: &str, kind: ParticipantKind) -> bool {
        let expected = match kind {
            ParticipantKind::Human => CharacterUsage::Human,
            ParticipantKind::MJAI => CharacterUsage::Mjai,
            ParticipantKind::MCP => CharacterUsage::Mcp,
            ParticipantKind::BuiltInBot => CharacterUsage::BuiltInBot,
        };
        self.usage(id) == Some(expected)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomConfig {
    pub room_name: String,
    pub mode: GameMode,
    pub character_catalog: CharacterCatalog,
    pub time_control: TimeControl,
    pub replay_save: bool,
    pub max_participants: usize,
    pub disconnected_participant_expiry: Duration,
    pub empty_room_cleanup: Duration,
}

impl RoomConfig {
    pub fn new(
        room_name: impl Into<String>,
        mode: GameMode,
        character_catalog: CharacterCatalog,
    ) -> Self {
        Self {
            room_name: normalize_name(&room_name.into()).unwrap_or_else(|| "Room".to_owned()),
            mode,
            character_catalog,
            time_control: TimeControl::Casual,
            replay_save: true,
            max_participants: DEFAULT_MAX_PARTICIPANTS,
            disconnected_participant_expiry: Duration::from_secs(10 * 60),
            empty_room_cleanup: Duration::from_secs(30 * 60),
        }
    }

    pub fn with_time_control(mut self, time_control: TimeControl) -> Self {
        self.time_control = time_control;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermanentAutoReason {
    LeftDuringMatch,
    AgentLeft,
    TokenRevoked,
    ConnectionLost,
    BuiltInBot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomController {
    Interactive,
    TemporaryAuto,
    PermanentAuto(PermanentAutoReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchRole {
    None,
    Player(Seat),
    Spectator,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomPhase {
    Lobby,
    Playing(MatchId),
    PostMatch(MatchId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomParticipantSnapshot {
    pub id: ParticipantId,
    pub display_name: String,
    pub kind: ParticipantKind,
    pub character_id: String,
    pub presence: Presence,
    pub selected: bool,
    pub ready: bool,
    pub role: MatchRole,
    pub controller: RoomController,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchPlayerSnapshot {
    pub participant_id: ParticipantId,
    pub display_name: String,
    pub kind: ParticipantKind,
    pub seat: Seat,
    pub character_id: Option<String>,
    pub controller: RoomController,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub id: RoomId,
    pub join_code: RoomJoinCode,
    pub room_name: String,
    pub created_at: i64,
    pub participant_limit: usize,
    pub mode: GameMode,
    pub time_control: TimeControl,
    pub replay_save: bool,
    pub phase: RoomPhase,
    pub participants: Vec<RoomParticipantSnapshot>,
    pub match_players: Vec<MatchPlayerSnapshot>,
    pub roster: Vec<MatchPlayerSnapshot>,
    pub result: Option<MatchResult>,
    pub revision: u64,
    pub persistence_degraded: bool,
    pub replay_available: bool,
}

#[derive(Clone, Debug)]
pub enum RoomEvent {
    Snapshot(RoomSnapshot),
    ParticipantJoined(RoomParticipantSnapshot),
    ParticipantLeft(ParticipantId),
    SelectionChanged,
    PhaseChanged(RoomPhase),
    MatchStarted(MatchId),
    DecisionOpened {
        match_id: MatchId,
        decision: Decision,
    },
    ActionResolved {
        match_id: MatchId,
        result: DecisionResult,
    },
    MatchCompleted {
        match_id: MatchId,
        result: MatchResult,
    },
    MatchAborted {
        match_id: MatchId,
        reason: String,
    },
    StorageDegraded,
    RoomDeleted,
    ServerShutdown,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomEffectError {
    #[error("persistence failed: {0}")]
    Failed(String),
    #[error("persistence worker closed")]
    Closed,
}

#[derive(Debug)]
pub enum RoomEffect {
    OpenMatch {
        match_id: MatchId,
        mode: GameMode,
        roster: Vec<MatchPlayerSnapshot>,
        initial_events: Vec<GameEvent>,
        completion: oneshot::Sender<Result<(), RoomEffectError>>,
    },
    AppendEvents {
        match_id: MatchId,
        events: Vec<GameEvent>,
    },
    FlushKyoku {
        match_id: MatchId,
        completion: oneshot::Sender<Result<(), RoomEffectError>>,
    },
    FinalizeMatch {
        match_id: MatchId,
        result: MatchResult,
        completion: oneshot::Sender<Result<(), RoomEffectError>>,
    },
    DeleteIncomplete {
        match_id: MatchId,
    },
}

impl RoomEffect {
    pub fn acknowledge(self, result: Result<(), RoomEffectError>) {
        match self {
            Self::OpenMatch { completion, .. }
            | Self::FlushKyoku { completion, .. }
            | Self::FinalizeMatch { completion, .. } => {
                let _ = completion.send(result);
            }
            Self::AppendEvents { .. } | Self::DeleteIncomplete { .. } => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownMode {
    Graceful,
    Forced,
}

#[derive(Clone, Debug)]
pub enum RoomCommand {
    Join {
        participant: Participant,
        character_id: Option<String>,
        token_id: Option<String>,
    },
    Select {
        participant_id: ParticipantId,
        character_id: Option<String>,
    },
    Deselect {
        participant_id: ParticipantId,
    },
    FillWithBots,
    SetMode {
        mode: GameMode,
    },
    SetRoomName {
        room_name: String,
    },
    SetTimeControl {
        time_control: TimeControl,
    },
    SetReplaySave {
        enabled: bool,
    },
    SetMaxParticipants {
        max_participants: usize,
    },
    Configure {
        room_name: Option<String>,
        mode: Option<GameMode>,
        time_control: Option<TimeControl>,
        replay_save: Option<bool>,
        max_participants: Option<usize>,
    },
    SetReady {
        participant_id: ParticipantId,
        preloaded_characters: Vec<String>,
    },
    Disconnect {
        participant_id: ParticipantId,
    },
    Reconnect {
        participant_id: ParticipantId,
    },
    PersistenceFailed,
    PersistenceCompleted {
        success: bool,
    },
    Tick,
    Leave {
        participant_id: ParticipantId,
    },
    RevokeToken {
        token_id: String,
    },
    BackToLobby,
    Delete,
    Start,
    SubmitAction {
        participant_id: ParticipantId,
        decision_id: DecisionId,
        action_id: ActionId,
    },
    Rematch,
    Shutdown {
        mode: ShutdownMode,
    },
    GetSnapshot,
    GetProjection {
        participant_id: ParticipantId,
    },
}

impl RoomCommand {
    pub fn join(participant: Participant) -> Self {
        Self::Join {
            participant,
            character_id: None,
            token_id: None,
        }
    }

    pub fn join_with_character(participant: Participant, character_id: impl Into<String>) -> Self {
        Self::Join {
            participant,
            character_id: Some(character_id.into()),
            token_id: None,
        }
    }

    pub fn join_with_token(participant: Participant, token_id: impl Into<String>) -> Self {
        Self::Join {
            participant,
            character_id: None,
            token_id: Some(token_id.into()),
        }
    }

    pub fn select(participant_id: impl Into<ParticipantId>) -> Self {
        Self::Select {
            participant_id: participant_id.into(),
            character_id: None,
        }
    }

    pub fn select_with_character(
        participant_id: impl Into<ParticipantId>,
        character_id: impl Into<String>,
    ) -> Self {
        Self::Select {
            participant_id: participant_id.into(),
            character_id: Some(character_id.into()),
        }
    }

    pub fn deselect(participant_id: impl Into<ParticipantId>) -> Self {
        Self::Deselect {
            participant_id: participant_id.into(),
        }
    }

    pub fn fill_with_bots() -> Self {
        Self::FillWithBots
    }

    pub fn set_mode(mode: GameMode) -> Self {
        Self::SetMode { mode }
    }

    pub fn set_room_name(room_name: impl Into<String>) -> Self {
        Self::SetRoomName {
            room_name: room_name.into(),
        }
    }

    pub fn set_time_control(time_control: TimeControl) -> Self {
        Self::SetTimeControl { time_control }
    }

    pub fn set_replay_save(enabled: bool) -> Self {
        Self::SetReplaySave { enabled }
    }

    pub fn set_max_participants(max_participants: usize) -> Self {
        Self::SetMaxParticipants { max_participants }
    }

    pub fn configure(
        room_name: Option<String>,
        mode: Option<GameMode>,
        time_control: Option<TimeControl>,
        replay_save: Option<bool>,
        max_participants: Option<usize>,
    ) -> Self {
        Self::Configure {
            room_name,
            mode,
            time_control,
            replay_save,
            max_participants,
        }
    }

    pub fn set_ready(
        participant_id: impl Into<ParticipantId>,
        preloaded_characters: Vec<String>,
    ) -> Self {
        Self::SetReady {
            participant_id: participant_id.into(),
            preloaded_characters,
        }
    }

    pub fn disconnect(participant_id: impl Into<ParticipantId>) -> Self {
        Self::Disconnect {
            participant_id: participant_id.into(),
        }
    }

    pub fn reconnect(participant_id: impl Into<ParticipantId>) -> Self {
        Self::Reconnect {
            participant_id: participant_id.into(),
        }
    }

    pub fn leave(participant_id: impl Into<ParticipantId>) -> Self {
        Self::Leave {
            participant_id: participant_id.into(),
        }
    }

    pub fn revoke_token(token_id: impl Into<String>) -> Self {
        Self::RevokeToken {
            token_id: token_id.into(),
        }
    }

    pub fn submit_action(
        participant_id: impl Into<ParticipantId>,
        decision_id: impl Into<DecisionId>,
        action_id: impl Into<ActionId>,
    ) -> Self {
        Self::SubmitAction {
            participant_id: participant_id.into(),
            decision_id: decision_id.into(),
            action_id: action_id.into(),
        }
    }

    pub fn shutdown(mode: ShutdownMode) -> Self {
        Self::Shutdown { mode }
    }

    pub fn start() -> Self {
        Self::Start
    }

    pub fn rematch() -> Self {
        Self::Rematch
    }

    pub fn back_to_lobby() -> Self {
        Self::BackToLobby
    }
}

#[derive(Debug)]
pub enum RoomResponse {
    Joined(RoomParticipantSnapshot),
    Accepted(RoomSnapshot),
    Projection(Option<AudienceProjection>),
    Started(MatchId),
    Action(DecisionResult),
    Deleted,
    Shutdown,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomError {
    #[error("room command queue is full")]
    Busy,
    #[error("room actor is closed")]
    Closed,
    #[error("room was deleted")]
    Deleted,
    #[error("room is shutting down")]
    ShuttingDown,
    #[error("invalid room identifier")]
    InvalidIdentifier,
    #[error("invalid room join code")]
    InvalidJoinCode,
    #[error("invalid room name")]
    InvalidRoomName,
    #[error("room is full")]
    RoomFull,
    #[error("invalid participant limit")]
    InvalidParticipantLimit,
    #[error("participant already exists")]
    DuplicateParticipant,
    #[error("participant not found: {0}")]
    ParticipantNotFound(ParticipantId),
    #[error("participant is disconnected")]
    Disconnected,
    #[error("participant is not selected")]
    NotSelected,
    #[error("participant is already selected")]
    AlreadySelected,
    #[error("no available seat")]
    NoSeat,
    #[error("participant kind cannot use character")]
    InvalidCharacter,
    #[error("character preload is incomplete")]
    PreloadIncomplete,
    #[error("only selected Humans may set Ready")]
    ReadyNotAllowed,
    #[error("participant is not ready")]
    NotReady,
    #[error("room is not in Lobby")]
    NotLobby,
    #[error("room is already Playing")]
    Playing,
    #[error("room is not Playing")]
    NotPlaying,
    #[error("room is not eligible for deletion while Playing")]
    DeleteWhilePlaying,
    #[error("room is not eligible for a Match")]
    NotEnoughPlayers,
    #[error("rematch is unavailable")]
    RematchUnavailable,
    #[error("controller is not interactive")]
    ControllerNotInteractive,
    #[error("persistence worker failed")]
    Persistence,
    #[error("Match failed: {0}")]
    Match(String),
}

struct Envelope {
    request: ActorRequest,
}

enum ActorRequest {
    Command {
        command: RoomCommand,
        reply: oneshot::Sender<Result<RoomResponse, RoomError>>,
    },
    Subscribe {
        reply: oneshot::Sender<Result<RoomConnection, RoomError>>,
    },
}

pub struct RoomConnection {
    pub id: u64,
    receiver: mpsc::Receiver<RoomEvent>,
}

impl RoomConnection {
    pub async fn recv(&mut self) -> Option<RoomEvent> {
        self.receiver.recv().await
    }

    pub fn try_recv(&mut self) -> Result<RoomEvent, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

#[derive(Clone, Debug)]
pub struct RoomHandle {
    sender: mpsc::Sender<Envelope>,
    id: RoomId,
    join_code: RoomJoinCode,
}

impl RoomHandle {
    pub fn id(&self) -> &RoomId {
        &self.id
    }

    pub fn join_code(&self) -> &RoomJoinCode {
        &self.join_code
    }

    pub fn try_send(
        &self,
        command: RoomCommand,
    ) -> Result<oneshot::Receiver<Result<RoomResponse, RoomError>>, RoomError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(Envelope {
                request: ActorRequest::Command { command, reply },
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RoomError::Busy,
                mpsc::error::TrySendError::Closed(_) => RoomError::Closed,
            })?;
        Ok(receiver)
    }

    pub async fn send(&self, command: RoomCommand) -> Result<RoomResponse, RoomError> {
        let receiver = self.try_send(command)?;
        receiver.await.map_err(|_| RoomError::Closed)?
    }

    pub async fn snapshot(&self) -> Result<RoomSnapshot, RoomError> {
        match self.send(RoomCommand::GetSnapshot).await? {
            RoomResponse::Accepted(snapshot) => Ok(snapshot),
            _ => Err(RoomError::Closed),
        }
    }

    pub async fn projection(
        &self,
        participant_id: impl Into<ParticipantId>,
    ) -> Result<Option<AudienceProjection>, RoomError> {
        match self
            .send(RoomCommand::GetProjection {
                participant_id: participant_id.into(),
            })
            .await?
        {
            RoomResponse::Projection(projection) => Ok(projection),
            _ => Err(RoomError::Closed),
        }
    }

    pub async fn subscribe(&self) -> Result<RoomConnection, RoomError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(Envelope {
                request: ActorRequest::Subscribe { reply },
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RoomError::Busy,
                mpsc::error::TrySendError::Closed(_) => RoomError::Closed,
            })?;
        receiver.await.map_err(|_| RoomError::Closed)?
    }
}

struct ParticipantState {
    participant: Participant,
    character_id: String,
    token_id: Option<String>,
    presence: Presence,
    selected: bool,
    ready: bool,
    role: MatchRole,
    controller: RoomController,
    disconnected_at: Option<Instant>,
    remove_after_match: bool,
}

impl ParticipantState {
    fn snapshot(&self) -> RoomParticipantSnapshot {
        RoomParticipantSnapshot {
            id: self.participant.id.clone(),
            display_name: self.participant.display_name.clone(),
            kind: self.participant.kind,
            character_id: self.character_id.clone(),
            presence: self.presence,
            selected: self.selected,
            ready: self.ready,
            role: self.role,
            controller: self.controller,
        }
    }
}

pub struct RoomState {
    pub id: RoomId,
    pub join_code: RoomJoinCode,
    pub config: RoomConfig,
    phase: RoomPhase,
    participants: HashMap<ParticipantId, ParticipantState>,
    match_roster: Vec<MatchPlayerSnapshot>,
    match_machine: Option<MatchMachine>,
    result: Option<MatchResult>,
    revision: u64,
    deleted: bool,
    shutting_down: bool,
    persistence_degraded: bool,
    replay_available: bool,
    empty_since: Option<Instant>,
    created_at: i64,
}

impl fmt::Debug for RoomState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoomState")
            .field("id", &self.id)
            .field("join_code", &self.join_code)
            .field("phase", &self.phase)
            .field("participants", &self.participants.len())
            .field("revision", &self.revision)
            .finish()
    }
}

impl RoomState {
    pub fn new(config: RoomConfig) -> Result<Self, RoomError> {
        Self::with_ids(RoomId::generate(), RoomJoinCode::generate(), config)
    }

    pub fn with_ids(
        id: RoomId,
        join_code: RoomJoinCode,
        config: RoomConfig,
    ) -> Result<Self, RoomError> {
        if config.max_participants == 0 || normalize_name(&config.room_name).is_none() {
            return Err(RoomError::InvalidRoomName);
        }
        Ok(Self {
            id,
            join_code,
            phase: RoomPhase::Lobby,
            participants: HashMap::new(),
            match_roster: Vec::new(),
            match_machine: None,
            result: None,
            revision: 0,
            deleted: false,
            shutting_down: false,
            persistence_degraded: false,
            replay_available: config.replay_save,
            empty_since: Some(Instant::now()),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            config,
        })
    }

    pub fn snapshot(&self) -> RoomSnapshot {
        let roster = self.match_roster.clone();
        RoomSnapshot {
            id: self.id.clone(),
            join_code: self.join_code.clone(),
            room_name: self.config.room_name.clone(),
            created_at: self.created_at,
            participant_limit: self.config.max_participants,
            mode: self.config.mode,
            time_control: self.config.time_control,
            replay_save: self.config.replay_save,
            phase: self.phase.clone(),
            participants: {
                let mut participants: Vec<_> = self
                    .participants
                    .values()
                    .map(ParticipantState::snapshot)
                    .collect();
                participants.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
                participants
            },
            match_players: roster.clone(),
            roster,
            result: self.result.clone(),
            revision: self.revision,
            persistence_degraded: self.persistence_degraded,
            replay_available: self.replay_available,
        }
    }

    pub fn phase(&self) -> &RoomPhase {
        &self.phase
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    fn projection_for(
        &mut self,
        participant_id: &ParticipantId,
    ) -> Result<Option<AudienceProjection>, RoomError> {
        let participant = self
            .participants
            .get(participant_id)
            .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
        if matches!(participant.controller, RoomController::PermanentAuto(_)) {
            return Err(RoomError::ControllerNotInteractive);
        }
        let audience = match participant.role {
            MatchRole::Player(seat) => Audience::Player(seat),
            MatchRole::None | MatchRole::Spectator => Audience::Public,
        };
        let Some(machine) = self.match_machine.as_mut() else {
            return Ok(None);
        };
        machine
            .project(audience)
            .map(Some)
            .map_err(|error| RoomError::Match(error.to_string()))
    }

    fn add_participant(
        &mut self,
        participant: Participant,
        character_id: Option<String>,
        token_id: Option<String>,
        _now: Instant,
    ) -> Result<RoomParticipantSnapshot, RoomError> {
        if self.participants.contains_key(&participant.id) {
            return Err(RoomError::DuplicateParticipant);
        }
        if self.participants.len() >= self.config.max_participants {
            return Err(RoomError::RoomFull);
        }
        let character_id = character_id
            .or_else(|| self.config.character_catalog.default_for(participant.kind))
            .ok_or(RoomError::InvalidCharacter)?;
        if !self
            .config
            .character_catalog
            .valid_for(&character_id, participant.kind)
        {
            return Err(RoomError::InvalidCharacter);
        }
        let role = if matches!(self.phase, RoomPhase::Playing(_) | RoomPhase::PostMatch(_)) {
            MatchRole::Spectator
        } else {
            MatchRole::None
        };
        let participant_id = participant.id.clone();
        self.participants.insert(
            participant_id.clone(),
            ParticipantState {
                participant,
                character_id,
                token_id,
                presence: Presence::Connected,
                selected: false,
                ready: false,
                role,
                controller: RoomController::Interactive,
                disconnected_at: None,
                remove_after_match: false,
            },
        );
        self.empty_since = None;
        self.bump_revision();
        Ok(self
            .participants
            .get(&participant_id)
            .expect("inserted participant")
            .snapshot())
    }

    fn select(
        &mut self,
        participant_id: &ParticipantId,
        character_id: Option<String>,
    ) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby) {
            return Err(RoomError::NotLobby);
        }
        let participant = self
            .participants
            .get(participant_id)
            .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
        let kind = participant.participant.kind;
        if participant.presence != Presence::Connected && kind != ParticipantKind::BuiltInBot {
            return Err(RoomError::Disconnected);
        }
        if participant.selected {
            return Err(RoomError::AlreadySelected);
        }
        if self.config.mode.is_three_player() && kind == ParticipantKind::MJAI {
            return Err(RoomError::InvalidCharacter);
        }
        if self
            .participants
            .values()
            .filter(|state| state.selected)
            .count()
            >= self.config.mode.seat_count()
        {
            return Err(RoomError::NoSeat);
        }
        let participant = self
            .participants
            .get_mut(participant_id)
            .expect("participant checked above");
        if let Some(character_id) = character_id {
            if !self.config.character_catalog.valid_for(&character_id, kind) {
                return Err(RoomError::InvalidCharacter);
            }
            participant.character_id = character_id;
        }
        participant.selected = true;
        self.clear_selected_human_ready();
        self.bump_revision();
        Ok(())
    }

    fn deselect(&mut self, participant_id: &ParticipantId) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby) {
            return Err(RoomError::NotLobby);
        }
        let participant = self
            .participants
            .get_mut(participant_id)
            .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
        participant.selected = false;
        participant.ready = false;
        participant.role = MatchRole::None;
        self.clear_selected_human_ready();
        self.bump_revision();
        Ok(())
    }

    fn clear_selected_human_ready(&mut self) {
        for participant in self.participants.values_mut() {
            if participant.selected && participant.participant.kind == ParticipantKind::Human {
                participant.ready = false;
            }
        }
    }

    fn fill_with_bots(&mut self) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby) {
            return Err(RoomError::NotLobby);
        }
        let needed = self.config.mode.seat_count().saturating_sub(
            self.participants
                .values()
                .filter(|state| state.selected)
                .count(),
        );
        if needed == 0 {
            return Ok(());
        }
        let existing: Vec<_> = self
            .participants
            .iter()
            .filter(|(_, state)| {
                state.participant.kind == ParticipantKind::BuiltInBot && !state.selected
            })
            .map(|(id, _)| id.clone())
            .take(needed)
            .collect();
        let required_new = needed.saturating_sub(existing.len());
        if self.participants.len().saturating_add(required_new) > self.config.max_participants {
            return Err(RoomError::RoomFull);
        }
        for id in existing {
            if let Some(participant) = self.participants.get_mut(&id) {
                participant.selected = true;
            }
        }
        let selected = self
            .participants
            .values()
            .filter(|state| state.selected)
            .count();
        let remaining = self.config.mode.seat_count().saturating_sub(selected);
        for index in 0..remaining {
            let id = ParticipantId::new(format!(
                "builtin-bot-{}",
                self.revision.saturating_add(index as u64 + 1)
            ));
            let mut suffix = index;
            let mut unique = id.clone();
            while self.participants.contains_key(&unique) {
                suffix = suffix.saturating_add(1);
                unique = ParticipantId::new(format!("builtin-bot-{}-{}", self.revision, suffix));
            }
            let character_id = self
                .config
                .character_catalog
                .default_for(ParticipantKind::BuiltInBot)
                .ok_or(RoomError::InvalidCharacter)?;
            self.participants.insert(
                unique.clone(),
                ParticipantState {
                    participant: Participant::new(
                        unique.clone(),
                        format!("Bot {}", index + 1),
                        ParticipantKind::BuiltInBot,
                    ),
                    character_id,
                    token_id: None,
                    presence: Presence::Connected,
                    selected: true,
                    ready: true,
                    role: MatchRole::None,
                    controller: RoomController::PermanentAuto(PermanentAutoReason::BuiltInBot),
                    disconnected_at: None,
                    remove_after_match: false,
                },
            );
        }
        self.clear_selected_human_ready();
        self.bump_revision();
        Ok(())
    }

    fn set_mode(&mut self, mode: GameMode) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby) {
            return Err(RoomError::NotLobby);
        }
        if mode.is_three_player()
            && self
                .participants
                .values()
                .any(|participant| participant.participant.kind == ParticipantKind::MJAI)
        {
            return Err(RoomError::InvalidCharacter);
        }
        self.config.mode = mode;
        for participant in self.participants.values_mut() {
            participant.selected = false;
            participant.ready = false;
            participant.role = MatchRole::None;
        }
        self.bump_revision();
        Ok(())
    }

    fn configure(
        &mut self,
        room_name: Option<String>,
        mode: Option<GameMode>,
        time_control: Option<TimeControl>,
        replay_save: Option<bool>,
        max_participants: Option<usize>,
    ) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby) {
            return Err(RoomError::NotLobby);
        }
        let target_name = room_name
            .as_deref()
            .map(|value| normalize_name(value).ok_or(RoomError::InvalidRoomName))
            .transpose()?;
        let target_mode = mode.unwrap_or(self.config.mode);
        if target_mode.is_three_player()
            && self
                .participants
                .values()
                .any(|participant| participant.participant.kind == ParticipantKind::MJAI)
        {
            return Err(RoomError::InvalidCharacter);
        }
        let target_limit = max_participants.unwrap_or(self.config.max_participants);
        if !(target_mode.seat_count()..=DEFAULT_MAX_PARTICIPANTS).contains(&target_limit)
            || self.participants.len() > target_limit
        {
            return Err(RoomError::InvalidParticipantLimit);
        }
        let mode_changed = target_mode != self.config.mode;
        if let Some(name) = target_name {
            self.config.room_name = name;
        }
        self.config.mode = target_mode;
        self.config.time_control = time_control.unwrap_or(self.config.time_control);
        if let Some(enabled) = replay_save {
            self.config.replay_save = enabled;
            self.replay_available = enabled;
        }
        self.config.max_participants = target_limit;
        if mode_changed {
            for participant in self.participants.values_mut() {
                participant.selected = false;
                participant.ready = false;
                participant.role = MatchRole::None;
            }
        }
        self.bump_revision();
        Ok(())
    }

    fn set_ready(
        &mut self,
        participant_id: &ParticipantId,
        preloaded_characters: &[String],
    ) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby | RoomPhase::PostMatch(_)) {
            return Err(RoomError::NotLobby);
        }
        let participant = self
            .participants
            .get(participant_id)
            .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
        if participant.participant.kind != ParticipantKind::Human {
            return Err(RoomError::ReadyNotAllowed);
        }
        if !participant.selected {
            return Err(RoomError::NotSelected);
        }
        if participant.presence != Presence::Connected {
            return Err(RoomError::Disconnected);
        }
        let preloaded: HashSet<&str> = preloaded_characters.iter().map(String::as_str).collect();
        let complete = self
            .participants
            .values()
            .filter(|state| state.selected)
            .all(|state| preloaded.contains(state.character_id.as_str()));
        if !complete {
            return Err(RoomError::PreloadIncomplete);
        }
        self.participants
            .get_mut(participant_id)
            .expect("participant checked above")
            .ready = true;
        self.bump_revision();
        Ok(())
    }

    fn disconnected_cleanup(&mut self, now: Instant) -> Vec<ParticipantId> {
        let expired: Vec<_> = self
            .participants
            .iter()
            .filter_map(|(id, participant)| {
                let disconnected_at = participant.disconnected_at?;
                if participant.presence != Presence::Disconnected
                    || now.duration_since(disconnected_at)
                        < self.config.disconnected_participant_expiry
                {
                    return None;
                }
                let lobby_selection_expired =
                    matches!(self.phase, RoomPhase::Lobby) && participant.selected;
                let removable = (lobby_selection_expired || !participant.selected)
                    && !matches!(participant.role, MatchRole::Player(_))
                    && participant.participant.kind != ParticipantKind::BuiltInBot;
                removable.then(|| id.clone())
            })
            .collect();
        for id in &expired {
            self.participants.remove(id);
        }
        if !expired.is_empty() {
            self.bump_revision();
        }
        self.refresh_empty_since(now);
        expired
    }

    fn refresh_empty_since(&mut self, now: Instant) {
        let external_connected = self.participants.values().any(|participant| {
            matches!(
                participant.participant.kind,
                ParticipantKind::Human | ParticipantKind::MJAI | ParticipantKind::MCP
            ) && participant.presence == Presence::Connected
        });
        if external_connected || matches!(self.phase, RoomPhase::Playing(_)) {
            self.empty_since = None;
        } else if self.empty_since.is_none() {
            self.empty_since = Some(now);
        }
    }

    fn is_empty_expired(&mut self, now: Instant) -> bool {
        self.refresh_empty_since(now);
        self.empty_since.is_some_and(|since| {
            !matches!(self.phase, RoomPhase::Playing(_))
                && now.duration_since(since) >= self.config.empty_room_cleanup
        })
    }

    fn build_match(&self) -> Result<(MatchId, MatchMachine, Vec<MatchPlayerSnapshot>), RoomError> {
        if !matches!(self.phase, RoomPhase::Lobby | RoomPhase::PostMatch(_)) {
            return Err(RoomError::NotLobby);
        }
        let selected: Vec<_> = if matches!(self.phase, RoomPhase::PostMatch(_)) {
            self.match_roster
                .iter()
                .filter_map(|entry| self.participants.get(&entry.participant_id))
                .collect()
        } else {
            self.participants
                .values()
                .filter(|state| state.selected)
                .collect()
        };
        if selected.len() != self.config.mode.seat_count() {
            return Err(RoomError::NotEnoughPlayers);
        }
        for participant in &selected {
            if participant.presence != Presence::Connected {
                return Err(RoomError::Disconnected);
            }
            let ready = match participant.participant.kind {
                ParticipantKind::Human => participant.ready,
                ParticipantKind::MJAI | ParticipantKind::MCP => true,
                ParticipantKind::BuiltInBot => true,
            };
            if !ready {
                return Err(RoomError::NotReady);
            }
        }
        let participants: Vec<_> = selected
            .iter()
            .map(|state| state.participant.clone())
            .collect();
        let all_bots = selected
            .iter()
            .all(|participant| participant.participant.kind == ParticipantKind::BuiltInBot);
        let mut machine = MatchMachine::with_time_control(
            self.config.mode,
            participants,
            self.config.time_control,
        )
        .map_err(|error| RoomError::Match(error.to_string()))?;
        if all_bots {
            machine.set_time_control(TimeControl::Unlimited);
        }
        let roster = machine
            .players()
            .iter()
            .enumerate()
            .map(|(index, participant)| {
                let seat = Seat::new(index as u8).expect("mode seat count is bounded");
                let state = self
                    .participants
                    .get(&participant.id)
                    .expect("machine players came from selected roster");
                MatchPlayerSnapshot {
                    participant_id: participant.id.clone(),
                    display_name: participant.display_name.clone(),
                    kind: participant.kind,
                    seat,
                    character_id: Some(state.character_id.clone()),
                    controller: if participant.kind == ParticipantKind::BuiltInBot {
                        RoomController::PermanentAuto(PermanentAutoReason::BuiltInBot)
                    } else {
                        RoomController::Interactive
                    },
                }
            })
            .collect();
        Ok((MatchId::generate(), machine, roster))
    }

    fn commit_match(
        &mut self,
        match_id: MatchId,
        machine: MatchMachine,
        roster: Vec<MatchPlayerSnapshot>,
    ) {
        self.phase = RoomPhase::Playing(match_id);
        self.match_machine = Some(machine);
        self.match_roster = roster.clone();
        self.result = None;
        for entry in roster {
            if let Some(participant) = self.participants.get_mut(&entry.participant_id) {
                participant.selected = true;
                participant.ready = false;
                participant.role = MatchRole::Player(entry.seat);
                participant.controller = entry.controller;
                participant.remove_after_match = false;
            }
        }
        self.empty_since = None;
        self.bump_revision();
    }

    fn finish_match(&mut self, result: MatchResult) -> Option<MatchId> {
        let match_id = match self.phase.clone() {
            RoomPhase::Playing(match_id) => match_id,
            _ => return None,
        };
        self.phase = RoomPhase::PostMatch(match_id.clone());
        self.match_machine = None;
        self.result = Some(result);
        let remove: Vec<_> = self
            .participants
            .iter()
            .filter_map(|(id, participant)| participant.remove_after_match.then(|| id.clone()))
            .collect();
        for id in remove {
            self.participants.remove(&id);
        }
        self.refresh_empty_since(Instant::now());
        self.bump_revision();
        Some(match_id)
    }

    fn can_rematch(&self) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::PostMatch(_)) {
            return Err(RoomError::RematchUnavailable);
        }
        if self.match_roster.len() != self.config.mode.seat_count() {
            return Err(RoomError::RematchUnavailable);
        }
        for entry in &self.match_roster {
            let participant = self
                .participants
                .get(&entry.participant_id)
                .ok_or(RoomError::RematchUnavailable)?;
            if participant.remove_after_match
                || matches!(
                    participant.controller,
                    RoomController::PermanentAuto(reason)
                        if reason != PermanentAutoReason::BuiltInBot
                )
                || participant.presence != Presence::Connected
            {
                return Err(RoomError::RematchUnavailable);
            }
            if participant.participant.kind == ParticipantKind::Human && !participant.ready {
                return Err(RoomError::NotReady);
            }
        }
        Ok(())
    }

    fn back_to_lobby(&mut self) -> Result<(), RoomError> {
        if !matches!(self.phase, RoomPhase::PostMatch(_)) {
            return Err(RoomError::NotPlaying);
        }
        self.phase = RoomPhase::Lobby;
        self.match_machine = None;
        self.match_roster.clear();
        self.result = None;
        for participant in self.participants.values_mut() {
            participant.selected = false;
            participant.ready = false;
            participant.role = MatchRole::None;
            participant.controller = RoomController::Interactive;
            participant.remove_after_match = false;
        }
        self.refresh_empty_since(Instant::now());
        self.bump_revision();
        Ok(())
    }

    fn seat_for(&self, participant_id: &ParticipantId) -> Result<Seat, RoomError> {
        self.participants
            .get(participant_id)
            .and_then(|participant| match participant.role {
                MatchRole::Player(seat) => Some(seat),
                MatchRole::None | MatchRole::Spectator => None,
            })
            .ok_or(RoomError::NotPlaying)
    }

    fn update_roster_controller(
        &mut self,
        participant_id: &ParticipantId,
        controller: RoomController,
    ) {
        if let Some(entry) = self
            .match_roster
            .iter_mut()
            .find(|entry| &entry.participant_id == participant_id)
        {
            entry.controller = controller;
        }
    }

    fn apply_simple(
        &mut self,
        command: RoomCommand,
        now: Instant,
    ) -> Result<RoomResponse, RoomError> {
        if self.deleted {
            return Err(RoomError::Deleted);
        }
        if self.shutting_down {
            return Err(RoomError::Closed);
        }
        let response = match command {
            RoomCommand::Join {
                participant,
                character_id,
                token_id,
            } => RoomResponse::Joined(self.add_participant(
                participant,
                character_id,
                token_id,
                now,
            )?),
            RoomCommand::Select {
                participant_id,
                character_id,
            } => {
                self.select(&participant_id, character_id)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Deselect { participant_id } => {
                self.deselect(&participant_id)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::FillWithBots => {
                self.fill_with_bots()?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::SetMode { mode } => {
                self.set_mode(mode)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::SetRoomName { room_name } => {
                if !matches!(self.phase, RoomPhase::Lobby) {
                    return Err(RoomError::NotLobby);
                }
                self.config.room_name =
                    normalize_name(&room_name).ok_or(RoomError::InvalidRoomName)?;
                self.bump_revision();
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::SetTimeControl { time_control } => {
                if !matches!(self.phase, RoomPhase::Lobby) {
                    return Err(RoomError::NotLobby);
                }
                self.config.time_control = time_control;
                self.bump_revision();
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::SetReplaySave { enabled } => {
                if !matches!(self.phase, RoomPhase::Lobby) {
                    return Err(RoomError::NotLobby);
                }
                self.config.replay_save = enabled;
                self.replay_available = enabled;
                self.bump_revision();
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::SetMaxParticipants { max_participants } => {
                self.configure(None, None, None, None, Some(max_participants))?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Configure {
                room_name,
                mode,
                time_control,
                replay_save,
                max_participants,
            } => {
                self.configure(room_name, mode, time_control, replay_save, max_participants)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::SetReady {
                participant_id,
                preloaded_characters,
            } => {
                self.set_ready(&participant_id, &preloaded_characters)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Disconnect { participant_id } => {
                let seat = self.seat_for(&participant_id).ok();
                let participant = self
                    .participants
                    .get_mut(&participant_id)
                    .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
                participant.presence = Presence::Disconnected;
                participant.disconnected_at = Some(now);
                participant.ready = false;
                if let (Some(machine), Some(seat)) = (self.match_machine.as_mut(), seat) {
                    machine
                        .disconnect(seat)
                        .map_err(|error| RoomError::Match(error.to_string()))?;
                }
                self.refresh_empty_since(now);
                self.bump_revision();
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Reconnect { participant_id } => {
                let seat = self.seat_for(&participant_id).ok();
                let participant = self
                    .participants
                    .get_mut(&participant_id)
                    .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
                if matches!(participant.controller, RoomController::PermanentAuto(_)) {
                    return Err(RoomError::ControllerNotInteractive);
                }
                participant.presence = Presence::Connected;
                participant.disconnected_at = None;
                if matches!(self.phase, RoomPhase::Lobby) {
                    participant.ready = false;
                }
                if let (Some(machine), Some(seat)) = (self.match_machine.as_mut(), seat) {
                    machine
                        .reconnect(seat)
                        .map_err(|error| RoomError::Match(error.to_string()))?;
                    participant.controller = RoomController::Interactive;
                } else {
                    participant.controller = RoomController::Interactive;
                }
                self.update_roster_controller(&participant_id, RoomController::Interactive);
                self.empty_since = None;
                self.bump_revision();
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::PersistenceFailed
            | RoomCommand::PersistenceCompleted { success: false } => {
                self.persistence_degraded = true;
                self.replay_available = false;
                self.bump_revision();
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::PersistenceCompleted { success: true } => {
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Tick => {
                self.disconnected_cleanup(now);
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Leave { participant_id } => {
                self.leave(&participant_id)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::RevokeToken { token_id } => {
                self.revoke_token(&token_id)?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::BackToLobby => {
                self.back_to_lobby()?;
                RoomResponse::Accepted(self.snapshot())
            }
            RoomCommand::Delete => {
                if matches!(self.phase, RoomPhase::Playing(_)) {
                    return Err(RoomError::DeleteWhilePlaying);
                }
                self.deleted = true;
                self.bump_revision();
                RoomResponse::Deleted
            }
            RoomCommand::Shutdown { .. }
            | RoomCommand::Start
            | RoomCommand::SubmitAction { .. }
            | RoomCommand::Rematch
            | RoomCommand::GetProjection { .. } => {
                return Err(RoomError::Match("actor-only command".into()));
            }
            RoomCommand::GetSnapshot => RoomResponse::Accepted(self.snapshot()),
        };
        Ok(response)
    }

    fn leave(&mut self, participant_id: &ParticipantId) -> Result<(), RoomError> {
        let role = self
            .participants
            .get(participant_id)
            .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?
            .role;
        match role {
            MatchRole::Player(seat) if matches!(self.phase, RoomPhase::Playing(_)) => {
                if let Some(participant) = self.participants.get_mut(participant_id) {
                    participant.remove_after_match = true;
                    participant.presence = Presence::Disconnected;
                    participant.controller =
                        RoomController::PermanentAuto(PermanentAutoReason::LeftDuringMatch);
                }
                if let Some(machine) = &mut self.match_machine {
                    machine
                        .set_permanent_auto(seat)
                        .map_err(|error| RoomError::Match(error.to_string()))?;
                }
                self.update_roster_controller(
                    participant_id,
                    RoomController::PermanentAuto(PermanentAutoReason::LeftDuringMatch),
                );
                self.bump_revision();
                Ok(())
            }
            MatchRole::Player(_) | MatchRole::Spectator | MatchRole::None => {
                self.participants.remove(participant_id);
                self.bump_revision();
                Ok(())
            }
        }
    }

    fn revoke_token(&mut self, token_id: &str) -> Result<(), RoomError> {
        let affected: Vec<_> = self
            .participants
            .iter()
            .filter(|(_, participant)| participant.token_id.as_deref() == Some(token_id))
            .map(|(id, participant)| (id.clone(), participant.role))
            .collect();
        for (id, role) in affected.iter().cloned() {
            match role {
                MatchRole::Player(seat) if matches!(self.phase, RoomPhase::Playing(_)) => {
                    if let Some(participant) = self.participants.get_mut(&id) {
                        participant.presence = Presence::Disconnected;
                        participant.controller =
                            RoomController::PermanentAuto(PermanentAutoReason::TokenRevoked);
                        participant.remove_after_match = true;
                    }
                    if let Some(machine) = &mut self.match_machine {
                        machine
                            .set_permanent_auto(seat)
                            .map_err(|error| RoomError::Match(error.to_string()))?;
                    }
                    self.update_roster_controller(
                        &id,
                        RoomController::PermanentAuto(PermanentAutoReason::TokenRevoked),
                    );
                }
                _ => {
                    self.participants.remove(&id);
                }
            }
        }
        if !affected.is_empty() {
            self.bump_revision();
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Actor {
    state: RoomState,
    commands: mpsc::Sender<Envelope>,
    receiver: mpsc::Receiver<Envelope>,
    effects: mpsc::Sender<RoomEffect>,
    subscriptions: HashMap<u64, mpsc::Sender<RoomEvent>>,
    next_subscription: u64,
}

pub struct RoomActor;

impl RoomActor {
    pub fn spawn(config: RoomConfig) -> RoomHandle {
        let (handle, mut effects) = Self::spawn_with_effect_channel(config);
        tokio::spawn(async move {
            while let Some(effect) = effects.recv().await {
                effect.acknowledge(Ok(()));
            }
        });
        handle
    }

    pub fn spawn_with_effect_channel(
        config: RoomConfig,
    ) -> (RoomHandle, mpsc::Receiver<RoomEffect>) {
        let state = RoomState::new(config).expect("RoomConfig must be valid");
        Self::spawn_state_with_effect_channel(state)
    }

    pub fn spawn_with_state(state: RoomState) -> RoomHandle {
        let (effects, mut receiver) = mpsc::channel(ROOM_EFFECT_CAPACITY);
        let handle = Self::spawn_actor(state, effects);
        tokio::spawn(async move {
            while let Some(effect) = receiver.recv().await {
                effect.acknowledge(Ok(()));
            }
        });
        handle
    }

    pub fn spawn_state_with_effect_channel(
        state: RoomState,
    ) -> (RoomHandle, mpsc::Receiver<RoomEffect>) {
        let (effects, receiver) = mpsc::channel(ROOM_EFFECT_CAPACITY);
        let handle = Self::spawn_actor(state, effects);
        (handle, receiver)
    }

    pub fn spawn_with_effect_sender(
        config: RoomConfig,
        effects: mpsc::Sender<RoomEffect>,
    ) -> RoomHandle {
        let state = RoomState::new(config).expect("RoomConfig must be valid");
        let (sender, receiver) = mpsc::channel(ROOM_COMMAND_CAPACITY);
        let handle = RoomHandle {
            sender: sender.clone(),
            id: state.id.clone(),
            join_code: state.join_code.clone(),
        };
        tokio::spawn(
            Actor {
                state,
                commands: sender,
                receiver,
                effects,
                subscriptions: HashMap::new(),
                next_subscription: 1,
            }
            .run(),
        );
        handle
    }

    fn spawn_actor(state: RoomState, effects: mpsc::Sender<RoomEffect>) -> RoomHandle {
        let (sender, receiver) = mpsc::channel(ROOM_COMMAND_CAPACITY);
        let handle = RoomHandle {
            sender: sender.clone(),
            id: state.id.clone(),
            join_code: state.join_code.clone(),
        };
        tokio::spawn(
            Actor {
                state,
                commands: sender,
                receiver,
                effects,
                subscriptions: HashMap::new(),
                next_subscription: 1,
            }
            .run(),
        );
        handle
    }
}

impl Actor {
    async fn run(mut self) {
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        loop {
            if self.state.deleted {
                break;
            }
            let wake = self.next_wake();
            tokio::select! {
                envelope = self.receiver.recv() => {
                    let Some(envelope) = envelope else { break };
                    match envelope.request {
                        ActorRequest::Command { command, reply } => {
                            let result = self.process(command).await;
                            let should_stop = self.state.deleted || self.state.shutting_down;
                            let _ = reply.send(result);
                            if should_stop { break; }
                        }
                        ActorRequest::Subscribe { reply } => {
                            let (sender, receiver) = mpsc::channel(CONNECTION_OUTBOUND_CAPACITY);
                            let id = self.next_subscription;
                            self.next_subscription = self.next_subscription.saturating_add(1);
                            self.subscriptions.insert(id, sender.clone());
                            let _ = sender.try_send(RoomEvent::Snapshot(self.state.snapshot()));
                            let _ = reply.send(Ok(RoomConnection { id, receiver }));
                        }
                    }
                }
                _ = time::sleep_until(wake) => {
                    let _ = self.process(RoomCommand::Tick).await;
                }
            }
        }
        self.subscriptions.clear();
    }

    fn next_wake(&mut self) -> Instant {
        let now = Instant::now();
        let mut wake = now + Duration::from_secs(1);
        if let Some(machine) = self.state.match_machine.as_mut() {
            if let Ok(Some(decision)) = machine.current_decision() {
                for seat in decision.eligible() {
                    if let Some(deadline) = decision.deadline_for(seat) {
                        wake = wake.min(deadline);
                    }
                }
            }
        }
        for participant in self.state.participants.values() {
            if let Some(at) = participant.disconnected_at {
                wake = wake.min(at + self.state.config.disconnected_participant_expiry);
            }
        }
        if let Some(since) = self.state.empty_since {
            wake = wake.min(since + self.state.config.empty_room_cleanup);
        }
        wake.max(now)
    }

    async fn process(&mut self, command: RoomCommand) -> Result<RoomResponse, RoomError> {
        let now = Instant::now();
        match command {
            RoomCommand::GetSnapshot => Ok(RoomResponse::Accepted(self.state.snapshot())),
            RoomCommand::GetProjection { participant_id } => self
                .state
                .projection_for(&participant_id)
                .map(RoomResponse::Projection),
            RoomCommand::Start => self.start_match().await,
            RoomCommand::SubmitAction {
                participant_id,
                decision_id,
                action_id,
            } => {
                self.submit_action(participant_id, decision_id, action_id)
                    .await
            }
            RoomCommand::Rematch => self.rematch().await,
            RoomCommand::Shutdown { mode } => self.shutdown(mode).await,
            RoomCommand::Tick => self.tick(now).await,
            command => {
                let response = self.state.apply_simple(command.clone(), now)?;
                self.emit_for_command(&command);
                if self.state.is_empty_expired(now) {
                    self.state.deleted = true;
                    self.publish(RoomEvent::RoomDeleted);
                }
                Ok(response)
            }
        }
    }

    fn emit_for_command(&mut self, command: &RoomCommand) {
        match command {
            RoomCommand::Join { participant, .. } => {
                if let Some(joined) = self.state.participants.get(&participant.id) {
                    self.publish(RoomEvent::ParticipantJoined(joined.snapshot()));
                }
            }
            RoomCommand::Select { .. }
            | RoomCommand::Deselect { .. }
            | RoomCommand::FillWithBots
            | RoomCommand::SetMode { .. } => self.publish(RoomEvent::SelectionChanged),
            RoomCommand::Leave { participant_id } => {
                self.publish(RoomEvent::ParticipantLeft(participant_id.clone()));
            }
            RoomCommand::Delete => self.publish(RoomEvent::RoomDeleted),
            RoomCommand::PersistenceFailed
            | RoomCommand::PersistenceCompleted { success: false } => {
                self.publish(RoomEvent::StorageDegraded)
            }
            _ => {}
        }
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
    }

    async fn start_match(&mut self) -> Result<RoomResponse, RoomError> {
        if !matches!(self.state.phase, RoomPhase::Lobby) {
            return Err(RoomError::NotLobby);
        }
        let (match_id, machine, roster) = self.state.build_match()?;
        if self.state.config.replay_save {
            let initial_events = machine.events().to_vec();
            let (effect, completion) =
                self.open_effect(&match_id, machine.mode(), &roster, initial_events);
            if self.send_ack_effect(effect, completion).await.is_err() {
                self.state.persistence_degraded = true;
                self.state.replay_available = false;
                self.publish(RoomEvent::StorageDegraded);
            }
        }
        self.state.commit_match(match_id.clone(), machine, roster);
        self.publish(RoomEvent::PhaseChanged(self.state.phase.clone()));
        self.publish(RoomEvent::MatchStarted(match_id.clone()));
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        self.advance_match().await?;
        Ok(RoomResponse::Started(match_id))
    }

    fn open_effect(
        &self,
        match_id: &MatchId,
        mode: GameMode,
        roster: &[MatchPlayerSnapshot],
        initial_events: Vec<GameEvent>,
    ) -> (RoomEffect, oneshot::Receiver<Result<(), RoomEffectError>>) {
        let (completion, receiver) = oneshot::channel();
        (
            RoomEffect::OpenMatch {
                match_id: match_id.clone(),
                mode,
                roster: roster.to_vec(),
                initial_events,
                completion,
            },
            receiver,
        )
    }

    async fn send_ack_effect(
        &mut self,
        effect: RoomEffect,
        completion: oneshot::Receiver<Result<(), RoomEffectError>>,
    ) -> Result<(), RoomError> {
        self.effects
            .try_send(effect)
            .map_err(|_| RoomError::Persistence)?;
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let success = matches!(
                time::timeout(PERSISTENCE_ACK_TIMEOUT, completion).await,
                Ok(Ok(Ok(())))
            );
            let (reply, _receiver) = oneshot::channel();
            if commands
                .send(Envelope {
                    request: ActorRequest::Command {
                        command: RoomCommand::PersistenceCompleted { success },
                        reply,
                    },
                })
                .await
                .is_err()
            {
                return;
            }
        });
        Ok(())
    }

    async fn submit_action(
        &mut self,
        participant_id: ParticipantId,
        decision_id: DecisionId,
        action_id: ActionId,
    ) -> Result<RoomResponse, RoomError> {
        let seat = self.state.seat_for(&participant_id)?;
        let participant = self
            .state
            .participants
            .get(&participant_id)
            .ok_or_else(|| RoomError::ParticipantNotFound(participant_id.clone()))?;
        if participant.presence != Presence::Connected {
            return Err(RoomError::Disconnected);
        }
        if participant.controller != RoomController::Interactive {
            return Err(RoomError::ControllerNotInteractive);
        }
        let machine = self
            .state
            .match_machine
            .as_mut()
            .ok_or(RoomError::Playing)?;
        let result = machine
            .submit_action(seat, decision_id, action_id)
            .map_err(|error| RoomError::Match(error.to_string()))?;
        self.handle_decision_result(result.clone()).await?;
        self.sync_machine_controllers();
        self.advance_match().await?;
        Ok(RoomResponse::Action(result))
    }

    async fn tick(&mut self, now: Instant) -> Result<RoomResponse, RoomError> {
        let expired = self.state.disconnected_cleanup(now);
        if !expired.is_empty() {
            self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        }
        if self.state.match_machine.is_some() {
            match self
                .state
                .match_machine
                .as_mut()
                .expect("machine remains installed")
                .resolve_expired()
            {
                Ok(Some(result)) => self.handle_decision_result(result).await?,
                Ok(None) => {}
                Err(error) => {
                    self.abort_match(error.to_string()).await?;
                }
            }
            self.sync_machine_controllers();
            self.advance_match().await?;
        }
        if self.state.is_empty_expired(now) {
            self.state.deleted = true;
            self.publish(RoomEvent::RoomDeleted);
        }
        Ok(RoomResponse::Accepted(self.state.snapshot()))
    }

    fn sync_machine_controllers(&mut self) {
        let updates: Vec<_> = {
            let Some(machine) = self.state.match_machine.as_ref() else {
                return;
            };
            self.state
                .match_roster
                .iter()
                .filter_map(|entry| {
                    let state = self.state.participants.get(&entry.participant_id)?;
                    let controller = match machine.controller(entry.seat).ok()? {
                        ControllerState::Interactive => RoomController::Interactive,
                        ControllerState::TemporaryAuto => RoomController::TemporaryAuto,
                        ControllerState::PermanentAuto => match state.controller {
                            RoomController::PermanentAuto(reason) => {
                                RoomController::PermanentAuto(reason)
                            }
                            _ => RoomController::PermanentAuto(PermanentAutoReason::BuiltInBot),
                        },
                    };
                    Some((entry.participant_id.clone(), controller))
                })
                .collect()
        };
        for (participant_id, controller) in updates {
            if let Some(participant) = self.state.participants.get_mut(&participant_id) {
                participant.controller = controller;
            }
            self.state
                .update_roster_controller(&participant_id, controller);
        }
    }

    async fn advance_match(&mut self) -> Result<(), RoomError> {
        for _ in 0..10_000 {
            let Some(_) = self.state.match_machine.as_ref() else {
                return Ok(());
            };
            if self
                .state
                .match_machine
                .as_ref()
                .is_some_and(MatchMachine::is_complete)
            {
                let result = self
                    .state
                    .match_machine
                    .as_ref()
                    .and_then(MatchMachine::result)
                    .cloned()
                    .ok_or(RoomError::Match("completed Match has no result".into()))?;
                self.complete_match(result).await?;
                return Ok(());
            }
            let decision = match self
                .state
                .match_machine
                .as_mut()
                .expect("machine checked above")
                .current_decision()
            {
                Ok(decision) => decision,
                Err(error) => {
                    self.abort_match(error.to_string()).await?;
                    return Ok(());
                }
            };
            let Some(decision) = decision else {
                return Ok(());
            };
            let has_auto = decision.eligible().any(|seat| {
                self.state
                    .match_roster
                    .iter()
                    .find(|entry| entry.seat == seat)
                    .is_some_and(|entry| {
                        matches!(
                            entry.controller,
                            RoomController::TemporaryAuto | RoomController::PermanentAuto(_)
                        ) || entry.kind == ParticipantKind::BuiltInBot
                    })
            });
            self.publish(RoomEvent::DecisionOpened {
                match_id: match self.state.phase {
                    RoomPhase::Playing(ref id) => id.clone(),
                    _ => return Ok(()),
                },
                decision: decision.clone(),
            });
            if !has_auto {
                return Ok(());
            }
            match self
                .state
                .match_machine
                .as_mut()
                .expect("machine remains installed")
                .resolve_expired()
            {
                Ok(Some(result)) => self.handle_decision_result(result).await?,
                Ok(None) => return Ok(()),
                Err(error) => {
                    self.abort_match(error.to_string()).await?;
                    return Ok(());
                }
            }
        }
        Err(RoomError::Match(
            "automatic decision loop exceeded bound".into(),
        ))
    }

    async fn handle_decision_result(&mut self, result: DecisionResult) -> Result<(), RoomError> {
        let match_id = match self.state.phase {
            RoomPhase::Playing(ref id) => id.clone(),
            _ => return Ok(()),
        };
        for event in result.events() {
            self.send_append(match_id.clone(), vec![event.clone()])
                .await;
            if matches!(event, GameEvent::EndKyoku) {
                let _ = self.flush_kyoku(match_id.clone()).await;
            }
        }
        self.publish(RoomEvent::ActionResolved { match_id, result });
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        Ok(())
    }

    async fn send_append(&mut self, match_id: MatchId, events: Vec<GameEvent>) {
        if !self.state.config.replay_save || !self.state.replay_available {
            return;
        }
        if self
            .effects
            .try_send(RoomEffect::AppendEvents { match_id, events })
            .is_err()
        {
            self.state.persistence_degraded = true;
            self.state.replay_available = false;
            self.publish(RoomEvent::StorageDegraded);
        }
    }

    async fn flush_kyoku(&mut self, match_id: MatchId) -> Result<(), RoomError> {
        if !self.state.config.replay_save || !self.state.replay_available {
            return Ok(());
        }
        let (completion, receiver) = oneshot::channel();
        let effect = RoomEffect::FlushKyoku {
            match_id,
            completion,
        };
        if let Err(error) = self.send_ack_effect(effect, receiver).await {
            self.state.persistence_degraded = true;
            self.state.replay_available = false;
            self.publish(RoomEvent::StorageDegraded);
            let _ = error;
        }
        Ok(())
    }

    async fn complete_match(&mut self, result: MatchResult) -> Result<(), RoomError> {
        let match_id = self
            .state
            .finish_match(result.clone())
            .ok_or(RoomError::Playing)?;
        self.publish(RoomEvent::PhaseChanged(self.state.phase.clone()));
        self.publish(RoomEvent::MatchCompleted {
            match_id: match_id.clone(),
            result: result.clone(),
        });
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        if self.state.config.replay_save && self.state.replay_available {
            let (completion, receiver) = oneshot::channel();
            let effect = RoomEffect::FinalizeMatch {
                match_id,
                result,
                completion,
            };
            if self.send_ack_effect(effect, receiver).await.is_err() {
                self.state.persistence_degraded = true;
                self.state.replay_available = false;
                self.publish(RoomEvent::StorageDegraded);
            }
        }
        Ok(())
    }

    async fn abort_match(&mut self, reason: String) -> Result<RoomResponse, RoomError> {
        let match_id = match self.state.phase {
            RoomPhase::Playing(ref id) => id.clone(),
            _ => return Ok(RoomResponse::Accepted(self.state.snapshot())),
        };
        if self.state.config.replay_save {
            let _ = self.effects.try_send(RoomEffect::DeleteIncomplete {
                match_id: match_id.clone(),
            });
        }
        self.state.phase = RoomPhase::Lobby;
        self.state.match_machine = None;
        self.state.match_roster.clear();
        self.state.result = None;
        for participant in self.state.participants.values_mut() {
            participant.selected = false;
            participant.ready = false;
            participant.role = MatchRole::None;
            participant.controller = RoomController::Interactive;
            participant.remove_after_match = false;
        }
        self.state.bump_revision();
        self.publish(RoomEvent::MatchAborted { match_id, reason });
        self.publish(RoomEvent::PhaseChanged(self.state.phase.clone()));
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        Ok(RoomResponse::Accepted(self.state.snapshot()))
    }

    async fn rematch(&mut self) -> Result<RoomResponse, RoomError> {
        self.state.can_rematch()?;
        let (match_id, machine, roster) = self.state.build_match()?;
        if self.state.config.replay_save {
            let (effect, completion) = self.open_effect(
                &match_id,
                machine.mode(),
                &roster,
                machine.events().to_vec(),
            );
            if self.send_ack_effect(effect, completion).await.is_err() {
                self.state.persistence_degraded = true;
                self.state.replay_available = false;
                self.publish(RoomEvent::StorageDegraded);
            }
        }
        self.state.commit_match(match_id.clone(), machine, roster);
        self.publish(RoomEvent::PhaseChanged(self.state.phase.clone()));
        self.publish(RoomEvent::MatchStarted(match_id.clone()));
        self.publish(RoomEvent::Snapshot(self.state.snapshot()));
        self.advance_match().await?;
        Ok(RoomResponse::Started(match_id))
    }

    async fn shutdown(&mut self, mode: ShutdownMode) -> Result<RoomResponse, RoomError> {
        if self.state.shutting_down {
            return Ok(RoomResponse::Shutdown);
        }
        self.state.shutting_down = true;
        self.publish(RoomEvent::ServerShutdown);
        if matches!(mode, ShutdownMode::Graceful | ShutdownMode::Forced) {
            if let RoomPhase::Playing(match_id) = self.state.phase.clone() {
                let _ = self.effects.try_send(RoomEffect::DeleteIncomplete {
                    match_id: match_id.clone(),
                });
                self.publish(RoomEvent::MatchAborted {
                    match_id,
                    reason: "server shutdown".to_owned(),
                });
            }
        }
        Ok(RoomResponse::Shutdown)
    }

    fn publish(&mut self, event: RoomEvent) {
        let mut slow = Vec::new();
        for (id, sender) in &self.subscriptions {
            match sender.try_send(event.clone()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_))
                | Err(mpsc::error::TrySendError::Closed(_)) => slow.push(*id),
            }
        }
        for id in slow {
            self.subscriptions.remove(&id);
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoomRegistry {
    rooms: Arc<RwLock<BTreeMap<RoomJoinCode, RoomHandle>>>,
    cooldowns: Arc<RwLock<BTreeMap<RoomJoinCode, Instant>>>,
    max_rooms: usize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomRegistryError {
    #[error("room capacity is full")]
    Full,
    #[error("room code is unavailable")]
    CodeUnavailable,
    #[error("room was not found")]
    NotFound,
    #[error("room operation failed: {0}")]
    Room(#[from] RoomError),
}

impl RoomRegistry {
    pub fn new() -> Self {
        Self::with_max_rooms(DEFAULT_MAX_ROOMS)
    }

    pub fn with_max_rooms(max_rooms: usize) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(BTreeMap::new())),
            cooldowns: Arc::new(RwLock::new(BTreeMap::new())),
            max_rooms,
        }
    }

    async fn purge_closed(&self) {
        let entries: Vec<_> = self
            .rooms
            .read()
            .await
            .iter()
            .map(|(code, handle)| (code.clone(), handle.clone()))
            .collect();
        let mut closed = Vec::new();
        for (code, handle) in entries {
            if matches!(
                handle.snapshot().await,
                Err(RoomError::Closed | RoomError::Deleted)
            ) {
                closed.push(code);
            }
        }
        if closed.is_empty() {
            return;
        }
        let mut rooms = self.rooms.write().await;
        for code in closed {
            rooms.remove(&code);
        }
    }

    pub async fn create(&self, config: RoomConfig) -> Result<RoomHandle, RoomRegistryError> {
        self.purge_closed().await;
        {
            let rooms = self.rooms.read().await;
            if rooms.len() >= self.max_rooms {
                return Err(RoomRegistryError::Full);
            }
        }
        let now = Instant::now();
        let mut cooldowns = self.cooldowns.write().await;
        cooldowns.retain(|_, expires| *expires > now);
        let mut join_code = None;
        for _ in 0..128 {
            let candidate = RoomJoinCode::generate();
            if !cooldowns.contains_key(&candidate) {
                let rooms = self.rooms.read().await;
                if !rooms.contains_key(&candidate) {
                    join_code = Some(candidate);
                    break;
                }
            }
        }
        drop(cooldowns);
        let join_code = join_code.ok_or(RoomRegistryError::CodeUnavailable)?;
        let state = RoomState::with_ids(RoomId::generate(), join_code.clone(), config)?;
        let handle = RoomActor::spawn_with_state(state);
        let mut rooms = self.rooms.write().await;
        if rooms.len() >= self.max_rooms {
            return Err(RoomRegistryError::Full);
        }
        rooms.insert(join_code, handle.clone());
        Ok(handle)
    }

    pub async fn list(&self) -> Vec<RoomHandle> {
        self.purge_closed().await;
        self.rooms.read().await.values().cloned().collect()
    }

    pub async fn get(&self, join_code: &str) -> Option<RoomHandle> {
        let code = RoomJoinCode::new(join_code).ok()?;
        let handle = self.rooms.read().await.get(&code).cloned()?;
        if matches!(
            handle.snapshot().await,
            Err(RoomError::Closed | RoomError::Deleted)
        ) {
            self.rooms.write().await.remove(&code);
            return None;
        }
        Some(handle)
    }

    pub async fn remove(&self, join_code: &str) -> Result<(), RoomRegistryError> {
        let code = RoomJoinCode::new(join_code).map_err(|_| RoomRegistryError::NotFound)?;
        let handle = self
            .rooms
            .read()
            .await
            .get(&code)
            .cloned()
            .ok_or(RoomRegistryError::NotFound)?;
        let mut result = Err(RoomError::Busy);
        for _ in 0..=ROOM_COMMAND_CAPACITY {
            result = handle.send(RoomCommand::Delete).await;
            if !matches!(result, Err(RoomError::Busy)) {
                break;
            }
            tokio::task::yield_now().await;
        }
        match result {
            Ok(RoomResponse::Deleted) | Err(RoomError::Closed | RoomError::Deleted) => {
                self.rooms.write().await.remove(&code);
                self.cooldowns
                    .write()
                    .await
                    .insert(code, Instant::now() + ROOM_CODE_COOLDOWN);
                Ok(())
            }
            Ok(_) => Err(RoomRegistryError::Room(RoomError::Closed)),
            Err(error) => Err(RoomRegistryError::Room(error)),
        }
    }

    pub async fn revoke_token(&self, token_id: &str) -> Result<(), RoomRegistryError> {
        self.purge_closed().await;
        let handles: Vec<_> = self.rooms.read().await.values().cloned().collect();
        for handle in handles {
            let mut result = Err(RoomError::Busy);
            for _ in 0..=ROOM_COMMAND_CAPACITY {
                result = handle.send(RoomCommand::revoke_token(token_id)).await;
                if !matches!(result, Err(RoomError::Busy)) {
                    break;
                }
                tokio::task::yield_now().await;
            }
            match result {
                Ok(_) => {}
                Err(RoomError::Closed | RoomError::Deleted) => {}
                Err(error) => return Err(RoomRegistryError::Room(error)),
            }
        }
        Ok(())
    }

    pub async fn len(&self) -> usize {
        self.purge_closed().await;
        self.rooms.read().await.len()
    }

    pub async fn shutdown(&self, mode: ShutdownMode) {
        let handles: Vec<_> = self.rooms.read().await.values().cloned().collect();
        for handle in handles {
            let _ = handle.send(RoomCommand::shutdown(mode)).await;
        }
        self.rooms.write().await.clear();
    }
}

impl Default for RoomRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn _match_player_result(_result: &MatchPlayerResult) {}

// Keep this import surface stable for callers that build projection messages at
// the Room boundary without introducing a second state serializer.
#[allow(dead_code)]
fn _audience_for_role(role: MatchRole) -> Option<Audience> {
    match role {
        MatchRole::Player(seat) => Some(Audience::Player(seat)),
        MatchRole::Spectator | MatchRole::None => Some(Audience::Public),
    }
}
