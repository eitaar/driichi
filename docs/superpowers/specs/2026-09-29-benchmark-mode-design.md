# Benchmark mode design

Date: 2026-09-29
Status: Design approved in conversation; implementation pending written-spec review

## Goal and boundaries

Compare the playing strength of a fixed group of software Participants over a bounded series of Matches. A Benchmark Room is distinct from a normal Room but shares its Match engine, Room participant lifecycle, replay storage, and bot transports. It admits selected MJAI, MCP, and explicitly selected Built-in Bot Participants, never Humans or ordinary Spectators. Do not fill vacant seats automatically. Three-player Matches continue to reject MJAI.

A Benchmark Run is one fixed roster and 1–1,000 *successfully completed and persisted* Matches. Results and statistics belong to Participant IDs within that Run, not Bot Tokens or names. Past Runs are not combined. The administrator can observe the live table with every current hand; no other client can gain this visibility. Reproducible seed input, cross-Run rankings, and unattended recovery are out of scope.

## Selected approach

Extend the existing Room match orchestration with a Benchmark-only Run lifecycle. A new orchestrator or a parallel game engine would either race the Room's completion and replay acknowledgements or duplicate decisions, disconnections, and serialization. Keep the benchmark state and transition rules beside the existing Room lifecycle, but persist Run identity, roster, sequence, results, and stop reasons in SQLite. Match and Run persistence must agree: do not advance the completed counter or launch the next Match until the just-finished result has been durably recorded. Normal Room and compat Match behavior stays unchanged.

A dedicated Admin UI creates and manages Benchmark Rooms/Runs. The Admin explicitly selects exactly the game's seat count of bot Participants and chooses the positive target Match count. Built-in Bots may be selected but never inserted as a side effect of start. Validate at the authoritative Room boundary as well as at HTTP entry: no Humans, no ordinary Spectators, no late join/selection changes after start, no MJAI in three-player mode, complete fixed roster, and target within 1–1,000. Token authentication continues to identify external bots; a shared token does not merge Participant identities. An Admin observer has no Participant/Seat and cannot submit game actions.

## Run progression

Create a Run with the selected immutable Participant IDs and their initial ordered seating. Match 1 uses that order; for Match k, rotate the roster one seat from the previous Match, so every participant sees every seat once per full cycle. Do not reuse the current Rematch implementation's unchanged seat order. Each Match otherwise has normal independent random dealing and normal legal-action/time-control semantics.

After a Match ends, persist its replay and final results and associate its Match ID and sequence number with the Run. Only after successful persistence increment the Run's completed count. If below target and no stop was requested, start the next Match immediately using the next rotated seating; if at target, mark the Run completed. Run writes must be idempotent on Match ID/sequence to prevent duplicate results when a completion signal is retried. A Match that finished on the board but whose result cannot be saved is not counted; record the failure reason when storage is available and never launch another Match from that Run.

An Admin stop request during a Match sets stop-after-current-match; on successful completion and persistence mark the Run stopped instead of starting another Match. If the Run has not started a Match yet, stop immediately. Disconnect/revocation/loss of an external bot during a running Match, any Match abort, or result/replay persistence failure stops the Run instead: do not include auto-play output in that Participant's benchmark results and do not auto-resume when it reconnects. Record an incomplete attempt and its cause when persistence is available; incomplete attempts never count toward the target or statistics. Keep the previously completed results. No replacement players and no retry within the same Run.

After a process restart, previously active Runs become interrupted and read-only, with their committed results retained; neither a Room nor a Run is auto-recreated to continue play. Deleting/expiring a Room must not delete Run history or completed replays referenced by the Run. If a database fault prevents saving a failure reason, preserve the underlying persistence error in server logs without exposing credentials; do not claim that the unsaved attempt was counted.

## Visibility and Admin experience

Only an authenticated Admin on a Benchmark Room's private live-observer endpoint receives an omniscient *current* projection: all Players' concealed hands and the current publicly observable decision/board state. Reuse the central projection's all-hands visibility logic, but do not make `ReplayAdmin` a general live-public audience or expose other Players' private action options. Never expose unrevealed wall order. On connect/reconnect send an authoritative current snapshot, then bounded live updates; observing and reconnecting have no effect on Run progression. Protect the endpoint at connection and on subsequent streaming authorization expiry/revocation as appropriate. Normal Rooms' Admin replay access remains post-Match; public lookups, bot projections/resources, and ordinary spectator paths must never deliver live all-hands data.

The Admin UI shows Run progress, stop/interruption reason, the current live table, per-Match results, and per-Participant statistics computed from persisted successful results only: finishing place and final score per Match, mean place, first-place rate, and a line graph of cumulative *net score relative to that Match's table-average final score*. This centered per-Match value avoids a meaningless ever-increasing sum of raw final scores and does not add an unrequested uma/oka rule. At zero completed Matches, show zero progress and no fabricated average, rate, or graph points. Sequence order, not wall-clock time, is the graph's x-axis. Result history remains viewable after a stop or restart; the incomplete attempt, if any, is shown separately from counted Matches.

## Verification

- Core lifecycle tests: reject Human/ordinary Spectator and invalid roster/count; explicitly selected Built-in Bot works; three-player MJAI remains rejected; `n=1` and `n=1,000` boundaries; consecutive seat rotations and fixed Participant identity; precisely one next start after persisted completion; manual stop after the current completed Match; disconnect/revocation/abort stop before auto-play can affect results; normal Room Rematch stays unchanged.
- Storage tests: persist Run, fixed roster, Match sequence and scores; repeated completion does not double-count; replay/result write failure prevents advancement; interrupted active Runs become read-only on startup; completed results survive Room deletion/expiration; incomplete attempts are excluded from statistics.
- HTTP/visibility tests: unauthenticated and bot/public requests cannot reach live all-hands state; authenticated Admin can see every present hand but neither wall order nor other Players' private legal actions; reconnection returns current state; normal Room and compat visibility contracts remain intact.
- UI tests: Run creation validation, stop/progress states, empty statistics, correct averages/first-place rates and centered cumulative graph including nonmultiples of the seat count, exclusion of incomplete attempts, and retained history after interruption.

## Scope and existing work

The repository currently has local, unrelated edits in the ChatGPT gateway, MCP, OAuth store, and its test. This design does not modify or rely on those changes. `CONTEXT.md` records the agreed Benchmark terms. No product code is implemented by this spec.
