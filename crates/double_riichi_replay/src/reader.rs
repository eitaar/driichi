use std::path::{Path, PathBuf};

use crate::{
    AuxiliaryRecord, CanonicalEvent, ReplayError, ReplayFrame, build_replay_frames,
    build_replay_frames_for_mode, build_replay_frames_with_auxiliary,
    build_replay_frames_with_auxiliary_for_mode, parse_mjson, read_mjson,
};
use double_riichi_core::GameMode;

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

    pub fn frames_for_mode(&self, mode: GameMode) -> Result<Vec<ReplayFrame>, ReplayError> {
        build_replay_frames_for_mode(&self.events, mode)
    }

    pub fn frames_with_auxiliary(
        &self,
        auxiliary_events: &[AuxiliaryRecord],
    ) -> Result<Vec<ReplayFrame>, ReplayError> {
        build_replay_frames_with_auxiliary(&self.events, auxiliary_events)
    }

    pub fn frames_with_auxiliary_for_mode(
        &self,
        auxiliary_events: &[AuxiliaryRecord],
        mode: GameMode,
    ) -> Result<Vec<ReplayFrame>, ReplayError> {
        build_replay_frames_with_auxiliary_for_mode(&self.events, auxiliary_events, mode)
    }
}
