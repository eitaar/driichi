use std::{
    collections::HashMap,
    fmt, fs,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime},
};

use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::random;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::broadcast;
use zeroize::Zeroize;

use crate::storage::{RevokeOutcome, Storage, StorageError};

const PASSWORD_MIN_BYTES: usize = 12;
const PASSWORD_MAX_BYTES: usize = 1_024;
const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
const ADMIN_USERNAME_KEY: &str = "ADMIN_USERNAME";
const ADMIN_PASSWORD_HASH_KEY: &str = "ADMIN_PASSWORD_HASH";

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("password length is outside the allowed range")]
    InvalidLength,
    #[error("password hash could not be generated")]
    Hashing,
    #[error("password hash is invalid")]
    InvalidHash,
}

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    password_hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hashing)
}

pub fn hash_password_for_cli(
    password: &str,
    confirmation: &str,
) -> Result<Option<String>, PasswordError> {
    if password != confirmation {
        return Ok(None);
    }
    hash_password(password).map(Some)
}

pub fn verify_password(password: &str, encoded_hash: &str) -> Result<bool, PasswordError> {
    let hash = PasswordHash::new(encoded_hash).map_err(|_| PasswordError::InvalidHash)?;
    match password_hasher().verify_password(password.as_bytes(), &hash) {
        Ok(()) => Ok(true),
        Err(error) if error.to_string().contains("password") => Ok(false),
        Err(_) => Err(PasswordError::InvalidHash),
    }
}

fn is_required_password_hash(encoded_hash: &str) -> bool {
    encoded_hash.starts_with("$argon2id$v=19$m=65536,t=3,p=1$")
}

fn validate_password(password: &str) -> Result<(), PasswordError> {
    let length = password.len();
    if (PASSWORD_MIN_BYTES..=PASSWORD_MAX_BYTES).contains(&length) {
        Ok(())
    } else {
        Err(PasswordError::InvalidLength)
    }
}

fn password_hasher() -> Argon2<'static> {
    let params = Params::new(65_536, 3, 1, Some(32)).expect("fixed Argon2id parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("required .env could not be read")]
    Io(#[source] std::io::Error),
    #[error(".env has invalid syntax")]
    Syntax,
    #[error(".env contains an unsupported or duplicate key")]
    Key,
    #[error(".env is missing a required value")]
    Missing,
    #[error("ADMIN_PASSWORD_HASH is invalid")]
    InvalidPasswordHash,
}

pub struct AdminSecrets {
    username: String,
    password_hash: String,
}

impl AdminSecrets {
    pub fn load(data_root: &Path) -> Result<Self, SecretsError> {
        let path = data_root.join(".env");
        let text = fs::read_to_string(path).map_err(SecretsError::Io)?;
        let mut username = None;
        let mut password_hash = None;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, raw_value)) = line.split_once('=') else {
                return Err(SecretsError::Syntax);
            };
            let key = key.trim();
            if key != ADMIN_USERNAME_KEY && key != ADMIN_PASSWORD_HASH_KEY {
                return Err(SecretsError::Key);
            }
            let value = parse_env_value(raw_value.trim())?;
            match key {
                ADMIN_USERNAME_KEY => {
                    if username.replace(value).is_some() {
                        return Err(SecretsError::Key);
                    }
                }
                ADMIN_PASSWORD_HASH_KEY => {
                    if password_hash.replace(value).is_some() {
                        return Err(SecretsError::Key);
                    }
                }
                _ => unreachable!(),
            }
        }

        let username = username.ok_or(SecretsError::Missing)?;
        let password_hash = password_hash.ok_or(SecretsError::Missing)?;
        if username.is_empty() || password_hash.is_empty() {
            return Err(SecretsError::Missing);
        }
        let parsed =
            PasswordHash::new(&password_hash).map_err(|_| SecretsError::InvalidPasswordHash)?;
        if !is_required_password_hash(&password_hash) || parsed.algorithm.as_str() != "argon2id" {
            return Err(SecretsError::InvalidPasswordHash);
        }
        Ok(Self {
            username,
            password_hash,
        })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password_hash(&self) -> &str {
        &self.password_hash
    }
}

