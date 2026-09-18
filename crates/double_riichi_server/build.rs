use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FALLBACK_COMMIT: &str = "dev000000000";

fn main() {
    println!("cargo:rerun-if-changed=../../frontend/dist");
    println!("cargo:rerun-if-changed=../../spec/openapi.yaml");
    println!("cargo:rerun-if-changed=migrations");
    watch_git_metadata();
    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rustc-check-cfg=cfg(frontend_dist)");
    println!("cargo:rustc-env=GIT_COMMIT={}", build_commit());

    if std::env::var("PROFILE").as_deref() != Ok("release") {
        return;
    }

    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist");
    if !dist.is_dir() {
        panic!(
            "frontend/dist is required for production server embedding; run `npm ci --prefix frontend && npm run build --prefix frontend` before `cargo build --release`"
        );
    }

    println!("cargo:rustc-cfg=frontend_dist");
}

fn watch_git_metadata() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let git_entry = manifest_dir.join("../..").join(".git");
    let Some(git_dir) = git_dir(&git_entry) else {
        return;
    };
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    if let Ok(head) = fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.trim().strip_prefix("ref: ")
    {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference).display()
        );
    }
}

fn git_dir(entry: &Path) -> Option<PathBuf> {
    if entry.is_dir() {
        return Some(entry.to_owned());
    }
    let pointer = fs::read_to_string(entry).ok()?;
    let path = pointer.trim().strip_prefix("gitdir: ")?;
    let path = Path::new(path);
    Some(if path.is_absolute() {
        path.to_owned()
    } else {
        entry.parent()?.join(path)
    })
}

fn build_commit() -> String {
    std::env::var("GIT_COMMIT")
        .ok()
        .or_else(|| std::env::var("GITHUB_SHA").ok())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short=12", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        })
        .and_then(|value| {
            let value = value.trim();
            (value.len() >= 12
                && value
                    .chars()
                    .take(12)
                    .all(|character| character.is_ascii_hexdigit()))
            .then(|| value[..12].to_ascii_lowercase())
        })
        .unwrap_or_else(|| FALLBACK_COMMIT.to_owned())
}
