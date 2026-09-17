use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use double_riichi_core::{GameMode, MatchResult, Participant};
use double_riichi_replay::ReplayArtifact;
use serde_json::{Value, json};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use thiserror::Error;

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

pub struct Storage {
    pool: SqlitePool,
    data_root: PathBuf,
    replay_root: PathBuf,
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
            max_connections: 4,
        };
        storage.startup_cleanup().await?;
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
        if fs::symlink_metadata(&candidate)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(StorageError::UnsafeReplayPath);
        }
        let canonical_root = fs::canonicalize(&self.replay_root).map_err(StorageError::Io)?;
        let canonical_candidate = if candidate.exists() {
            fs::canonicalize(&candidate).map_err(StorageError::Io)?
        } else {
            let parent = candidate.parent().ok_or(StorageError::UnsafeReplayPath)?;
            let canonical_parent = fs::canonicalize(parent).map_err(StorageError::Io)?;
            canonical_parent.join(
                candidate
                    .file_name()
                    .ok_or(StorageError::UnsafeReplayPath)?,
            )
        };
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(StorageError::UnsafeReplayPath);
        }
        Ok(candidate)
    }

    pub async fn startup_cleanup(&self) -> Result<(), StorageError> {
        let rows =
            sqlx::query("SELECT match_id, replay_path FROM matches WHERE status = 'writing'")
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

        for (match_id, replay_path) in &unfinished {
            if let Some(replay_path) = replay_path {
                let path = self.resolve_replay_path(replay_path)?;
                remove_if_exists(&path)?;
            }
            remove_match_files(&self.replay_root, match_id)?;
        }

        if !unfinished.is_empty() {
            let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
            for (match_id, _) in &unfinished {
                sqlx::query("DELETE FROM matches WHERE match_id = ? AND status = 'writing'")
                    .bind(match_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(StorageError::Sqlx)?;
            }
            transaction.commit().await.map_err(StorageError::Sqlx)?;
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

    pub(crate) async fn complete_ranked_match(
        &self,
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
            "UPDATE matches SET completed_at = ?, status = 'completed', replay_path = ?, file_size = ? WHERE match_id = ? AND source = 'ranked' AND status = 'writing'",
        )
        .bind(completed_at)
        .bind(artifact.relative_path_string())
        .bind(file_size)
        .bind(match_id)
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
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn delete_writing_match(&self, match_id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM matches WHERE match_id = ? AND status = 'writing'")
            .bind(match_id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::Sqlx)?;
        Ok(())
    }

    pub(crate) fn remove_replay_file(&self, relative_path: &str) -> Result<(), StorageError> {
        let path = self.resolve_replay_path(relative_path)?;
        remove_if_exists(&path)
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
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
    if request_id.is_empty()
        || target_id.is_empty()
        || string_contains_raw_token(request_id)
        || string_contains_raw_token(target_id)
        || !validate_audit_summary(action, summary)
    {
        return Err(StorageError::InvalidAuditSummary);
    }
    let summary_json =
        serde_json::to_string(summary).map_err(|_| StorageError::InvalidAuditSummary)?;
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
    match participant.kind {
        double_riichi_core::ParticipantKind::Human => "human",
        double_riichi_core::ParticipantKind::MJAI => "mjai",
        double_riichi_core::ParticipantKind::MCP => "mcp",
        double_riichi_core::ParticipantKind::BuiltInBot => "builtin_bot",
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

#[allow(dead_code)]
fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