impl fmt::Debug for AdminSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminSecrets")
            .field("username", &self.username)
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}

fn parse_env_value(value: &str) -> Result<String, SecretsError> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if let Some(stripped) = value.strip_prefix('"') {
        if !stripped.ends_with('"') || stripped.len() < 2 {
            return Err(SecretsError::Syntax);
        }
        return serde_json::from_str(value).map_err(|_| SecretsError::Syntax);
    }
    if let Some(stripped) = value.strip_prefix('\'') {
        if !stripped.ends_with('\'') || stripped.len() < 2 {
            return Err(SecretsError::Syntax);
        }
        return Ok(stripped[1..stripped.len() - 1].to_owned());
    }
    if value.contains(char::is_whitespace) {
        return Err(SecretsError::Syntax);
    }
    Ok(value.to_owned())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CredentialError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("bot token is already revoked")]
    AlreadyRevoked,
    #[error("invalid bot token name")]
    InvalidTokenName,
    #[error("credential storage operation failed")]
    Storage,
    #[error("invalid credential configuration")]
    InvalidConfiguration,
}

#[derive(Clone)]
pub struct AdminSessionStore {
    sessions: Arc<Mutex<HashMap<[u8; 32], SystemTime>>>,
}

impl fmt::Debug for AdminSessionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminSessionStore")
            .field(
                "session_count",
                &self
                    .sessions
                    .lock()
                    .map(|sessions| sessions.len())
                    .unwrap_or(0),
            )
            .finish()
    }
}

impl Default for AdminSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AdminSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn issue(&self, now: SystemTime) -> AdminSession {
        let credential: [u8; 32] = random();
        let expires_at = now + SESSION_LIFETIME;
        self.sessions
            .lock()
            .expect("admin session mutex poisoned")
            .insert(hash_bytes(&credential), expires_at);
        AdminSession {
            credential: AdminSessionCredential(credential.to_vec()),
            expires_at,
        }
    }

    pub fn validate<C: AsRef<[u8]>>(&self, credential: C, now: SystemTime) -> bool {
        let hash = hash_bytes(credential.as_ref());
        let mut sessions = self.sessions.lock().expect("admin session mutex poisoned");
        match sessions.get(&hash).copied() {
            Some(expires_at) if expires_at > now => true,
            Some(_) => {
                sessions.remove(&hash);
                false
            }
            None => false,
        }
    }

    pub fn revoke<C: AsRef<[u8]>>(&self, credential: C) {
        self.sessions
            .lock()
            .expect("admin session mutex poisoned")
            .remove(&hash_bytes(credential.as_ref()));
    }

    pub fn clear(&self) {
        self.sessions
            .lock()
            .expect("admin session mutex poisoned")
            .clear();
    }

    pub fn len(&self) -> usize {
        self.sessions
            .lock()
            .expect("admin session mutex poisoned")
            .len()
    }
}

pub struct AdminSessionCredential(Vec<u8>);

impl AdminSessionCredential {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for AdminSessionCredential {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for AdminSessionCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Deref for AdminSessionCredential {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl Drop for AdminSessionCredential {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub struct AdminSession {
    credential: AdminSessionCredential,
    expires_at: SystemTime,
}

impl AdminSession {
    pub fn credential(&self) -> &AdminSessionCredential {
        &self.credential
    }

    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }
}

impl fmt::Debug for AdminSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminSession")
            .field("credential", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

pub struct AdminAuthenticator {
    username: String,
    password_hash: String,
    sessions: AdminSessionStore,
}

impl fmt::Debug for AdminAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminAuthenticator")
            .field("username", &self.username)
            .field("password_hash", &"[REDACTED]")
            .field("sessions", &self.sessions)
            .finish()
    }
}

impl AdminAuthenticator {
    pub fn new(
        username: impl Into<String>,
        password_hash: impl Into<String>,
    ) -> Result<Self, CredentialError> {
        let username = username.into();
        let password_hash = password_hash.into();
        if username.is_empty()
            || PasswordHash::new(&password_hash).is_err()
            || !is_required_password_hash(&password_hash)
        {
            return Err(CredentialError::InvalidConfiguration);
        }
        Ok(Self {
            username,
            password_hash,
            sessions: AdminSessionStore::new(),
        })
    }

