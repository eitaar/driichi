# riichi.dev protocol compatibility audit

Date: 2026-09-23

## Scope

This note compares driichi's `/ws/ranked` and `/ws/validate` implementation with public first-party riichi.dev and RiichiEnv protocol evidence. The riichi.dev production server source and an immutable revision of its documentation are not public, so the live documentation is the highest available source for WebSocket behavior.

## Authoritative boundaries

Three related contracts must not be conflated:

1. **riichi.dev WebSocket protocol** — the compatibility target for driichi.
2. **RiichiEnv-generated MJAI events** — engine/observation events, including an id-less `start_game` path.
3. **Generic Cryolite/mjai schemas** — a broader MJAI format where fields such as `start_game.id` may be optional.

Driichi currently forwards RiichiEnv-shaped events at a riichi.dev-compatible WebSocket boundary. Recipient-specific fields therefore need to be added at that boundary.

## Confirmed findings

### 1. `start_game.id` is missing and has the wrong local type

riichi.dev documents:

```json
{"type":"start_game","id":0}
```

`id` is the recipient bot's seat index (`0`–`3`). Driichi instead models this field as `Option<String>` and forwards the engine event unchanged. RiichiEnv can generate `{"type":"start_game"}`, so Serde omits `id` entirely. This reproduces the reported mechanism and is a compatibility bug.

Sources:

- [riichi.dev protocol: `start_game`](https://www.riichi.dev/docs/protocol#start_game)
- [RiichiEnv 4-player event generation at `479c1fa`](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/state/mod.rs#L149-L174)
- [RiichiEnv generic replay type at `479c1fa`](https://github.com/smly/RiichiEnv/blob/479c1faeb33d082965eef8198f63261a79c0fce3/riichienv-core/src/replay/mjai_replay.rs#L61-L68)
- [Cryolite/mjai generic schema at `5211f3f`](https://github.com/Cryolite/mjai/blob/5211f3fce66924f0451f1adeff850c6adc107044/schema/start_game.json?plain=1)

### 2. `end_game.scores` is missing

riichi.dev documents `end_game` with final scores in seat order. Driichi's `ServerEvent::EndGame` has no payload and serializes only `{"type":"end_game"}`. This is a second confirmed server-to-bot compatibility bug.

Source:

- [riichi.dev protocol: `end_game`](https://www.riichi.dev/docs/protocol#end_game)

### 3. Unknown extra fields in bot actions are incorrectly rejected

riichi.dev explicitly says the server ignores unknown fields in bot responses. Driichi's `ClientAction` uses `#[serde(deny_unknown_fields)]`, turning otherwise valid actions with harmless metadata into `unparseable` responses. This is a confirmed bot-to-server compatibility bug.

Source:

- [riichi.dev protocol compatibility guidance](https://www.riichi.dev/docs/protocol)

### 4. Validation timeouts incorrectly fail validation

riichi.dev validation allows timeouts: the server applies the default action, and validation can still pass if the bot remains connected and never submits an illegal action. Driichi sets `validation_failed = true` with reason `timeout` after every request timeout. This is a confirmed `/ws/validate` behavior bug.

Sources:

- [riichi.dev validation](https://www.riichi.dev/docs/validation)
- [riichi.dev protocol timing/default actions](https://www.riichi.dev/docs/protocol)

### 5. Duplicate concurrent connections are not rejected

riichi.dev matchmaking rejects a new connection when the same bot is already queued or playing. Driichi's ranked and validation admission paths do not check for the same token in the queue or active matches. This is a confirmed matchmaking compatibility gap.

Source:

- [riichi.dev matchmaking](https://www.riichi.dev/docs/matchmaking)

### 6. `kakan.consumed` is missing

riichi.dev requires `kakan.consumed` to contain the three tiles in the existing pon. Driichi retained these tiles in `GameAction::Kakan`, but discarded them when adapting RiichiEnv's narrower kakan event into `GameEvent::Kakan`. Consequently both the WebSocket event and canonical MJSON omitted the field.

The supplied `../../NEA/yamai/misc/test.mjson` independently demonstrates the same shape at line 845. It is a full-information replay and is used only as event-shape evidence, not as a bot visibility contract.

Source:

- [riichi.dev protocol: `kakan`](https://www.riichi.dev/docs/protocol#kakan)

## Checked and currently consistent

- Bearer authentication is read from the `Authorization` header.
- `/ws/ranked` and `/ws/validate` use the documented paths.
- WebSocket ping frames receive pong frames.
- `request_action` carries `request_id`, `time`, `possible_actions`, and `observation`.
- Timing defaults are 3,000 ms grace plus a 15,000 ms per-kyoku bank.
- Legacy replies without `request_id` are accepted.
- `action_ack` supports `accepted`, `rejected`, `unparseable`, `stale`, and `defaulted`.
- Opponent draws and concealed hands are masked.
- Unknown server event types/fields are tolerated by the parser.
- Disconnect makes the bot inactive and subsequent decisions use defaults.
- `validation_result` uses `passed`, with `reason` only on failure.

## Supplied full-information MJSON cross-check

`../../NEA/yamai/misc/test.mjson` uses the common replay spellings `kyotaku`, `deltas`, and `ura_markers`. The riichi.dev boundary already emits `kyotaku` and `deltas`. Driichi's generic replay format remains RiichiEnv-shaped (`kyoutaku`, `delta`, and `uradora_markers`), so that separate format does not fully round-trip the supplied replay. This is not classified as a riichi.dev endpoint bug because the replay and bot-visibility contracts are distinct.

The file's complete `tehais` and every player's `tsumo.pai` are replay-only full information and must not be copied to bot projections.

## Unresolved or intentionally not classified

Public evidence does not fully specify every required/optional field for `hora`, ordinary `ryukyoku`, `end_kyoku`, and every `action_ack` status. These should not be changed without an authenticated production transcript or a published riichi.dev schema.

## Recommended regression seams

Use live WebSocket integration tests at the public compatibility boundary:

- every assigned connection receives its own numeric `start_game.id`;
- `end_game` contains final scores;
- a legal action with an unknown extra field is accepted;
- timeout-only validation still passes;
- a second connection for the same token is rejected while queued or playing;
- `kakan` preserves and emits all three `consumed` pon tiles.
