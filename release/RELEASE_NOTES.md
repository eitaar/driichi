# Double Riichi release notes template

Copy this template into the release entry and replace every bracketed value.
Do not publish a release until the archive, versioned
`character-packs-[Starter Pack version].zip`, and all adjacent SHA-256 files
have been verified by the release workflow.

- Version: `[semver]`
- Source commit: `[short git SHA]`
- Targets: Linux x86_64, Windows x86_64, Linux ARM64
- Starter Pack version: `[character-packs/STARTER_VERSION]`
- Cargo.lock: `[sha256]`
- frontend/package-lock.json: `[sha256]`
- Release archive checksums: `[workflow artifacts]`

## Verification

For each target, run `driichi --version`, then run
`scripts/release/smoke.py <archive> --platform <target> --version <semver>
--commit <sha>`. The smoke command verifies archive paths, notices, metadata,
and SHA-256 manifests before its version/start check. Cross-platform archives
may use `--dry-run` on a host that cannot execute the target binary.

## Known release limitations

v1 does not provide Windows code signing or Sigstore signatures. The scheduled
riichi.dev compatibility check is deliberately separate from deterministic
local tests and does not redefine pinned contracts.
