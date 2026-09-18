# Task 16 release/CI lane

## Scope

This lane owns the release staging scripts, release notices and templates,
`justfile`, and GitHub Actions workflows. It does not change frontend, Rust,
Cargo, specification, progress, or the Task 16 report sources.

## Deterministic release contract

`python scripts/release/package.py build` runs `npm ci`, the production
frontend build, `cargo build --release --locked`, and the release-only Starter
Pack generator in that order. Generated Starter media is created in a
temporary directory and is never written to or staged from repository
`character-packs/`. The package command then creates a fixed-metadata ZIP with
POSIX-sorted entries, normalized timestamps and permissions, an archive
`SHA256SUMS`, and an adjacent `.sha256` file.

Each archive contains `driichi`, `driichi-mcp`, the embedded production
frontend, example config and environment files, README, MIT and Apache
licenses, third-party notices, version/release metadata, and generated CC0
Starter Packs. It excludes runtime config, secrets, databases, and replays.
The standalone Starter Pack ZIP is staged beside each platform archive and
has its own checksum.

The supported CI matrix is:

| artifact | runner | Rust target |
| --- | --- | --- |
| `linux-x86_64` | Ubuntu 22.04 | `x86_64-unknown-linux-gnu` |
| `windows-x86_64` | Windows 2022 | `x86_64-pc-windows-msvc` |
| `macos-arm64` | macOS 14 arm64 | `aarch64-apple-darwin` |

Every archive is verified on the build host. `smoke.py --dry-run` verifies
member safety, metadata, notices, and nested SHA-256 manifests and prints the
version/start argv without trying to execute a foreign binary. Native jobs
also have the option to run the full version/start smoke.

## Gates

- `just test-all` runs deterministic Rust, frontend, contract, spec, smoke,
  release-script, and browser gates. It has no live credential dependency.
- `just build-release` delegates to the temporary-staging package command.
- `just release-smoke-dry-run` verifies a selected archive without execution.
- `just test-live` fails unless `RUN_LIVE_TESTS=1`; only the schedule workflow
  supplies that opt-in and the two `RIICHI_DEV_*` secrets.
- GitHub Actions references use full commit SHAs. Release and CI action pins
  are intentionally immutable; toolchain and npm/Cargo lock versions are
  explicit in the workflow and repository.

## Acceptance evidence

The native dependency-free script suite is `python scripts/release/test_release.py`.
It proves deterministic archive bytes, nested checksums, path safety, generated
media rejection, platform executable naming, and scheduled endpoint validation.
The recovery worker must run `python scripts/release/test_release.py` and the
release smoke dry-run against a staged archive before committing. A full
workspace Rust suite is intentionally not run by the recovery worker; CI's
`test-all` job remains the authoritative broad gate.

## CI-only residual risks

- Windows and macOS native builds, ffmpeg availability, and executable startup
  remain CI-host checks; the recovery host cannot certify foreign binaries.
- The scheduled compatibility check requires configured riichi.dev secrets and
  reports external drift without changing local pins.
- v1 publishes no Windows code-signing or Sigstore signature.
