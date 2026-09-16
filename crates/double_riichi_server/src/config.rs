use std::{
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
const DEFAULT_EMPTY_ROOM_CLEANUP_SECONDS: u64 = 3_600;
const DEFAULT_SHUTDOWN_SECONDS: u64 = 10;

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
    time_controls: RawTimeControls,
    #[serde(default = "default_unlimited_watchdog_seconds")]
    unlimited_watchdog_seconds: u64,
    #[serde(default = "default_empty_room_cleanup_seconds")]
    empty_room_cleanup_seconds: u64,
    #[serde(default = "default_shutdown_seconds")]
    shutdown_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

impl Default for RawTimeControls {
    fn default() -> Self {
        Self {
            casual: RawCasualTimeControl::default(),
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

fn default_empty_room_cleanup_seconds() -> u64 {
    DEFAULT_EMPTY_ROOM_CLEANUP_SECONDS
}

fn default_shutdown_seconds() -> u64 {
    DEFAULT_SHUTDOWN_SECONDS
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
pub struct RuntimeConfig {
    pub bind: String,
    pub public_origin: String,
    pub time_controls: TimeControls,
    pub unlimited_watchdog_seconds: u64,
    pub empty_room_cleanup_seconds: u64,
    pub shutdown_seconds: u64,
    data_root: PathBuf,
}

impl RuntimeConfig {
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(ConfigError::Io)?;
        let raw: RawRuntimeConfig = toml::from_str(&text).map_err(ConfigError::Toml)?;
        validate_bind(&raw.bind)?;
        validate_origin(&raw.public_origin)?;
        validate_duration(raw.time_controls.casual.turn_seconds, 1, 3_600)?;
        validate_duration(raw.time_controls.casual.response_seconds, 1, 3_600)?;
        validate_duration(raw.unlimited_watchdog_seconds, 10, 3_600)?;
        validate_duration(raw.empty_room_cleanup_seconds, 1, 86_400)?;
        validate_duration(raw.shutdown_seconds, 1, 86_400)?;

        let data_root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        Ok(Self {
            bind: raw.bind,
            public_origin: raw.public_origin,
            time_controls: TimeControls {
                casual: CasualTimeControl {
                    turn_seconds: raw.time_controls.casual.turn_seconds,
                    response_seconds: raw.time_controls.casual.response_seconds,
                },
            },
            unlimited_watchdog_seconds: raw.unlimited_watchdog_seconds,
            empty_room_cleanup_seconds: raw.empty_room_cleanup_seconds,
            shutdown_seconds: raw.shutdown_seconds,
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

fn validate_duration(value: u64, minimum: u64, maximum: u64) -> Result<(), ConfigError> {
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(ConfigError::Invalid(
            "duration is outside its allowed range",
        ))
    }
}
