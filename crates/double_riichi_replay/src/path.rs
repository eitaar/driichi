use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use crate::ReplayError;

/// Resolve a registered replay path while keeping it below the configured root.
pub fn resolve_replay_path(
    root: impl AsRef<Path>,
    relative: impl AsRef<Path>,
) -> Result<PathBuf, ReplayError> {
    let root = root.as_ref();
    let relative = relative.as_ref();
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ReplayError::InvalidPath(
            "replay path must be relative and cannot escape the root".into(),
        ));
    }
    let root = fs::canonicalize(root)?;
    let candidate = fs::canonicalize(root.join(relative))?;
    if !candidate.starts_with(&root) {
        return Err(ReplayError::InvalidPath(
            "replay path resolves outside the replay root".into(),
        ));
    }
    Ok(candidate)
}
