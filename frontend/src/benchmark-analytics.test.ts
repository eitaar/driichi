import { describe, expect, it } from "vitest";
import type { BenchmarkRun } from "./api";
import { deriveRunAnalytics } from "./benchmark-analytics";

type Input = Pick<BenchmarkRun, "game_mode" | "completed" | "roster" | "matches">;
function fixture(count = 1, players = 4): Input {
  return {
    game_mode: players === 3 ? "3p-red-east" : "4p-red-east", completed: count,
    roster: ["A", "B", "C", "D"].slice(0, players).map((id, seat) => ({
      participant_id: id, display_name: "Same name", initial_seat: seat,
      participant_kind: "builtin", character_id: null,
    })),
    matches: Array.from({ length: count }, (_, index) => ({
      sequence: index + 1, match_id: `match-${index + 1}`,
      results: [40000, 30000, 20000, 10000].slice(0, players).map((score, seat) => ({
        participant_id: ["A", "B", "C", "D"][seat], seat: (seat + index) % players,
        final_score: score, rank: seat + 1,
      })),
    })),
  };
}
function ready(input: Input) {
  const result = deriveRunAnalytics(input);
  expect(result.kind).toBe("ready");
  if (result.kind !== "ready") throw new Error(result.reason);
  return result;
}

describe("deriveRunAnalytics", () => {
  it("derives literal four-player net scores from persisted outcomes, not names or Seats", () => {
    const input = fixture(2);
    input.matches[1].results.forEach((result, index) => {
      result.final_score = [10000, 20000, 30000, 40000][index];
      result.rank = [4, 3, 2, 1][index];
    });
    const result = ready(input);
    expect(result.participants.map(p => p.points[0].net)).toEqual([15000, 5000, -5000, -15000]);
    expect(result.participants.map(p => p.participantId)).toEqual(["A", "B", "C", "D"]);
    expect(result.participants[0]).toEqual({
      participantId: "A", displayName: "Same name", averageRank: 2.5, firstPlaceRate: 0.5,
      cumulativeNet: 0, rankCounts: [1, 0, 0, 1], points: [
        { sequence: 1, matchId: "match-1", net: 15000, cumulative: 15000, movingRank: 1, movingN: 1 },
        { sequence: 2, matchId: "match-2", net: -15000, cumulative: 0, movingRank: 2.5, movingN: 2 },
      ],
    });
  });

  it.each(["3p-red-east", "3p-red-half"])("uses the actual three-player mean and only three rank bins for %s", mode => {
    const input = fixture(1, 3); input.game_mode = mode;
    const result = ready(input);
    expect(result.participants.map(p => p.cumulativeNet)).toEqual([10000, 0, -10000]);
    expect(result.participants[0].rankCounts).toEqual([1, 0, 0]);
  });

  it("keeps authoritative tied ranks rather than reconstructing winners from scores", () => {
    const input = fixture();
    input.game_mode = "4p-red-half";
    input.matches[0].results.forEach(r => { r.rank = 1; });
    expect(ready(input).participants.map(p => [p.averageRank, p.firstPlaceRate])).toEqual([[1, 1], [1, 1], [1, 1], [1, 1]]);
  });

  it("allows negative scores and preserves fractional mean precision", () => {
    const input = fixture(1, 3);
    input.matches[0].results.forEach((r, i) => { r.final_score = [-1000, 0, 0][i]; });
    const result = ready(input);
    expect(result.participants[0].cumulativeNet).toBeCloseTo(-666.6666666666667, 10);
    expect(result.participants[1].cumulativeNet).toBeCloseTo(333.3333333333333, 10);
  });

  it("sorts Match copies without mutating input or fixed roster identity order", () => {
    const input = fixture(2);
    input.matches.reverse(); input.roster.reverse();
    const before = structuredClone(input);
    const result = ready(input);
    expect(input).toEqual(before);
    expect(result.participants.map(p => p.participantId)).toEqual(["D", "C", "B", "A"]);
    expect(result.participants[3].points.map(p => [p.sequence, p.cumulative])).toEqual([[1, 15000], [2, 30000]]);
  });

  it("returns empty ready results with null rank/rate for zero completed", () => {
    const result = ready(fixture(0));
    expect(result.completed).toBe(0); expect(result.provisional).toBe(true);
    expect(result.participants[0]).toEqual({ participantId: "A", displayName: "Same name", averageRank: null,
      firstPlaceRate: null, cumulativeNet: 0, rankCounts: [0, 0, 0, 0], points: [] });
  });

  it.each([[9, true], [10, false]] as const)("marks %i Matches provisional=%s", (n, provisional) => {
    expect(ready(fixture(n)).provisional).toBe(provisional);
  });

  it("uses partial then trailing ten ranks, but full-run rate and distribution", () => {
    const input = fixture(11);
    input.matches[0].results[0].rank = 4;
    const participant = ready(input).participants[0];
    expect(participant.points[1].movingRank).toBe(2.5);
    expect(participant.points[8].movingRank).toBeCloseTo(1.3333333333333333);
    expect(participant.points[9].movingRank).toBe(1.3);
    expect(participant.points[10]).toMatchObject({ movingRank: 1, movingN: 10, cumulative: 165000 });
    expect(participant.averageRank).toBeCloseTo(1.2727272727272727);
    expect(participant.firstPlaceRate).toBeCloseTo(0.9090909090909091);
    expect(participant.rankCounts).toEqual([10, 0, 0, 1]);
  });

  it.each([[50, 750000], [51, 765000], [1000, 15000000]])("keeps all %i points and full cumulative before consumer windowing", (count, total) => {
    const participant = ready(fixture(count)).participants[0];
    expect(participant.points).toHaveLength(count);
    expect(participant.points[0].cumulative).toBe(15000);
    expect(participant.cumulativeNet).toBe(total);
    expect(participant.points.at(-1)?.movingN).toBe(10);
    expect(participant.firstPlaceRate).toBe(1);
  });

  const invalid: [string, (input: Input) => void][] = [
    ["unknown mode", x => { x.game_mode = "4p-unknown"; }],
    ["negative completed", x => { x.completed = -1; }],
    ["fractional completed", x => { x.completed = 1.5; }],
    ["nonfinite completed", x => { x.completed = NaN; }],
    ["over 1000 completed", x => { x.completed = 1001; }],
    ["missing advertised Match", x => { x.matches.pop(); }],
    ["extra Match", x => { x.completed = 0; }],
    ["duplicate Match", x => { x.matches[1].match_id = x.matches[0].match_id; }],
    ["empty Match ID", x => { x.matches[0].match_id = ""; }],
    ["duplicate sequence", x => { x.matches[1].sequence = 1; }],
    ["missing sequence", x => { x.matches[1].sequence = 3; }],
    ["fractional sequence", x => { x.matches[0].sequence = 0.5; }],
    ["missing roster participant", x => { x.roster.pop(); }],
    ["duplicate roster participant", x => { x.roster[1].participant_id = "A"; }],
    ["empty participant ID", x => { x.roster[0].participant_id = ""; }],
    ["duplicate initial Seat", x => { x.roster[1].initial_seat = 0; }],
    ["invalid initial Seat", x => { x.roster[0].initial_seat = -1; }],
    ["missing result", x => { x.matches[0].results.pop(); }],
    ["duplicate result participant", x => { x.matches[0].results[1].participant_id = "A"; }],
    ["unknown result participant", x => { x.matches[0].results[1].participant_id = "other"; }],
    ["duplicate Seat", x => { x.matches[0].results[1].seat = 0; }],
    ["out-of-range Seat", x => { x.matches[0].results[0].seat = 4; }],
    ["fractional Seat", x => { x.matches[0].results[0].seat = 0.5; }],
    ["zero rank", x => { x.matches[0].results[0].rank = 0; }],
    ["out-of-range rank", x => { x.matches[0].results[0].rank = 5; }],
    ["fractional rank", x => { x.matches[0].results[0].rank = 1.5; }],
    ["nonfinite rank", x => { x.matches[0].results[0].rank = NaN; }],
    ["fractional score", x => { x.matches[0].results[0].final_score = 1.5; }],
    ["infinite score", x => { x.matches[0].results[0].final_score = Infinity; }],
    ["NaN score", x => { x.matches[0].results[0].final_score = NaN; }],
    ["overflowing table total", x => { x.matches[0].results.forEach(r => { r.final_score = Number.MAX_VALUE; }); }],
    ["overflowing cumulative", x => { x.matches.forEach(m => m.results.forEach((r, i) => { r.final_score = [1e308, -1e308, 0, 0][i]; })); }],
  ];
  it.each(invalid)("rejects %s instead of filling missing or invalid evidence", (_, mutate) => {
    const input = fixture(2); mutate(input);
    expect(deriveRunAnalytics(input)).toMatchObject({ kind: "invalid", reason: expect.any(String) });
  });

  it.each([
    { roster: null }, { matches: null }, { roster: [null, null, null, null] },
    { matches: [null, null] }, { matches: [{ sequence: 1, match_id: "m", results: null }, null] },
    { roster: [{ participant_id: 123 }] },
  ])("rejects malformed persisted containers without throwing: %j", override => {
    const input = { ...fixture(2), ...override } as unknown as Input;
    expect(deriveRunAnalytics(input).kind).toBe("invalid");
  });

  it("does not accept summary statistics as a substitute or depend on Replay availability", () => {
    const input = { ...fixture(), statistics: [{ average_rank: 4 }], replay_available: false };
    expect(ready(input).participants[0].averageRank).toBe(1);
    input.matches = [];
    expect(deriveRunAnalytics(input).kind).toBe("invalid");
  });

  it("rejects a fourth rank in three-player mode", () => {
    const input = fixture(1, 3); input.matches[0].results[0].rank = 4;
    expect(deriveRunAnalytics(input).kind).toBe("invalid");
  });
});
