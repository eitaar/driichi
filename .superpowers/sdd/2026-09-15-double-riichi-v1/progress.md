# SDD ledger — plan: docs/superpowers/plans/2026-09-15-double-riichi-v1.md

Workspace branch: `feat/double-riichi-v1`
Merge base: `4dc5d86678d420065bd60d4dab7132f8010182b6`
Spec: `spec/implementation-v1.md` (reachable; binding authority)

## Pre-flight consistency scan

### Task self-consistency

| Task | Code/artifacts vs specified checks | Result |
|---|---|---|
| 1 | Workspace/config/spec/ADR against Cargo and frontend smoke checks | Consistent; build-script missing-dist assertion must not block development checks. |
| 2 | Pins and probes against contract/yamai commands | Consistent; task is an evidence gate and may stop downstream work. |
| 3 | Domain/engine adapter against four-preset deterministic tests | Consistent. |
| 4 | Decision/time/projection against paused-time and serialized visibility tests | Consistent. |
| 5 | MJSON/persistence/frame reconstruction against failure, corruption, limit, and yamai tests | Consistent. |
| 6 | Config/SQLite/auth/token behavior against focused server tests | Consistent. |
| 7 | Character registry/assets/routes against parser and server tests | Consistent. |
| 8 | Room Actor lifecycle against phase, failure, cleanup, and shutdown tests | Consistent. |
| 9 | HTTP/Human WebSocket against API/protocol integration tests | Consistent. |
| 10 | Entry shell/tokens/GSAP/Pixi vignette against UI, accessibility, visual, and performance checks | Consistent. |
| 11 | Admin/Lobby controls against UI mutation and polling tests | Consistent. |
| 12 | Pixi gameplay/actions/audio/reconnect against visual and protocol integration tests | Consistent. |
| 13 | Strict MJAI compatibility against captured transcripts and production-Bot smoke test | Consistent; depends on Task 2 evidence. |
| 14 | MCP resources/tools/bridge against ownership/revision/full-Match tests | Consistent; depends on Task 2 rmcp evidence. |
| 15 | Replay Admin UI against API/playback/corruption/delete tests | Consistent. |
| 16 | Observability/specs/E2E/release against aggregate and archive smoke tests | Consistent; Design Freeze remains conditional. |

### Cross-task shared files and interfaces

