# Task 2 report — PARTIALLY UNBLOCKED

Status: `PARTIALLY_UNBLOCKED` — owner-supplied yamai evidence is green; authenticated immutable riichi.dev evidence remains blocked.

Worker run: `ee912626-695a-41f4-b2ff-978862870ca3`
Workflow: `f62fd2bc-a82c-4c62-88a8-59b7e2ca58b4`
HEAD: `b6cadc15880dee5bda086749c61425d6d0891e11`

## Verified during live probing

- `riichienv-core` 0.4.10, source revision prefix `479c1fa` (full source metadata captured in the worker transcript).
- A deterministic temporary Cargo probe completed all four required application presets: `4p-red-east`, `4p-red-half`, `3p-red-east`, and `3p-red-half`.
- `rmcp` 3.4.0 source and upstream tests were available; upstream session, subscription/event-store, and initialization tests passed.
- Candidate font and tile assets were reachable for further exact revision/license recording.
- riichi.dev documentation, status, and public MJAI logs were reachable and hashable.
- A worktree-local `just` 1.58.0 was installed; no global installation was performed.

## Blocking external evidence

1. No authoritative yamai repository/package, immutable commit, license, or `ReplayProcessor` API could be located through GitHub, crates.io, npm, PyPI, or Sourcegraph searches. Therefore no real ReplayProcessor execution or accepted/rejected MJSON proof can be produced.
2. riichi.dev ranked/validate endpoints require a Bot Token unavailable in this environment. Therefore accepted action, validation-success, queue-boundary, and reconnect transcripts cannot be captured.
3. The live riichi.dev site exposes no immutable Protocol v2 revision suitable for the required pin.

## Local follow-up once external evidence is supplied

- Add `spec/external-contracts.toml` with exact immutable pins and checksums.
- Add the minimal contract harness and authoritative fixtures.
- Add `test-contract` and `test-yamai` justfile recipes.
- Run literal `just test-contract` and `just test-yamai`.
- Commit `test: pin and prove external contracts` and send the diff through task review.

## Preserved evidence

- Worker output/transcript: `C:/Users/eitab/AppData/Local/Temp/pi-subagents-user-eitab/async-subagent-runs/ee912626-695a-41f4-b2ff-978862870ca3`
- Fresh synthesis audit: `dad77d03-df52-4e20-a2ca-fd13df6f2d6d`
- Five upstream research artifacts: `C:/Users/eitab/.pi/agent/sessions/--C--Users-eitab-Documents-js-driichi--/subagent-artifacts/outputs/aac0a61b-ee68-49f8-bd83-5939e543302c/`

No downstream task may begin while these gate requirements remain unresolved.

## Subsequent yamai resolution

The owner supplied <https://github.com/eitaar/yamai> as authoritative. Revision `226cb84d917376d7513fbfdf987cc6a2294767cc`, its exact `ReplayProcessor` hash, Python requirement, and upstream-locked NumPy/riichienv versions are recorded in `spec/external-contracts.toml`.

`python scripts/test_yamai.py` now verifies the source pin, generates a complete four-player Replay, and passes all 1,146 events through the real upstream `ReplayProcessor` (8 rounds, 560 discard samples). See `task-5-yamai-evidence.md`.

The pinned yamai repository has no project-level license metadata, so it is never vendored or redistributed. The remaining Task 2 external blocker is authenticated and immutable riichi.dev Protocol v2 evidence.
