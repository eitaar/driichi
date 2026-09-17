use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use double_riichi_core::{GameEvent, GameMode, Seat};
use serde::{Deserialize, Serialize};

use crate::{
    error::{PersistenceError, ReplayError},
    mjson::{CanonicalEvent, serialize_event},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxiliaryPhase {
    Before,
    After,
}

impl AuxiliaryPhase {
    pub const fn sort_key(self) -> u8 {
        match self {
            Self::Before => 0,
            Self::After => 2,
        }
    }
}

impl Serialize for AuxiliaryPhase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Before => "before",
            Self::After => "after",
        })
    }
}

impl<'de> Deserialize<'de> for AuxiliaryPhase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "before" => Ok(Self::Before),
            "after" => Ok(Self::After),
            other => Err(serde::de::Error::custom(format!(
                "invalid auxiliary phase {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuxiliaryEvent {
    Disconnected { seat: Seat },
    Reconnected { seat: Seat },
    AutoStarted { seat: Seat },
    Left { seat: Seat },
    TokenRevoked { seat: Seat },
    Kicked { seat: Seat },
}

impl AuxiliaryEvent {
    pub const fn seat(&self) -> Seat {
        match self {
            Self::Disconnected { seat }
            | Self::Reconnected { seat }
            | Self::AutoStarted { seat }
            | Self::Left { seat }
            | Self::TokenRevoked { seat }
            | Self::Kicked { seat } => *seat,
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Disconnected { .. } => "disconnected",
            Self::Reconnected { .. } => "reconnected",
            Self::AutoStarted { .. } => "auto_started",
            Self::Left { .. } => "left",
            Self::TokenRevoked { .. } => "token_revoked",
            Self::Kicked { .. } => "kicked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuxiliaryRecord {
    pub event: AuxiliaryEvent,
    pub line_index: usize,
    pub phase: AuxiliaryPhase,
    pub sequence: u64,
}

pub type ReplayAuxiliaryEvent = AuxiliaryRecord;

impl AuxiliaryRecord {
    pub fn kind(&self) -> &'static str {
        self.event.kind()
    }

    pub fn seat(&self) -> Seat {
        self.event.seat()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayArtifact {
    pub relative_path: PathBuf,
    pub file_size: u64,
    pub auxiliary_events: Vec<AuxiliaryRecord>,
}

impl ReplayArtifact {
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn relative_path_string(&self) -> String {
        self.relative_path.to_string_lossy().replace('\\', "/")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureInjection {
    fail_after_writes: usize,
}

impl FailureInjection {
    pub const fn fail_after_writes(writes: usize) -> Self {
        Self {
            fail_after_writes: writes,
        }
    }
}

pub struct ReplayWriter {
    root: PathBuf,
    mode: GameMode,
    match_id: String,
    part_path: PathBuf,
    relative_path: PathBuf,
    writer: Option<BufWriter<File>>,
    emitted_lines: usize,
    auxiliary_events: Vec<AuxiliaryRecord>,
    auxiliary_sequence: u64,
    writes: usize,
    failure_injection: Option<FailureInjection>,
}

impl ReplayWriter {
    pub fn new(
        root: impl AsRef<Path>,
        match_id: impl Into<String>,
        mode: GameMode,
    ) -> Result<Self, ReplayError> {
        Self::create_at(root, match_id, mode, SystemTime::now())
    }

    pub fn create(
        root: impl AsRef<Path>,
        match_id: impl Into<String>,
        mode: GameMode,
    ) -> Result<Self, ReplayError> {
        Self::new(root, match_id, mode)
    }

    pub fn create_at(
        root: impl AsRef<Path>,
        match_id: impl Into<String>,
        mode: GameMode,
        started_at: SystemTime,
    ) -> Result<Self, ReplayError> {
        Self::create_with_failure(root, match_id, mode, started_at, None)
    }

    pub fn with_failure_after_writes(
        root: impl AsRef<Path>,
        match_id: impl Into<String>,
        mode: GameMode,
        writes: usize,
    ) -> Result<Self, ReplayError> {
        Self::create_with_failure(
            root,
            match_id,
            mode,
            SystemTime::now(),
            Some(FailureInjection::fail_after_writes(writes)),
        )
    }

    pub fn with_failure_injection(
        root: impl AsRef<Path>,
        match_id: impl Into<String>,
        mode: GameMode,
        failure: FailureInjection,
    ) -> Result<Self, ReplayError> {
        Self::create_with_failure(root, match_id, mode, SystemTime::now(), Some(failure))
    }

    fn create_with_failure(
        root: impl AsRef<Path>,
        match_id: impl Into<String>,
        mode: GameMode,
        started_at: SystemTime,
        failure_injection: Option<FailureInjection>,
    ) -> Result<Self, ReplayError> {
        let root = root.as_ref().to_path_buf();
        let match_id = match_id.into();
        validate_match_id(&match_id)?;
        let directory = root.join(".incomplete");
        fs::create_dir_all(&directory).map_err(PersistenceError::from)?;
        let timestamp = utc_filename_timestamp(started_at);
        let filename = format!("{timestamp}_{}_{}.mjson", mode.as_str(), match_id);
        let part_path = directory.join(format!("{match_id}.mjson.part"));
        let relative_path = PathBuf::from(format!("{}/{filename}", mode_directory(mode)));
        let file = File::create(&part_path).map_err(PersistenceError::from)?;
        Ok(Self {
            root,
            mode,
            match_id,
            part_path,
            relative_path,
            writer: Some(BufWriter::new(file)),
            emitted_lines: 0,
            auxiliary_events: Vec::new(),
            auxiliary_sequence: 0,
            writes: 0,
            failure_injection,
        })
    }

    pub fn mode(&self) -> GameMode {
        self.mode
    }

    pub fn match_id(&self) -> &str {
        &self.match_id
    }

    pub fn part_path(&self) -> &Path {
        &self.part_path
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn relative_path_string(&self) -> String {
        self.relative_path.to_string_lossy().replace('\\', "/")
    }

    pub fn emitted_lines(&self) -> usize {
        self.emitted_lines
    }

    pub fn auxiliary_events(&self) -> &[AuxiliaryRecord] {
        &self.auxiliary_events
    }

    pub fn append(&mut self, event: CanonicalEvent) -> Result<usize, ReplayError> {
        crate::mjson::validate_event(&event, self.mode).map_err(ReplayError::InvalidEvent)?;
        let line = serialize_event(&event)?;
        self.write_line(&line)?;
        let index = self.emitted_lines;
        self.emitted_lines += 1;
        if matches!(event, GameEvent::EndKyoku) {
            self.flush_kyoku()?;
        }
        Ok(index)
    }

    pub fn append_event(&mut self, event: CanonicalEvent) -> Result<usize, ReplayError> {
        self.append(event)
    }

    pub fn write_event(&mut self, event: CanonicalEvent) -> Result<usize, ReplayError> {
        self.append(event)
    }

    pub fn record_auxiliary(
        &mut self,
        event: AuxiliaryEvent,
        phase: AuxiliaryPhase,
    ) -> Result<usize, ReplayError> {
        self.ensure_open()?;
        let line_index = match phase {
            AuxiliaryPhase::Before => self.emitted_lines,
            AuxiliaryPhase::After if self.emitted_lines > 0 => self.emitted_lines - 1,
            AuxiliaryPhase::After => {
                return Err(ReplayError::InvalidEvent(
                    "after auxiliary event requires an emitted MJSON event".into(),
                ));
            }
        };
        let sequence = self.auxiliary_sequence;
        self.auxiliary_sequence += 1;
        self.auxiliary_events.push(AuxiliaryRecord {
            event,
            line_index,
            phase,
            sequence,
        });
        Ok(line_index)
    }

    pub fn append_auxiliary(
        &mut self,
        event: AuxiliaryEvent,
        phase: AuxiliaryPhase,
    ) -> Result<usize, ReplayError> {
        self.record_auxiliary(event, phase)
    }

    pub fn flush_kyoku(&mut self) -> Result<(), ReplayError> {
        self.flush_buffer()
    }

    pub fn flush(&mut self) -> Result<(), ReplayError> {
        self.flush_buffer()
    }

    pub fn finalize(mut self) -> Result<ReplayArtifact, ReplayError> {
        if let Err(error) = self.validate_auxiliary_positions() {
            self.fail_and_cleanup();
            return Err(error);
        }
        if let Err(error) = self.flush_buffer() {
            return Err(error);
        }
        let result = self.finalize_inner();
        if result.is_err() {
            self.fail_and_cleanup();
        }
        result
    }

    fn finalize_inner(&mut self) -> Result<ReplayArtifact, ReplayError> {
        let writer = self.writer.take().ok_or(PersistenceError::Closed)?;
        let file = writer
            .into_inner()
            .map_err(|error| PersistenceError::Io(error.into_error()))?;
        file.sync_all().map_err(PersistenceError::from)?;
        let destination = self.root.join(&self.relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(PersistenceError::from)?;
        }
        fs::rename(&self.part_path, &destination).map_err(PersistenceError::Rename)?;
        let file_size = fs::metadata(&destination)
            .map_err(PersistenceError::from)?
            .len();
        Ok(ReplayArtifact {
            relative_path: self.relative_path.clone(),
            file_size,
            auxiliary_events: self.auxiliary_events.clone(),
        })
    }

    pub fn finish(self) -> Result<ReplayArtifact, ReplayError> {
        self.finalize()
    }

    /// Discard an unfinished replay without making it visible.
    pub fn abort(mut self) {
        self.fail_and_cleanup();
    }

    fn write_line(&mut self, line: &str) -> Result<(), ReplayError> {
        self.ensure_open()?;
        if let Some(failure) = self.failure_injection {
            if self.writes >= failure.fail_after_writes {
                self.fail_and_cleanup();
                return Err(PersistenceError::InjectedFailure.into());
            }
        }
        self.writes += 1;
        let result = self
            .writer
            .as_mut()
            .ok_or(PersistenceError::Closed)?
            .write_all(format!("{line}\n").as_bytes());
        if let Err(error) = result {
            self.fail_and_cleanup();
            return Err(PersistenceError::Io(error).into());
        }
        Ok(())
    }

    fn flush_buffer(&mut self) -> Result<(), ReplayError> {
        self.ensure_open()?;
        let result = self
            .writer
            .as_mut()
            .ok_or(PersistenceError::Closed)?
            .flush();
        if let Err(error) = result {
            self.fail_and_cleanup();
            return Err(PersistenceError::Io(error).into());
        }
        Ok(())
    }

    fn validate_auxiliary_positions(&self) -> Result<(), ReplayError> {
        if let Some(record) = self
            .auxiliary_events
            .iter()
            .find(|record| record.line_index >= self.emitted_lines)
        {
            return Err(ReplayError::InvalidEvent(format!(
                "auxiliary line index {} is outside emitted events",
                record.line_index
            )));
        }
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), ReplayError> {
        if self.writer.is_some() {
            Ok(())
        } else {
            Err(PersistenceError::Closed.into())
        }
    }

    fn fail_and_cleanup(&mut self) {
        self.writer.take();
        let _ = fs::remove_file(&self.part_path);
    }
}

pub fn read_mjson(path: impl AsRef<Path>) -> Result<Vec<CanonicalEvent>, ReplayError> {
    let path = path.as_ref();
    let size = fs::metadata(path)?.len();
    if size > crate::error::MAX_DECOMPRESSED_REPLAY_BYTES as u64 {
        return Err(ReplayError::ReplayTooLarge {
            actual: usize::try_from(size).unwrap_or(usize::MAX),
            limit: crate::error::MAX_DECOMPRESSED_REPLAY_BYTES,
        });
    }
    let bytes = fs::read(path)?;
    let contents = String::from_utf8(bytes).map_err(|error| ReplayError::Corrupt {
        line: 0,
        message: format!("replay is not valid UTF-8: {error}"),
    })?;
    crate::mjson::parse_mjson(contents)
}

pub fn startup_cleanup<I, S>(
    root: impl AsRef<Path>,
    unfinished_match_ids: I,
) -> Result<(), ReplayError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let root = root.as_ref();
    let ids: Vec<String> = unfinished_match_ids
        .into_iter()
        .map(|id| id.as_ref().to_owned())
        .collect();
    let incomplete = root.join(".incomplete");
    if let Ok(metadata) = fs::symlink_metadata(&incomplete)
        && !metadata.file_type().is_symlink()
        && metadata.is_dir()
    {
        for entry in fs::read_dir(&incomplete)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.file_type().is_symlink()
                && metadata.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "part")
            {
                fs::remove_file(path)?;
            }
        }
    }
    for directory in [root.join("4p"), root.join("3p")] {
        let Ok(metadata) = fs::symlink_metadata(&directory) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if ids.iter().any(|id| filename_matches_match_id(name, id)) {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

fn filename_matches_match_id(name: &str, match_id: &str) -> bool {
    if match_id.is_empty() {
        return false;
    }
    let Some(name) = name.strip_suffix(".mjson") else {
        return false;
    };
    name.ends_with(&format!("_{match_id}"))
}

fn mode_directory(mode: GameMode) -> &'static str {
    if mode.is_three_player() { "3p" } else { "4p" }
}

fn validate_match_id(match_id: &str) -> Result<(), ReplayError> {
    if match_id.is_empty()
        || match_id == "."
        || match_id == ".."
        || match_id.contains('/')
        || match_id.contains('\\')
        || match_id.contains('_')
        || match_id.contains('\0')
    {
        return Err(ReplayError::InvalidPath(
            "match ID is not a filename-safe component".into(),
        ));
    }
    Ok(())
}

impl Drop for ReplayWriter {
    fn drop(&mut self) {
        if self.writer.is_some() {
            let _ = fs::remove_file(&self.part_path);
        }
    }
}

fn utc_filename_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = seconds / 86_400;
    let day_seconds = seconds % 86_400;
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

// Howard Hinnant's public-domain civil date conversion, kept local to avoid a
// time dependency solely for deterministic replay filenames.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}