| Tasks | Producer → consumer | Finding / ruling |
|---|---|---|
| 1 → 2 | Workspace commands/manifests → external probes | Compatible; Task 1 must leave dependency pins amendable by Task 2 evidence. |
| 1 → 6 | Config examples/workspace → runtime config/storage | Compatible; Task 1 scaffolds only, Task 6 owns behavior. |
| 1 → 10 | Frontend shell/build order → production frontend | Compatible; Task 1 smoke UI must remain disposable. |
| 1 → 16 | justfile/build metadata → aggregate/release commands | Compatible; Task 16 completes, rather than replaces, bootstrap commands. |
| 2 → 3 | riichienv pin/action/event evidence → engine adapter | Compatible; evidence is binding. |
| 2 → 5 | yamai pin/execution → Replay output | Compatible. |
| 2 → 13 | riichi.dev transcripts → MJAI adapter | Compatible; no implementation before evidence. |
| 2 → 14 | rmcp probe → MCP transport/session behavior | Compatible. |
| 2 → 16 | all external pins/licenses → release gate | Compatible. |
| 3 → 4 | neutral actions/events/state → Decision and projection | Compatible; Task 4 may extend neutral domain types without leaking engine types. |
| 3 → 5 | Match events/state → MJSON and frames | Compatible. |
| 3 → 8 | MatchMachine → RoomActor | Compatible. |
| 3 → 13 | MatchMachine/actions/events → CompatMatchActor/MJAI | Compatible. |
| 4 → 8 | Decision/time/projection → Room lifecycle | Compatible. |
| 4 → 9 | projected snapshots/updates and action IDs → Human protocol | Compatible. |
| 4 → 12 | projected state/action IDs → Pixi gameplay client | Compatible. |
| 4 → 13 | shared Decision boundary → MJAI action mapping | Compatible. |
| 4 → 14 | revisions/deadlines/action IDs → MCP resources/tools | Compatible. |
| 4 → 16 | visibility invariants → release serialization gate | Compatible. |
| 5 → 6 | Replay metadata/path persistence → SQLite schema/config root | Compatible; portable paths stay config-root relative. |
| 5 → 8 | buffered writer failure semantics → Room Actor | Compatible; storage failure cannot terminate gameplay. |
| 5 → 15 | ReplayFrames/metadata/delete → Replay Admin UI | Compatible. |
| 5 → 16 | yamai/replay verification → release gate | Compatible. |
| 6 → 7 | config-root and HTTP auth/storage → Character registry/routes | Compatible. |
| 6 → 8 | Participant/token authority/persistence → Room lifecycle | Compatible. |
| 6 → 9 | Admin sessions/Human guest cookie/token cache → APIs | Compatible. |
| 6 → 13 | Bot Tokens/revocation → MJAI endpoints | Compatible. |
| 6 → 14 | Bot Tokens/revocation → MCP sessions | Compatible. |
| 6 → 15 | Admin auth/Replay metadata → Replay UI/API | Compatible. |
| 7 → 8 | Character registry/selection → Room roster/start commit | Compatible; complete selected roster preloads before Match. |
| 7 → 9 | Character listing/assets → Human/Admin APIs | Compatible. |
| 7 → 10 | fonts/tile/portrait assets → frontend shell | Compatible; Task 10 uses verified, vendored assets. |
| 7 → 11 | Character choices → Lobby UI | Compatible. |
| 7 → 12 | selected roster/voice/effects → gameplay preload | Compatible. |
| 7 → 13 | ranked/missing Character behavior → compat Match | Compatible. |
| 7 → 15 | missing/ranked Character display → Replay UI | Compatible. |
| 8 → 9 | Room commands/events/snapshots → HTTP/WebSocket | Compatible. |
| 8 → 11 | Room phases/roster/Ready/control → Lobby UI | Compatible. |
| 8 → 12 | live projected state/reconnect → Pixi table | Compatible. |
| 8 → 14 | Participant ownership/revisions → MCP server | Compatible. |
| 8 → 16 | complete 3p/4p flow/shutdown → E2E/release | Compatible. |
| 9 → 10 | same-origin/static serving/security headers → entry frontend | Compatible. |
| 9 → 11 | Admin/Public/Human APIs → Lobby UI | Compatible. |
| 9 → 12 | Human WebSocket contract → gameplay client | Compatible. |
| 9 → 15 | Replay Admin APIs → Replay UI | Compatible. |
| 9 → 16 | health/security/HTTP behavior → OpenAPI/E2E/release | Compatible. |
| 10 → 11 | design tokens/shell/router → Admin/Lobby UI | Compatible. |
| 10 → 12 | Pixi renderer/assets/tokens → gameplay table | Compatible; GSAP remains entry-only. |
| 10 → 15 | renderer/tokens → Replay UI | Compatible; Replay reuses live renderer. |
| 10 → 16 | accessibility/performance baseline → release E2E | Compatible. |
| 11 → 12 | selected roster/Room navigation → gameplay screen | Compatible. |
| 11 → 16 | Admin/Lobby flows → Playwright completion | Compatible. |
| 12 → 15 | authoritative renderer → Replay playback | Compatible; animation driver changes, state renderer does not. |
| 12 → 16 | complete Human gameplay → 3p/4p Playwright tests | Compatible. |
| 13 → 16 | compat endpoints/live Bot smoke → release contract gate | Compatible. |
| 14 → 16 | MCP Match smoke/bridge → release gate | Compatible. |
| 15 → 16 | Replay UI/API behavior → release E2E | Compatible. |

Ruling: Task 1's build-script missing-dist failure applies to production server embedding only; development `cargo check --workspace` must remain runnable before a frontend production build — otherwise Task 1's own verification is impossible — cost if wrong: production embedding behavior may need adjustment in Task 16.

