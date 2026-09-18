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
    let candidate = root.join(relative);
    let mut current = root.clone();
    let components: Vec<_> = relative.components().collect();
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
                    return Err(ReplayError::InvalidPath(
                        "replay path resolves outside the replay root".into(),
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ReplayError::Io(error)),
        }
    }
    Ok(candidate)
}
