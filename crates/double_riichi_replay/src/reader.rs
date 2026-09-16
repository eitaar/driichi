use std::path::{Path, PathBuf};

use crate::{
    AuxiliaryRecord, CanonicalEvent, ReplayError, ReplayFrame, build_replay_frames,
    build_replay_frames_with_auxiliary, parse_mjson, read_mjson,
};

/// Parsed replay input used by the Admin-only frame builder.
#[derive(Debug, Clone)]
pub struct ReplayReader {
    path: Option<PathBuf>,
    events: Vec<CanonicalEvent>,
}

impl ReplayReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReplayError> {
        let path = path.as_ref().to_path_buf();
        let events = read_mjson(&path)?;
        Ok(Self {
            path: Some(path),
            events,
        })
    }

    pub fn from_str(input: impl AsRef<str>) -> Result<Self, ReplayError> {
        Ok(Self {
            path: None,
            events: parse_mjson(input)?,
        })
    }

    pub fn events(&self) -> &[CanonicalEvent] {
        &self.events
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn frames(&self) -> Result<Vec<ReplayFrame>, ReplayError> {
        build_replay_frames(&self.events)
    }

    pub fn frames_with_auxiliary(
        &self,
        auxiliary_events: &[AuxiliaryRecord],
    ) -> Result<Vec<ReplayFrame>, ReplayError> {
        build_replay_frames_with_auxiliary(&self.events, auxiliary_events)
    }
}
