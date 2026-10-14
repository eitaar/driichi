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

use double_riichi_core::room::BenchmarkStatus;
use double_riichi_core::{
    GameMode, KyokuResultRecord, MatchPlayerSnapshot, MatchResult, Participant, ParticipantKind,
    RoomAuxiliaryEvent, RoomAuxiliaryPhase, RoomEffect, RoomEffectError, RoomPersistenceFailure,
};
use double_riichi_replay::{
    AuxiliaryPhase, AuxiliaryRecord, MAX_DECOMPRESSED_REPLAY_BYTES, MAX_REPLAY_EVENTS,
    ReplayArtifact, ReplayError, ReplayFrame, ReplayReader, ReplayWriter, attach_result_records,
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
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
// Finalization is acknowledged by the Room after this acquisition window;
// keep it longer than the former Room-side six-second deadline so the worker
// owns the terminal outcome instead of racing cleanup.
const STORAGE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);

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
    pub result: Option<MatchResult>,
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
    result: &'a Option<MatchResult>,
}

fn validate_result_bundle(
    game_mode: &str,
    players: &[ReplayPlayer],
    frames: &mut [ReplayFrame],
    records: &[KyokuResultRecord],
    result: &MatchResult,
) -> Result<(), StorageError> {
    let mode = game_mode
        .parse::<GameMode>()
        .map_err(|_| StorageError::ReplayCorrupt)?;
    result.validate().map_err(|_| StorageError::ReplayCorrupt)?;
    if result.mode != mode || players.len() != mode.seat_count() {
        return Err(StorageError::ReplayCorrupt);
    }
    for (seat, player) in players.iter().enumerate() {
        let outcome = &result.players[seat];
        if player.seat != seat as i64
            || player.participant_id != outcome.participant_id.as_str()
            || player.display_name != outcome.display_name
            || player.participant_kind != participant_kind_kind(outcome.kind)
            || player.final_points != Some(i64::from(outcome.final_score))
        {
            return Err(StorageError::ReplayCorrupt);
        }
    }
    for record in records {
        if record.mode != mode || record.settlement.changes.len() != players.len() {
            return Err(StorageError::ReplayCorrupt);
        }
        for (seat, (change, player)) in record.settlement.changes.iter().zip(players).enumerate() {
            if usize::from(change.seat.index()) != seat
                || change.participant_id.as_str() != player.participant_id
            {
                return Err(StorageError::ReplayCorrupt);
            }
        }
    }
    if records.last().map(|record| {
        record
            .settlement
            .changes
            .iter()
            .map(|change| change.after)
            .collect::<Vec<_>>()
    }) != Some(result.final_scores.clone())
    {
        return Err(StorageError::ReplayCorrupt);
    }
    attach_result_records(frames, records).map_err(|error| match error {
        ReplayError::ReplayTooLarge { .. } => StorageError::ReplayTooLarge,
        _ => StorageError::ReplayCorrupt,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunStatus {
    Running,
    Completed,
    Stopped,
    Failed,
    Interrupted,
}

impl BenchmarkRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(StorageError::ReplayMetadata),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkRosterRecord {
    pub participant_id: String,
    pub display_name: String,
    pub participant_kind: String,
    pub initial_seat: i64,
    pub character_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResultRecord {
    pub participant_id: String,
    pub seat: i64,
    pub final_score: i64,
    pub rank: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkMatchRecord {
    pub sequence: u16,
    pub match_id: String,
    pub results: Vec<BenchmarkResultRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkRunRecord {
    pub run_id: String,
    pub room_code: String,
    pub game_mode: String,
    pub target: u16,
    pub completed: u16,
    pub status: BenchmarkRunStatus,
    pub reason: Option<String>,
    pub failed_match_id: Option<String>,
    pub roster: Vec<BenchmarkRosterRecord>,
    pub matches: Vec<BenchmarkMatchRecord>,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkParticipantStatistics {
    pub participant_id: String,
    pub display_name: String,
    pub average_rank: Option<f64>,
    pub first_place_rate: Option<f64>,
    pub cumulative_net_scores: Vec<f64>,
}

impl BenchmarkRunRecord {
    pub fn statistics(&self) -> Vec<BenchmarkParticipantStatistics> {
        self.roster
            .iter()
            .map(|player| {
                let mut ranks = Vec::new();
                let mut cumulative = 0.0;
                let mut series = Vec::new();
                for game in &self.matches {
                    if let Some(result) = game
                        .results
                        .iter()
                        .find(|result| result.participant_id == player.participant_id)
                    {
                        ranks.push(result.rank);
                        let mean = game
                            .results
                            .iter()
                            .map(|r| r.final_score as f64)
                            .sum::<f64>()
                            / game.results.len() as f64;
                        cumulative += result.final_score as f64 - mean;
                        series.push(cumulative);
                    }
                }
                BenchmarkParticipantStatistics {
                    participant_id: player.participant_id.clone(),
                    display_name: player.display_name.clone(),
                    average_rank: (!ranks.is_empty())
                        .then(|| ranks.iter().sum::<i64>() as f64 / ranks.len() as f64),
                    first_place_rate: (!ranks.is_empty()).then(|| {
                        ranks.iter().filter(|rank| **rank == 1).count() as f64 / ranks.len() as f64
                    }),
                    cumulative_net_scores: series,
                }
            })
            .collect()
    }
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
            .busy_timeout(SQLITE_BUSY_TIMEOUT);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .acquire_timeout(STORAGE_ACQUIRE_TIMEOUT)
            .connect_with(options)
            .await
            .map_err(StorageError::Sqlx)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(StorageError::Migration)?;

        sqlx::query("UPDATE benchmark_runs SET status = 'interrupted', reason = 'server restarted' WHERE status = 'running'")
            .execute(&pool)
            .await
            .map_err(StorageError::Sqlx)?;

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

    pub async fn create_benchmark_run(
        &self,
        run_id: &str,
        room_code: &str,
        mode: GameMode,
        target: u16,
        roster: &[MatchPlayerSnapshot],
    ) -> Result<(), StorageError> {
        if !(1..=1000).contains(&target) || roster.len() != mode.seat_count() {
            return Err(StorageError::ReplayMetadata);
        }
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        sqlx::query("INSERT INTO benchmark_runs (run_id, room_code, game_mode, target, status) VALUES (?, ?, ?, ?, 'running')")
            .bind(run_id).bind(room_code).bind(mode.as_str()).bind(i64::from(target))
            .execute(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        for player in roster {
            sqlx::query("INSERT INTO benchmark_roster (run_id, participant_id, display_name, participant_kind, initial_seat, character_id) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(run_id)
                .bind(player.participant_id.as_str())
                .bind(&player.display_name)
                .bind(participant_kind_kind(player.kind))
                .bind(i64::from(player.seat.index()))
                .bind(&player.character_id)
                .execute(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        }
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub async fn fail_benchmark_run(
        &self,
        run_id: &str,
        match_id: Option<&str>,
        reason: &str,
    ) -> Result<(), StorageError> {
        let updated = sqlx::query("UPDATE benchmark_runs SET status = 'failed', reason = ?, failed_match_id = ? WHERE run_id = ? AND status = 'running'")
            .bind(reason).bind(match_id).bind(run_id)
            .execute(&self.pool).await.map_err(StorageError::Sqlx)?;
        if updated.rows_affected() != 1 {
            return Err(StorageError::ReplayMetadata);
        }
        Ok(())
    }

    pub async fn list_benchmark_runs(&self) -> Result<Vec<BenchmarkRunRecord>, StorageError> {
        let rows = sqlx::query("SELECT run_id FROM benchmark_runs ORDER BY rowid DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(StorageError::Sqlx)?;
        let mut runs = Vec::with_capacity(rows.len());
        for row in rows {
            let run_id: String = row.try_get("run_id").map_err(StorageError::Sqlx)?;
            runs.push(self.load_benchmark_run(&run_id).await?);
        }
        Ok(runs)
    }

    pub async fn load_benchmark_run(
        &self,
        run_id: &str,
    ) -> Result<BenchmarkRunRecord, StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        let row = sqlx::query("SELECT run_id, room_code, game_mode, target, completed, status, reason, failed_match_id FROM benchmark_runs WHERE run_id = ?")
            .bind(run_id).fetch_one(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        let roster = sqlx::query("SELECT participant_id, display_name, participant_kind, initial_seat, character_id FROM benchmark_roster WHERE run_id = ? ORDER BY initial_seat")
            .bind(run_id).fetch_all(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        let matches = sqlx::query(
            "SELECT sequence, match_id FROM benchmark_matches WHERE run_id = ? ORDER BY sequence",
        )
        .bind(run_id)
        .fetch_all(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        let mut match_records = Vec::with_capacity(matches.len());
        for entry in matches {
            let sequence: i64 = entry.try_get("sequence").map_err(StorageError::Sqlx)?;
            let results = sqlx::query("SELECT participant_id, seat, final_score, rank FROM benchmark_results WHERE run_id = ? AND sequence = ? ORDER BY seat")
                .bind(run_id).bind(sequence).fetch_all(&mut *transaction).await.map_err(StorageError::Sqlx)?;
            match_records.push(BenchmarkMatchRecord {
                sequence: u16::try_from(sequence).map_err(|_| StorageError::ReplayMetadata)?,
                match_id: entry.try_get("match_id").map_err(StorageError::Sqlx)?,
                results: results
                    .into_iter()
                    .map(|result| {
                        Ok(BenchmarkResultRecord {
                            participant_id: result.try_get("participant_id")?,
                            seat: result.try_get("seat")?,
                            final_score: result.try_get("final_score")?,
                            rank: result.try_get("rank")?,
                        })
                    })
                    .collect::<Result<_, sqlx::Error>>()
                    .map_err(StorageError::Sqlx)?,
            });
        }
        transaction.commit().await.map_err(StorageError::Sqlx)?;
        let status: String = row.try_get("status").map_err(StorageError::Sqlx)?;
        Ok(BenchmarkRunRecord {
            run_id: row.try_get("run_id").map_err(StorageError::Sqlx)?,
            room_code: row.try_get("room_code").map_err(StorageError::Sqlx)?,
            game_mode: row.try_get("game_mode").map_err(StorageError::Sqlx)?,
            target: u16::try_from(
                row.try_get::<i64, _>("target")
                    .map_err(StorageError::Sqlx)?,
            )
            .map_err(|_| StorageError::ReplayMetadata)?,
            completed: u16::try_from(
                row.try_get::<i64, _>("completed")
                    .map_err(StorageError::Sqlx)?,
            )
            .map_err(|_| StorageError::ReplayMetadata)?,
            status: BenchmarkRunStatus::parse(&status)?,
            reason: row.try_get("reason").map_err(StorageError::Sqlx)?,
            failed_match_id: row.try_get("failed_match_id").map_err(StorageError::Sqlx)?,
            roster: roster
                .into_iter()
                .map(|player| {
                    Ok(BenchmarkRosterRecord {
                        participant_id: player.try_get("participant_id")?,
                        display_name: player.try_get("display_name")?,
                        participant_kind: player.try_get("participant_kind")?,
                        initial_seat: player.try_get("initial_seat")?,
                        character_id: player.try_get("character_id")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()
                .map_err(StorageError::Sqlx)?,
            matches: match_records,
        })
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
        let mut rows = sqlx::query("SELECT match_id FROM matches WHERE status = 'completed'")
            .fetch(&self.pool);
        while let Some(row) = rows.try_next().await.map_err(StorageError::Sqlx)? {
            let match_id: String = row.try_get("match_id").map_err(StorageError::Sqlx)?;
            let result = self
                .load_replay_view(&match_id)
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
            let availability = match self.replay_availability(&summary.match_id).await {
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
        self.load_replay_view(match_id).await
    }

    async fn load_replay_view(&self, match_id: &str) -> Result<ReplayView, StorageError> {
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        let row = sqlx::query(
            "SELECT match_id, source, room_name, game_mode, started_at, completed_at, replay_path, file_size
             FROM matches WHERE match_id = ? AND status = 'completed'",
        )
        .bind(match_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?
        .ok_or(StorageError::ReplayNotFound)?;
        let replay_path: String = row.try_get("replay_path").map_err(StorageError::Sqlx)?;
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
        let auxiliary = self
            .load_auxiliary(match_id, &mut transaction)
            .await
            .inspect_err(|_| self.mark_replay_degraded())?;
        let mut frames = self
            .reconstruct_replay(
                &summary.game_mode,
                &replay_path,
                summary.file_size,
                &auxiliary,
            )
            .await?;
        let players = self.load_players(match_id, &mut transaction).await?;
        let result = self
            .load_authoritative_result(
                match_id,
                &summary.game_mode,
                &players,
                &mut frames,
                &mut transaction,
            )
            .await
            .inspect_err(|_| self.mark_replay_degraded())?;
        transaction.commit().await.map_err(StorageError::Sqlx)?;
        Ok(ReplayView {
            summary,
            players,
            frames,
            result,
        })
    }

    async fn load_kyoku_records(
        &self,
        match_id: &str,
        connection: &mut sqlx::SqliteConnection,
    ) -> Result<Vec<KyokuResultRecord>, StorageError> {
        let stats = sqlx::query("SELECT COUNT(*) AS n, COALESCE(SUM(length(CAST(payload_json AS BLOB))), 0) AS bytes FROM match_kyoku_results WHERE match_id = ?")
            .bind(match_id).fetch_one(&mut *connection).await.map_err(StorageError::Sqlx)?;
        let count: i64 = stats.try_get("n").map_err(StorageError::Sqlx)?;
        let bytes: i64 = stats.try_get("bytes").map_err(StorageError::Sqlx)?;
        if count < 0
            || count as u64 > MAX_REPLAY_EVENTS as u64
            || bytes < 0
            || bytes as u64 > MAX_DECOMPRESSED_REPLAY_BYTES as u64
        {
            return Err(StorageError::ReplayTooLarge);
        }
        let rows = sqlx::query("SELECT kyoku_sequence, version, end_event_index, payload_json FROM match_kyoku_results WHERE match_id = ? ORDER BY kyoku_sequence")
            .bind(match_id).fetch_all(&mut *connection).await.map_err(StorageError::Sqlx)?;
        if rows.len() > MAX_REPLAY_EVENTS {
            return Err(StorageError::ReplayTooLarge);
        }
        let mut total = 0usize;
        rows.iter()
            .map(|row| {
                let payload: String = row.try_get("payload_json").map_err(StorageError::Sqlx)?;
                total = total
                    .checked_add(payload.len())
                    .ok_or(StorageError::ReplayTooLarge)?;
                if total > MAX_DECOMPRESSED_REPLAY_BYTES {
                    return Err(StorageError::ReplayTooLarge);
                }
                let record: KyokuResultRecord =
                    serde_json::from_str(&payload).map_err(|_| StorageError::ReplayCorrupt)?;
                let sequence: i64 = row.try_get("kyoku_sequence").map_err(StorageError::Sqlx)?;
                let version: i64 = row.try_get("version").map_err(StorageError::Sqlx)?;
                let index: i64 = row.try_get("end_event_index").map_err(StorageError::Sqlx)?;
                if record.version != 1
                    || version != 1
                    || i64::try_from(record.kyoku_sequence).ok() != Some(sequence)
                    || i64::try_from(record.end_event_index).ok() != Some(index)
                {
                    return Err(StorageError::ReplayCorrupt);
                }
                Ok(record)
            })
            .collect()
    }

    async fn load_authoritative_result(
        &self,
        match_id: &str,
        game_mode: &str,
        players: &[ReplayPlayer],
        frames: &mut [ReplayFrame],
        connection: &mut sqlx::SqliteConnection,
    ) -> Result<Option<MatchResult>, StorageError> {
        let row = sqlx::query("SELECT authoritative_results_version, length(CAST(authoritative_result_json AS BLOB)) AS result_bytes FROM matches WHERE match_id = ? AND status = 'completed'")
            .bind(match_id).fetch_one(&mut *connection).await.map_err(StorageError::Sqlx)?;
        let version: Option<i64> = row
            .try_get("authoritative_results_version")
            .map_err(StorageError::Sqlx)?;
        let result_bytes: Option<i64> = row.try_get("result_bytes").map_err(StorageError::Sqlx)?;
        if result_bytes
            .is_some_and(|bytes| bytes < 0 || bytes as u64 > MAX_DECOMPRESSED_REPLAY_BYTES as u64)
        {
            return Err(StorageError::ReplayTooLarge);
        }
        let records = self.load_kyoku_records(match_id, connection).await?;
        let payload: Option<String> = sqlx::query_scalar("SELECT authoritative_result_json FROM matches WHERE match_id = ? AND status = 'completed'")
            .bind(match_id).fetch_one(&mut *connection).await.map_err(StorageError::Sqlx)?;
        if version.is_none() && payload.is_none() && records.is_empty() {
            return Ok(None);
        }
        if version != Some(1) || payload.is_none() {
            return Err(StorageError::ReplayCorrupt);
        }
        let result: MatchResult = serde_json::from_str(payload.as_ref().unwrap())
            .map_err(|_| StorageError::ReplayCorrupt)?;
        validate_result_bundle(game_mode, players, frames, &records, &result)?;
        Ok(Some(result))
    }

    async fn load_players(
        &self,
        match_id: &str,
        connection: &mut sqlx::SqliteConnection,
    ) -> Result<Vec<ReplayPlayer>, StorageError> {
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
        .fetch_one(&mut *connection)
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
        .fetch(&mut *connection);
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
            result: &replay.result,
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
        game_mode: &str,
        replay_path: &str,
        file_size: i64,
        auxiliary: &[AuxiliaryRecord],
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
        reader
            .frames_with_auxiliary_for_mode(auxiliary, mode)
            .map_err(|error| {
                self.mark_replay_degraded();
                match error {
                    ReplayError::ReplayTooLarge { .. } => StorageError::ReplayTooLarge,
                    _ => StorageError::ReplayCorrupt,
                }
            })
    }

    async fn load_auxiliary(
        &self,
        match_id: &str,
        connection: &mut sqlx::SqliteConnection,
    ) -> Result<Vec<AuxiliaryRecord>, StorageError> {
        let stats = sqlx::query(
            "SELECT count(*) AS row_count, COALESCE(sum(length(CAST(payload_json AS BLOB))), 0) AS payload_bytes
             FROM replay_auxiliary_events WHERE match_id = ?",
        )
        .bind(match_id)
        .fetch_one(&mut *connection)
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
        .fetch(&mut *connection);
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

    async fn replay_availability(&self, match_id: &str) -> Result<&'static str, StorageError> {
        let replay = self.load_replay_view(match_id).await?;
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
            "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status, replay_path, authoritative_results_version) VALUES (?, 'ranked', NULL, ?, ?, 'writing', ?, 1)",
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
            "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status, replay_path, authoritative_results_version) VALUES (?, 'room', ?, ?, ?, 'writing', ?, 1)",
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

    async fn persist_kyoku_record(
        &self,
        match_id: &str,
        record: &KyokuResultRecord,
    ) -> Result<(), StorageError> {
        let payload = bounded_result_json(record)?;
        if record.end_event_index >= MAX_REPLAY_EVENTS {
            return Err(StorageError::ReplayTooLarge);
        }
        // Reserve the writer before reads so another commit cannot stale this WAL snapshot.
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(StorageError::Sqlx)?;
        let row = sqlx::query("SELECT game_mode, status, authoritative_results_version FROM matches WHERE match_id = ?")
            .bind(match_id).fetch_one(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        let mode: String = row.try_get("game_mode").map_err(StorageError::Sqlx)?;
        let status: String = row.try_get("status").map_err(StorageError::Sqlx)?;
        let version: Option<i64> = row
            .try_get("authoritative_results_version")
            .map_err(StorageError::Sqlx)?;
        if status != "writing" || version != Some(1) || mode != record.mode.as_str() {
            return Err(StorageError::ReplayMetadata);
        }
        let players = self.load_players(match_id, &mut transaction).await?;
        let roster = players
            .into_iter()
            .map(|player| {
                Participant::new(
                    player.participant_id,
                    "Recorded seat",
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect::<Vec<_>>();
        record
            .validate(&roster)
            .map_err(|_| StorageError::ReplayCorrupt)?;
        let sequence =
            i64::try_from(record.kyoku_sequence).map_err(|_| StorageError::ReplayMetadata)?;
        let stats = sqlx::query("SELECT COUNT(*) AS n, COALESCE(SUM(length(CAST(payload_json AS BLOB))), 0) AS bytes FROM match_kyoku_results WHERE match_id = ?")
            .bind(match_id).fetch_one(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        let count: i64 = stats.try_get("n").map_err(StorageError::Sqlx)?;
        let bytes: i64 = stats.try_get("bytes").map_err(StorageError::Sqlx)?;
        if count < 0
            || count as u64 > MAX_REPLAY_EVENTS as u64
            || bytes < 0
            || bytes as u64 > MAX_DECOMPRESSED_REPLAY_BYTES as u64
        {
            return Err(StorageError::ReplayTooLarge);
        }
        let existing = sqlx::query("SELECT payload_json FROM match_kyoku_results WHERE match_id = ? AND kyoku_sequence = ?")
            .bind(match_id).bind(sequence).fetch_optional(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        if let Some(existing) = existing {
            let previous: String = existing
                .try_get("payload_json")
                .map_err(StorageError::Sqlx)?;
            return if previous == payload {
                Ok(())
            } else {
                Err(StorageError::ReplayCorrupt)
            };
        }
        if count as u64 >= MAX_REPLAY_EVENTS as u64
            || (bytes as u64)
                .checked_add(payload.len() as u64)
                .is_none_or(|total| total > MAX_DECOMPRESSED_REPLAY_BYTES as u64)
        {
            return Err(StorageError::ReplayTooLarge);
        }
        if sequence != count + 1 {
            return Err(StorageError::ReplayCorrupt);
        }
        sqlx::query("INSERT INTO match_kyoku_results (match_id, kyoku_sequence, version, end_event_index, payload_json) VALUES (?, ?, 1, ?, ?)")
            .bind(match_id).bind(sequence)
            .bind(i64::try_from(record.end_event_index).map_err(|_| StorageError::ReplayMetadata)?)
            .bind(payload).execute(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        transaction.commit().await.map_err(StorageError::Sqlx)
    }

    pub(crate) async fn complete_ranked_match(
        &self,
        match_id: &str,
        artifact: &ReplayArtifact,
        result: &MatchResult,
        completed_at: i64,
        records: &[KyokuResultRecord],
    ) -> Result<(), StorageError> {
        self.complete_match(
            "ranked",
            match_id,
            artifact,
            result,
            completed_at,
            None,
            Some(records),
        )
        .await
    }

    pub(crate) async fn complete_room_match(
        &self,
        match_id: &str,
        artifact: &ReplayArtifact,
        result: &MatchResult,
        completed_at: i64,
    ) -> Result<(), StorageError> {
        self.complete_match("room", match_id, artifact, result, completed_at, None, None)
            .await
    }

    pub async fn record_benchmark_completion(
        &self,
        run_id: &str,
        sequence: u16,
        match_id: &str,
        result: &MatchResult,
        artifact: &ReplayArtifact,
        completed_at: i64,
    ) -> Result<(), StorageError> {
        self.complete_match(
            "room",
            match_id,
            artifact,
            result,
            completed_at,
            Some((run_id, sequence)),
            None,
        )
        .await
    }

    pub async fn stop_benchmark_run(
        &self,
        run_id: &str,
        status: BenchmarkRunStatus,
        reason: Option<&str>,
    ) -> Result<(), StorageError> {
        if status == BenchmarkRunStatus::Running {
            return Err(StorageError::ReplayMetadata);
        }
        let updated = sqlx::query("UPDATE benchmark_runs SET status = ?, reason = ? WHERE run_id = ? AND status = 'running'")
            .bind(status.as_str()).bind(reason).bind(run_id)
            .execute(&self.pool).await.map_err(StorageError::Sqlx)?;
        if updated.rows_affected() != 1 {
            return Err(StorageError::ReplayMetadata);
        }
        Ok(())
    }

    async fn complete_match(
        &self,
        source: &str,
        match_id: &str,
        artifact: &ReplayArtifact,
        result: &MatchResult,
        completed_at: i64,
        benchmark: Option<(&str, u16)>,
        ranked_records: Option<&[KyokuResultRecord]>,
    ) -> Result<(), StorageError> {
        self.resolve_replay_path(&artifact.relative_path_string())?;
        if let Some(records) = ranked_records {
            if records.len() > MAX_REPLAY_EVENTS {
                return Err(StorageError::ReplayTooLarge);
            }
            let mut bytes = 0usize;
            for record in records {
                bytes = bytes
                    .checked_add(bounded_result_json(record)?.len())
                    .ok_or(StorageError::ReplayTooLarge)?;
                if bytes > MAX_DECOMPRESSED_REPLAY_BYTES {
                    return Err(StorageError::ReplayTooLarge);
                }
            }
        }
        if let Some((run_id, sequence)) = benchmark {
            let previous = sqlx::query(
                "SELECT match_id FROM benchmark_matches WHERE run_id = ? AND sequence = ?",
            )
            .bind(run_id)
            .bind(i64::from(sequence))
            .fetch_optional(&self.pool)
            .await
            .map_err(StorageError::Sqlx)?;
            if let Some(previous) = previous {
                let previous_id: String =
                    previous.try_get("match_id").map_err(StorageError::Sqlx)?;
                return if previous_id == match_id {
                    Ok(())
                } else {
                    Err(StorageError::ReplayMetadata)
                };
            }
        }
        let file_size =
            i64::try_from(artifact.file_size).map_err(|_| StorageError::ReplayMetadata)?;
        // Completion also reads before writing; acquire its writer on a fresh snapshot.
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(StorageError::Sqlx)?;
        let admission = sqlx::query("SELECT game_mode, authoritative_results_version FROM matches WHERE match_id = ? AND source = ? AND status = 'writing'")
            .bind(match_id).bind(source).fetch_one(&mut *transaction).await.map_err(StorageError::Sqlx)?;
        let game_mode: String = admission.try_get("game_mode").map_err(StorageError::Sqlx)?;
        let persisted_mode = game_mode
            .parse::<GameMode>()
            .map_err(|_| StorageError::ReplayCorrupt)?;
        let marker: Option<i64> = admission
            .try_get("authoritative_results_version")
            .map_err(StorageError::Sqlx)?;
        if (marker.is_some() && marker != Some(1)) || result.mode != persisted_mode {
            return Err(StorageError::ReplayCorrupt);
        }
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
        if marker == Some(1) {
            let payload = bounded_result_json(result)?;
            let room_records = if ranked_records.is_some() {
                None
            } else {
                Some(self.load_kyoku_records(match_id, &mut transaction).await?)
            };
            if ranked_records.is_some() && source != "ranked" {
                return Err(StorageError::ReplayMetadata);
            }
            let records: &[KyokuResultRecord] =
                ranked_records.unwrap_or_else(|| room_records.as_deref().unwrap());
            let path = self.resolve_replay_path(&artifact.relative_path_string())?;
            let mut frames = ReplayReader::open(&path)
                .map_err(|_| StorageError::ReplayCorrupt)?
                .frames_with_auxiliary_for_mode(&artifact.auxiliary_events, persisted_mode)
                .map_err(|_| StorageError::ReplayCorrupt)?;
            let players = self.load_players(match_id, &mut transaction).await?;
            validate_result_bundle(&game_mode, &players, &mut frames, records, result)?;
            if ranked_records.is_some() {
                for record in records {
                    let json = bounded_result_json(record)?;
                    sqlx::query("INSERT INTO match_kyoku_results (match_id, kyoku_sequence, version, end_event_index, payload_json) VALUES (?, ?, 1, ?, ?)")
                        .bind(match_id).bind(i64::try_from(record.kyoku_sequence).map_err(|_| StorageError::ReplayMetadata)?)
                        .bind(i64::try_from(record.end_event_index).map_err(|_| StorageError::ReplayMetadata)?)
                        .bind(json).execute(&mut *transaction).await.map_err(StorageError::Sqlx)?;
                }
            }
            sqlx::query("UPDATE matches SET authoritative_result_json = ? WHERE match_id = ?")
                .bind(payload)
                .bind(match_id)
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
        if let Some((run_id, sequence)) = benchmark {
            let advanced = sqlx::query("UPDATE benchmark_runs SET completed = completed + 1 WHERE run_id = ? AND status = 'running' AND completed = ? AND completed < target")
                .bind(run_id).bind(i64::from(sequence) - 1)
                .execute(&mut *transaction).await.map_err(StorageError::Sqlx)?;
            if advanced.rows_affected() != 1 || result.players.len() != result.mode.seat_count() {
                return Err(StorageError::ReplayMetadata);
            }
            sqlx::query(
                "INSERT INTO benchmark_matches (run_id, sequence, match_id) VALUES (?, ?, ?)",
            )
            .bind(run_id)
            .bind(i64::from(sequence))
            .bind(match_id)
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
            for player in &result.players {
                sqlx::query("INSERT INTO benchmark_results (run_id, sequence, participant_id, seat, final_score, rank) VALUES (?, ?, ?, ?, ?, ?)")
                    .bind(run_id).bind(i64::from(sequence)).bind(player.participant_id.as_str())
                    .bind(i64::from(player.seat.index())).bind(i64::from(player.final_score))
                    .bind(i64::from(player.rank))
                    .execute(&mut *transaction).await.map_err(StorageError::Sqlx)?;
            }
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
        let mut transaction = self.pool.begin().await.map_err(StorageError::Sqlx)?;
        // Claim the incomplete row with a write before touching the files.
        // This serializes cleanup against a concurrent finalization: whichever
        // operation claims the row first owns the terminal outcome.
        let claimed = sqlx::query(
            "UPDATE matches SET status = 'failed' WHERE match_id = ? AND status IN ('writing', 'failed')",
        )
        .bind(match_id)
        .execute(&mut *transaction)
        .await
        .map_err(StorageError::Sqlx)?;
        if claimed.rows_affected() == 0 {
            transaction.commit().await.map_err(StorageError::Sqlx)?;
            return Ok(());
        }

        // Hold the transaction while removing artifacts so a failed removal
        // rolls back the claim and leaves metadata retryable.
        remove_match_files(&self.replay_root, match_id)?;
        sqlx::query("DELETE FROM matches WHERE match_id = ? AND status IN ('writing', 'failed')")
            .bind(match_id)
            .execute(&mut *transaction)
            .await
            .map_err(StorageError::Sqlx)?;
        transaction.commit().await.map_err(StorageError::Sqlx)
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
                    record,
                    completion,
                } => {
                    let id = match_id.to_string();
                    let result = match replays.get_mut(&id) {
                        Some(RoomReplay::Writing(writer)) => {
                            if writer.emitted_lines().checked_sub(1) != Some(record.end_event_index)
                            {
                                Err(room_effect_error(
                                    "result index differs from canonical writer",
                                ))
                            } else {
                                writer
                                    .flush_kyoku()
                                    .map_err(|error| room_effect_error(error.to_string()))
                            }
                        }
                        Some(RoomReplay::Failed(error)) => Err(room_effect_error(error.clone())),
                        None => Err(room_effect_error("replay was not opened")),
                    };
                    let result = match result {
                        Ok(()) => storage
                            .persist_kyoku_record(&id, &record)
                            .await
                            .map_err(|error| room_effect_error(error.to_string())),
                        error => error,
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
                RoomEffect::CreateBenchmarkRun {
                    run_id,
                    room_code,
                    mode,
                    target,
                    roster,
                    completion,
                } => {
                    let result = storage
                        .create_benchmark_run(&run_id, &room_code, mode, target, &roster)
                        .await
                        .map_err(|error| room_effect_error(error.to_string()));
                    let _ = completion.send(result);
                }
                RoomEffect::StopBenchmarkRun {
                    run_id,
                    status,
                    reason,
                    failed_match_id,
                    completion,
                } => {
                    let result = if status == BenchmarkStatus::Failed {
                        storage
                            .fail_benchmark_run(
                                &run_id,
                                failed_match_id.as_ref().map(|id| id.as_str()),
                                reason.as_deref().unwrap_or("benchmark failed"),
                            )
                            .await
                    } else {
                        let status = match status {
                            BenchmarkStatus::Completed => BenchmarkRunStatus::Completed,
                            BenchmarkStatus::Stopped => BenchmarkRunStatus::Stopped,
                            _ => BenchmarkRunStatus::Failed,
                        };
                        storage
                            .stop_benchmark_run(&run_id, status, reason.as_deref())
                            .await
                    };
                    if let Err(error) = &result {
                        tracing::warn!(error = ?error, "benchmark terminal state persistence failed");
                    }
                    let _ = completion
                        .send(result.map_err(|error| room_effect_error(error.to_string())));
                }
                RoomEffect::FinalizeMatch {
                    match_id,
                    result,
                    benchmark,
                    completed_at,
                    completion,
                } => {
                    let id = match_id.to_string();
                    let replay = replays.remove(&id);
                    let persisted = finalize_room_replay(
                        &storage,
                        &id,
                        replay,
                        &result,
                        benchmark,
                        completed_at,
                    )
                    .await;
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
    benchmark: Option<(String, u16)>,
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
    let persisted = if let Some((run_id, sequence)) = benchmark {
        storage
            .record_benchmark_completion(
                &run_id,
                sequence,
                match_id,
                result,
                &artifact,
                completed_at,
            )
            .await
    } else {
        storage
            .complete_room_match(match_id, &artifact, result, completed_at)
            .await
    };
    match persisted {
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

fn bounded_result_json<T: serde::Serialize>(value: &T) -> Result<String, StorageError> {
    let mut writer = LimitedJsonWriter::new(MAX_DECOMPRESSED_REPLAY_BYTES);
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => String::from_utf8(writer.into_inner()).map_err(|_| StorageError::ReplayMetadata),
        Err(_) if writer.overflowed => Err(StorageError::ReplayTooLarge),
        Err(_) => Err(StorageError::ReplayMetadata),
    }
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
    fn benchmark_statistics_use_stored_ranks_center_scores_and_keep_identities_separate() {
        let mut run = BenchmarkRunRecord {
            run_id: "stats".into(),
            room_code: "123456".into(),
            game_mode: "4p-red-east".into(),
            target: 3,
            completed: 0,
            status: BenchmarkRunStatus::Failed,
            reason: Some("failed attempt excluded".into()),
            failed_match_id: Some("failed".into()),
            roster: (0..4)
                .map(|seat| BenchmarkRosterRecord {
                    participant_id: format!("p{seat}"),
                    display_name: "same name".into(),
                    participant_kind: "mcp".into(),
                    initial_seat: seat,
                    character_id: None,
                })
                .collect(),
            matches: vec![],
        };
        let stats = run.statistics();
        assert_eq!(stats.len(), 4);
        assert!(stats.iter().all(|s| s.average_rank.is_none()
            && s.first_place_rate.is_none()
            && s.cumulative_net_scores.is_empty()));
        for (sequence, scores, ranks) in [
            (1, [40000, 30000, 20000, 10000], [2, 1, 3, 4]),
            (2, [10000, 20000, 30000, 40000], [4, 3, 2, 1]),
            (3, [30000, 30000, 20000, 20000], [1, 2, 3, 4]),
        ] {
            run.matches.push(BenchmarkMatchRecord {
                sequence,
                match_id: format!("m{sequence}"),
                results: (0..4)
                    .map(|seat| BenchmarkResultRecord {
                        participant_id: format!("p{seat}"),
                        seat: seat as i64,
                        final_score: scores[seat],
                        rank: ranks[seat],
                    })
                    .collect(),
            });
        }
        run.completed = 3;
        let stats = run.statistics();
        assert_eq!(stats[0].average_rank, Some(7.0 / 3.0));
        assert_eq!(stats[0].first_place_rate, Some(1.0 / 3.0));
        assert_eq!(stats[0].cumulative_net_scores, [15000.0, 0.0, 5000.0]);
        assert_eq!(stats[1].average_rank, Some(2.0));
        assert_eq!(stats[1].cumulative_net_scores, [5000.0, 0.0, 5000.0]);
    }

    #[tokio::test]
    async fn benchmark_read_keeps_header_matches_and_statistics_in_one_snapshot() {
        let root = test_root("benchmark-read-snapshot");
        let storage = Storage::connect(&root).await.unwrap();
        let roster: Vec<_> = players()
            .into_iter()
            .enumerate()
            .map(|(seat, p)| MatchPlayerSnapshot {
                participant_id: p.id,
                display_name: p.display_name,
                kind: p.kind,
                seat: Seat::new(seat as u8).unwrap(),
                character_id: None,
                controller: double_riichi_core::RoomController::PermanentAuto(
                    double_riichi_core::PermanentAutoReason::BuiltInBot,
                ),
            })
            .collect();
        storage
            .create_benchmark_run(
                "snapshot",
                "123456",
                GameMode::FourPlayerRedEast,
                2,
                &roster,
            )
            .await
            .unwrap();
        storage
            .open_room_match(
                "snapshot-match",
                GameMode::FourPlayerRedEast,
                "benchmark",
                1,
                "4p/snapshot-match.mjson",
                &roster,
            )
            .await
            .unwrap();
        // This snapshot test has no canonical artifact or result records. Model
        // only its own row as historical, rather than bypassing new admissions.
        sqlx::query("UPDATE matches SET authoritative_results_version = NULL WHERE match_id = ?")
            .bind("snapshot-match")
            .execute(storage.pool())
            .await
            .unwrap();
        let released = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let first = Arc::new(AtomicBool::new(true));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .after_release({
                let released = released.clone();
                let resume = resume.clone();
                move |_, _| {
                    let released = released.clone();
                    let resume = resume.clone();
                    let first = first.clone();
                    Box::pin(async move {
                        if first.swap(false, Ordering::SeqCst) {
                            released.notify_one();
                            resume.notified().await;
                        }
                        Ok(true)
                    })
                }
            })
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(storage.database_path())
                    .journal_mode(SqliteJournalMode::Wal),
            )
            .await
            .unwrap();
        let reader = Arc::new(Storage {
            pool,
            data_root: storage.data_root.clone(),
            replay_root: storage.replay_root.clone(),
            replay_degraded: AtomicBool::new(false),
            max_connections: 1,
        });
        let read = tokio::spawn({
            let reader = reader.clone();
            async move { reader.load_benchmark_run("snapshot").await.unwrap() }
        });
        // Commit exactly after the reader releases its first connection. Without a
        // read transaction this splits the header and Match association reads.
        tokio::time::timeout(Duration::from_secs(5), released.notified())
            .await
            .unwrap();
        let result = MatchResult {
            mode: GameMode::FourPlayerRedEast,
            final_scores: vec![25000; 4],
            players: roster
                .iter()
                .enumerate()
                .map(|(i, p)| MatchPlayerResult {
                    participant_id: p.participant_id.clone(),
                    display_name: p.display_name.clone(),
                    kind: p.kind,
                    seat: p.seat,
                    final_score: 25000,
                    rank: i as u8 + 1,
                })
                .collect(),
        };
        storage
            .record_benchmark_completion(
                "snapshot",
                1,
                "snapshot-match",
                &result,
                &ReplayArtifact {
                    relative_path: "4p/snapshot-match.mjson".into(),
                    file_size: 100,
                    auxiliary_events: vec![],
                },
                2,
            )
            .await
            .unwrap();
        resume.notify_one();
        let before = read.await.unwrap();
        assert_eq!(before.completed, 0);
        assert!(
            before.matches.is_empty(),
            "a completion after the header read must not appear in that snapshot"
        );
        assert!(
            before
                .statistics()
                .iter()
                .all(|stat| stat.cumulative_net_scores.is_empty())
        );
        let after = reader.load_benchmark_run("snapshot").await.unwrap();
        assert_eq!(after.completed, 1);
        assert_eq!(after.matches.len(), 1);
        reader.close().await;
        storage.close().await;
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
    async fn incomplete_cleanup_never_removes_completed_replay() {
        let root = test_root("completed-cleanup");
        let storage = Storage::connect(&root).await.unwrap();
        let mut writer = ReplayWriter::new(
            storage.replay_root(),
            "completed-cleanup",
            GameMode::FourPlayerRedEast,
        )
        .unwrap();
        for event in events() {
            writer.append(event).unwrap();
        }
        let artifact = writer.finalize().unwrap();
        let replay_path = artifact.relative_path_string();
        sqlx::query(
            "INSERT INTO matches (match_id, source, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES (?, 'room', ?, ?, ?, 'completed', ?, ?)",
        )
        .bind("completed-cleanup")
        .bind("4p-red-east")
        .bind(1_i64)
        .bind(2_i64)
        .bind(&replay_path)
        .bind(i64::try_from(artifact.file_size).unwrap())
        .execute(storage.pool())
        .await
        .unwrap();

        storage
            .delete_incomplete_match("completed-cleanup")
            .await
            .unwrap();
        assert!(storage.resolve_replay_path(&replay_path).unwrap().is_file());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM matches WHERE match_id = 'completed-cleanup'",
            )
            .fetch_one(storage.pool())
            .await
            .unwrap(),
            1
        );
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    // StartKyoku/reach prefix is test-controlled; the terminal Ryukyoku,
    // EndKyoku, EndGame and private record are frozen pinned-engine output.
    // This final result is built from measured fixture scores, NOT a full
    // MatchMachine final-result or saved-Room capture.
    fn final_draw_capture(
        mode: GameMode,
    ) -> (
        Vec<GameEvent>,
        KyokuResultRecord,
        MatchResult,
        Vec<Participant>,
    ) {
        let captures: serde_json::Value = serde_json::from_str(include_str!(
            "../../double_riichi_core/tests/fixtures/authoritative-draw-captures.json"
        ))
        .unwrap();
        let capture = captures
            .as_array()
            .unwrap()
            .iter()
            .find(|capture| {
                capture["scenario"] == "final"
                    && capture["mode"] == serde_json::to_value(mode).unwrap()
            })
            .unwrap();
        let events: Vec<GameEvent> = serde_json::from_value(capture["events"].clone()).unwrap();
        let record: KyokuResultRecord = serde_json::from_value(capture["record"].clone()).unwrap();
        let final_scores = record
            .settlement
            .changes
            .iter()
            .map(|change| change.after)
            .collect::<Vec<_>>();
        assert_eq!(
            final_scores,
            serde_json::from_value::<Vec<i32>>(capture["actual_after_scores"].clone()).unwrap()
        );
        let roster = (0..mode.seat_count())
            .map(|seat| {
                Participant::new(
                    format!("seat{seat}"),
                    format!("Recorded {seat}"),
                    ParticipantKind::BuiltInBot,
                )
            })
            .collect::<Vec<_>>();
        let result = MatchResult {
            mode,
            players: roster
                .iter()
                .enumerate()
                .map(|(seat, participant)| MatchPlayerResult {
                    participant_id: participant.id.clone(),
                    display_name: participant.display_name.clone(),
                    kind: participant.kind,
                    seat: Seat::new(seat as u8).unwrap(),
                    final_score: final_scores[seat],
                    rank: (1 + final_scores
                        .iter()
                        .enumerate()
                        .filter(|(other, score)| {
                            **score > final_scores[seat]
                                || (**score == final_scores[seat] && *other < seat)
                        })
                        .count()) as u8,
                })
                .collect(),
            final_scores,
        };
        result.validate().unwrap();
        (events, record, result, roster)
    }

    #[tokio::test]
    async fn replay_view_keeps_summary_roster_auxiliary_and_results_in_one_snapshot() {
        for delete in [false, true] {
            let root = test_root(if delete {
                "whole-replay-snapshot-delete"
            } else {
                "whole-replay-snapshot-update"
            });
            let storage = Storage::connect(&root).await.unwrap();
            let match_id = "whole-replay-snapshot";
            let mode = GameMode::FourPlayerRedEast;
            let (events, record, result, roster) = final_draw_capture(mode);
            let mut writer = ReplayWriter::new(storage.replay_root(), match_id, mode).unwrap();
            for event in &events {
                writer.append(event.clone()).unwrap();
            }
            let artifact = writer.finalize().unwrap();
            storage
                .open_ranked_match(match_id, mode, 1, &artifact.relative_path_string(), &roster)
                .await
                .unwrap();
            storage
                .complete_ranked_match(
                    match_id,
                    &artifact,
                    &result,
                    2,
                    std::slice::from_ref(&record),
                )
                .await
                .unwrap();
            if delete {
                let auxiliary = serde_json::to_string(&AuxiliaryEvent::Disconnected {
                    seat: Seat::new(0).unwrap(),
                })
                .unwrap();
                sqlx::query("INSERT INTO replay_auxiliary_events (match_id, line_index, phase, sequence, payload_json) VALUES (?, 0, 'before', 0, ?)")
                    .bind(match_id).bind(auxiliary).execute(storage.pool()).await.unwrap();
            }
            let prior = storage
                .encode_replay_view(&storage.load_replay(match_id).await.unwrap())
                .unwrap();

            let released = Arc::new(tokio::sync::Notify::new());
            let resume = Arc::new(tokio::sync::Notify::new());
            let first = Arc::new(AtomicBool::new(true));
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .after_release({
                    let released = released.clone();
                    let resume = resume.clone();
                    move |_, _| {
                        let released = released.clone();
                        let resume = resume.clone();
                        let first = first.clone();
                        Box::pin(async move {
                            if first.swap(false, Ordering::SeqCst) {
                                released.notify_one();
                                resume.notified().await;
                            }
                            Ok(true)
                        })
                    }
                })
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(storage.database_path())
                        .journal_mode(SqliteJournalMode::Wal),
                )
                .await
                .unwrap();
            let reader = Arc::new(Storage {
                pool,
                data_root: storage.data_root.clone(),
                replay_root: storage.replay_root.clone(),
                replay_degraded: AtomicBool::new(false),
                max_connections: 1,
            });
            // Before the fix, the first connection release follows the summary
            // query and precedes roster, auxiliary and authoritative result reads.
            let mut read = tokio::spawn({
                let reader = reader.clone();
                async move {
                    let view = reader.load_replay(match_id).await?;
                    reader.encode_replay_view(&view)
                }
            });
            let finished_before_commit = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::select! {
                    _ = released.notified() => None,
                    outcome = &mut read => Some(outcome.unwrap()),
                }
            })
            .await
            .unwrap();
            let next = if delete {
                sqlx::query("DELETE FROM matches WHERE match_id = ?")
                    .bind(match_id)
                    .execute(storage.pool())
                    .await
                    .unwrap();
                for table in [
                    "match_players",
                    "match_kyoku_results",
                    "replay_auxiliary_events",
                ] {
                    let count: i64 = sqlx::query_scalar(&format!(
                        "SELECT count(*) FROM {table} WHERE match_id = ?"
                    ))
                    .bind(match_id)
                    .fetch_one(storage.pool())
                    .await
                    .unwrap();
                    assert_eq!(count, 0, "delete cascades {table}");
                }
                assert!(matches!(
                    storage.load_replay(match_id).await,
                    Err(StorageError::ReplayNotFound)
                ));
                None
            } else {
                let mut change = storage.pool().begin().await.unwrap();
                sqlx::query("UPDATE matches SET room_name = 'next-room' WHERE match_id = ?")
                    .bind(match_id)
                    .execute(&mut *change)
                    .await
                    .unwrap();
                sqlx::query("UPDATE match_players SET character_id = 'next-character', display_name = 'Next Recorded 0' WHERE match_id = ? AND seat = 0")
            .bind(match_id).execute(&mut *change).await.unwrap();
                let mut next_result = result.clone();
                next_result.players[0].display_name = "Next Recorded 0".into();
                sqlx::query("UPDATE matches SET authoritative_result_json = ? WHERE match_id = ?")
                    .bind(serde_json::to_string(&next_result).unwrap())
                    .bind(match_id)
                    .execute(&mut *change)
                    .await
                    .unwrap();
                let auxiliary_json = serde_json::to_string(&AuxiliaryEvent::Disconnected {
                    seat: Seat::new(0).unwrap(),
                })
                .unwrap();
                sqlx::query("INSERT INTO replay_auxiliary_events (match_id, line_index, phase, sequence, payload_json) VALUES (?, 0, 'before', 0, ?)")
            .bind(match_id).bind(auxiliary_json).execute(&mut *change).await.unwrap();
                change.commit().await.unwrap();
                let next = storage
                    .encode_replay_view(&storage.load_replay(match_id).await.unwrap())
                    .unwrap();
                assert_ne!(prior, next, "the committed second view must differ");
                Some(next)
            };
            resume.notify_one();
            let completed_before_commit = finished_before_commit.is_some();
            let read_result = match finished_before_commit {
                Some(outcome) => outcome,
                None => tokio::time::timeout(Duration::from_secs(5), read)
                    .await
                    .unwrap()
                    .unwrap(),
            };
            let observed = read_result.unwrap();
            assert!(
                observed == prior || next.as_ref().is_some_and(|next| &observed == next),
                "Replay view mixed summary/roster/auxiliary/results across the first connection release"
            );
            if completed_before_commit {
                // A fixed read can finish before the concurrent commit; the
                // barrier then witnesses release, not forced interleaving.
                assert_eq!(observed, prior);
            }
            eprintln!(
                "snapshot observation: completed_before_commit={completed_before_commit}, observed={}",
                if observed == prior { "prior" } else { "next" }
            );
            reader.close().await;
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test]
    async fn replay_file_disappearance_never_returns_a_fabricated_view() {
        let root = test_root("replay-file-disappearance");
        let storage = Storage::connect(&root).await.unwrap();
        let match_id = "replay-file-disappearance";
        let mode = GameMode::FourPlayerRedEast;
        let (events, record, result, roster) = final_draw_capture(mode);
        let mut writer = ReplayWriter::new(storage.replay_root(), match_id, mode).unwrap();
        for event in &events {
            writer.append(event.clone()).unwrap();
        }
        let artifact = writer.finalize().unwrap();
        storage
            .open_ranked_match(match_id, mode, 1, &artifact.relative_path_string(), &roster)
            .await
            .unwrap();
        storage
            .complete_ranked_match(
                match_id,
                &artifact,
                &result,
                2,
                std::slice::from_ref(&record),
            )
            .await
            .unwrap();
        assert_eq!(
            storage.load_replay(match_id).await.unwrap().result,
            Some(result)
        );
        fs::remove_file(storage.replay_root().join(artifact.relative_path_string())).unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM matches WHERE match_id = ?")
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert_eq!(
            status, "completed",
            "delete only the owned finalized artifact"
        );
        assert!(matches!(
            storage.load_replay(match_id).await,
            Err(StorageError::ReplayUnavailable)
        ));
        let (listed, total) = storage.list_replays(0, 10).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].match_id, match_id);
        assert_eq!(listed[0].availability, "unavailable");
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn ranked_saved_authoritative_result_round_trips_and_rejects_missing_rows() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let root = test_root(if mode.seat_count() == 4 {
                "saved-final-4p"
            } else {
                "saved-final-3p"
            });
            let storage = Storage::connect(&root).await.unwrap();
            let match_id = if mode.seat_count() == 4 {
                "saved-final-4p"
            } else {
                "saved-final-3p"
            };
            let (events, record, result, roster) = final_draw_capture(mode);
            let mut writer = ReplayWriter::new(storage.replay_root(), match_id, mode).unwrap();
            for event in &events {
                writer.append(event.clone()).unwrap();
            }
            let artifact = writer.finalize().unwrap();
            storage
                .open_ranked_match(match_id, mode, 1, &artifact.relative_path_string(), &roster)
                .await
                .unwrap();
            let marker: Option<i64> = sqlx::query_scalar(
                "SELECT authoritative_results_version FROM matches WHERE match_id = ?",
            )
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(marker, Some(1));
            storage
                .complete_ranked_match(
                    match_id,
                    &artifact,
                    &result,
                    2,
                    std::slice::from_ref(&record),
                )
                .await
                .unwrap();
            let view = storage.load_replay(match_id).await.unwrap();
            assert_eq!(view.result, Some(result.clone()));
            assert_eq!(
                view.frames[record.end_event_index].settlement,
                Some(record.settlement.clone())
            );
            assert_eq!(
                view.frames[record.end_event_index - 1].visible_event,
                events[record.end_event_index - 1]
            );
            assert!(view.frames.iter().all(|frame| frame.result.is_none()));
            let stored: String = sqlx::query_scalar("SELECT payload_json FROM match_kyoku_results WHERE match_id = ? AND kyoku_sequence = 1")
                .bind(match_id).fetch_one(storage.pool()).await.unwrap();
            assert_eq!(
                serde_json::from_str::<KyokuResultRecord>(&stored).unwrap(),
                record
            );
            let final_json: String = sqlx::query_scalar(
                "SELECT authoritative_result_json FROM matches WHERE match_id = ?",
            )
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(
                serde_json::from_str::<MatchResult>(&final_json).unwrap(),
                result
            );
            sqlx::query(
                "DELETE FROM match_kyoku_results WHERE match_id = ? AND kyoku_sequence = 1",
            )
            .bind(match_id)
            .execute(storage.pool())
            .await
            .unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            let version: Option<i64> = sqlx::query_scalar(
                "SELECT authoritative_results_version FROM matches WHERE match_id = ?",
            )
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(
                version,
                Some(1),
                "missing version-1 record is not historical NULL"
            );
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test]
    async fn ranked_authoritative_completion_rejects_persisted_mode_mismatch() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let root = test_root(if mode.seat_count() == 4 {
                "mode-mismatch-4p"
            } else {
                "mode-mismatch-3p"
            });
            let storage = Storage::connect(&root).await.unwrap();
            let match_id = if mode.seat_count() == 4 {
                "mode-mismatch-4p"
            } else {
                "mode-mismatch-3p"
            };
            let (events, record, result, roster) = final_draw_capture(mode);
            let mut writer = ReplayWriter::new(storage.replay_root(), match_id, mode).unwrap();
            for event in &events {
                writer.append(event.clone()).unwrap();
            }
            let artifact = writer.finalize().unwrap();
            let admitted_mode = if mode.seat_count() == 4 {
                GameMode::FourPlayerRedHalf
            } else {
                GameMode::ThreePlayerRedHalf
            };
            storage
                .open_ranked_match(
                    match_id,
                    admitted_mode,
                    1,
                    &artifact.relative_path_string(),
                    &roster,
                )
                .await
                .unwrap();
            assert!(
                storage
                    .complete_ranked_match(match_id, &artifact, &result, 2, &[record])
                    .await
                    .is_err(),
                "completion must reject a normally admitted Half row with an East bundle"
            );
            let row = sqlx::query("SELECT status, game_mode, authoritative_results_version, authoritative_result_json, completed_at FROM matches WHERE match_id = ?")
                .bind(match_id).fetch_one(storage.pool()).await.unwrap();
            assert_eq!(row.try_get::<String, _>("status").unwrap(), "writing");
            assert_eq!(
                row.try_get::<String, _>("game_mode").unwrap(),
                admitted_mode.as_str()
            );
            assert_eq!(
                row.try_get::<Option<i64>, _>("authoritative_results_version")
                    .unwrap(),
                Some(1)
            );
            assert_eq!(
                row.try_get::<Option<String>, _>("authoritative_result_json")
                    .unwrap(),
                None
            );
            assert_eq!(row.try_get::<Option<i64>, _>("completed_at").unwrap(), None);
            let points: Vec<Option<i64>> = sqlx::query_scalar(
                "SELECT final_points FROM match_players WHERE match_id = ? ORDER BY seat",
            )
            .bind(match_id)
            .fetch_all(storage.pool())
            .await
            .unwrap();
            assert_eq!(points, vec![None; mode.seat_count()]);
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM match_kyoku_results WHERE match_id = ?")
                    .bind(match_id)
                    .fetch_one(storage.pool())
                    .await
                    .unwrap();
            assert_eq!(count, 0);
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
    }

    async fn open_final_draw_ranked(
        storage: &Storage,
        match_id: &str,
        mode: GameMode,
    ) -> (ReplayArtifact, KyokuResultRecord, MatchResult) {
        let (events, record, result, roster) = final_draw_capture(mode);
        let mut writer = ReplayWriter::new(storage.replay_root(), match_id, mode).unwrap();
        for event in events {
            writer.append(event).unwrap();
        }
        let artifact = writer.finalize().unwrap();
        storage
            .open_ranked_match(match_id, mode, 1, &artifact.relative_path_string(), &roster)
            .await
            .unwrap();
        (artifact, record, result)
    }

    async fn assert_writing_without_authoritative_result(
        storage: &Storage,
        match_id: &str,
        seats: usize,
    ) {
        let row = sqlx::query("SELECT status, authoritative_results_version, authoritative_result_json, completed_at FROM matches WHERE match_id = ?")
            .bind(match_id).fetch_one(storage.pool()).await.unwrap();
        assert_eq!(row.try_get::<String, _>("status").unwrap(), "writing");
        assert_eq!(
            row.try_get::<Option<i64>, _>("authoritative_results_version")
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("authoritative_result_json")
                .unwrap(),
            None
        );
        assert_eq!(row.try_get::<Option<i64>, _>("completed_at").unwrap(), None);
        let points: Vec<Option<i64>> = sqlx::query_scalar(
            "SELECT final_points FROM match_players WHERE match_id = ? ORDER BY seat",
        )
        .bind(match_id)
        .fetch_all(storage.pool())
        .await
        .unwrap();
        assert_eq!(points, vec![None; seats]);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM match_kyoku_results WHERE match_id = ?")
                .bind(match_id)
                .fetch_one(storage.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn bounded_result_json_rejects_overflow() {
        assert_eq!(bounded_result_json(&"ok").unwrap(), "\"ok\"");
        let mut payload = "x".repeat(MAX_DECOMPRESSED_REPLAY_BYTES - 2);
        assert_eq!(
            bounded_result_json(&payload).unwrap().len(),
            MAX_DECOMPRESSED_REPLAY_BYTES
        );
        payload.push('x');
        assert!(
            matches!(
                bounded_result_json(&payload),
                Err(StorageError::ReplayTooLarge)
            ),
            "a serialized JSON string one byte beyond the boundary must be rejected"
        );
    }

    #[tokio::test]
    async fn authoritative_final_json_is_bounded_before_decode() {
        let root = test_root("final-json-byte-bound");
        let storage = Storage::connect(&root).await.unwrap();
        let match_id = "final-json-byte-bound";
        let mode = GameMode::FourPlayerRedEast;
        let (artifact, record, result) = open_final_draw_ranked(&storage, match_id, mode).await;
        storage
            .complete_ranked_match(match_id, &artifact, &result, 2, &[record])
            .await
            .unwrap();
        sqlx::query("UPDATE matches SET authoritative_result_json = CAST(X'FF' || zeroblob(?) AS TEXT) WHERE match_id = ?")
            .bind(i64::try_from(MAX_DECOMPRESSED_REPLAY_BYTES).unwrap())
            .bind(match_id)
            .execute(storage.pool()).await.unwrap();
        let oversized = storage.load_replay(match_id).await;
        assert!(
            matches!(oversized, Err(StorageError::ReplayTooLarge)),
            "oversized invalid UTF-8 must be rejected by byte length before decoding: {oversized:?}"
        );
        sqlx::query("UPDATE matches SET authoritative_result_json = '{' WHERE match_id = ?")
            .bind(match_id)
            .execute(storage.pool())
            .await
            .unwrap();
        assert!(matches!(
            storage.load_replay(match_id).await,
            Err(StorageError::ReplayCorrupt)
        ));
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authoritative_replay_rejects_foreign_identity_and_short_settlement() {
        let root = test_root("restored-tile-domain");
        let storage = Storage::connect(&root).await.unwrap();
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let record: KyokuResultRecord = serde_json::from_value(case["record"].clone()).unwrap();
            let match_id = format!("restored-tile-{}", record.mode.seat_count());
            let events: Vec<GameEvent> = serde_json::from_value(case["events"].clone()).unwrap();
            let mut writer =
                ReplayWriter::new(storage.replay_root(), &match_id, record.mode).unwrap();
            for event in events {
                writer.append(event).unwrap();
            }
            writer.append(GameEvent::EndGame).unwrap();
            let artifact = writer.finalize().unwrap();
            // Storage-invariant result from measured fixture scores, not a MatchMachine capture.
            let (_, _, mut result, roster) = final_draw_capture(record.mode);
            result.final_scores = record.settlement.changes.iter().map(|c| c.after).collect();
            for (seat, player) in result.players.iter_mut().enumerate() {
                player.final_score = result.final_scores[seat];
                player.rank = (1 + result
                    .final_scores
                    .iter()
                    .enumerate()
                    .filter(|(other, score)| {
                        **score > player.final_score
                            || (**score == player.final_score && *other < seat)
                    })
                    .count()) as u8;
            }
            result.validate().unwrap();
            storage
                .open_ranked_match(
                    &match_id,
                    record.mode,
                    1,
                    &artifact.relative_path_string(),
                    &roster,
                )
                .await
                .unwrap();
            storage
                .complete_ranked_match(
                    &match_id,
                    &artifact,
                    &result,
                    2,
                    std::slice::from_ref(&record),
                )
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(&match_id).await.unwrap().result,
                Some(result)
            );
            for corruption in 0..5 {
                let mut corrupt = record.clone();
                match corruption {
                    0 => {
                        let winner = &mut corrupt.wins[0];
                        winner.winner = "foreign-identity".into();
                        corrupt.settlement.changes[usize::from(winner.seat.index())]
                            .participant_id = winner.winner.clone();
                    }
                    1 => {
                        corrupt.settlement.changes.pop();
                    }
                    2 => corrupt.settlement.changes.swap(0, 1),
                    3 => {
                        let double_riichi_core::WinEvaluation::Ordinary { limit, .. } =
                            &mut corrupt.wins[1].evaluation
                        else {
                            panic!("ordinary fixture");
                        };
                        *limit = Some(double_riichi_core::ScoreLimit::CountedYakuman);
                    }
                    _ => corrupt.wins[0].winning_tile = serde_json::from_value(json!(255)).unwrap(),
                }
                let payload = serde_json::to_string(&corrupt).unwrap();
                sqlx::query("UPDATE match_kyoku_results SET payload_json = ? WHERE match_id = ? AND kyoku_sequence = 1")
                    .bind(&payload).bind(&match_id).execute(storage.pool()).await.unwrap();
                assert!(
                    matches!(
                        storage.load_replay(&match_id).await,
                        Err(StorageError::ReplayCorrupt)
                    ),
                    "identity/count/seat/limit/tile corruption {corruption}"
                );
                let stored: String = sqlx::query_scalar("SELECT payload_json FROM match_kyoku_results WHERE match_id = ? AND kyoku_sequence = 1")
                    .bind(&match_id).fetch_one(storage.pool()).await.unwrap();
                assert_eq!(stored, payload, "read must not repair a corrupt record");
            }
        }
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authoritative_persistence_bounds_stored_roster_before_decode() {
        let root = test_root("persist-roster-bounds");
        let storage = Storage::connect(&root).await.unwrap();
        let mode = GameMode::FourPlayerRedEast;
        for case in ["ascii", "invalid-utf8", "aggregate"] {
            let match_id = format!("persist-roster-{case}");
            let (_, record, _) = open_final_draw_ranked(&storage, &match_id, mode).await;
            let before = record.clone();
            let bytes = if case == "aggregate" {
                MAX_DECOMPRESSED_REPLAY_BYTES / 2
            } else {
                MAX_DECOMPRESSED_REPLAY_BYTES
            };
            sqlx::query("UPDATE match_players SET participant_id = CAST(? || zeroblob(?) || CAST(seat AS TEXT) AS TEXT) WHERE match_id = ? AND seat < ?")
                .bind(if case == "invalid-utf8" { vec![255u8] } else { vec![b'x'] })
                .bind(i64::try_from(bytes).unwrap()).bind(&match_id)
                .bind(if case == "aggregate" { 2i64 } else { 1i64 })
                .execute(storage.pool()).await.unwrap();
            let outcome = storage.persist_kyoku_record(&match_id, &record).await;
            assert!(
                matches!(outcome, Err(StorageError::ReplayTooLarge)),
                "stored roster {case} must be bounded before decode: {outcome:?}"
            );
            assert_eq!(record, before);
            assert_writing_without_authoritative_result(&storage, &match_id, mode.seat_count())
                .await;
        }
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authoritative_persistence_bounds_stored_records_before_decode_and_insert() {
        let root = test_root("persist-record-bounds");
        let storage = Storage::connect(&root).await.unwrap();
        let mode = GameMode::FourPlayerRedEast;
        for case in [
            "ascii",
            "invalid-utf8",
            "aggregate",
            "prospective",
            "count",
            "exact-limit",
        ] {
            let match_id = format!("persist-record-{case}");
            let (_, mut record, _) = open_final_draw_ranked(&storage, &match_id, mode).await;
            storage
                .persist_kyoku_record(&match_id, &record)
                .await
                .unwrap();
            let original = serde_json::to_string(&record).unwrap();
            if case == "count" {
                sqlx::query("WITH RECURSIVE numbers(n) AS (SELECT 2 UNION ALL SELECT n + 1 FROM numbers WHERE n < 100000) INSERT INTO match_kyoku_results (match_id, kyoku_sequence, version, end_event_index, payload_json) SELECT ?, n, 1, 0, '{' FROM numbers")
                    .bind(&match_id).execute(storage.pool()).await.unwrap();
                record.kyoku_sequence = 100001;
            } else if case == "prospective" || case == "exact-limit" {
                // Valid JSON padded to the exact byte ceiling; no oversized Rust allocation.
                sqlx::query("UPDATE match_kyoku_results SET payload_json = ? || replace(hex(zeroblob(?)), '00', ' ') WHERE match_id = ?")
                    .bind(&original).bind(i64::try_from(MAX_DECOMPRESSED_REPLAY_BYTES - original.len()).unwrap())
                    .bind(&match_id).execute(storage.pool()).await.unwrap();
                if case == "prospective" {
                    record.kyoku_sequence = 2;
                }
            } else {
                let bytes = if case == "aggregate" {
                    MAX_DECOMPRESSED_REPLAY_BYTES / 2
                } else {
                    MAX_DECOMPRESSED_REPLAY_BYTES
                };
                sqlx::query("UPDATE match_kyoku_results SET payload_json = CAST(? || zeroblob(?) AS TEXT) WHERE match_id = ?")
                    .bind(if case == "invalid-utf8" { vec![255u8] } else { vec![b'x'] })
                    .bind(i64::try_from(bytes).unwrap()).bind(&match_id).execute(storage.pool()).await.unwrap();
                if case == "aggregate" {
                    sqlx::query("INSERT INTO match_kyoku_results (match_id, kyoku_sequence, version, end_event_index, payload_json) SELECT match_id, 2, version, end_event_index, payload_json FROM match_kyoku_results WHERE match_id = ?")
                        .bind(&match_id).execute(storage.pool()).await.unwrap();
                }
            }
            let before = record.clone();
            let stats_before: (i64, i64) = sqlx::query_as("SELECT count(*), COALESCE(sum(length(CAST(payload_json AS BLOB))), 0) FROM match_kyoku_results WHERE match_id = ?")
                .bind(&match_id).fetch_one(storage.pool()).await.unwrap();
            let outcome = storage.persist_kyoku_record(&match_id, &record).await;
            if case == "exact-limit" {
                // Equality is still byte-exact: padding conflicts, but is NOT TooLarge.
                assert!(
                    matches!(outcome, Err(StorageError::ReplayCorrupt)),
                    "exact boundary must reach comparison: {outcome:?}"
                );
            } else {
                assert!(
                    matches!(outcome, Err(StorageError::ReplayTooLarge)),
                    "stored records {case} must reject before decode/insert: {outcome:?}"
                );
            }
            assert_eq!(record, before);
            let stats_after: (i64, i64) = sqlx::query_as("SELECT count(*), COALESCE(sum(length(CAST(payload_json AS BLOB))), 0) FROM match_kyoku_results WHERE match_id = ?")
                .bind(&match_id).fetch_one(storage.pool()).await.unwrap();
            assert_eq!(stats_after, stats_before);
            let status: String =
                sqlx::query_scalar("SELECT status FROM matches WHERE match_id = ?")
                    .bind(&match_id)
                    .fetch_one(storage.pool())
                    .await
                    .unwrap();
            assert_eq!(status, "writing");
        }
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authoritative_completion_bounds_frozen_roster_before_materialization() {
        let root = test_root("completion-roster-bounds");
        let storage = Storage::connect(&root).await.unwrap();
        let mode = GameMode::FourPlayerRedEast;
        for invalid_utf8 in [false, true] {
            let match_id = format!("completion-roster-{invalid_utf8}");
            let (artifact, record, result) =
                open_final_draw_ranked(&storage, &match_id, mode).await;
            let before = (record.clone(), result.clone());
            sqlx::query("UPDATE match_players SET character_id = CAST(? || zeroblob(?) AS TEXT) WHERE match_id = ? AND seat = 0")
                .bind(if invalid_utf8 { vec![255u8] } else { vec![b'x'] })
                .bind(i64::try_from(MAX_DECOMPRESSED_REPLAY_BYTES).unwrap())
                .bind(&match_id).execute(storage.pool()).await.unwrap();
            let completion = storage
                .complete_ranked_match(
                    &match_id,
                    &artifact,
                    &result,
                    2,
                    std::slice::from_ref(&record),
                )
                .await;
            assert!(
                matches!(completion, Err(StorageError::ReplayTooLarge)),
                "oversized roster must be refused before decoding/acknowledgment: {completion:?}"
            );
            assert_eq!((record, result), before);
            assert_writing_without_authoritative_result(&storage, &match_id, mode.seat_count())
                .await;
        }
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authoritative_completion_bounds_room_record_rows_before_decode() {
        let root = test_root("room-row-bounds");
        let storage = Storage::connect(&root).await.unwrap();
        let mode = GameMode::FourPlayerRedEast;
        let (events, _, result, roster) = final_draw_capture(mode);
        let snapshots = roster
            .iter()
            .enumerate()
            .map(|(seat, p)| MatchPlayerSnapshot {
                participant_id: p.id.clone(),
                display_name: p.display_name.clone(),
                kind: p.kind,
                seat: Seat::new(seat as u8).unwrap(),
                character_id: None,
                controller: double_riichi_core::RoomController::PermanentAuto(
                    double_riichi_core::PermanentAutoReason::BuiltInBot,
                ),
            })
            .collect::<Vec<_>>();
        for case in ["count", "bytes"] {
            let match_id = format!("room-row-bounds-{case}");
            let mut writer = ReplayWriter::new(storage.replay_root(), &match_id, mode).unwrap();
            for event in &events {
                writer.append(event.clone()).unwrap();
            }
            let artifact = writer.finalize().unwrap();
            storage
                .open_room_match(
                    &match_id,
                    mode,
                    "bounds",
                    1,
                    &artifact.relative_path_string(),
                    &snapshots,
                )
                .await
                .unwrap();
            if case == "count" {
                sqlx::query("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM numbers WHERE n < 100001) INSERT INTO match_kyoku_results (match_id, kyoku_sequence, version, end_event_index, payload_json) SELECT ?, n, 1, 0, '{' FROM numbers")
                    .bind(&match_id).execute(storage.pool()).await.unwrap();
            } else {
                sqlx::query("INSERT INTO match_kyoku_results (match_id, kyoku_sequence, version, end_event_index, payload_json) VALUES (?, 1, 1, 0, CAST(X'FF' || zeroblob(?) AS TEXT))")
                    .bind(&match_id).bind(i64::try_from(MAX_DECOMPRESSED_REPLAY_BYTES).unwrap())
                    .execute(storage.pool()).await.unwrap();
            }
            let completion = storage
                .complete_room_match(&match_id, &artifact, &result, 2)
                .await;
            assert!(
                matches!(completion, Err(StorageError::ReplayTooLarge)),
                "{case} must reject by metadata before decoding: {completion:?}"
            );
            assert_writing_without_authoritative_result_with_rows(
                &storage,
                &match_id,
                mode.seat_count(),
            )
            .await;
        }
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    async fn assert_writing_without_authoritative_result_with_rows(
        storage: &Storage,
        match_id: &str,
        seats: usize,
    ) {
        let row =
            sqlx::query("SELECT status, authoritative_result_json FROM matches WHERE match_id = ?")
                .bind(match_id)
                .fetch_one(storage.pool())
                .await
                .unwrap();
        assert_eq!(row.try_get::<String, _>("status").unwrap(), "writing");
        assert_eq!(
            row.try_get::<Option<String>, _>("authoritative_result_json")
                .unwrap(),
            None
        );
        let points: Vec<Option<i64>> = sqlx::query_scalar(
            "SELECT final_points FROM match_players WHERE match_id = ? ORDER BY seat",
        )
        .bind(match_id)
        .fetch_all(storage.pool())
        .await
        .unwrap();
        assert_eq!(points, vec![None; seats]);
    }

    #[tokio::test]
    async fn ranked_completion_bounds_borrowed_records_and_rolls_back() {
        let root = test_root("ranked-borrowed-bound");
        let storage = Storage::connect(&root).await.unwrap();
        let match_id = "ranked-borrowed-bound";
        let mode = GameMode::FourPlayerRedEast;
        let (artifact, record, result) = open_final_draw_ranked(&storage, match_id, mode).await;
        let before_result = result.clone();
        let records = vec![record.clone(); MAX_REPLAY_EVENTS + 1];
        // The registered path is valid; only its test-owned canonical bytes are corrupt.
        // Count must reject before opening/decoding the artifact or cloning this slice.
        fs::write(
            storage
                .resolve_replay_path(&artifact.relative_path_string())
                .unwrap(),
            b"not canonical replay json",
        )
        .unwrap();
        let completion = storage
            .complete_ranked_match(match_id, &artifact, &result, 2, &records)
            .await;
        assert!(
            matches!(completion, Err(StorageError::ReplayTooLarge)),
            "count preflight must precede canonical decode: {completion:?}"
        );
        assert_eq!(result, before_result);
        assert!(
            records.iter().all(|item| item == &record),
            "borrowed records stay unchanged"
        );
        assert_writing_without_authoritative_result(&storage, match_id, mode.seat_count()).await;

        // Use an unmodified pinned multi-Ron record: unlike the shorter draw
        // record, its bounded JSON crosses the aggregate limit below 100_000 rows.
        let captures: serde_json::Value = serde_json::from_str(include_str!(
            "../../double_riichi_core/tests/fixtures/authoritative-multi-ron-captures.json"
        ))
        .unwrap();
        let aggregate_record: KyokuResultRecord = serde_json::from_value(
            captures
                .as_array()
                .unwrap()
                .iter()
                .find(|capture| {
                    capture["scenario"] == "ordered_multi_ron"
                        && capture["mode"] == serde_json::to_value(mode).unwrap()
                })
                .unwrap()["record"]
                .clone(),
        )
        .unwrap();
        let record_bytes = bounded_result_json(&aggregate_record).unwrap().len();
        assert!(record_bytes < MAX_DECOMPRESSED_REPLAY_BYTES);
        let aggregate_count = MAX_DECOMPRESSED_REPLAY_BYTES / record_bytes + 1;
        assert!(aggregate_count <= MAX_REPLAY_EVENTS);
        assert!(aggregate_count * record_bytes > MAX_DECOMPRESSED_REPLAY_BYTES);
        let aggregate_id = "ranked-aggregate-bound";
        let (aggregate_artifact, _, aggregate_result) =
            open_final_draw_ranked(&storage, aggregate_id, mode).await;
        let aggregate_before = aggregate_result.clone();
        let aggregate_records = vec![aggregate_record.clone(); aggregate_count];
        fs::write(
            storage
                .resolve_replay_path(&aggregate_artifact.relative_path_string())
                .unwrap(),
            b"not canonical replay json",
        )
        .unwrap();
        let aggregate_completion = storage
            .complete_ranked_match(
                aggregate_id,
                &aggregate_artifact,
                &aggregate_result,
                2,
                &aggregate_records,
            )
            .await;
        assert!(
            matches!(aggregate_completion, Err(StorageError::ReplayTooLarge)),
            "aggregate preflight must precede canonical decode: {aggregate_completion:?}"
        );
        assert_eq!(aggregate_result, aggregate_before);
        assert!(
            aggregate_records
                .iter()
                .all(|item| item == &aggregate_record)
        );
        assert_writing_without_authoritative_result(&storage, aggregate_id, mode.seat_count())
            .await;
        storage.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn authoritative_record_persistence_is_idempotent_and_rejects_conflicts() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let match_id = if mode.seat_count() == 4 {
                "record-invariants-4p"
            } else {
                "record-invariants-3p"
            };
            let root = test_root(match_id);
            let storage = Storage::connect(&root).await.unwrap();
            let (_, record, _) = open_final_draw_ranked(&storage, match_id, mode).await;
            storage
                .persist_kyoku_record(match_id, &record)
                .await
                .unwrap();
            storage
                .persist_kyoku_record(match_id, &record)
                .await
                .unwrap();
            let initial: String = sqlx::query_scalar("SELECT payload_json FROM match_kyoku_results WHERE match_id = ? AND kyoku_sequence = 1")
                .bind(match_id).fetch_one(storage.pool()).await.unwrap();
            assert_eq!(
                serde_json::from_str::<KyokuResultRecord>(&initial).unwrap(),
                record
            );
            let mut conflicting = record.clone();
            conflicting.draw_event_delta_adjustment.as_mut().unwrap()[0] += 1;
            let roster = final_draw_capture(mode).3;
            conflicting.validate(&roster).unwrap();
            assert!(matches!(
                storage.persist_kyoku_record(match_id, &conflicting).await,
                Err(StorageError::ReplayCorrupt)
            ));
            let mut skipped = record.clone();
            skipped.kyoku_sequence = 3;
            assert!(matches!(
                storage.persist_kyoku_record(match_id, &skipped).await,
                Err(StorageError::ReplayCorrupt)
            ));
            let mut unsupported = record.clone();
            unsupported.version = 2;
            assert!(
                storage
                    .persist_kyoku_record(match_id, &unsupported)
                    .await
                    .is_err()
            );
            let stored: Vec<String> = sqlx::query_scalar("SELECT payload_json FROM match_kyoku_results WHERE match_id = ? ORDER BY kyoku_sequence")
                .bind(match_id).fetch_all(storage.pool()).await.unwrap();
            assert_eq!(stored, vec![initial]);
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test]
    async fn authoritative_completion_requires_complete_consistent_results() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let suffix = if mode.seat_count() == 4 { "4p" } else { "3p" };
            let root = test_root(&format!("complete-invariants-{suffix}"));
            let storage = Storage::connect(&root).await.unwrap();
            let valid = format!("complete-valid-{suffix}");
            let (artifact, record, result) = open_final_draw_ranked(&storage, &valid, mode).await;
            storage
                .complete_ranked_match(&valid, &artifact, &result, 2, std::slice::from_ref(&record))
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(&valid).await.unwrap().result,
                Some(result.clone())
            );
            for case in ["missing", "final-score", "roster"] {
                let match_id = format!("complete-{case}-{suffix}");
                let (artifact, record, mut result) =
                    open_final_draw_ranked(&storage, &match_id, mode).await;
                if case == "final-score" {
                    result.final_scores[1] += 100;
                    result.players[1].final_score += 100;
                    result.validate().unwrap();
                } else if case == "roster" {
                    result.players[0].participant_id = "different-participant".into();
                    result.validate().unwrap();
                }
                let records = if case == "missing" {
                    &[][..]
                } else {
                    std::slice::from_ref(&record)
                };
                assert!(
                    storage
                        .complete_ranked_match(&match_id, &artifact, &result, 2, records)
                        .await
                        .is_err(),
                    "{case} {mode:?}"
                );
                assert_writing_without_authoritative_result(&storage, &match_id, mode.seat_count())
                    .await;
            }
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test]
    async fn authoritative_replay_rejects_corrupt_metadata_without_legacy_fallback() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let match_id = if mode.seat_count() == 4 {
                "read-corrupt-4p"
            } else {
                "read-corrupt-3p"
            };
            let root = test_root(match_id);
            let storage = Storage::connect(&root).await.unwrap();
            let (artifact, record, result) = open_final_draw_ranked(&storage, match_id, mode).await;
            storage
                .complete_ranked_match(
                    match_id,
                    &artifact,
                    &result,
                    2,
                    std::slice::from_ref(&record),
                )
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(match_id).await.unwrap().result,
                Some(result.clone())
            );
            let original_record = serde_json::to_string(&record).unwrap();
            let original_final = serde_json::to_string(&result).unwrap();
            sqlx::query("UPDATE matches SET authoritative_results_version = 2 WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            sqlx::query("UPDATE matches SET authoritative_results_version = 1 WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            sqlx::query("UPDATE match_kyoku_results SET kyoku_sequence = 2 WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            sqlx::query("UPDATE match_kyoku_results SET kyoku_sequence = 1 WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            sqlx::query("UPDATE match_kyoku_results SET version = 2 WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            sqlx::query("UPDATE match_kyoku_results SET version = 1 WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(match_id).await.unwrap().result,
                Some(result.clone())
            );
            sqlx::query("UPDATE match_kyoku_results SET end_event_index = end_event_index + 1 WHERE match_id = ?")
                .bind(match_id).execute(storage.pool()).await.unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            sqlx::query("UPDATE match_kyoku_results SET end_event_index = ? WHERE match_id = ?")
                .bind(i64::try_from(record.end_event_index).unwrap())
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(match_id).await.unwrap().result,
                Some(result.clone())
            );
            sqlx::query("UPDATE match_kyoku_results SET payload_json = '{' WHERE match_id = ?")
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            sqlx::query("UPDATE match_kyoku_results SET payload_json = ? WHERE match_id = ?")
                .bind(&original_record)
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(match_id).await.unwrap().result,
                Some(result.clone())
            );
            let mut incorrect_final = result.clone();
            incorrect_final.final_scores[1] += 100;
            incorrect_final.players[1].final_score += 100;
            incorrect_final.validate().unwrap();
            sqlx::query("UPDATE matches SET authoritative_result_json = ? WHERE match_id = ?")
                .bind(serde_json::to_string(&incorrect_final).unwrap())
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert!(matches!(
                storage.load_replay(match_id).await,
                Err(StorageError::ReplayCorrupt)
            ));
            sqlx::query("UPDATE matches SET authoritative_result_json = ? WHERE match_id = ?")
                .bind(original_final)
                .bind(match_id)
                .execute(storage.pool())
                .await
                .unwrap();
            assert_eq!(
                storage.load_replay(match_id).await.unwrap().result,
                Some(result)
            );
            let version: Option<i64> = sqlx::query_scalar(
                "SELECT authoritative_results_version FROM matches WHERE match_id = ?",
            )
            .bind(match_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(version, Some(1));
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test]
    async fn authoritative_completion_database_failure_rolls_back_metadata() {
        for mode in [GameMode::FourPlayerRedEast, GameMode::ThreePlayerRedEast] {
            let match_id = if mode.seat_count() == 4 {
                "rollback-trigger-4p"
            } else {
                "rollback-trigger-3p"
            };
            let root = test_root(match_id);
            let storage = Storage::connect(&root).await.unwrap();
            let (artifact, record, result) = open_final_draw_ranked(&storage, match_id, mode).await;
            sqlx::query("CREATE TRIGGER reject_final_authority BEFORE UPDATE OF authoritative_result_json ON matches WHEN NEW.authoritative_result_json IS NOT NULL BEGIN SELECT RAISE(ABORT, 'test-owned final metadata failure'); END")
                .execute(storage.pool()).await.unwrap();
            assert!(matches!(
                storage
                    .complete_ranked_match(match_id, &artifact, &result, 2, &[record])
                    .await,
                Err(StorageError::Sqlx(_))
            ));
            assert_writing_without_authoritative_result(&storage, match_id, mode.seat_count())
                .await;
            storage.close().await;
            let _ = fs::remove_dir_all(root);
        }
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
        // This auxiliary-order fixture has no terminal result; only its own
        // synthetic match row represents historical nullable metadata.
        sqlx::query("UPDATE matches SET authoritative_results_version = NULL WHERE match_id = ?")
            .bind("ranked-aux15")
            .execute(storage.pool())
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
            .complete_ranked_match("ranked-aux15", &artifact, &result, 2, &[])
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