    pub fn login(
        &self,
        username: &str,
        password: &str,
        now: SystemTime,
    ) -> Result<AdminSession, CredentialError> {
        let username_matches = username == self.username;
        let password_matches = verify_password(password, &self.password_hash).unwrap_or(false);
        if !username_matches || !password_matches {
            return Err(CredentialError::InvalidCredentials);
        }
        Ok(self.sessions.issue(now))
    }

    pub fn sessions(&self) -> &AdminSessionStore {
        &self.sessions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenState {
    Active,
    Revoked,
}

impl TokenState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BotTokenRecord {
    token_id: String,
    name: String,
    token_hash: [u8; 32],
    state: TokenState,
    created_at: i64,
    revoked_at: Option<i64>,
}

impl BotTokenRecord {
    pub(crate) fn new(
        token_id: String,
        name: String,
        token_hash: [u8; 32],
        state: TokenState,
        created_at: i64,
        revoked_at: Option<i64>,
    ) -> Self {
        Self {
            token_id,
            name,
            token_hash,
            state,
            created_at,
            revoked_at,
        }
    }

    pub fn token_id(&self) -> &str {
        &self.token_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn state(&self) -> TokenState {
        self.state
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    pub fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }

    pub(crate) fn token_hash(&self) -> &[u8; 32] {
        &self.token_hash
    }
}

impl fmt::Debug for BotTokenRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BotTokenRecord")
            .field("token_id", &self.token_id)
            .field("name", &self.name)
            .field("state", &self.state)
            .field("created_at", &self.created_at)
            .field("revoked_at", &self.revoked_at)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenRevoked {
    token_id: String,
    state: TokenState,
}

impl TokenRevoked {
    pub fn new(token_id: String) -> Self {
        Self {
            token_id,
            state: TokenState::Revoked,
        }
    }

    pub fn token_id(&self) -> &str {
        &self.token_id
    }

    pub fn state(&self) -> TokenState {
        self.state
    }
}

pub struct BotTokenAuthority {
    records: RwLock<HashMap<[u8; 32], BotTokenRecord>>,
    revocations: broadcast::Sender<TokenRevoked>,
}

impl fmt::Debug for BotTokenAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BotTokenAuthority")
            .field(
                "token_count",
                &self
                    .records
                    .read()
                    .map(|records| records.len())
                    .unwrap_or(0),
            )
            .finish()
    }
}

impl BotTokenAuthority {
    pub fn empty() -> Self {
        let (revocations, _) = broadcast::channel(64);
        Self {
            records: RwLock::new(HashMap::new()),
            revocations,
        }
    }

    pub fn from_records(records: Vec<BotTokenRecord>) -> Self {
        let authority = Self::empty();
        let mut cache = authority
            .records
            .write()
            .expect("token authority lock poisoned");
        for record in records {
            cache.insert(record.token_hash, record);
        }
        drop(cache);
        authority
    }

    pub fn authenticate(&self, raw_token: &str) -> Result<BotTokenRecord, CredentialError> {
        let hash = hash_token(raw_token);
        let records = self.records.read().expect("token authority lock poisoned");
        match records
            .get(&hash)
            .filter(|record| record.state == TokenState::Active)
        {
            Some(record) => Ok(record.clone()),
            None => Err(CredentialError::InvalidCredentials),
        }
    }

