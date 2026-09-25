//! Configuration, credentials, and durable storage for double-riichi.

pub const BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_COMMIT: &str = env!("GIT_COMMIT");

mod auth;
mod characters;
mod chatgpt_gateway;
mod compat;
mod config;
mod http;
mod mcp;
mod oauth;
mod storage;

pub use auth::{
    AdminAuthenticator, AdminSecrets, AdminSession, AdminSessionCredential, AdminSessionStore,
    BotTokenAuthority, BotTokenRecord, BotTokenSecret, BotTokenService, CreatedBotToken,
    CredentialError, PasswordError, SecretsError, TokenRevoked, TokenState, hash_password,
    hash_password_for_cli, hash_token, verify_password,
};
pub use characters::{
    CharacterAsset, CharacterAssetFile, CharacterPack, CharacterRegistry, CharacterRegistryError,
    CharacterRequirements, CharacterSummary, CharacterUsage, VoiceLine, character_router,
    is_safe_character_id, starter_version,
};
pub use config::{
    CasualTimeControl, CharacterConfig, ChatgptOAuthConfig, ConfigError, NetworkConfig,
    RuntimeConfig, TimeControls, TracingFormat,
};
pub use http::{IpCidr, ServerInitError, ServerLimits, ServerState, server_router};
pub use storage::{Storage, StorageError, spawn_room_effect_worker};

#[cfg(frontend_dist)]
use rust_embed::RustEmbed;

#[cfg(frontend_dist)]
#[derive(RustEmbed)]
#[folder = "../../frontend/dist/"]
pub struct FrontendAssets;
