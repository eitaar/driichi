use std::{
    collections::HashMap,
    fs,
    fs::OpenOptions,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use double_riichi_core::{
    GameMode, MatchPlayerSnapshot, MatchResult, Participant, ParticipantKind, RoomAuxiliaryEvent,
    RoomAuxiliaryPhase, RoomEffect, RoomEffectError, RoomPersistenceFailure,
};
use double_riichi_replay::{
    AuxiliaryPhase, AuxiliaryRecord, MAX_DECOMPRESSED_REPLAY_BYTES, MAX_REPLAY_EVENTS,
    ReplayArtifact, ReplayError, ReplayFrame, ReplayReader, ReplayWriter,
    startup_cleanup as cleanup_replay_root,
};
use futures_util::TryStreamExt;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::auth::validate_audit_summary;
use crate::{BotTokenRecord, TokenState};

const AUDIT_RETENTION_SECONDS: i64 = 90 * 24 * 60 * 60;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database operation failed")]
    Sqlx(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("storage filesystem operation failed")]
    Io(#[source] std::io::Error),
    #[error("replay path is outside the replay root")]
    UnsafeReplayPath,
    #[error("audit summary is not allowlisted")]
    InvalidAuditSummary,
    #[error("database returned an invalid token record")]
    InvalidTokenRecord,
    #[error("replay metadata operation failed")]
    ReplayMetadata,
    #[error("replay was not found")]
    ReplayNotFound,
    #[error("replay is unavailable")]
    ReplayUnavailable,
    #[error("replay is too large")]
    ReplayTooLarge,
    #[error("replay is corrupt")]
    ReplayCorrupt,
    #[error("replay startup cleanup failed")]
    ReplayCleanup(#[source] ReplayError),
    #[error("Admin audit operation is not pending")]
    AuditPending,
}

impl StorageError {
    pub(crate) fn replay_failure_kind(&self) -> &'static str {
        match self {
            Self::ReplayTooLarge => "too_large",
            Self::ReplayCorrupt => "corrupt",
            Self::ReplayUnavailable => "unavailable",
            Self::UnsafeReplayPath => "unsafe_path",
            Self::ReplayNotFound => "not_found",
            Self::ReplayMetadata => "metadata",
            Self::Io(_) => "filesystem",
            Self::Sqlx(_) => "database",
            Self::Migration(_) => "migration",
            Self::InvalidAuditSummary => "audit_summary",
            Self::InvalidTokenRecord => "token_record",
            Self::ReplayCleanup(_) => "cleanup",
            Self::AuditPending => "audit_pending",
        }
    }
}

impl From<sqlx::Error> for StorageError {
    fn from(error: sqlx::Error) -> Self {
        Self::Sqlx(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RevokeOutcome {
    Revoked { name: String },
    AlreadyRevoked,
    NotFound,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplaySummary {
    pub match_id: String,
    pub source: String,
    pub room_name: Option<String>,
    pub game_mode: String,
    pub started_at: i64,
    pub completed_at: i64,
    pub file_size: i64,
    pub availability: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayPlayer {
    pub participant_id: String,
    pub display_name: String,
    pub participant_kind: String,
    pub seat: i64,
    pub character_id: Option<String>,
    pub final_points: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplayView {
    pub summary: ReplaySummary,
    pub players: Vec<ReplayPlayer>,
    pub frames: Vec<ReplayFrame>,
}

#[derive(Serialize)]
struct ReplayResponse<'a> {
    match_id: &'a str,
    source: &'a str,
    room_name: &'a Option<String>,
    game_mode: &'a str,
    started_at: String,
    completed_at: String,
    file_size: i64,
    availability: &'static str,
    replay_available: bool,
    players: &'a [ReplayPlayer],
    frames: &'a [ReplayFrame],
}

pub struct Storage {
    pool: SqlitePool,
    data_root: PathBuf,
    replay_root: PathBuf,
    replay_degraded: AtomicBool,
    max_connections: u32,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Storage")
            .field("data_root", &self.data_root)
            .field("replay_root", &self.replay_root)
            .field("max_connections", &self.max_connections())
            .finish()
    }
}

impl Storage {
    pub async fn connect(data_root: &Path) -> Result<Self, StorageError> {
        fs::create_dir_all(data_root).map_err(StorageError::Io)?;
        let replay_root = data_root.join("replays");
        for directory in [
            replay_root.clone(),
            replay_root.join("4p"),
            replay_root.join("3p"),
            replay_root.join(".incomplete"),
        ] {
            fs::create_dir_all(directory).map_err(StorageError::Io)?;
        }

        let database_path = data_root.join("double-riichi.db");
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(StorageError::Sqlx)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(StorageError::Migration)?;

        let storage = Self {
            pool,
            data_root: data_root.to_path_buf(),
            replay_root,
            replay_degraded: AtomicBool::new(false),
            max_connections: 4,
        };
        storage.startup_cleanup().await?;
        if let Err(error) = storage.validate_completed_replays().await {
            storage.mark_replay_degraded();
            tracing::warn!(error = ?error, "completed replay startup validation failed");
        }
        if let Err(error) = storage.retry_pending_audits().await {
            tracing::warn!(error = ?error, "pending Admin audit recovery failed during startup");
        }
        if let Err(error) = storage.cleanup_audit(now_unix_seconds()).await {
            tracing::warn!(error = ?error, "audit retention cleanup failed during startup");
        }
        Ok(storage)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn max_connections(&self) -> u32 {
        self.max_connections
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_root.join("double-riichi.db")
    }

    pub fn replay_root(&self) -> &Path {
        &self.replay_root
    }

    pub fn replay_degraded(&self) -> bool {
        self.replay_degraded.load(Ordering::Acquire)
    }

    fn mark_replay_degraded(&self) {
        self.replay_degraded.store(true, Ordering::Release);
    }

    pub fn probe_replay(&self) -> Result<(), StorageError> {
        for directory in ["", "4p", "3p", ".incomplete"] {
            let path = if directory.is_empty() {
                self.replay_root.clone()
            } else {
                self.replay_root.join(directory)
            };
            let metadata = fs::symlink_metadata(path).map_err(StorageError::Io)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotADirectory,
                    "replay path is not a directory",
                )));
            }
        }
        let probe = self.replay_root.join(".incomplete").join(format!(
            ".health-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)
                .map_err(StorageError::Io)?;
            file.write_all(b"health\n").map_err(StorageError::Io)?;
            file.sync_all().map_err(StorageError::Io)
        })();
        let _ = fs::remove_file(&probe);
        result
    }

    pub async fn scalar_text(&self, statement: &str) -> Result<String, StorageError> {
        sqlx::query_scalar(statement)
            .fetch_one(&self.pool)
            .await
            .map_err(StorageError::Sqlx)
    }

    pub async fn scalar_i64(&self, statement: &str) -> Result<i64, StorageError> {
        sqlx::query_scalar(statement)
            .fetch_one(&self.pool)
            .await
            .map_err(StorageError::Sqlx)
    }

    pub fn resolve_replay_path(&self, relative: &str) -> Result<PathBuf, StorageError> {
        let relative = relative.strip_prefix("replays/").unwrap_or(relative);
        let relative_path = Path::new(relative);
        if relative.is_empty()
            || relative_path.is_absolute()
            || relative_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(StorageError::UnsafeReplayPath);
        }
        let candidate = self.replay_root.join(relative_path);
        if !candidate.starts_with(&self.replay_root) {
            return Err(StorageError::UnsafeReplayPath);
        }
        let canonical_root = fs::canonicalize(&self.replay_root).map_err(StorageError::Io)?;
        let mut current = canonical_root;
        let components: Vec<_> = relative_path.components().collect();
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(name) = component else {
                continue;
            };
            current.push(name);
            match fs::symlink_metadata(&current) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink()
                        || (index + 1 < components.len() && !metadata.is_dir())
                    {
                        return Err(StorageError::UnsafeReplayPath);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(StorageError::Io(error)),
            }
        }
        Ok(candidate)
    }

    pub async fn startup_cleanup(&self) -> Result<(), StorageError> {
        let rows = sqlx::query(
            "SELECT match_id, replay_path FROM matches WHERE status IN ('writing', 'failed')",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        let mut unfinished = Vec::with_capacity(rows.len());
        for row in rows {
            unfinished.push((
                row.try_get::<String, _>("match_id")
                    .map_err(StorageError::Sqlx)?,
                row.try_get::<Option<String>, _>("replay_path")
                    .map_err(StorageError::Sqlx)?,
            ));
        }

        // Orphan parts are safe to remove by construction: this helper only
        // visits the replay root and refuses symlinked entries. A failure here
        // must not prevent independent metadata cleanup below.
        if let Err(error) = cleanup_replay_root(&self.replay_root, std::iter::empty::<&str>()) {
            let error = StorageError::ReplayCleanup(error);
            self.log_startup_cleanup_failure(None, &error);
        }

        for (match_id, replay_path) in unfinished {
            let mut artifacts_clean = true;
            if let Some(replay_path) = replay_path {
                match self.resolve_replay_path(&replay_path) {
                    Ok(path) => {
                        if let Err(error) = remove_if_exists(&path) {
                            artifacts_clean = false;
                            self.log_startup_cleanup_failure(Some(&match_id), &error);
                        }
                    }
                    Err(StorageError::UnsafeReplayPath) => {
                        // Never touch a path rejected by the root/symlink
                        // checks. The row itself is still safe to repair.
                        self.mark_replay_degraded();
                        tracing::warn!(
                            match_id = %redact_audit_target_id(&match_id),
                            error_kind = "unsafe_path",
                            "unsafe incomplete replay path ignored during startup cleanup"
                        );
                    }
                    Err(error) => {
                        artifacts_clean = false;
                        self.log_startup_cleanup_failure(Some(&match_id), &error);
                    }
                }
            }
            if let Err(error) = remove_match_files(&self.replay_root, &match_id) {
                artifacts_clean = false;
                self.log_startup_cleanup_failure(Some(&match_id), &error);
            }
            if !artifacts_clean {
                continue;
            }
            if let Err(error) = sqlx::query(
                "DELETE FROM matches WHERE match_id = ? AND status IN ('writing', 'failed')",
            )
            .bind(&match_id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::Sqlx)
            {
                self.log_startup_cleanup_failure(Some(&match_id), &error);
            }
        }
        Ok(())
    }

    fn log_startup_cleanup_failure(&self, match_id: Option<&str>, error: &StorageError) {
        self.mark_replay_degraded();
        if let Some(match_id) = match_id {
            tracing::warn!(
                match_id = %redact_audit_target_id(match_id),
                error_kind = error.replay_failure_kind(),
                "incomplete replay startup cleanup deferred"
            );
        } else {
            tracing::warn!(
                error_kind = error.replay_failure_kind(),
                "incomplete replay startup cleanup deferred"
            );
        }
    }

    async fn validate_completed_replays(&self) -> Result<(), StorageError> {
        let mut rows = sqlx::query(
            "SELECT match_id, source, room_name, game_mode, started_at, completed_at, replay_path, file_size
             FROM matches WHERE status = 'completed'",
        )
        .fetch(&self.pool);
        while let Some(row) = rows.try_next().await.map_err(StorageError::Sqlx)? {
            let summary = ReplaySummary {
                match_id: row.try_get("match_id").map_err(StorageError::Sqlx)?,
                source: row.try_get("source").map_err(StorageError::Sqlx)?,
                room_name: row.try_get("room_name").map_err(StorageError::Sqlx)?,
                game_mode: row.try_get("game_mode").map_err(StorageError::Sqlx)?,
                started_at: row.try_get("started_at").map_err(StorageError::Sqlx)?,
                completed_at: row.try_get("completed_at").map_err(StorageError::Sqlx)?,
                file_size: row.try_get("file_size").map_err(StorageError::Sqlx)?,
                availability: "available",
            };
            let match_id = summary.match_id.clone();
            let replay_path = row
                .try_get::<String, _>("replay_path")
                .map_err(StorageError::Sqlx)?;
            let result = self
                .load_replay_view(summary, &replay_path)
                .await
                .and_then(|replay| self.encode_replay_view(&replay).map(|_| replay));
            if let Err(error) = result {
                self.mark_replay_degraded();
                let safe_match_id = redact_audit_target_id(&match_id);
                tracing::warn!(
                    match_id = %safe_match_id,
                    error_kind = error.replay_failure_kind(),
                    "completed replay failed startup validation"
                );
            }
        }
        Ok(())
    }

    pub async fn load_bot_tokens(&self) -> Result<Vec<BotTokenRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT token_id, name, token_hash, state, created_at, revoked_at FROM bot_tokens ORDER BY token_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        rows.into_iter()
            .map(|row| {
                let token_id = row
                    .try_get::<String, _>("token_id")
                    .map_err(StorageError::Sqlx)?;
                let name = row
                    .try_get::<String, _>("name")
                    .map_err(StorageError::Sqlx)?;
                let hash = row
                    .try_get::<Vec<u8>, _>("token_hash")
                    .map_err(StorageError::Sqlx)?;
                let token_hash: [u8; 32] = hash
                    .try_into()
                    .map_err(|_| StorageError::InvalidTokenRecord)?;
                let state = match row
                    .try_get::<String, _>("state")
                    .map_err(StorageError::Sqlx)?
                    .as_str()
                {
                    "active" => TokenState::Active,
                    "revoked" => TokenState::Revoked,
                    _ => return Err(StorageError::InvalidTokenRecord),
                };
                let created_at = row
                    .try_get::<i64, _>("created_at")
                    .map_err(StorageError::Sqlx)?;
                let revoked_at = row
                    .try_get::<Option<i64>, _>("revoked_at")
                    .map_err(StorageError::Sqlx)?;
                Ok(BotTokenRecord::new(
                    token_id, name, token_hash, state, created_at, revoked_at,
                ))
            })
            .collect()
    }

    pub(crate) async fn insert_bot_token(
        &self,
        record: &BotTokenRecord,
        request_id: &str,
    ) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        sqlx::query(
            "INSERT INTO bot_tokens (token_id, name, token_hash, state, created_at, revoked_at) VALUES (?, ?, ?, 'active', ?, NULL)",
        )
        .bind(record.token_id())
        .bind(record.name())
        .bind(record.token_hash().as_slice())
        .bind(record.created_at())
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        insert_audit_tx(
            &mut transaction,
            record.created_at(),
            request_id,
            "token_create",
            "bot_token",
            record.token_id(),
            &json!({"name": record.name()}),
        )
        .await?;
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn revoke_bot_token(
        &self,
        token_id: &str,
        occurred_at: i64,
        request_id: &str,
    ) -> Result<RevokeOutcome, StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        let row = sqlx::query("SELECT name, state FROM bot_tokens WHERE token_id = ?")
            .bind(token_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        let Some(row) = row else {
            return Ok(RevokeOutcome::NotFound);
        };
        let name = row
            .try_get::<String, _>("name")
            .map_err(StorageError::Sqlx)?;
        let state = row
            .try_get::<String, _>("state")
            .map_err(StorageError::Sqlx)?;
        if state == "revoked" {
            return Ok(RevokeOutcome::AlreadyRevoked);
        }
        sqlx::query("UPDATE bot_tokens SET state = 'revoked', revoked_at = ? WHERE token_id = ? AND state = 'active'")
            .bind(occurred_at)
            .bind(token_id)
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        insert_audit_tx(
            &mut transaction,
            occurred_at,
            request_id,
            "token_revoke",
            "bot_token",
            token_id,
            &json!({"name": name, "token_id": token_id}),
        )
        .await?;
        transaction.commit().await.map_err(StorageError::Sqlx)?;
        Ok(RevokeOutcome::Revoked { name })
    }

    pub async fn insert_audit(
        &self,
        occurred_at: i64,
        request_id: &str,
        action: &str,
        target_type: &str,
        target_id: &str,
        summary: Value,
    ) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        insert_audit_tx(
            &mut transaction,
            occurred_at,
            request_id,
            action,
            target_type,
            target_id,
            &summary,
        )
        .await?;
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub async fn cleanup_audit(&self, now: i64) -> Result<u64, StorageError> {
        let cutoff = now.saturating_sub(AUDIT_RETENTION_SECONDS);
        let result = sqlx::query("DELETE FROM audit_logs WHERE occurred_at <= ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(StorageError::Sqlx)?;
        Ok(result.rows_affected())
    }

    pub(crate) async fn preflight_audit(
        &self,
        occurred_at: i64,
        request_id: &str,
        action: &str,
        target_type: &str,
        target_id: &str,
        summary: &Value,
    ) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        insert_audit_tx(
            &mut transaction,
            occurred_at,
            request_id,
            action,
            target_type,
            target_id,
            summary,
        )
        .await?;
        transaction.rollback().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn prepare_admin_audit(
        &self,
        occurred_at: i64,
        request_id: &str,
        action: &str,
        target_type: &str,
        target_id: &str,
        summary: &Value,
    ) -> Result<(), StorageError> {
        self.preflight_audit(
            occurred_at,
            request_id,
            action,
            target_type,
            target_id,
            summary,
        )
        .await?;
        let summary_json = audit_summary_json(request_id, action, target_id, summary)?;
        sqlx::query(
            "INSERT INTO admin_audit_pending (request_id, occurred_at, action, target_type, target_id, summary_json, state)
             VALUES (?, ?, ?, ?, ?, ?, 'prepared')",
        )
        .bind(request_id)
        .bind(occurred_at)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(summary_json)
        .execute(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        Ok(())
    }

    pub(crate) async fn cancel_admin_audit(&self, request_id: &str) -> Result<(), StorageError> {
        let deleted = sqlx::query(
            "DELETE FROM admin_audit_pending WHERE request_id = ? AND state = 'prepared'",
        )
        .bind(request_id)
        .execute(&self.pool)
        .await;
        match deleted {
            Ok(result) if result.rows_affected() == 1 => Ok(()),
            Ok(_) => self.ensure_admin_audit_cancelled(request_id).await,
            Err(_) => {
                let rolled_back = sqlx::query(
                    "UPDATE admin_audit_pending SET state = 'rolled_back'
                     WHERE request_id = ? AND state = 'prepared'",
                )
                .bind(request_id)
                .execute(&self.pool)
                .await;
                match rolled_back {
                    Ok(result) if result.rows_affected() == 1 => Ok(()),
                    Ok(_) => self.ensure_admin_audit_cancelled(request_id).await,
                    Err(error) => Err(StorageError::Sqlx(error)),
                }
            }
        }
    }

    async fn ensure_admin_audit_cancelled(&self, request_id: &str) -> Result<(), StorageError> {
        let state = sqlx::query_scalar::<_, String>(
            "SELECT state FROM admin_audit_pending WHERE request_id = ?",
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        match state.as_deref() {
            None | Some("rolled_back") => Ok(()),
            Some(_) => Err(StorageError::AuditPending),
        }
    }

    pub(crate) async fn rollback_admin_audit(&self, request_id: &str) -> Result<(), StorageError> {
        let updated = sqlx::query(
            "UPDATE admin_audit_pending SET state = 'rolled_back'
             WHERE request_id = ? AND state IN ('prepared', 'applied')",
        )
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        if updated.rows_affected() != 1 {
            return Err(StorageError::AuditPending);
        }
        Ok(())
    }

    pub(crate) async fn complete_admin_audit(&self, request_id: &str) -> Result<(), StorageError> {
        // The mutation has already been accepted when this is called. Flush the
        // prepared row directly so a separate prepared->applied update cannot
        // strand a successful mutation outside startup recovery.
        self.flush_pending_audit(request_id).await
    }

    async fn flush_pending_audit(&self, request_id: &str) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        let inserted = sqlx::query(
            "INSERT INTO audit_logs (occurred_at, request_id, action, target_type, target_id, summary_json)
             SELECT occurred_at, request_id, action, target_type, target_id, summary_json
             FROM admin_audit_pending
             WHERE request_id = ? AND state IN ('prepared', 'applied')
               AND NOT EXISTS (SELECT 1 FROM audit_logs WHERE request_id = ?)",
        )
        .bind(request_id)
        .bind(request_id)
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        if inserted.rows_affected() != 1 {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM audit_logs WHERE request_id = ?)",
            )
            .bind(request_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
            if exists == 0 {
                return Err(StorageError::AuditPending);
            }
        }
        sqlx::query(
            "DELETE FROM admin_audit_pending WHERE request_id = ? AND state IN ('prepared', 'applied')",
        )
        .bind(request_id)
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn retry_pending_audits(&self) -> Result<(), StorageError> {
        let request_ids = sqlx::query_scalar::<_, String>(
            "SELECT request_id FROM admin_audit_pending WHERE state IN ('prepared', 'applied') ORDER BY occurred_at, request_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        for request_id in request_ids {
            self.flush_pending_audit(&request_id).await?;
        }
        Ok(())
    }

    pub(crate) async fn list_replays(
        &self,
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<ReplaySummary>, u64), StorageError> {
        let total =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE status = 'completed'")
                .fetch_one(&self.pool)
                .await
                .map_err(StorageError::Sqlx)?;
        let rows = sqlx::query(
            "SELECT match_id, source, room_name, game_mode, started_at, completed_at, replay_path, file_size
             FROM matches WHERE status = 'completed' ORDER BY completed_at DESC, match_id DESC LIMIT ? OFFSET ?",
        )
        .bind(i64::try_from(limit).map_err(|_| StorageError::ReplayMetadata)?)
        .bind(i64::try_from(offset).map_err(|_| StorageError::ReplayMetadata)?)
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        let mut summaries = Vec::with_capacity(rows.len());
        for row in rows {
            let summary = ReplaySummary {
                match_id: row.try_get("match_id").map_err(StorageError::Sqlx)?,
                source: row.try_get("source").map_err(StorageError::Sqlx)?,
                room_name: row.try_get("room_name").map_err(StorageError::Sqlx)?,
                game_mode: row.try_get("game_mode").map_err(StorageError::Sqlx)?,
                started_at: row.try_get("started_at").map_err(StorageError::Sqlx)?,
                completed_at: row.try_get("completed_at").map_err(StorageError::Sqlx)?,
                file_size: row.try_get("file_size").map_err(StorageError::Sqlx)?,
                availability: "available",
            };
            let replay_path = row
                .try_get::<String, _>("replay_path")
                .map_err(StorageError::Sqlx)?;
            let availability = match self.replay_availability(&summary, &replay_path).await {
                Ok(availability) => availability,
                Err(error) => {
                    self.mark_replay_degraded();
                    let safe_match_id = redact_audit_target_id(&summary.match_id);
                    tracing::warn!(
                        match_id = %safe_match_id,
                        error_kind = error.replay_failure_kind(),
                        "completed replay failed list-time validation"
                    );
                    match error {
                        StorageError::ReplayTooLarge => "too_large",
                        _ => "unavailable",
                    }
                }
            };
            summaries.push(ReplaySummary {
                availability,
                ..summary
            });
        }
        Ok((summaries, u64::try_from(total).unwrap_or(0)))
    }

    pub(crate) async fn load_replay(&self, match_id: &str) -> Result<ReplayView, StorageError> {
        let row = sqlx::query(
            "SELECT match_id, source, room_name, game_mode, started_at, completed_at, replay_path, file_size
             FROM matches WHERE match_id = ? AND status = 'completed'",
        )
        .bind(match_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?
        .ok_or(StorageError::ReplayNotFound)?;
        let replay_path = row
            .try_get::<String, _>("replay_path")
            .map_err(StorageError::Sqlx)?;
        let summary = ReplaySummary {
            match_id: row.try_get("match_id").map_err(StorageError::Sqlx)?,
            source: row.try_get("source").map_err(StorageError::Sqlx)?,
            room_name: row.try_get("room_name").map_err(StorageError::Sqlx)?,
            game_mode: row.try_get("game_mode").map_err(StorageError::Sqlx)?,
            started_at: row.try_get("started_at").map_err(StorageError::Sqlx)?,
            completed_at: row.try_get("completed_at").map_err(StorageError::Sqlx)?,
            file_size: row.try_get("file_size").map_err(StorageError::Sqlx)?,
            availability: "available",
        };
        self.load_replay_view(summary, &replay_path).await
    }

    async fn load_replay_view(
        &self,
        summary: ReplaySummary,
        replay_path: &str,
    ) -> Result<ReplayView, StorageError> {
        let frames = self
            .reconstruct_replay(
                &summary.match_id,
                &summary.game_mode,
                replay_path,
                summary.file_size,
            )
            .await?;
        let players = self.load_players(&summary.match_id).await?;
        Ok(ReplayView {
            summary,
            players,
            frames,
        })
    }

    async fn load_players(&self, match_id: &str) -> Result<Vec<ReplayPlayer>, StorageError> {
        let stats = sqlx::query(
            "SELECT count(*) AS row_count,
                    COALESCE(sum(
                        length(CAST(participant_id AS BLOB))
                        + length(CAST(display_name AS BLOB))
                        + length(CAST(participant_kind AS BLOB))
                        + COALESCE(length(CAST(character_id AS BLOB)), 0)
                        + 64
                    ), 0) AS payload_bytes
             FROM match_players WHERE match_id = ?",
        )
        .bind(match_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        let row_count = stats
            .try_get::<i64, _>("row_count")
            .map_err(StorageError::Sqlx)?;
        let payload_bytes = stats
            .try_get::<i64, _>("payload_bytes")
            .map_err(StorageError::Sqlx)?;
        if row_count < 0
            || u64::try_from(row_count).unwrap_or(u64::MAX) > MAX_REPLAY_EVENTS as u64
            || payload_bytes < 0
            || u64::try_from(payload_bytes).unwrap_or(u64::MAX)
                > MAX_DECOMPRESSED_REPLAY_BYTES as u64
        {
            self.mark_replay_degraded();
            return Err(StorageError::ReplayTooLarge);
        }
        let mut rows = sqlx::query(
            "SELECT participant_id, display_name, participant_kind, seat, character_id, final_points
             FROM match_players WHERE match_id = ? ORDER BY seat",
        )
        .bind(match_id)
        .fetch(&self.pool);
        let mut players = Vec::with_capacity(usize::try_from(row_count).unwrap_or(0));
        while let Some(row) = rows.try_next().await.map_err(StorageError::Sqlx)? {
            players.push(ReplayPlayer {
                participant_id: row.try_get("participant_id").map_err(StorageError::Sqlx)?,
                display_name: row.try_get("display_name").map_err(StorageError::Sqlx)?,
                participant_kind: row
                    .try_get("participant_kind")
                    .map_err(StorageError::Sqlx)?,
                seat: row.try_get("seat").map_err(StorageError::Sqlx)?,
                character_id: row.try_get("character_id").map_err(StorageError::Sqlx)?,
                final_points: row.try_get("final_points").map_err(StorageError::Sqlx)?,
            });
        }
        Ok(players)
    }

    pub(crate) fn encode_replay_view(&self, replay: &ReplayView) -> Result<Vec<u8>, StorageError> {
        let response = ReplayResponse {
            match_id: &replay.summary.match_id,
            source: &replay.summary.source,
            room_name: &replay.summary.room_name,
            game_mode: &replay.summary.game_mode,
            started_at: unix_seconds_rfc3339(replay.summary.started_at),
            completed_at: unix_seconds_rfc3339(replay.summary.completed_at),
            file_size: replay.summary.file_size,
            availability: replay.summary.availability,
            replay_available: replay.summary.availability == "available",
            players: &replay.players,
            frames: &replay.frames,
        };
        let mut writer = LimitedJsonWriter::new(MAX_DECOMPRESSED_REPLAY_BYTES);
        match serde_json::to_writer(&mut writer, &response) {
            Ok(()) => Ok(writer.into_inner()),
            Err(_error) if writer.overflowed => {
                self.mark_replay_degraded();
                Err(StorageError::ReplayTooLarge)
            }
            Err(_) => Err(StorageError::ReplayMetadata),
        }
    }

    async fn reconstruct_replay(
        &self,
        match_id: &str,
        game_mode: &str,
        replay_path: &str,
        file_size: i64,
    ) -> Result<Vec<ReplayFrame>, StorageError> {
        if file_size < 0 || file_size as u64 > MAX_DECOMPRESSED_REPLAY_BYTES as u64 {
            self.mark_replay_degraded();
            return Err(StorageError::ReplayTooLarge);
        }
        let path = self
            .resolve_replay_path(replay_path)
            .inspect_err(|_error| {
                self.mark_replay_degraded();
            })?;
        let metadata = fs::metadata(&path).map_err(|error| {
            self.mark_replay_degraded();
            if error.kind() == std::io::ErrorKind::NotFound {
                StorageError::ReplayUnavailable
            } else {
                StorageError::Io(error)
            }
        })?;
        if metadata.len() > MAX_DECOMPRESSED_REPLAY_BYTES as u64 {
            self.mark_replay_degraded();
            return Err(StorageError::ReplayTooLarge);
        }
        let mode = game_mode
            .parse::<GameMode>()
            .map_err(|_| StorageError::ReplayCorrupt)?;
        let reader = ReplayReader::open(&path).map_err(|error| {
            self.mark_replay_degraded();
            match error {
                ReplayError::ReplayTooLarge { .. } => StorageError::ReplayTooLarge,
                ReplayError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                    StorageError::ReplayUnavailable
                }
                _ => StorageError::ReplayCorrupt,
            }
        })?;
        let auxiliary = self.load_auxiliary(match_id).await.inspect_err(|_error| {
            self.mark_replay_degraded();
        })?;
        reader
            .frames_with_auxiliary_for_mode(&auxiliary, mode)
            .map_err(|error| {
                self.mark_replay_degraded();
                match error {
                    ReplayError::ReplayTooLarge { .. } => StorageError::ReplayTooLarge,
                    _ => StorageError::ReplayCorrupt,
                }
            })
    }

    async fn load_auxiliary(&self, match_id: &str) -> Result<Vec<AuxiliaryRecord>, StorageError> {
        let stats = sqlx::query(
            "SELECT count(*) AS row_count, COALESCE(sum(length(CAST(payload_json AS BLOB))), 0) AS payload_bytes
             FROM replay_auxiliary_events WHERE match_id = ?",
        )
        .bind(match_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        let row_count = stats
            .try_get::<i64, _>("row_count")
            .map_err(StorageError::Sqlx)?;
        let payload_bytes = stats
            .try_get::<i64, _>("payload_bytes")
            .map_err(StorageError::Sqlx)?;
        if row_count < 0
            || u64::try_from(row_count).unwrap_or(u64::MAX) > MAX_REPLAY_EVENTS as u64
            || payload_bytes < 0
            || u64::try_from(payload_bytes).unwrap_or(u64::MAX)
                > MAX_DECOMPRESSED_REPLAY_BYTES as u64
        {
            self.mark_replay_degraded();
            return Err(StorageError::ReplayTooLarge);
        }
        let mut rows = sqlx::query(
            "SELECT line_index, phase, sequence, payload_json FROM replay_auxiliary_events
             WHERE match_id = ? ORDER BY line_index, CASE phase WHEN 'before' THEN 0 ELSE 2 END, sequence",
        )
        .bind(match_id)
        .fetch(&self.pool);
        let mut auxiliary = Vec::with_capacity(usize::try_from(row_count).unwrap_or(0));
        while let Some(row) = rows.try_next().await.map_err(StorageError::Sqlx)? {
            let line_index = row
                .try_get::<i64, _>("line_index")
                .map_err(StorageError::Sqlx)?;
            let sequence = row
                .try_get::<i64, _>("sequence")
                .map_err(StorageError::Sqlx)?;
            let phase = row
                .try_get::<String, _>("phase")
                .map_err(StorageError::Sqlx)?;
            let payload = row
                .try_get::<String, _>("payload_json")
                .map_err(StorageError::Sqlx)?;
            let phase = serde_json::from_value::<AuxiliaryPhase>(Value::String(phase))
                .map_err(|_| StorageError::ReplayCorrupt)?;
            let event = serde_json::from_str(&payload).map_err(|_| StorageError::ReplayCorrupt)?;
            auxiliary.push(AuxiliaryRecord {
                event,
                line_index: usize::try_from(line_index).map_err(|_| StorageError::ReplayCorrupt)?,
                phase,
                sequence: u64::try_from(sequence).map_err(|_| StorageError::ReplayCorrupt)?,
            });
        }
        Ok(auxiliary)
    }

    async fn replay_availability(
        &self,
        summary: &ReplaySummary,
        replay_path: &str,
    ) -> Result<&'static str, StorageError> {
        let replay = self.load_replay_view(summary.clone(), replay_path).await?;
        self.encode_replay_view(&replay)?;
        Ok("available")
    }

    pub(crate) async fn delete_replay(
        &self,
        match_id: &str,
        occurred_at: i64,
        request_id: &str,
    ) -> Result<(), StorageError> {
        let replay_path = sqlx::query_scalar::<_, String>(
            "SELECT replay_path FROM matches WHERE match_id = ? AND status = 'completed'",
        )
        .bind(match_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?
        .ok_or(StorageError::ReplayNotFound)?;
        match self.resolve_replay_path(&replay_path) {
            Ok(path) => remove_if_exists(&path)?,
            Err(StorageError::UnsafeReplayPath) => {}
            Err(error) => return Err(error),
        }
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        let deleted =
            sqlx::query("DELETE FROM matches WHERE match_id = ? AND status = 'completed'")
                .bind(match_id)
                .execute(&mut *transaction)
                .await
                .map_err(StorageError::Sqlx)?;
        if deleted.rows_affected() != 1 {
            return Err(StorageError::ReplayNotFound);
        }
        insert_audit_tx(
            &mut transaction,
            occurred_at,
            request_id,
            "replay_delete",
            "match",
            match_id,
            &json!({"match_id": match_id}),
        )
        .await?;
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn open_ranked_match(
        &self,
        match_id: &str,
        mode: GameMode,
        started_at: i64,
        replay_path: &str,
        players: &[Participant],
    ) -> Result<(), StorageError> {
        self.resolve_replay_path(replay_path)?;
        if players.len() != mode.seat_count() {
            return Err(StorageError::ReplayMetadata);
        }
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        sqlx::query(
            "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status, replay_path) VALUES (?, 'ranked', NULL, ?, ?, 'writing', ?)",
        )
        .bind(match_id)
        .bind(mode.as_str())
        .bind(started_at)
        .bind(replay_path)
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        for (seat, player) in players.iter().enumerate() {
            sqlx::query(
                "INSERT INTO match_players (match_id, participant_id, display_name, participant_kind, seat, character_id) VALUES (?, ?, ?, ?, ?, NULL)",
            )
            .bind(match_id)
            .bind(player.id.as_str())
            .bind(&player.display_name)
            .bind(participant_kind(player))
            .bind(i64::try_from(seat).map_err(|_| StorageError::ReplayMetadata)?)
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        }
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn open_room_match(
        &self,
        match_id: &str,
        mode: GameMode,
        room_name: &str,
        started_at: i64,
        replay_path: &str,
        roster: &[MatchPlayerSnapshot],
    ) -> Result<(), StorageError> {
        self.resolve_replay_path(replay_path)?;
        if roster.len() != mode.seat_count() {
            return Err(StorageError::ReplayMetadata);
        }
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        sqlx::query(
            "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status, replay_path) VALUES (?, 'room', ?, ?, ?, 'writing', ?)",
        )
        .bind(match_id)
        .bind(room_name)
        .bind(mode.as_str())
        .bind(started_at)
        .bind(replay_path)
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        for player in roster {
            sqlx::query(
                "INSERT INTO match_players (match_id, participant_id, display_name, participant_kind, seat, character_id) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(match_id)
            .bind(player.participant_id.as_str())
            .bind(&player.display_name)
            .bind(participant_kind_kind(player.kind))
            .bind(i64::from(player.seat.index()))
            .bind(&player.character_id)
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        }
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn complete_ranked_match(
        &self,
        match_id: &str,
        artifact: &ReplayArtifact,
        result: &MatchResult,
        completed_at: i64,
    ) -> Result<(), StorageError> {
        self.complete_match("ranked", match_id, artifact, result, completed_at)
            .await
    }

    pub(crate) async fn complete_room_match(
        &self,
        match_id: &str,
        artifact: &ReplayArtifact,
        result: &MatchResult,
        completed_at: i64,
    ) -> Result<(), StorageError> {
        self.complete_match("room", match_id, artifact, result, completed_at)
            .await
    }

    async fn complete_match(
        &self,
        source: &str,
        match_id: &str,
        artifact: &ReplayArtifact,
        result: &MatchResult,
        completed_at: i64,
    ) -> Result<(), StorageError> {
        self.resolve_replay_path(&artifact.relative_path_string())?;
        let file_size =
            i64::try_from(artifact.file_size).map_err(|_| StorageError::ReplayMetadata)?;
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        let updated = sqlx::query(
            "UPDATE matches SET completed_at = ?, status = 'completed', replay_path = ?, file_size = ? WHERE match_id = ? AND source = ? AND status = 'writing'",
        )
        .bind(completed_at)
        .bind(artifact.relative_path_string())
        .bind(file_size)
        .bind(match_id)
        .bind(source)
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        if updated.rows_affected() != 1 {
            return Err(StorageError::ReplayMetadata);
        }
        for player in &result.players {
            sqlx::query(
                "UPDATE match_players SET final_points = ? WHERE match_id = ? AND seat = ?",
            )
            .bind(player.final_score)
            .bind(match_id)
            .bind(i64::from(player.seat.index()))
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        }
        for auxiliary in &artifact.auxiliary_events {
            let phase = match auxiliary.phase {
                AuxiliaryPhase::Before => "before",
                AuxiliaryPhase::After => "after",
            };
            let payload = serde_json::to_string(&auxiliary.event)
                .map_err(|_| StorageError::ReplayMetadata)?;
            sqlx::query(
                "INSERT INTO replay_auxiliary_events (match_id, line_index, phase, sequence, payload_json) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(match_id)
            .bind(i64::try_from(auxiliary.line_index).map_err(|_| StorageError::ReplayMetadata)?)
            .bind(phase)
            .bind(i64::try_from(auxiliary.sequence).map_err(|_| StorageError::ReplayMetadata)?)
            .bind(payload)
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        }
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn mark_match_failed(&self, match_id: &str) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE matches SET status = 'failed' WHERE match_id = ? AND status = 'writing'",
        )
        .bind(match_id)
        .execute(&self.pool)
        .await
        .map_err(StorageError::Sqlx)?;
        Ok(())
    }

    pub(crate) async fn delete_incomplete_match(&self, match_id: &str) -> Result<(), StorageError> {
        remove_match_files(&self.replay_root, match_id)?;
        sqlx::query("DELETE FROM matches WHERE match_id = ? AND status IN ('writing', 'failed')")
            .bind(match_id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::Sqlx)?;
        Ok(())
    }

    pub(crate) async fn delete_writing_match(&self, match_id: &str) -> Result<(), StorageError> {
        self.delete_incomplete_match(match_id).await
    }

    pub(crate) fn remove_replay_file(&self, relative_path: &str) -> Result<(), StorageError> {
        let path = self.resolve_replay_path(relative_path)?;
        remove_if_exists(&path)
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

enum RoomReplay {
    Writing(ReplayWriter),
    Failed(String),
}

pub fn spawn_room_effect_worker(
    storage: Arc<Storage>,
    mut effects: mpsc::Receiver<RoomEffect>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut replays = HashMap::<String, RoomReplay>::new();
        while let Some(effect) = effects.recv().await {
            match effect {
                RoomEffect::OpenMatch {
                    match_id,
                    mode,
                    room_name,
                    roster,
                    initial_events,
                    started_at,
                    completion,
                } => {
                    let id = match_id.to_string();
                    match open_room_replay(
                        &storage,
                        &id,
                        mode,
                        &room_name,
                        &roster,
                        initial_events,
                        started_at,
                    )
                    .await
                    {
                        Ok(writer) => {
                            replays.insert(id, RoomReplay::Writing(writer));
                            let _ = completion.send(Ok(()));
                        }
                        Err(error) => {
                            storage.mark_replay_degraded();
                            let _ = completion.send(Err(error));
                        }
                    }
                }
                RoomEffect::AppendEvents {
                    match_id,
                    events,
                    failure_sender,
                } => {
                    let id = match_id.to_string();
                    let failure = match replays.get_mut(&id) {
                        Some(RoomReplay::Writing(writer)) => {
                            let mut failure = None;
                            for event in events {
                                if let Err(error) = writer.append(event) {
                                    failure = Some(error.to_string());
                                    break;
                                }
                            }
                            failure
                        }
                        Some(RoomReplay::Failed(error)) => Some(error.clone()),
                        None => Some("replay was not opened".to_owned()),
                    };
                    if let Some(error) = failure {
                        storage.mark_replay_degraded();
                        let newly_failed =
                            mark_room_replay_failed(&mut replays, &id, error.clone());
                        if newly_failed
                            && let Err(mark_error) = storage.mark_match_failed(&id).await
                        {
                            tracing::warn!(error = ?mark_error, "room replay failure metadata update deferred");
                        }
                        notify_room_failure(
                            failure_sender,
                            RoomPersistenceFailure { match_id, error },
                        );
                    }
                }
                RoomEffect::RecordAuxiliary {
                    match_id,
                    event,
                    phase,
                    failure_sender,
                } => {
                    let id = match_id.to_string();
                    let result = match replays.get_mut(&id) {
                        Some(RoomReplay::Writing(writer)) => writer
                            .record_auxiliary(
                                to_replay_auxiliary_event(event),
                                to_replay_auxiliary_phase(phase),
                            )
                            .map(|_| ())
                            .map_err(|error| error.to_string()),
                        Some(RoomReplay::Failed(error)) => Err(error.clone()),
                        None => Err("replay was not opened".to_owned()),
                    };
                    if let Err(error) = result {
                        storage.mark_replay_degraded();
                        let newly_failed =
                            mark_room_replay_failed(&mut replays, &id, error.clone());
                        if newly_failed
                            && let Err(mark_error) = storage.mark_match_failed(&id).await
                        {
                            tracing::warn!(error = ?mark_error, "room replay failure metadata update deferred");
                        }
                        notify_room_failure(
                            failure_sender,
                            RoomPersistenceFailure { match_id, error },
                        );
                    }
                }
                RoomEffect::FlushKyoku {
                    match_id,
                    completion,
                } => {
                    let id = match_id.to_string();
                    let result = match replays.get_mut(&id) {
                        Some(RoomReplay::Writing(writer)) => writer
                            .flush_kyoku()
                            .map_err(|error| room_effect_error(error.to_string())),
                        Some(RoomReplay::Failed(error)) => Err(room_effect_error(error.clone())),
                        None => Err(room_effect_error("replay was not opened")),
                    };
                    if let Err(error) = &result {
                        storage.mark_replay_degraded();
                        let newly_failed =
                            mark_room_replay_failed(&mut replays, &id, error.to_string());
                        if newly_failed
                            && let Err(mark_error) = storage.mark_match_failed(&id).await
                        {
                            tracing::warn!(error = ?mark_error, "room replay failure metadata update deferred");
                        }
                    }
                    let _ = completion.send(result);
                }
                RoomEffect::FinalizeMatch {
                    match_id,
                    result,
                    completed_at,
                    completion,
                } => {
                    let id = match_id.to_string();
                    let replay = replays.remove(&id);
                    let persisted =
                        finalize_room_replay(&storage, &id, replay, &result, completed_at).await;
                    let _ = completion.send(persisted);
                }
                RoomEffect::DeleteIncomplete { match_id } => {
                    let id = match_id.to_string();
                    let _ = cleanup_room_replay(&storage, &mut replays, &id).await;
                }
                RoomEffect::CleanupIncomplete {
                    match_id,
                    completion,
                } => {
                    let id = match_id.to_string();
                    let result = cleanup_room_replay(&storage, &mut replays, &id).await;
                    let _ = completion.send(result);
                }
            }
        }
        for (id, replay) in replays {
            if let RoomReplay::Writing(writer) = replay {
                writer.abort();
            }
            if let Err(error) = storage.delete_incomplete_match(&id).await {
                storage.mark_replay_degraded();
                tracing::warn!(error = ?error, "room replay shutdown cleanup deferred");
            }
        }
    })
}

async fn finalize_room_replay(
    storage: &Storage,
    match_id: &str,
    replay: Option<RoomReplay>,
    result: &double_riichi_core::MatchResult,
    completed_at: i64,
) -> Result<(), RoomEffectError> {
    let replay = match replay {
        Some(replay) => replay,
        None => {
            storage.mark_replay_degraded();
            return Err(room_effect_error("replay was not opened"));
        }
    };
    let writer = match replay {
        RoomReplay::Writing(writer) => writer,
        RoomReplay::Failed(error) => {
            storage.mark_replay_degraded();
            let _ = storage.mark_match_failed(match_id).await;
            cleanup_incomplete_after_failure(storage, match_id).await;
            return Err(room_effect_error(error));
        }
    };
    let artifact = match writer.finalize() {
        Ok(artifact) => artifact,
        Err(error) => {
            storage.mark_replay_degraded();
            let _ = storage.mark_match_failed(match_id).await;
            cleanup_incomplete_after_failure(storage, match_id).await;
            return Err(room_effect_error(error.to_string()));
        }
    };
    match storage
        .complete_room_match(match_id, &artifact, result, completed_at)
        .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            storage.mark_replay_degraded();
            let _ = storage.mark_match_failed(match_id).await;
            cleanup_incomplete_after_failure(storage, match_id).await;
            Err(room_effect_error(error.to_string()))
        }
    }
}

async fn cleanup_incomplete_after_failure(storage: &Storage, match_id: &str) {
    if let Err(error) = storage.delete_incomplete_match(match_id).await {
        storage.mark_replay_degraded();
        tracing::warn!(error = ?error, "room replay cleanup deferred after persistence failure");
    }
}

async fn cleanup_room_replay(
    storage: &Storage,
    replays: &mut HashMap<String, RoomReplay>,
    match_id: &str,
) -> Result<(), RoomEffectError> {
    if let Some(replay) = replays.remove(match_id)
        && let RoomReplay::Writing(writer) = replay
    {
        writer.abort();
    }
    match storage.delete_incomplete_match(match_id).await {
        Ok(()) => Ok(()),
        Err(error) => {
            storage.mark_replay_degraded();
            tracing::warn!(
                error_kind = error.replay_failure_kind(),
                "room replay cleanup deferred"
            );
            Err(room_effect_error(error.to_string()))
        }
    }
}

async fn open_room_replay(
    storage: &Storage,
    match_id: &str,
    mode: GameMode,
    room_name: &str,
    roster: &[MatchPlayerSnapshot],
    initial_events: Vec<double_riichi_core::GameEvent>,
    started_at: i64,
) -> Result<ReplayWriter, RoomEffectError> {
    let mut writer = ReplayWriter::new(storage.replay_root(), match_id, mode)
        .map_err(|error| room_effect_error(error.to_string()))?;
    for event in initial_events {
        if let Err(error) = writer.append(event) {
            writer.abort();
            return Err(room_effect_error(error.to_string()));
        }
    }
    if let Err(error) = storage
        .open_room_match(
            match_id,
            mode,
            room_name,
            started_at,
            &writer.relative_path_string(),
            roster,
        )
        .await
    {
        writer.abort();
        return Err(room_effect_error(error.to_string()));
    }
    Ok(writer)
}

fn to_replay_auxiliary_event(event: RoomAuxiliaryEvent) -> double_riichi_replay::AuxiliaryEvent {
    match event {
        RoomAuxiliaryEvent::Disconnected { seat } => {
            double_riichi_replay::AuxiliaryEvent::Disconnected { seat }
        }
        RoomAuxiliaryEvent::Reconnected { seat } => {
            double_riichi_replay::AuxiliaryEvent::Reconnected { seat }
        }
        RoomAuxiliaryEvent::AutoStarted { seat } => {
            double_riichi_replay::AuxiliaryEvent::AutoStarted { seat }
        }
        RoomAuxiliaryEvent::Left { seat } => double_riichi_replay::AuxiliaryEvent::Left { seat },
        RoomAuxiliaryEvent::TokenRevoked { seat } => {
            double_riichi_replay::AuxiliaryEvent::TokenRevoked { seat }
        }
        RoomAuxiliaryEvent::Kicked { seat } => {
            double_riichi_replay::AuxiliaryEvent::Kicked { seat }
        }
    }
}

fn to_replay_auxiliary_phase(phase: RoomAuxiliaryPhase) -> AuxiliaryPhase {
    match phase {
        RoomAuxiliaryPhase::Before => AuxiliaryPhase::Before,
        RoomAuxiliaryPhase::After => AuxiliaryPhase::After,
    }
}

fn notify_room_failure(
    sender: mpsc::Sender<RoomPersistenceFailure>,
    failure: RoomPersistenceFailure,
) {
    match sender.try_send(failure) {
        Ok(()) | Err(mpsc::error::TrySendError::Closed(_)) => {}
        Err(mpsc::error::TrySendError::Full(failure)) => {
            tokio::spawn(async move {
                let _ = sender.send(failure).await;
            });
        }
    }
}

fn mark_room_replay_failed(
    replays: &mut HashMap<String, RoomReplay>,
    match_id: &str,
    error: String,
) -> bool {
    let Some(replay) = replays.get_mut(match_id) else {
        return false;
    };
    if matches!(replay, RoomReplay::Failed(_)) {
        return false;
    }
    let previous = std::mem::replace(replay, RoomReplay::Failed(error));
    if let RoomReplay::Writing(writer) = previous {
        writer.abort();
    }
    true
}

fn room_effect_error(error: impl Into<String>) -> RoomEffectError {
    RoomEffectError::Failed(error.into())
}

struct LimitedJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl LimitedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            overflowed: false,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for LimitedJsonWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(total) = self.bytes.len().checked_add(bytes.len()) else {
            self.overflowed = true;
            return Err(io::Error::other("replay response size overflow"));
        };
        if total > self.limit {
            self.overflowed = true;
            return Err(io::Error::other("replay response size limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn unix_seconds_rfc3339(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        day_seconds / 3_600,
        (day_seconds / 60) % 60,
        day_seconds % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u8, day as u8)
}

fn audit_summary_json(
    request_id: &str,
    action: &str,
    target_id: &str,
    summary: &Value,
) -> Result<String, StorageError> {
    if request_id.is_empty()
        || target_id.is_empty()
        || string_contains_raw_token(request_id)
        || string_contains_raw_token(target_id)
        || !validate_audit_summary(action, summary)
    {
        return Err(StorageError::InvalidAuditSummary);
    }
    serde_json::to_string(summary).map_err(|_| StorageError::InvalidAuditSummary)
}

async fn insert_audit_tx(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    occurred_at: i64,
    request_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
    summary: &Value,
) -> Result<(), StorageError> {
    let summary_json = audit_summary_json(request_id, action, target_id, summary)?;
    sqlx::query(
        "INSERT INTO audit_logs (occurred_at, request_id, action, target_type, target_id, summary_json) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(occurred_at)
    .bind(request_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(summary_json)
    .execute(&mut **transaction)
    .await
    .map_err(StorageError::Sqlx)?;
    Ok(())
}

fn participant_kind(participant: &Participant) -> &'static str {
    participant_kind_kind(participant.kind)
}

fn participant_kind_kind(kind: ParticipantKind) -> &'static str {
    match kind {
        ParticipantKind::Human => "human",
        ParticipantKind::MJAI => "mjai",
        ParticipantKind::MCP => "mcp",
        ParticipantKind::BuiltInBot => "builtin_bot",
    }
}

pub(crate) fn redact_audit_target_id(target_id: &str) -> String {
    if string_contains_raw_token(target_id) {
        "[REDACTED]".to_owned()
    } else {
        target_id.to_owned()
    }
}

pub(crate) fn string_contains_raw_token(value: &str) -> bool {
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

fn remove_if_exists(path: &Path) -> Result<(), StorageError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::Io(error)),
    }
}

fn remove_match_files(root: &Path, match_id: &str) -> Result<(), StorageError> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(StorageError::Io)? {
        let entry = entry.map_err(StorageError::Io)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(StorageError::Io)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            remove_match_files(&path, match_id)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let part_name = format!("{match_id}.mjson.part");
        let completed_suffix = format!("_{match_id}.mjson");
        if name == part_name || name.ends_with(&completed_suffix) {
            remove_if_exists(&path)?;
        }
    }
    Ok(())
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use double_riichi_core::{
        GameEvent, MatchPlayerResult, Participant, ParticipantKind, RoomAuxiliaryEvent, Seat, Tile,
        Wind,
    };
    use double_riichi_replay::{AuxiliaryEvent, AuxiliaryPhase, ReplayWriter};

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "double-riichi-storage-task15-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn events() -> Vec<GameEvent> {
        vec![
            GameEvent::StartGame {
                names: Some(vec![
                    "East".into(),
                    "South".into(),
                    "West".into(),
                    "North".into(),
                ]),
                id: Some("ranked-aux15".into()),
            },
            GameEvent::StartKyoku {
                bakaze: Wind::East,
                kyoku: 1,
                honba: 0,
                kyotaku: 0,
                oya: Seat::new(0).unwrap(),
                scores: vec![25_000; 4],
                dora_marker: Tile::from_id(0).unwrap(),
                tehais: vec![vec![Tile::from_id(0).unwrap(); 13]; 4],
            },
            GameEvent::EndKyoku,
            GameEvent::EndGame,
        ]
    }

    fn players() -> Vec<Participant> {
        (0..4)
            .map(|seat| {
                Participant::new(
                    format!("ranked{seat}"),
                    format!("Ranked {seat}"),
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect()
    }

    #[test]
    fn kicked_room_auxiliary_event_maps_to_kicked_replay_event() {
        assert_eq!(
            to_replay_auxiliary_event(RoomAuxiliaryEvent::Kicked {
                seat: Seat::new(2).unwrap(),
            }),
            AuxiliaryEvent::Kicked {
                seat: Seat::new(2).unwrap(),
            }
        );
    }

    #[tokio::test]
    async fn ranked_completion_persists_auxiliary_events_in_frame_order() {
        let root = test_root("auxiliary");
        let storage = Storage::connect(&root).await.unwrap();
        let players = players();
        let mut writer = ReplayWriter::new(
            storage.replay_root(),
            "ranked-aux15",
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        writer
            .record_auxiliary(
                AuxiliaryEvent::Disconnected {
                    seat: Seat::new(1).unwrap(),
                },
                AuxiliaryPhase::Before,
            )
            .unwrap();
        writer.append(events()[0].clone()).unwrap();
        writer
            .record_auxiliary(
                AuxiliaryEvent::Reconnected {
                    seat: Seat::new(1).unwrap(),
                },
                AuxiliaryPhase::After,
            )
            .unwrap();
        writer.append(events()[1].clone()).unwrap();
        writer
            .record_auxiliary(
                AuxiliaryEvent::AutoStarted {
                    seat: Seat::new(2).unwrap(),
                },
                AuxiliaryPhase::Before,
            )
            .unwrap();
        writer.append(GameEvent::EndKyoku).unwrap();
        writer
            .record_auxiliary(
                AuxiliaryEvent::Left {
                    seat: Seat::new(2).unwrap(),
                },
                AuxiliaryPhase::After,
            )
            .unwrap();
        writer.append(GameEvent::EndGame).unwrap();
        let artifact = writer.finalize().unwrap();
        storage
            .open_ranked_match(
                "ranked-aux15",
                GameMode::FourPlayerRedEast,
                1,
                &artifact.relative_path_string(),
                &players,
            )
            .await
            .unwrap();
        let result = double_riichi_core::MatchResult {
            mode: GameMode::FourPlayerRedEast,
            players: players
                .iter()
                .enumerate()
                .map(|(seat, player)| MatchPlayerResult {
                    participant_id: player.id.clone(),
                    display_name: player.display_name.clone(),
                    kind: player.kind,
                    seat: Seat::new(seat as u8).unwrap(),
                    final_score: 25_000,
                    rank: (seat + 1) as u8,
                })
                .collect(),
            final_scores: vec![25_000; 4],
        };
        storage
            .complete_ranked_match("ranked-aux15", &artifact, &result, 2)
            .await
            .unwrap();
        let view = storage.load_replay("ranked-aux15").await.unwrap();
        assert_eq!(
            view.frames[0].auxiliary_events[0].phase,
            AuxiliaryPhase::Before
        );
        assert_eq!(
            view.frames[0].auxiliary_events[1].phase,
            AuxiliaryPhase::After
        );
        assert!(view.frames[1].auxiliary_events.is_empty());
        assert_eq!(
            view.frames[2].auxiliary_events[0].phase,
            AuxiliaryPhase::Before
        );
        assert_eq!(
            view.frames[2].auxiliary_events[1].phase,
            AuxiliaryPhase::After
        );
        assert_eq!(view.frames[0].auxiliary_events[0].sequence, 0);
        assert_eq!(view.frames[0].auxiliary_events[1].sequence, 1);
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }
}