## Progress

Task 1: Ruling: exact versions were absent from the brief/spec, so the implementer may select current stable environment-compatible exact pins; Rust 1.95.0, Node 24.11.0, and npm 11.4.0 are the toolchain baseline, dependencies stay minimal, and Task 2 owns external-contract pins — cost if wrong: Task 2 may need a reproducibility-only pin adjustment.
Task 1: infrastructure interruption after commit 4c3fffe — workflow ef1fd033-2d56-4469-bbfa-7e545f6b49ca stopped on extension reload before the review gate; worktree verified clean and review package/report verified present before same-protocol retry.
Task 1: fix round 1/5 (1 addressed, 0 open — align `.env.example` with required Admin keys; commits 4c3fffe..b6cadc1).
Task 1: complete (commits 4dc5d86..b6cadc1, review clean).
Task 2: infrastructure interruption before mutation — workflow aac0a61b-ee68-49f8-bd83-5939e543302c preserved five research artifacts but rejected the read-only evidence-auditor launch as an implementation task; worktree verified clean at b6cadc1 before owner-approved same-protocol retry.
Task 2: infrastructure retry 18bae33a-eb5a-4bf0-b365-e210fc3db6a6 hit the identical auditor-role classifier defect before creating a child session; worktree remained clean at b6cadc1.
Task 2: Ruling: owner approved using a fresh mutation-capable Luna worker under an explicit read-only audit contract, then a separate sole writer — this bypasses the broken evidence-auditor classifier while preserving independent contexts; cost if wrong: tool capability, rather than role enforcement, is relied on to keep the audit lane read-only.
Task 2: infrastructure interruption — workflow 732d27b2-bf8c-4147-8ccd-596aed054a69 audit runner disappeared after 132k tokens while probing riichienv; transcript/session preserved and worktree verified clean at b6cadc1 before exact-session revival.
Task 2: blocked at HEAD b6cadc1 — live probes verified riichienv-core 0.4.10/rev 479c1fa across all four presets and rmcp 3.4.0 source/tests, but no authoritative yamai/ReplayProcessor source was discoverable and accepted riichi.dev ranked/validate transcripts require an unavailable Bot Token; no external-contract files or commit were created.
Task 2: Ruling: owner deferred the missing yamai and authenticated riichi.dev evidence until later and authorized provisional downstream work — Task 2 remains incomplete, Conditional Design Freeze remains in force, strict MJAI/yamai/release acceptance cannot be claimed, and dependent tasks must carry these parked gates; cost if wrong: protocol or Replay work may require rework when authoritative contracts arrive.
Task 3: fix round 1/5 (3 addressed, 0 open — reject malformed/mode-invalid payloads, confine engine mode IDs, and exercise divergence through MatchMachine; commits 8105727..0432779).
Task 3: fix round 2/5 (prior fixes reverified; report evidence commit 0432779..d57082d).
Task 3: fix round 3/5 (clean structured verdict after fresh verification; commit d57082d..b678a3e).
Task 3: complete (commits b6cadc1..b678a3e, review clean; parent verification: fmt/check/workspace tests/diff check passed, 8 core tests).
Task 4: fix round 1/5 (4 addressed, 1 new open — concealed-meld leak, mixed Unlimited timing, reconnect preservation, edge tests fixed; partial timeout marker found; commits 5556984..371ea52).
Task 4: fix round 2/5 (1 addressed, 1 new open — partial timeout persistence fixed; staged timeout overwrite found; commits 371ea52..e4de776).
Task 4: fix round 3/5 (1 addressed, 0 genuinely open — staged timeout markers fixed; commits e4de776..12c43fa).
Task 4: fix round 4/5 (review/report evidence only; commits 12c43fa..74fe9df).
Task 4: fix round 5/5 (review/report evidence only; commits 74fe9df..426bdae).
Task 4: Ruling: the five-round breaker was triggered because scoped reviewers returned previously fixed Critical/Important items inside `findings` with `[ADDRESSED]`; the final verdict is spec PASS, quality APPROVED, every listed item is explicitly addressed, source inspection found no new breakage, and fresh parent fmt/check/workspace tests/diff check passed — treat zero findings as open; cost if wrong: an addressed timeout/projection edge could still require a later regression fix.
Task 4: complete (commits b678a3e..426bdae, 0 parked; breaker adjudicated clean; parent verification passed, 28 workspace tests).
Task 5: fix round 1/5 (local findings addressed except new four-player Kita validation gap; yamai remained deferred; commits a3ea496..12d2a8a).
Task 5: fix round 2/5 (four-player Kita rejection addressed; no new local breakage; commits 12d2a8a..f732db4).
Task 5: fix round 3/5 (local implementation reverified; report-only commit f732db4..56f0440).
Task 5: fix round 4/5 (local implementation reverified; report-only commit 56f0440..ba95982).
Task 5: fix round 5/5 (local implementation reverified; report-only commit ba95982..21d6552).
Task 5: parked — pinned yamai ReplayProcessor acceptance and yamai tests remain unavailable — Ruling: owner explicitly deferred this external gate; local canonical MJSON/persistence/ReplayFrame work may proceed provisionally, but Task 5 is not spec-complete and no yamai compatibility is claimed — cost if wrong: authoritative yamai may require format changes.
Task 5: provisional local complete (commits 426bdae..21d6552, 1 parked; parent fmt/check/replay/workspace/frontend verification passed, 15 replay tests).
Task 6: infrastructure interruption — workflow 5162a11c-fec5-459e-999d-2ab7db39fc2b runner disappeared after 335 minutes and its status JSON was corrupted; captured partial state is one untracked RED test (`crates/double_riichi_server/tests/task6_config_auth_storage.rs`), no production changes or commits, HEAD 21d6552; exact-session recovery was unsafe, so owner-authorized continuous execution used a fresh same-role fallback writer.
Task 6: fix round 1/5 (5 Important security/storage findings and 1 Minor test gap addressed — exact Match sources, recursive audit allowlisting, request IDs, relative config root, trimmed token names, real revocation signal; commits 6149eb0..e1ff3f2).
Task 6: complete (commits 21d6552..e1ff3f2, review clean; parent fmt/check/focused/workspace/frontend verification passed, 53 workspace tests).
Task 7: infrastructure interruption — workflow a71b418e-f881-48c5-a18e-94e7ea8e3628 runner disappeared after 87 minutes/313k tokens without a commit; captured dirty patch includes registry/config/routes/tests/generator/license changes. Fresh compile reproduces RED failures from unstable Windows metadata APIs and an Ogg duration integer mismatch. Exact-session recovery is unsafe; fresh same-role fallback owns the preserved patch at HEAD e1ff3f2.
Task 7: Ruling: Starter binary assets remain release-generated and uncommitted per spec §17.1; commit generator plus complete CC0/license/source inputs and deterministic staging/zip checks that prove release archive layout — cost if wrong: release packaging may need additional integration in Task 16, but source control stays free of generated binaries.
Task 7: Ruling: follow spec §17.3 and validate bounded size plus allowlisted headers only; do not decode image dimensions/audio duration or retain an undeclared 10-second cap — cost if wrong: malformed-but-header-valid media may fail later client decoding rather than startup validation.
Task 7: fix round 1/5 (Windows file-identity duplicate defense and standalone ZIP CC0 coverage addressed; commits edae1ab..38bf345).
Task 7: complete (commits e1ff3f2..38bf345, review clean; parent fmt/check/focused/workspace/frontend/generator ZIP verification passed, 64 workspace tests).
Task 8: infrastructure interruption — workflow 31ccac0d-0b1e-4373-a502-3917556db25d child 29a297b0-1ece-49af-a385-9c0019f7cbc7 runner process 20984 disappeared after 71 minutes/383k tokens without a result or commit. Preserved dirty patch at HEAD 38bf345: modified core Cargo.toml/lib.rs plus untracked room.rs and task8_room.rs. Fresh same-role fallback owns and must audit the patch; exact-session recovery is unsafe.
Task 8: fix round 1/5 addressed Fill capacity, selected expiry, Human Rematch, reconnect controller sync, persistence acknowledgements, abort backpressure, registry cleanup/revocation, and lifecycle coverage (0ca7d09..2f04c39).
Task 8: fix round 2/5 made persistence acknowledgement delivery reliable under full command-channel backpressure (7236535).
Task 8: complete (commits 38bf345..7236535, review clean; parent fmt/check/focused 15 tests/full 79 workspace tests/frontend verification passed; external gates remain provisional).
Task 9: fix rounds 1–5 closed Human leave/kick authorization leaks, semantic close draining, graceful shutdown, bounded rate/session/lock state, atomic Admin mutation/deletion, explicit wire normalization, trusted proxy/Origin handling, connection-generation races, and required live WS coverage (84ec5ae..ad7817f).
Task 9: breaker adjudication: loop exhausted immediately after final reviewer returned PASS / APPROVED with no findings; accepted as review-clean rather than an open breaker.
Task 9: complete (commits 7236535..ad7817f, final review clean; parent fmt/check, 9 focused HTTP/live WS tests, server/core/full workspace, frontend typecheck/build passed; external gates remain provisional).
Task 10: fix round 1/5 addressed exact npm pins, synchronous/dynamic reduced motion, Character retry, native radio semantics, 1024x600 clipping, vignette labeling, Pixi failure cleanup, and unused dependency removal (50e2127..2ee2682).
Task 10: complete (commits ad7817f..2ee2682, review clean; parent inspected exact 1024x600 and 1440x900 screenshots, Playwright/build/Cargo fmt/check/full workspace passed). Parent Vitest 5.0.1 process reproducibly exited 139 before discovery even for a one-line Node smoke test under this Pi environment; implementation/review runs reported 8/8 passing, so this is recorded as an environment/toolchain caveat rather than a code regression. Tile asset pin remains provisional pending Task 2.
Task 11: Ruling: Task 9 omitted required Admin Bot Token HTTP routes/ServerState wiring. Implement the minimal spec-defined create/list/revoke backend contract in Task 11, reusing Task 6 service and Task 8 revocation signal, rather than shipping a frontend against nonexistent paths — cost if wrong: Task 11 gains a narrow backend completion, but preserves functional API truth and one-time-secret guarantees.
Task 11: initial implementation committed 5516c35; first reviewer launch failed before analysis due provider usage limit. Fresh retry completed with one Critical and eight Important open findings: durable revocation signaling, healthy-socket command errors, WS generation ownership, reconnect preload invalidation, actual image decoding, Seat rendering/3p test mismatch, Rematch UI, modal focus management, and complete executable mutation evidence/screenshots.
Task 11: fix workflow 872d935d-152c-4ef4-b3ad-8616f05b73c3 failed when async runner process 21248 disappeared after 81 minutes; no child session/result persisted. Captured preserved uncommitted patch at HEAD 5516c35: http.rs, task9_http.rs, app.tsx, and app.test.tsx (275 additions/22 deletions). Exact resume is unavailable; fresh same-role fallback must audit and complete this patch.
Task 11: fix round 1/5 addressed durable/retryable revocation signaling, command-error ownership, WS generations, reconnect preload, actual image decoding, Seat axis, Rematch, modal focus, and stale reasons (ec4cd34).
Task 11: fix round 2/5 added durable 21/21 Vitest and 9/9 Playwright evidence plus Admin/Lobby screenshots at both required viewports (9ad0daa); 9c77ff1 changes only the report's evidence commit reference and was parent-inspected after reviewer approved 9ad0daa.
Task 11: complete (commits 2ee2682..9c77ff1, review PASS / APPROVED; parent npm ci/typecheck/build, parsed Vitest 21/21, recursively parsed Playwright 9/9, fmt/check, focused server 10/10, full workspace, screenshot inspection and clean-worktree checks passed).
Task 12: initial worker hit its hard tool limit after implementation/tests and returned uncommitted changes at HEAD 9c77ff1; requested report/commit/Cargo/full verification were not produced. Reviewer then failed infrastructure protocol (`Missing structured_output call`) after inspecting the dirty tree. Captured dirty patch: package manifests, app/styles plus new frontend/src/game and task12.spec.ts; git diff check also reports a blank EOF line in styles.css. Parent screenshot inspection found the Pixi table and portrait effect visibly blank in all supplied captures, so passing browser counts do not establish playable visual output.
Task 12: first fallback process 25768 disappeared after 151 tool calls when it broadly ran `taskkill /IM node.exe` to recover an npm EPERM lock; partial nonblank but severely oversized/overlapping visual patch was preserved. Narrow finisher was explicitly prohibited from process killing/reinstall, repaired bounded sprites/fixtures, passed claimed frontend/Cargo checks, wrote the report, and committed ec50bf4. Review required an inherited resume after another missing-structured-output protocol failure. Fresh verdict: FAIL / CHANGES_REQUIRED with seven Important findings (production projection lacks center/wall fields used only by synthetic fixture; no 3s character/Pixi asset timeout; semantic close traps user on stale table; synchronous audio.play failure stalls queue; no persistent Riichi legal discard highlight; animation items never consumed and kinds collapse to one pulse; no real WebSocket/authoritative projection integration evidence) and two Minor findings (no reconnect jitter; durable Vitest count says 27 while report says 28).
Task 12: first review-fix worker process disappeared after 89 tools while writing projection/assets/audio/animation fixes. Parent captured a coherent dirty ten-file patch with clean diff-check. Same-session resume then failed infrastructure startup (`Timed out after 10000ms waiting for async runner startup state ready`; no child session persisted). Owner explicitly chose a fresh worker retry over direct parent completion or pausing.
Task 12: complete at 680394c (implementation ec50bf4; review fixes 5a874db; illegal-action Decision preservation d290256; portrait evidence 680394c). Final independent review PASS / APPROVED with zero findings. Fresh parent verification passed frontend typecheck, 32/32 Vitest, production build, Task 12 Playwright 8/8, exact screenshot inspection including centered Mangan portrait plus standings, cargo fmt --check, cargo check --workspace, all workspace unit/integration/doc tests, diff check, and clean worktree. Provider usage-limit failure occurred after the portrait-evidence worker had already committed; resumed reviewer verified the generated artifact after the parent rerun. External-contract/yamai/riichi.dev claims remain provisional as previously deferred.
Task 13: parent captured a provisional first-party public-doc evidence report after the built-in researcher failed because its declared fetch/source-check extension tools were unavailable in the child runtime. First implementation workflow was then stopped by extension session replacement/reload during a read-heavy phase-1 worker (85 tools, no edits); branch remained clean at 680394c. Retry uses a scout handoff plus smaller durable implementation commits.
Task 13: rereview fix round 1/3 addressed all five Important lifecycle/persistence findings: Room setup teardown now disconnects/removes generations, replacement input is serialized against generation registration, Room joins serialize with token revocation, replay failures latch health degraded, and forced shutdown awaits tracked metadata cleanup. Added replay-health and forced-cleanup regression coverage; focused/server tests pass. External yamai, authenticated upstream-token, and immutable upstream-pin blockers remain unchanged.
Task 13: rereview fix round 2/3 addressed the residual Room setup teardown backpressure path: bounded retries now deliver `RoomCommand::disconnect` after transient `RoomError::Busy` responses before generation removal; added a full-command-queue regression test. Focused compat, Task 13 integration, fmt/check, and full workspace tests pass; blockers remain unchanged.
Task 13: provisionally complete at 3517a0d (adapter 6288880; endpoint/replay/lifecycle/evidence commits through 3517a0d). Final independent review PASS / APPROVED with zero findings. Fresh parent verification passed fmt, Task 13 live WebSocket 8/8, Compat unit 12/12, MJAI 13/13, Replay 15/15, cargo check, every workspace unit/integration/doc test, diff check, clean worktree, and LSP diagnostics on 14 changed Rust paths. Clippy remains blocked only by the five documented pre-existing core lints. Strict riichi.dev parity, immutable Protocol v2 pin, authenticated upstream transcripts, yamai acceptance, Conditional Design Freeze, and release acceptance remain unresolved external gates.
Task 14: review refresh at e1b9883 — added join watcher barrier, deterministic terminal wait/notification, unjoined transport cap+1, repeated subscribe/unsubscribe recovery, and bounded current-Kyoku/prior-summary Post-Match privacy assertions; reran the complete stdio bridge Match and existing TemporaryAuto follow-up tests. Focused MCP checks pass; the workspace command was run but two unrelated Task 13 live end-game observations failed in this environment. Strict server/workspace Clippy remains blocked by documented pre-existing core/replay/server baseline lints. Commit: `test(mcp): close Task 14 review findings`; review package regenerated as `review-e2b1b79..HEAD.diff`.
Task 14: rereview fix round 1/4 — closed the RevisionWake registration race, ignored post-initial generic snapshots, serialized revocation with joins plus tombstones, and leased active waits across idle reaping; added 3 focused regressions. Focused MCP tests (16 unit, 6 live bridge) passed; cargo fmt/check and diff check passed. Initial workspace verification exposed two Task 13 live end-game failures; systematic tracing proved they were a real expired-zero-duration Compat control-flow regression rather than environment noise.
Task 14: provisionally complete through 0fc7ff3, with cross-task regression fixes 670bf3f and bd99db1. Final independent review PASS / APPROVED with zero findings. Fresh parent verification passed Task 13 live 8/8, Task 14 live/complete stdio bridge Match 6/6, Compat unit 12/12, MCP unit 16/16, MCP crate/CLI tests, Task 9 HTTP 10/10, cargo fmt/check, every workspace unit/integration/doc test, and diff check. Strict upstream/yamai/release gates and pre-existing workspace Clippy baseline remain unresolved as documented.
Task 15: recovered the runner-corrupted storage implementation, completed authenticated Admin Replay list/view/delete API and Replay Viewer, and added corruption/size/health, auxiliary persistence, playback, and browser-flow coverage. Focused Task 15 tests (2/2), workspace tests, frontend typecheck/Vitest (36/36)/build, Playwright (2/2 at 1024x600 and 1440x900), cargo fmt/check, and diff check pass. Focused server Clippy remains blocked by the documented five pre-existing core lints; external yamai/riichi.dev/Conditional Design Freeze/release gates remain deferred. Commit: `feat(replay): add admin replay viewer`.
Task 15: review-fix finisher — two runner attempts crashed with EPERM; preserved dirty storage/frontend patch was audited and completed in place. Production replay URL pagination/back-forward, later-page delete clamp, list Retry, modifier-safe links, skip target, localized time/loading semantics, actual Character asset availability, and Room audio/portrait policy were reverified; browser coverage now exercises pagination/history, later-page deletion, Retry, Previous/Next/Jump pause/destinations, and generic/Room presentation. Replay-route auth/unsafe-origin, path/symlink containment, oversize/corrupt handling, auxiliary ordering, allowlisted audit output, and file-first database-failure retry evidence were completed. Focused Task 15 integration (4/4), Replay Vitest (9/9), frontend typecheck/build, focused Playwright at 1024x600 and 1440x900, fmt, and diff checks pass; parent reruns workspace gates. Commit: `feat(replay): close Task 15 review findings`.
Task 15: backend remediation complete — list/view now share bounded frame, auxiliary, player, and full-response validation; completed rows are validated during fresh Storage startup; global `.incomplete` cleanup runs alongside DB writing-row cleanup; ranked post-finalize cleanup preserves retryable writing metadata on unlink failure; Admin Replay View uses route-scoped gzip; and Admin mutations use serialized durable pending-audit completion with no-op suppression and exact room-delete outcomes. Focused Task 15 server integration 15/15, Replay 16/16, server lib 36/36, core lib 11/11, workspace tests, workspace check, fmt, and diff checks pass. Focused server Clippy remains blocked only by the five documented pre-existing core lints. Internal FailureInjection/ServerState::for_tests/Storage::pool surfaces remain explicitly non-blocking notes, not network-reachable production capabilities or security claims.
Task 15: provisionally complete at e8ae64e after Replay semantic/mode/path fixes and durable Admin audit cancellation/recovery. Fresh independent reviews returned Replay OK and Admin audit OK with notes, with no P0/P1 findings. Parent verification passed Replay 18/18, Task 6 11/11, Task 9 10/10, Task 15 22/22, the serial full workspace suite, frontend typecheck/41 Vitest/build, Task 15 Playwright 4/4 in isolation at both required viewports, clean LSP diagnostics on 13 changed Rust source files, screenshot inspection, fmt/check/diff checks, and a clean worktree. The remaining same-request-ID rolled-back-tombstone retry note is P2 and normal HTTP retries receive a fresh server request ID. External yamai, authenticated riichi.dev, Conditional Design Freeze, strict compatibility, and release acceptance remain blocked and unclaimed.
Task 16 slice 1: provisionally implemented local hand-authored OpenAPI/AsyncAPI contracts, pinned fixture/router validation, Admin-gated raw OpenAPI serving, Health/status/shutdown evidence, secret-safe configurable tracing, and compile-time build metadata. External yamai and authenticated immutable riichi.dev evidence remain unavailable; this slice does not claim those gates, release acceptance, or a status transition. Cost if wrong: authoritative upstream protocol or release evidence may require contract, adapter, or packaging rework before Task 16 can complete.
Task 16: fix round 1/5 — closed cookie-path/docs scope, DTO/event contract omissions, full official schema/reference and route/fixture validation, linked-worktree metadata watches, storage-backed Health/shutdown evidence, and exact Python dependency provisioning. Strict validator RED caught missing dependencies and invalid AsyncAPI/OpenAPI shapes before GREEN; external yamai/riichi.dev/release/frontend gates remain parked. Cost if wrong: upstream contract changes or a platform-specific Git/build behavior may still require follow-up before Task 16 acceptance.
Task 16: E2E/persistence lane complete through 3d8d37c — actual Rust-server 3p/4p Human Match completion, axe/screenshots, action correlation, per-Room acknowledged Replay persistence, isolated backpressure, authoritative pool-starved finalization, safe cleanup, and explicit Replay availability/health contracts passed final independent review.
Task 16: Release/CI lane complete through 0c51095 and integrated by 4a4846f — exact three-platform deterministic archives, versioned Starter ZIP/checksums/notices, embedded production frontend, native/dry-run smoke, immutable actions, and scheduled credential-gated live checks passed scoped review.
Task 16: final integrated review fixes 38c43c4 and 5a33092 — tagged artifacts now validate tag/version and publish to GitHub Releases; finished Room effect workers are reaped; shutdown closes Storage; request-log identifiers and OpenAPI are bounded; Human AsyncAPI events/close codes are exact; Windows CRLF and loaded-suite cleanup polling are stable. Final independent residual review returned Spec PASS / Quality APPROVED / Merge OK with zero P0/P1/P2 findings.
Task 16: provisionally complete locally — fresh parent contract/release tests, fmt/check, serial workspace tests, frontend typecheck/41 Vitest/build, real-server 3p/4p E2E, browser 23/23, production embedding, Windows archive/native smoke, screenshots, and diff/clean checks passed. Conditional Design Freeze, authenticated immutable riichi.dev parity, live credentials, foreign-platform workflow success, strict compatibility, and final release acceptance remain parked and unclaimed.
External evidence: owner-supplied `eitaar/yamai` revision 226cb84d917376d7513fbfdf987cc6a2294767cc is pinned with an exact ReplayProcessor hash. The executable gate generated a complete 4p East Match and yamai accepted all 1,146 events (8 rounds, 560 discards) from both fresh fetch and cache. The repository has no project-level license, so its source is neither vendored nor redistributed.
