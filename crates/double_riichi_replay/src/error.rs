use std::io;

use thiserror::Error;

pub const MAX_REPLAY_FRAME_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_REPLAY_BYTES: usize = MAX_REPLAY_FRAME_BYTES;
pub const MAX_DECOMPRESSED_REPLAY_BYTES: usize = MAX_REPLAY_FRAME_BYTES;
/// Bounds frame construction independently of the serialized byte ceiling.
pub const MAX_REPLAY_EVENTS: usize = 100_000;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("failure injection triggered")]
    InjectedFailure,
    #[error("replay writer is no longer usable")]
    Closed,
    #[error("atomic replay rename failed: {0}")]
    Rename(io::Error),
}

pub type ReplayPersistenceError = PersistenceError;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("replay I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("replay JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("corrupt replay at line {line}: {message}")]
    Corrupt { line: usize, message: String },
    #[error("invalid canonical replay event: {0}")]
    InvalidEvent(String),
    #[error("replay persistence failed: {0}")]
    Persistence(#[source] PersistenceError),
    #[error("replay frames are too large: {actual} bytes exceeds {limit} byte limit")]
    ReplayTooLarge { actual: usize, limit: usize },
    #[error("replay path is invalid: {0}")]
    InvalidPath(String),
}

impl From<PersistenceError> for ReplayError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error)
    }
}

pub fn validate_frame_payload_size(size: usize) -> Result<(), ReplayError> {
    if size > MAX_REPLAY_FRAME_BYTES {
        Err(ReplayError::ReplayTooLarge {
            actual: size,
            limit: MAX_REPLAY_FRAME_BYTES,
        })
    } else {
        Ok(())
    }
}
