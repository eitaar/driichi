# Pre-action result capture — research findings

**Status:** planning evidence, not implementation acceptance. Investigated main `54d5be6` and the unchanged prototype `3032942`; the relevant core/server/replay source is identical between them. No production source or dependency was changed.

## Governing decisions

- [#10: authoritative win, settlement and standings contract](https://github.com/eitaar/driichi/issues/10#issuecomment-5961109168).
- [#12: Kyoku-End Step publication and confirmation](https://github.com/eitaar/driichi/issues/12#issuecomment-5960734813).
- [#11: acceptance and release policy](https://github.com/eitaar/driichi/issues/11#issuecomment-5961363330).
- The user requested investigation of a **server-side copy immediately before applying the resolved action(s)** before proposing dependency patches. Implementation and dependency patching remain unapproved.

## What the existing code actually retains

| Information | Existing authority / capture seam |
| --- | --- |
| Current concealed hands, physical tile IDs, meld types/openness, rivers | `EngineAdapter.state` before `step`; project with the existing `table_state` adapter |
| Tsumo winning tile | `drawn_tile`, not a tile-kind membership test |
| Ron / robbed-kan winning tile and target | `last_discard` and the actual accepted Hora; `pending_kan` carries robbery context |
| Pre-resolution held points, honba, pot | Pre-action engine state; prior riichi expenditures already reflected stay deducted |
| Settled held points and pot | **Real engine after accepted terminal action(s)**; final unclaimed-pot award may not appear in Hora/Ryukyoku deltas |
| Winner order | Actual Hora log order, not iteration of `HashMap<u8, WinResult>` |
| Detailed win evaluation | The engine inserts public `win_results`; normal next-round initialization clears it |
| Disk record | Existing `.mjson` canonical events and database roster/final points; not a full state snapshot per action |

`MatchMachine::resolve_actions` routes single/simultaneous resolutions into `EngineAdapter::apply` / `apply_simultaneous`. Capture belongs inside that shared adapter boundary after legality/command translation and **before the first mutating step**. A compound RiichiDiscard requires one pre-resolution capture, not two unrelated settlements.

A photo of the pre-action state is sufficient for hand identity and score-before, but not by itself an adjudicated yaku/han/fu result. Do not call those fields already captured merely because the hand was copied.

## Existing API candidate: evaluate the captured copy without advancing its round

The exact dependency is `riichienv-core = "=0.4.10"`. `GameStateVariant`, `GameState` and `GameState3P` implement `Clone`; `step`, `game_mode`, `rule` and `win_results` are publicly accessible.

On a **private, disposable copy only**, setting `game_mode` to the existing single-Kyoku stop boundary (`0` for four players, `3` for three) lets the same engine execute the accepted raw command batch and retain its adjudicated `win_results`. Keep every scoring rule, hand, claim, player/dealer wind, kan/riichi/kita flag and meld unchanged. Never change the live adapter's mode or use the copy's artificial `end_game` event.

Source inspection found `game_mode` consulted by round progression and final-pot handling, not the win-evaluation branches. The stored `mode` / `sub_mode` stays unchanged. This is a version-specific adapter technique, **not a new documented library snapshot API or proof of universal equivalence**. Pin and test it rather than hiding that coupling.

### Executed experiment

Local, ignored research crate: `.git/riichienv-snapshot-probe/`; exact installed dependency, offline Cargo, no Room/network connection. Run locally:

```sh
cargo run --offline --manifest-path .git/riichienv-snapshot-probe/Cargo.toml
```

The final run passed **8 controlled winning snapshots + 2 final-draw boundary probes**:

- Three/four players × Tsumo/two-winner Ron × ordinary hand/Kokushi thirteen-sided wait.
- Copy and real engine had identical Hora arrays (winner order, deltas, ura fields) and held points in the eight win cases.
- The real engine initialized its next round and cleared win results; the copy retained each adjudicated result.
- The default Tenhou rule capped Kokushi's double-yakuman pattern to `han = 13` inside the engine; bare `HandEvaluator::calc` is not interchangeable with that adjudication.
- Final exhaustive-draw probes showed the real match awarding the remaining 2,000-point pot to initial seat 0 on a tie, while the single-Kyoku copy retained the pot. **Use real post-action scores for settlement, never copy scores indiscriminately.**

Fixtures were controlled engine states, not full legal-match lifecycle acceptance. An initial Kokushi multi-Ron fixture bankrupted the discarder and legitimately ended the match, so it did not demonstrate next-round clearing; increasing fixture balances isolated that transition. Raw probe source/output is local evidence, not a repository-distributed test suite. The implementation plan promotes the assertions into checked-in tests before adopting the technique.

### Remaining checks before adopting the candidate

- Four actual east/half modes, real seeded legal-action lifecycles, dealer continuation, bankruptcy/agari-yame and final match termination.
- Ippatsu, last tile, rinshan, kakan/ankan robbery, riichi declaration discard, three-player kita (including robbery without chankan), open/closed kans, ordinary/red/ura bonuses, counted/stacked yakuman and responsibility payments.
- Sanchaho and other abortive draws, exhaustive draw, nagashi-mangan; preserve engine draw classification rather than invent a completed winning hand for nagashi.
- Rejection/adapter divergence must not adopt a candidate or expose it. Snapshot copy/extra evaluation cost must be bounded and measured before release.

If equivalence fails, stop that implementation task and report the exact missing field/case. The plan does **not** authorize vendoring, a registry-cache edit, rule duplication, or silently falling back to guessed results.

## Smallest architecture suggested by the evidence

- The **engine adapter** is the seam with depth: it captures physical state and translates adjudicated library output once. Higher modules never reconstruct Mahjong scoring.
- `MatchMachine` owns the ended table plus withheld next-round events. It gates reads as well as actions: `project`, `current_decision`, `legal_actions`, timeout resolution and final result exposure.
- Existing serialized `RoomActor` owns confirmation cohorts, step identity and publication deadlines. No second scheduler, workflow service, or distributed synchronization is needed.
- Canonical full events remain available to the private persistence effect; public history/transports only see the released prefix. Merely withholding `start_kyoku` is insufficient: Hora deltas can reveal the combined settlement early.
- Reuse the existing writer/database/Replay adapter. Store small versioned result records, not all captured concealed hands. Do not alter MJAI wire conventions to carry UI-only metadata.

## Primary sources

Dependency revision from the published crate's `.cargo_vcs_info.json`: `479c1faeb33d082965eef8198f63261a79c0fce3`, path `riichienv-core`. The installed exact-version files were read directly; current-main documentation was discovery context only.

1. [Clone/public state, step, win adjudication, progression, clearing and pot](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/state/mod.rs): struct; Tsumo/Ron branches; `_advance_round`, `_initialize_round`, `_process_end_game`.
2. [Three-player counterparts and kita context](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/state_3p/mod.rs).
3. [Public WinResult](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/types.rs), [evaluator/counted-yakuman scoring](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/hand_evaluator.rs), [payment API](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/score.rs), [yaku names](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/yaku.rs).
4. [Upstream final-pot regressions](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/tests/match_end.rs).
5. Local production interfaces: `crates/double_riichi_core/src/{engine,match_machine,projection,room}.rs`, `crates/double_riichi_replay/src/{frames,persistence,reader}.rs`, `crates/double_riichi_server/src/{storage,http}.rs`, `frontend/src/game/{types,store}.ts`, `frontend/src/game/gameplay.tsx`.
