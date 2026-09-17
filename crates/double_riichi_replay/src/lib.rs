//! Canonical MJSON persistence and Admin Replay reconstruction.
//!
//! Canonical events are intentionally kept in this crate's writer/parser path.
//! Public and Player serializers continue to consume only core projections.

mod error;
mod frames;
mod mjson;
mod path;
mod persistence;
mod reader;

pub use error::{
    MAX_DECOMPRESSED_REPLAY_BYTES, MAX_REPLAY_BYTES, MAX_REPLAY_EVENTS, MAX_REPLAY_FRAME_BYTES,
    PersistenceError, ReplayError, ReplayPersistenceError, validate_frame_payload_size,
};
pub use frames::{
    ReplayFrame, build_replay_frames, build_replay_frames_with_auxiliary, encode_replay_frames,
    frames_from_artifact, replay_frame_limit,
};
pub use mjson::{
    CanonicalEvent, MjsonEvent, ReplayEvent, parse_event_line, parse_mjson, serialize_event,
    validate_canonical_events,
};
pub use path::resolve_replay_path;
pub use persistence::{
    AuxiliaryEvent, AuxiliaryPhase, AuxiliaryRecord, FailureInjection, ReplayArtifact,
    ReplayAuxiliaryEvent, ReplayWriter, read_mjson, startup_cleanup,
};
pub use reader::ReplayReader;
