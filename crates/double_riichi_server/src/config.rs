use std::{
    collections::BTreeMap,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;
use url::Url;

const DEFAULT_BIND: &str = "127.0.0.1:3000";
const DEFAULT_TURN_SECONDS: u64 = 30;
const DEFAULT_RESPONSE_SECONDS: u64 = 10;
const DEFAULT_UNLIMITED_WATCHDOG_SECONDS: u64 = 300;
const DEFAULT_MCP_SESSION_IDLE_SECONDS: u64 = 30 * 60;
const DEFAULT_EMPTY_ROOM_CLEANUP_SECONDS: u64 = 3_600;
const DEFAULT_SHUTDOWN_SECONDS: u64 = 10;
const DEFAULT_MJAI_CHARACTER: &str = "mjai-bot";
const DEFAULT_BUILTIN_CHARACTER: &str = "tsumogiri-bot";
const DEFAULT_MCP_CHARACTER: &str = "mcp-agent";
const DEFAULT_MAX_CONNECTIONS: usize = 256;
const DEFAULT_ROOM_PARTICIPANT_LIMIT: usize = 32;
const DEFAULT_CODE_LOOKUP_PER_MINUTE: usize = 20;
const DEFAULT_PARTICIPANT_CREATION_PER_MINUTE: usize = 10;
const DEFAULT_ADMIN_LOGIN_FAILURES: usize = 5;
const DEFAULT_AGENT_AUTH_FAILURES: usize = 20;
const DEFAULT_MAX_COMPAT_MATCHES: usize = 32;
const DEFAULT_MAX_RANKED_QUEUE: usize = 128;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration")]
    Io(#[source] std::io::Error),
    #[error("invalid TOML configuration")]
    Toml(#[source] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Invalid(&'static str),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRuntimeConfig {
    #[serde(default = "default_bind")]
    bind: String,
    public_origin: String,
    #[serde(default)]
    characters: RawCharacterConfig,
    #[serde(default)]
    time_controls: RawTimeControls,
    #[serde(default = "default_unlimited_watchdog_seconds")]
    unlimited_watchdog_seconds: u64,
    #[serde(default = "default_mcp_session_idle_seconds")]
    mcp_session_idle_seconds: u64,
    #[serde(default = "default_empty_room_cleanup_seconds")]
    empty_room_cleanup_seconds: u64,
    #[serde(default = "default_shutdown_seconds")]
    shutdown_seconds: u64,
    #[serde(default)]
    network: RawNetworkConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCharacterConfig {
    #[serde(default = "default_mjai_character")]
    mjai: String,
    #[serde(default = "default_builtin_character")]
    builtin: String,
    #[serde(default = "default_mcp_character")]
    mcp: String,
    #[serde(default)]
    mcp_providers: BTreeMap<String, String>,
}

impl Default for RawCharacterConfig {
    fn default() -> Self {
        Self {
            mjai: default_mjai_character(),
            builtin: default_builtin_character(),
            mcp: default_mcp_character(),
            mcp_providers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
struct RawTimeControls {
    #[serde(default)]
    casual: RawCasualTimeControl,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCasualTimeControl {
    #[serde(default = "default_turn_seconds")]
    turn_seconds: u64,
    #[serde(default = "default_response_seconds")]
    response_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetworkConfig {
    #[serde(default = "default_max_connections")]
    max_connections: usize,
    #[serde(default = "default_room_participant_limit")]
    room_participant_limit: usize,
    #[serde(default = "default_code_lookup_per_minute")]
    code_lookup_per_minute: usize,
    #[serde(default = "default_participant_creation_per_minute")]
    participant_creation_per_minute: usize,
    #[serde(default = "default_admin_login_failures")]
    admin_login_failures_per_15_minutes: usize,
    #[serde(default = "default_agent_auth_failures")]
    agent_auth_failures_per_minute: usize,
    #[serde(default)]
    trusted_proxy_cidrs: Vec<String>,
    #[serde(default = "default_max_compat_matches")]
    max_compat_matches: usize,
    #[serde(default = "default_max_ranked_queue")]
    max_ranked_queue: usize,
}

impl Default for RawNetworkConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            room_participant_limit: default_room_participant_limit(),
            code_lookup_per_minute: default_code_lookup_per_minute(),
            participant_creation_per_minute: default_participant_creation_per_minute(),
            admin_login_failures_per_15_minutes: default_admin_login_failures(),
            agent_auth_failures_per_minute: default_agent_auth_failures(),
            trusted_proxy_cidrs: Vec::new(),
            max_compat_matches: default_max_compat_matches(),
            max_ranked_queue: default_max_ranked_queue(),
        }
    }
}

impl Default for RawCasualTimeControl {
    fn default() -> Self {
        Self {
            turn_seconds: DEFAULT_TURN_SECONDS,
            response_seconds: DEFAULT_RESPONSE_SECONDS,
        }
    }
}

fn default_bind() -> String {
    DEFAULT_BIND.to_owned()
}

fn default_turn_seconds() -> u64 {
    DEFAULT_TURN_SECONDS
}

fn default_response_seconds() -> u64 {
    DEFAULT_RESPONSE_SECONDS
}

fn default_unlimited_watchdog_seconds() -> u64 {
    DEFAULT_UNLIMITED_WATCHDOG_SECONDS
}

fn default_mcp_session_idle_seconds() -> u64 {
    DEFAULT_MCP_SESSION_IDLE_SECONDS
}

fn default_empty_room_cleanup_seconds() -> u64 {
    DEFAULT_EMPTY_ROOM_CLEANUP_SECONDS
}

fn default_shutdown_seconds() -> u64 {
    DEFAULT_SHUTDOWN_SECONDS
}

fn default_max_connections() -> usize {
    DEFAULT_MAX_CONNECTIONS
}

fn default_room_participant_limit() -> usize {
    DEFAULT_ROOM_PARTICIPANT_LIMIT
}

fn default_code_lookup_per_minute() -> usize {
    DEFAULT_CODE_LOOKUP_PER_MINUTE
}

fn default_participant_creation_per_minute() -> usize {
    DEFAULT_PARTICIPANT_CREATION_PER_MINUTE
}

fn default_admin_login_failures() -> usize {
    DEFAULT_ADMIN_LOGIN_FAILURES
}

fn default_agent_auth_failures() -> usize {
    DEFAULT_AGENT_AUTH_FAILURES
}

fn default_max_compat_matches() -> usize {
    DEFAULT_MAX_COMPAT_MATCHES
}

fn default_max_ranked_queue() -> usize {
    DEFAULT_MAX_RANKED_QUEUE
}

fn default_mjai_character() -> String {
    DEFAULT_MJAI_CHARACTER.to_owned()
}

fn default_builtin_character() -> String {
    DEFAULT_BUILTIN_CHARACTER.to_owned()
}

fn default_mcp_character() -> String {
    DEFAULT_MCP_CHARACTER.to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterConfig {
    pub mjai: String,
    pub builtin: String,
    pub mcp: String,
    pub mcp_providers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasualTimeControl {
    pub turn_seconds: u64,
    pub response_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeControls {
    pub casual: CasualTimeControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkConfig {
    pub max_connections: usize,
    pub room_participant_limit: usize,
    pub code_lookup_per_minute: usize,
    pub participant_creation_per_minute: usize,
    pub admin_login_failures_per_15_minutes: usize,
    pub agent_auth_failures_per_minute: usize,
    pub trusted_proxy_cidrs: Vec<String>,
    pub max_compat_matches: usize,
    pub max_ranked_queue: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub bind: String,
    pub public_origin: String,
    pub characters: CharacterConfig,
    pub time_controls: TimeControls,
    pub unlimited_watchdog_seconds: u64,
    pub mcp_session_idle_seconds: u64,
    pub empty_room_cleanup_seconds: u64,
    pub shutdown_seconds: u64,
    pub network: NetworkConfig,
    data_root: PathBuf,
}

impl RuntimeConfig {
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(ConfigError::Io)?;
        let raw: RawRuntimeConfig = toml::from_str(&text).map_err(ConfigError::Toml)?;
        validate_bind(&raw.bind)?;
        validate_origin(&raw.public_origin)?;
        validate_character_config(&raw.characters)?;
        validate_duration(raw.time_controls.casual.turn_seconds, 1, 3_600)?;
        validate_duration(raw.time_controls.casual.response_seconds, 1, 3_600)?;
        validate_duration(raw.unlimited_watchdog_seconds, 10, 3_600)?;
        validate_duration(raw.mcp_session_idle_seconds, 1, 86_400)?;
        validate_duration(raw.empty_room_cleanup_seconds, 1, 86_400)?;
        validate_duration(raw.shutdown_seconds, 1, 86_400)?;
        validate_network(&raw.network)?;

        let data_root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        Ok(Self {
            bind: raw.bind,
            public_origin: raw.public_origin,
            characters: CharacterConfig {
                mjai: raw.characters.mjai,
                builtin: raw.characters.builtin,
                mcp: raw.characters.mcp,
                mcp_providers: raw.characters.mcp_providers,
            },
            time_controls: TimeControls {
                casual: CasualTimeControl {
                    turn_seconds: raw.time_controls.casual.turn_seconds,
                    response_seconds: raw.time_controls.casual.response_seconds,
                },
            },
            unlimited_watchdog_seconds: raw.unlimited_watchdog_seconds,
            mcp_session_idle_seconds: raw.mcp_session_idle_seconds,
            empty_room_cleanup_seconds: raw.empty_room_cleanup_seconds,
            shutdown_seconds: raw.shutdown_seconds,
            network: NetworkConfig {
                max_connections: raw.network.max_connections,
                room_participant_limit: raw.network.room_participant_limit,
                code_lookup_per_minute: raw.network.code_lookup_per_minute,
                participant_creation_per_minute: raw.network.participant_creation_per_minute,
                admin_login_failures_per_15_minutes: raw
                    .network
                    .admin_login_failures_per_15_minutes,
                agent_auth_failures_per_minute: raw.network.agent_auth_failures_per_minute,
                trusted_proxy_cidrs: raw.network.trusted_proxy_cidrs,
                max_compat_matches: raw.network.max_compat_matches,
                max_ranked_queue: raw.network.max_ranked_queue,
            },
            data_root,
        })
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_root.join("double-riichi.db")
    }

    pub fn replay_root(&self) -> PathBuf {
        self.data_root.join("replays")
    }

    pub fn character_pack_root(&self) -> PathBuf {
        self.data_root.join("character-packs")
    }

    pub fn character_requirements(&self) -> Result<crate::CharacterRequirements, ConfigError> {
        crate::CharacterRequirements::new(
            &self.characters.mjai,
            &self.characters.builtin,
            &self.characters.mcp,
            self.characters.mcp_providers.iter(),
        )
        .map_err(|_| ConfigError::Invalid("character configuration is invalid"))
    }

    pub fn load_character_registry(
        &self,
    ) -> Result<crate::CharacterRegistry, crate::CharacterRegistryError> {
        let requirements = self
            .character_requirements()
            .map_err(|_| crate::CharacterRegistryError::InvalidRequirement)?;
        crate::CharacterRegistry::load_from_data_root(self.data_root(), &requirements)
    }
}

fn validate_bind(bind: &str) -> Result<(), ConfigError> {
    bind.parse::<SocketAddr>()
        .map(|_| ())
        .map_err(|_| ConfigError::Invalid("bind must be a socket address"))
}

fn validate_origin(origin: &str) -> Result<(), ConfigError> {
    let parsed =
        Url::parse(origin).map_err(|_| ConfigError::Invalid("public_origin is invalid"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || (parsed.path() != "" && parsed.path() != "/")
    {
        return Err(ConfigError::Invalid("public_origin must be an origin"));
    }
    Ok(())
}

fn validate_character_config(config: &RawCharacterConfig) -> Result<(), ConfigError> {
    crate::CharacterRequirements::new(
        &config.mjai,
        &config.builtin,
        &config.mcp,
        config.mcp_providers.iter(),
    )
    .map(|_| ())
    .map_err(|_| ConfigError::Invalid("character configuration is invalid"))
}

fn validate_network(network: &RawNetworkConfig) -> Result<(), ConfigError> {
    if !(1..=4_096).contains(&network.max_connections)
        || !(1..=32).contains(&network.room_participant_limit)
        || network.room_participant_limit < 3
        || network.code_lookup_per_minute == 0
        || network.participant_creation_per_minute == 0
        || network.admin_login_failures_per_15_minutes == 0
        || network.agent_auth_failures_per_minute == 0
        || !(1..=256).contains(&network.max_compat_matches)
        || !(1..=4_096).contains(&network.max_ranked_queue)
    {
        return Err(ConfigError::Invalid(
            "network limit is outside its allowed range",
        ));
    }
    for cidr in &network.trusted_proxy_cidrs {
        let Some((address, prefix)) = cidr.split_once('/') else {
            return Err(ConfigError::Invalid("trusted proxy CIDR is invalid"));
        };
        let address = address
            .parse::<std::net::IpAddr>()
            .map_err(|_| ConfigError::Invalid("trusted proxy CIDR is invalid"))?;
        let prefix = prefix
            .parse::<u8>()
            .map_err(|_| ConfigError::Invalid("trusted proxy CIDR is invalid"))?;
        let max = if address.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(ConfigError::Invalid("trusted proxy CIDR is invalid"));
        }
    }
    Ok(())
}

fn validate_duration(value: u64, minimum: u64, maximum: u64) -> Result<(), ConfigError> {
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(ConfigError::Invalid(
            "duration is outside its allowed range",
        ))
    }
}