    pub fn subscribe_revocations(&self) -> broadcast::Receiver<TokenRevoked> {
        self.revocations.subscribe()
    }

    pub fn revoked_token_ids(&self) -> Vec<String> {
        self.records
            .read()
            .expect("token authority lock poisoned")
            .values()
            .filter(|record| record.state == TokenState::Revoked)
            .map(|record| record.token_id.clone())
            .collect()
    }

    pub(crate) fn insert_active(&self, record: BotTokenRecord) {
        self.records
            .write()
            .expect("token authority lock poisoned")
            .insert(record.token_hash, record);
    }

    pub(crate) fn revoke(
        &self,
        token_id: &str,
        revoked_at: i64,
    ) -> Result<TokenRevoked, CredentialError> {
        let mut records = self.records.write().expect("token authority lock poisoned");
        let Some(record) = records
            .values_mut()
            .find(|record| record.token_id == token_id)
        else {
            return Err(CredentialError::InvalidCredentials);
        };
        if record.state == TokenState::Revoked {
            return Err(CredentialError::AlreadyRevoked);
        }
        record.state = TokenState::Revoked;
        record.revoked_at = Some(revoked_at);
        let event = TokenRevoked::new(record.token_id.clone());
        let _ = self.revocations.send(event.clone());
        Ok(event)
    }
}

pub struct BotTokenSecret(String);

impl BotTokenSecret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for BotTokenSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for BotTokenSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

pub struct CreatedBotToken {
    record: BotTokenRecord,
    secret: BotTokenSecret,
}

impl CreatedBotToken {
    pub fn record(&self) -> &BotTokenRecord {
        &self.record
    }

