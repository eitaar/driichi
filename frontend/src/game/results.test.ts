import { describe, expect, it } from "vitest";

import {
  decodeFinalStandings,
  decodeKyokuEndStep,
  decodeWinResult,
} from "./results";
import { useGameStore } from "./store";

const win = {
  event_index: 4,
  winner: "p0",
  seat: 0,
  target: 1,
  tsumo: false,
  concealed: [0, 4, 8, 12, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56],
  melds: [],
  winning_tile: 56,
  winning_tile_index: 13,
  yaku: [{ id: 1, name: "riichi", han: 1, yakuman_multiplier: null }],
  evaluation: { Ordinary: { han: 1, fu: 30, limit: null } },
  win_points: 1_000,
};

const settlement = {
  changes: [
    { participant_id: "p0", seat: 0, before: 0, delta: 0, after: 0 },
    { participant_id: "p1", seat: 1, before: -1, delta: 1, after: 0 },
    { participant_id: "p2", seat: 2, before: 0, delta: 0, after: 0 },
    { participant_id: "p3", seat: 3, before: 0, delta: 0, after: 0 },
  ],
  pot_before: 0,
  pot_after: 0,
  draw_reason: null,
};

describe("authoritative result decoders", () => {
  it("sends only a current result confirmation and keeps PendingAction untouched", () => {
    useGameStore.getState().reset();
    const sent: unknown[] = [];
    const currentStep = { match_id: "match", kyoku_sequence: 1, step_index: 0 };
    useGameStore.getState().setStatus("connected");
    useGameStore.getState().receiveSnapshot({ game_mode: "4p-red-east", kyoku_end: stepFixture() } as never, null);

    expect(useGameStore.getState().confirmResult(currentStep, (message) => { sent.push(message); })).toBe(true);
    expect(sent).toEqual([{ type: "confirm_result", step_id: currentStep }]);
    expect(sent[0]).not.toHaveProperty("action_id");
    expect(useGameStore.getState().pendingAction).toBeNull();
  });

  it("rejects stale, disconnected and malformed confirmations without occupying PendingAction", () => {
    const state = useGameStore.getState();
    state.reset();
    state.setStatus("connected");
    state.receiveSnapshot({ game_mode: "4p-red-east", kyoku_end: stepFixture() } as never, null);
    const sent: unknown[] = [];
    const send = (message: unknown) => { sent.push(message); return true; };
    expect(state.confirmResult({ match_id: "old", kyoku_sequence: 1, step_index: 0 }, send)).toBe(false);
    expect(sent).toEqual([]);
    state.setStatus("reconnecting");
    expect(state.confirmResult(stepFixture().id, send)).toBe(false);
    state.setStatus("connected");
    expect(state.confirmResult(stepFixture().id, () => false)).toBe(false);
    expect(useGameStore.getState().confirmedResultStep).toBeNull();
    expect(state.confirmResult(stepFixture().id, send)).toBe(true);
    expect(state.confirmResult(stepFixture().id, send)).toBe(false);
    expect(sent).toHaveLength(1);
    state.receiveSnapshot({ game_mode: "4p-red-east", kyoku_end: null } as never, null);
    expect(useGameStore.getState().confirmedResultStep).toBeNull();
  });

  it("restores confirmation eligibility from the reconnect snapshot and rejects expired steps", () => {
    const state = useGameStore.getState();
    state.reset(); state.setStatus("connected");
    const room = { game_mode: "4p-red-east", kyoku_end: stepFixture() } as never;
    state.receiveSnapshot(room, null);
    expect(state.confirmResult(stepFixture().id, () => true)).toBe(true);
    state.resetForReconnect(); state.setStatus("connected"); state.receiveSnapshot(room, null);
    expect(state.confirmResult(stepFixture().id, () => true)).toBe(true);
    state.receiveSnapshot({ game_mode: "4p-red-east", kyoku_end: { ...stepFixture(), remaining_ms: 0 } } as never, null);
    state.resetForReconnect(); state.setStatus("connected");
    expect(state.confirmResult(stepFixture().id, () => true)).toBe(false);
  });

  it("accepts historical confirmations outside the remaining cohort without resetting the deadline", () => {
    const state = useGameStore.getState();
    state.reset(); state.setStatus("connected");
    const initial = { ...stepFixture(), remaining_ms: 3400, required: ["p0", "p1", "p2"], confirmed: ["p0"] };
    state.receiveSnapshot({ game_mode: "4p-red-east", kyoku_end: initial } as never, null);
    const deadline = useGameStore.getState().resultDeadline;
    const shrunk = { ...initial, required: ["p1", "p2"], remaining_ms: 5000 };
    expect(decodeKyokuEndStep(shrunk, "4p-red-east")?.confirmed).toEqual(["p0"]);
    state.receiveSnapshot({ game_mode: "4p-red-east", kyoku_end: shrunk } as never, null);
    expect(useGameStore.getState().resultDeadline).toBe(deadline);
    expect(state.confirmResult(shrunk.id, () => true)).toBe(true);
    expect(decodeKyokuEndStep({ ...shrunk, confirmed: ["p0", "p0"] }, "4p-red-east")).toBeNull();
    expect(decodeKyokuEndStep({ ...shrunk, confirmed: ["x".repeat(129)] }, "4p-red-east")).toBeNull();
  });

  it("accepts exact serialized Rust result tags and preserves legal signed settlement values", () => {
    const step = decodeKyokuEndStep({
      id: { match_id: "match", kyoku_sequence: 1, step_index: 0 },
      content: { Settlement: settlement },
      published_at_ms: 100,
      deadline_at_ms: 5_100,
      remaining_ms: 3_400,
      required: ["p0"],
      confirmed: [],
    }, "4p-red-east");

    expect(step?.content).toHaveProperty("Settlement");
    if (step && "Settlement" in step.content) {
      expect(step.content.Settlement.changes[1]).toMatchObject({ before: -1, delta: 1, after: 0 });
    }
  });

  it("rejects unknown result tags, illegal three-player tiles, and malformed positions", () => {
    expect(decodeKyokuEndStep({ ...stepFixture(), content: { Unknown: win } }, "4p-red-east")).toBeNull();
    expect(decodeWinResult({ ...win, concealed: [4, 8, 12, 20], winning_tile: 4, winning_tile_index: 0 }, "3p-red-east")).toBeNull();
    expect(decodeWinResult({ ...win, winning_tile_index: 99 }, "4p-red-east")).toBeNull();
  });

  it("preserves server Win Points without recalculating payments while rejecting malformed hands", () => {
    for (const win_points of [1, 2_600, 12_345]) {
      expect(decodeWinResult({ ...win, win_points }, "4p-red-east")?.win_points).toBe(win_points);
    }
    for (const bad of [
      { ...win, concealed: [win.concealed[1], ...win.concealed.slice(1)] },
      { ...win, concealed: win.concealed.slice(1), winning_tile_index: 12 },
    ]) expect(decodeWinResult(bad, "4p-red-east")).toBeNull();
  });

  it("preserves recognized server limit labels without classifying the score", () => {
    const ordinary = { ...win, evaluation: { Ordinary: { han: 2, fu: 40, limit: null } },
      yaku: [{ ...win.yaku[0], han: 2 }], win_points: 2_600 };
    expect(decodeWinResult(ordinary, "4p-red-east")?.evaluation).toEqual({ Ordinary: { han: 2, fu: 40 } });
    for (const limit of ["Mangan", "Haneman", "Baiman", "Sanbaiman", "CountedYakuman"]) {
      expect(decodeWinResult({ ...ordinary, evaluation: { Ordinary: { ...ordinary.evaluation.Ordinary, limit } } }, "4p-red-east")?.evaluation)
        .toEqual({ Ordinary: { han: 2, fu: 40, limit } });
    }
  });

  it.each([null, undefined])("keeps null or omitted server limit %s absent", (limit) => {
    const ordinary = { ...win, evaluation: { Ordinary: { han: 2, fu: 40, ...(limit === undefined ? {} : { limit }) } },
      yaku: [{ ...win.yaku[0], han: 2 }], win_points: 2_600 };
    const decoded = decodeWinResult(ordinary, "4p-red-east");
    expect(decoded?.evaluation).toEqual({ Ordinary: { han: 2, fu: 40 } });
    expect(decoded?.win_points).toBe(2_600);
  });

  it("accepts high fu adjudicated for multiple closed kans", () => {
    const highFu = { ...win, target: 0, tsumo: true, concealed: [40, 41], winning_tile: 41, winning_tile_index: 1,
      melds: [
        ...[[0, 1, 2, 3], [32, 33, 34, 35], [36, 37, 38, 39]].map(tiles => ({ kind: "Ankan", tiles, opened: false, from_who: null, called_tile: null })),
        { kind: "Pon", tiles: [84, 85, 86], opened: true, from_who: 1, called_tile: 84 },
      ],
      evaluation: { Ordinary: { han: 6, fu: 130, limit: "Haneman" } }, yaku: [20, 21, 22].map(id => ({ id, name: "pinned two-han yaku", han: 2 })), win_points: 12_000 };
    expect(decodeWinResult(highFu, "4p-red-east")).not.toBeNull();
  });

  it("rejects duplicate ranks instead of deriving an order from array position or score", () => {
    const duplicateRanks = {
      mode: "FourPlayerRedEast",
      players: [0, 1, 2, 3].map((seat) => ({
        participant_id: `p${seat}`,
        display_name: seat === 0 ? "Same" : `P${seat}`,
        kind: "Human",
        seat,
        final_score: 25_000,
        rank: 1,
      })),
      final_scores: [25_000, 25_000, 25_000, 25_000],
    };

    expect(decodeFinalStandings(duplicateRanks, "4p-red-east")).toBeNull();
  });

  it("requires the same complete mode and admits duplicate names with zero or negative scores", () => {
    const result = { mode: "ThreePlayerRedEast", final_scores: [0, -100, 0], players: [
      { participant_id: "a", display_name: "Same", kind: "Human", seat: 0, final_score: 0, rank: 1 },
      { participant_id: "b", display_name: "Same", kind: "MCP", seat: 1, final_score: -100, rank: 3 },
      { participant_id: "c", display_name: "Same", kind: "BuiltInBot", seat: 2, final_score: 0, rank: 2 },
    ] };
    expect(decodeFinalStandings(result, "3p-red-east")).not.toBeNull();
    expect(decodeFinalStandings(result, "3p-red-half")).toBeNull();
    expect(decodeFinalStandings({ ...result, players: [result.players[0], result.players[0], result.players[2]] }, "3p-red-east")).toBeNull();
  });

  it("bounds nested restored tiles, meld seats, arrays, strings and numbers", () => {
    const meld = { kind: "Kakan", tiles: [52, 53, 54, 55], opened: true, from_who: 1, called_tile: 52 };
    const meldWin = { ...win, concealed: win.concealed.filter(tile => ![44, 48, 52].includes(tile)), winning_tile_index: 10, melds: [meld] };
    expect(decodeWinResult(meldWin, "4p-red-east")).not.toBeNull();
    for (const bad of [
      { ...win, concealed: Array(65).fill(0) }, { ...win, melds: Array(257).fill(meld) },
      { ...win, melds: [{ ...meld, tiles: [255] }] }, { ...win, melds: [{ ...meld, from_who: 4 }] },
      { ...win, melds: [{ ...meld, called_tile: 255 }] }, { ...win, winner: "x".repeat(129) },
      { ...win, event_index: Number.MAX_SAFE_INTEGER + 1 }, { ...win, win_points: Infinity },
      { ...win, evaluation: { Unknown: {} } }, { ...win, evaluation: { Ordinary: { han: 1, fu: 30, limit: "Unknown" } } },
    ]) expect(decodeWinResult(bad, "4p-red-east")).toBeNull();
    const yakuman = { ...win, win_points: 64_000, evaluation: { Yakuman: { multiplier: 2 } }, yaku: [{ id: 42, name: "kokushi", han: null, yakuman_multiplier: 2 }] };
    expect(decodeWinResult(yakuman, "4p-red-east")?.evaluation).toEqual({ Yakuman: { multiplier: 2 } });
    expect(decodeWinResult({ ...yakuman, yaku: [{ id: 42, name: "kokushi", han: 26 }] }, "4p-red-east")).toBeNull();
  });

  it("keeps missing legacy optional fields absent rather than manufacturing display values", () => {
    const legacy = decodeWinResult({
      ...win,
      yaku: [{ id: 1, name: "riichi" }],
      evaluation: { Ordinary: { han: 1, fu: 30 } },
    }, "4p-red-east");

    expect(legacy).toBeNull(); // Partial legacy events are not a certified WinResult.
  });
});

function stepFixture() {
  return {
    id: { match_id: "match", kyoku_sequence: 1, step_index: 0 },
    content: { Win: win },
    published_at_ms: 100,
    deadline_at_ms: 5_100,
    remaining_ms: 5_000,
    required: [],
    confirmed: [],
  };
}