    pub fn secret(&self) -> &BotTokenSecret {
        &self.secret
    }
}

impl fmt::Debug for CreatedBotToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreatedBotToken")
            .field("record", &self.record)
            .field("secret", &self.secret)
            .finish()
    }
}

pub fn hash_token(raw_token: &str) -> [u8; 32] {
    Sha256::digest(raw_token.as_bytes()).into()
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(crate) fn generate_bot_secret() -> (String, [u8; 32]) {
    let bytes: [u8; 32] = random();
    let raw = format!("driichi_{}", URL_SAFE_NO_PAD.encode(bytes));
    let hash = hash_token(&raw);
    (raw, hash)
}

pub(crate) fn generate_token_id() -> String {
    let bytes: [u8; 16] = random();
    format!("tok_{}", URL_SAFE_NO_PAD.encode(bytes))
}

pub struct BotTokenService {
    storage: Arc<Storage>,
    authority: Arc<BotTokenAuthority>,
}

impl BotTokenService {
    pub fn new(storage: Arc<Storage>, authority: Arc<BotTokenAuthority>) -> Self {
        Self { storage, authority }
    }

    pub fn authenticate(&self, raw_token: &str) -> Result<BotTokenRecord, CredentialError> {
        self.authority.authenticate(raw_token)
    }

    pub fn subscribe_revocations(&self) -> broadcast::Receiver<TokenRevoked> {
        self.authority.subscribe_revocations()
    }

    pub fn revoked_token_ids(&self) -> Vec<String> {
        self.authority.revoked_token_ids()
    }

    pub async fn create(
        &self,
        name: &str,
        created_at: i64,
        request_id: &str,
    ) -> Result<CreatedBotToken, CredentialError> {
        let name = normalize_token_name(name)?;
        let (raw, token_hash) = generate_bot_secret();
        let record = BotTokenRecord::new(
            generate_token_id(),
            name,
            token_hash,
            TokenState::Active,
            created_at,
            None,
        );
        self.storage
            .insert_bot_token(&record, request_id)
            .await
            .map_err(map_storage_error)?;
        self.authority.insert_active(record.clone());
        Ok(CreatedBotToken {
            record,
            secret: BotTokenSecret(raw),
        })
    }

    pub async fn list(&self) -> Result<Vec<BotTokenRecord>, CredentialError> {
        self.storage
            .load_bot_tokens()
            .await
            .map_err(map_storage_error)
    }

    pub async fn revoke(
        &self,
        token_id: &str,
        revoked_at: i64,
        request_id: &str,
    ) -> Result<(), CredentialError> {
        match self
            .storage
            .revoke_bot_token(token_id, revoked_at, request_id)
            .await
            .map_err(map_storage_error)?
        {
            RevokeOutcome::AlreadyRevoked => Err(CredentialError::AlreadyRevoked),
            RevokeOutcome::NotFound => Err(CredentialError::InvalidCredentials),
            RevokeOutcome::Revoked { .. } => {
                self.authority.revoke(token_id, revoked_at)?;
                Ok(())
            }
        }
    }
}

fn map_storage_error(_error: StorageError) -> CredentialError {
    CredentialError::Storage
}

pub(crate) fn normalize_token_name(name: &str) -> Result<String, CredentialError> {
    let trimmed = name.trim_matches(char::is_whitespace);
    if trimmed.is_empty() || trimmed.chars().count() > 64 || trimmed.chars().any(char::is_control) {
        return Err(CredentialError::InvalidTokenName);
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn validate_audit_summary(action: &str, summary: &Value) -> bool {
    let Some(object) = summary.as_object() else {
        return false;
    };
    if contains_forbidden_key(summary) || contains_raw_token(summary) {
        return false;
    }
    match action {
        "login" | "logout" | "fill_with_bots" | "match_start" | "rematch" | "back_to_lobby" => {
            object.is_empty()
        }
        "room_create" | "room_delete" => object_has_string(object, "room_name"),
        "room_configure" => object_has_string_array(object, "changed_fields"),
        "participant_select" | "participant_deselect" | "participant_kick" => {
            object_has_string(object, "participant_id")
        }
        "token_create" => object_has_string(object, "name"),
        "token_revoke" => {
            object.get("name").is_some_and(Value::is_string)
                && object.iter().all(|(key, value)| {
                    matches!(key.as_str(), "name" | "token_id") && value.is_string()
                })
        }
        "replay_delete" => object_has_string(object, "match_id"),
        _ => false,
    }
}

fn object_has_string(object: &serde_json::Map<String, Value>, key: &str) -> bool {
    object.len() == 1 && object.get(key).is_some_and(Value::is_string)
}

fn object_has_string_array(object: &serde_json::Map<String, Value>, key: &str) -> bool {
    object.len() == 1
        && object
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().all(Value::is_string))
}

fn contains_forbidden_key(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            let lower = key.to_ascii_lowercase();
            lower.contains("password")
                || lower.contains("secret")
                || lower.contains("cookie")
                || lower.contains("authorization")
                || lower == "raw_token"
                || lower == "token"
                || contains_forbidden_key(value)
        }),
        Value::Array(values) => values.iter().any(contains_forbidden_key),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn contains_raw_token(value: &Value) -> bool {
    match value {
        Value::String(value) => string_contains_raw_token(value),
        Value::Array(values) => values.iter().any(contains_raw_token),
        Value::Object(values) => values.values().any(contains_raw_token),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn string_contains_raw_token(value: &str) -> bool {
    const PREFIX: &[u8] = b"driichi_";
    const SECRET_LENGTH: usize = 43;
    let bytes = value.as_bytes();
    bytes
        .windows(PREFIX.len())
        .enumerate()
        .any(|(offset, window)| {
            window == PREFIX
                && bytes
                    .get(offset + PREFIX.len()..offset + PREFIX.len() + SECRET_LENGTH)
                    .is_some_and(|candidate| candidate.iter().all(is_token_character))
        })
}

fn is_token_character(byte: &u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-')
}
