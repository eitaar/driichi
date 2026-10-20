import { createElement } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { focusManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, it, vi } from "vitest";
import type { AdminRoomDetail, BenchmarkRun } from "./api";
import { validateLoadedRuns, benchmarkReadiness, filterLoadedRuns, matchingStopRequested, replayAvailability, useBenchmarkEvidence } from "./benchmark-data";

const run: BenchmarkRun = { run_id: "r1", room_code: "123456", game_mode: "4p-red-east", target: 2, completed: 0, status: "running", reason: null, failed_match_id: null, roster: Array.from({ length: 4 }, (_, initial_seat) => ({ participant_id: `p${initial_seat}`, display_name: `Bot ${initial_seat}`, initial_seat, participant_kind: "built_in_bot", character_id: null })), matches: [] };
const selected = Array.from({ length: 4 }, (_, i) => ({ participant_id: `p${i}`, display_name: `Bot ${i}`, selected: true, kind: "built_in_bot", presence: "connected", ready: false, character_id: "", role: "none", controller: "interactive" }));
const room = { join_code: "123456", game_mode: "4p-red-east", selected_count: 4, phase: "lobby", participants: selected } as AdminRoomDetail;
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

it("rejects malformed legacy index snapshots instead of rendering partial identities", () => {
  expect(validateLoadedRuns([run])).toEqual([run]);
  for (const invalid of [{}, [null], [{ ...run, roster: null }], [{ ...run, room_code: 123456 }], [{ ...run, roster: [{ display_name: null }] }]]) {
    expect(() => validateLoadedRuns(invalid)).toThrow();
  }
});
it("pages only the loaded snapshot and retains names while combining local filters", () => {
  const runs = Array.from({ length: 27 }, (_, i) => ({ ...run, run_id: `r${i}`, status: i === 26 ? "interrupted" as const : "running" as const }));
  expect(filterLoadedRuns(runs, "", "", "", 0).items).toHaveLength(25);
  expect(filterLoadedRuns(runs, "", "", "", 25).items.map(r => r.run_id)).toEqual(["r25", "r26"]);
  expect(filterLoadedRuns(runs, "Bot 0", "interrupted", "4p-red-east", 0).items.map(r => r.run_id)).toEqual(["r26"]);
  expect(filterLoadedRuns(runs, "missing", "", "", 0)).toEqual({ items: [], total: 0 });
  expect(filterLoadedRuns(runs, "", "", "", 50).items).toEqual([]);
});
it("blocks inconsistent selection counts, unknown software, disconnected, human and 3p MJAI rosters", () => {
  expect(benchmarkReadiness(room, 1)).toEqual([]);
  for (const kind of ["human", "unknown"]) expect(benchmarkReadiness({ ...room, participants: [{ ...selected[0], kind }, ...selected.slice(1)] }, 1)).not.toEqual([]);
  expect(benchmarkReadiness({ ...room, selected_count: 3 }, 1)).not.toEqual([]);
  expect(benchmarkReadiness({ ...room, participants: [{ ...selected[0], presence: "disconnected" }, ...selected.slice(1)] }, 1)).not.toEqual([]);
  expect(benchmarkReadiness({ ...room, game_mode: "3p-red-east", selected_count: 3, participants: [{ ...selected[0], kind: "mjai" }, ...selected.slice(1, 3)] }, 1).join(" ")).toMatch(/MJAI/);
  for (const target of [0, 1001, 1.5]) expect(benchmarkReadiness(room, target).join(" ")).toMatch(/1–1,000/);
  expect(benchmarkReadiness({ ...room, phase: "playing" }, 1)).not.toEqual([]);
});
it("requires matching Run and Room identities for pending Stop, with terminal status winning", () => {
  const pending = { ...room, benchmark: { run_id: "r1", status: "running", stop_requested: true } } as AdminRoomDetail;
  expect(matchingStopRequested(run, pending)).toBe(true);
  expect(matchingStopRequested(run, { ...pending, benchmark: { ...pending.benchmark!, run_id: "other" } })).toBeNull();
  expect(matchingStopRequested(run, { ...pending, join_code: "654321" })).toBeNull();
  expect(matchingStopRequested(run, room)).toBeNull();
  expect(matchingStopRequested(run, { ...pending, benchmark: { ...pending.benchmark!, status: undefined } } as unknown as AdminRoomDetail)).toBeNull();
  expect(matchingStopRequested(run, { ...pending, benchmark: { ...pending.benchmark!, status: "stopped" } })).toBeNull();
  expect(matchingStopRequested({ ...run, status: "stopped" }, pending)).toBe(false);
});
it("never manufactures Replay availability from missing, conflicting or other Match metadata", () => {
  const page = { replays: [{ match_id: "m1", availability: "available", replay_available: true }] };
  expect(replayAvailability("m1", page)).toBe("available");
  expect(replayAvailability("m2", page)).toBe("unknown");
  expect(replayAvailability("m1", { replays: [{ match_id: "m1", availability: "available", replay_available: false }] })).toBe("unknown");
  expect(replayAvailability("m1", { replays: [{ match_id: "m1", availability: "too_large", replay_available: false }] })).toBe("too_large");
  expect(replayAvailability("m1", { replays: [{ match_id: "m1", availability: "unavailable", replay_available: false }] })).toBe("unavailable");
  expect(replayAvailability("m1", {})).toBe("unknown");
  expect(replayAvailability("m1", { replays: [page.replays[0], page.replays[0]] })).toBe("unknown");
});
function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return { client, wrapper: ({ children }: { children: React.ReactNode }) => createElement(QueryClientProvider, { client }, children) };
}
function response(value: unknown, status = 200) { return new Response(JSON.stringify(value), { status }); }
it("loads ordinary Room Stop state without any private observer and recovers other-tab intent on the same detail query", async () => {
  let stopped = false;
  const paths: string[] = [];
  vi.stubGlobal("fetch", async (path: string) => {
    paths.push(path);
    return response(path.endsWith("/r1") ? run : { ...room, benchmark: { run_id: "r1", status: "running", stop_requested: stopped } });
  });
  const { wrapper: provider } = wrapper();
  const { result } = renderHook(() => useBenchmarkEvidence("r1"), { wrapper: provider });
  await waitFor(() => expect(result.current.evidence.kind).toBe("complete"));
  expect(result.current.query.data?.stopRequested).toBe(false);
  stopped = true;
  await act(async () => { await result.current.refresh(); });
  await waitFor(() => expect(result.current.query.data?.stopRequested).toBe(true));
  expect(paths).toEqual(["/api/v1/admin/benchmark/runs/r1", "/api/v1/admin/rooms/123456", "/api/v1/admin/benchmark/runs/r1", "/api/v1/admin/rooms/123456"]);
});
it("keeps numeric evidence complete when ordinary Room metadata fails but propagates authorization loss", async () => {
  let status = 503;
  vi.stubGlobal("fetch", async (path: string) => path.endsWith("/r1") ? response(run) : response({ detail: "Room unavailable" }, status));
  const { wrapper: provider } = wrapper();
  const { result } = renderHook(() => useBenchmarkEvidence("r1"), { wrapper: provider });
  await waitFor(() => expect(result.current.evidence.kind).toBe("complete"));
  expect(result.current.query.data?.stopRequested).toBeNull();
  status = 401;
  await act(async () => { await result.current.refresh(); });
  await waitFor(() => expect(result.current.evidence.kind).toBe("error"));
});
it("never populates a new Run with an aborted old response and rejects mismatched detail identity", async () => {
  let resolveOld!: (value: Response) => void;
  let signal: AbortSignal | undefined;
  vi.stubGlobal("fetch", async (path: string, init?: RequestInit) => {
    if (path.endsWith("/r1")) { signal = init?.signal as AbortSignal; return new Promise<Response>(resolve => { resolveOld = resolve; }); }
    return response(path.endsWith("/r2") ? { ...run, run_id: "wrong" } : room);
  });
  const { wrapper: provider } = wrapper();
  const { result, rerender } = renderHook(({ id }) => useBenchmarkEvidence(id), { initialProps: { id: "r1" }, wrapper: provider });
  await waitFor(() => expect(resolveOld).toBeDefined());
  rerender({ id: "r2" });
  expect(signal?.aborted).toBe(true);
  await act(async () => { resolveOld(response(run)); });
  await waitFor(() => expect(result.current.evidence.kind).toBe("error"));
  expect(result.current.query.data).toBeUndefined();
});
it("polls at five seconds without overlapping detail reads and refreshes the same query on focus", async () => {
  vi.useFakeTimers();
  let reads = 0;
  let release: (() => void) | undefined;
  vi.stubGlobal("fetch", async (path: string) => {
    if (path.endsWith("/r1")) {
      reads++;
      if (reads === 2) await new Promise<void>(resolve => { release = resolve; });
      return response(run);
    }
    return response({ ...room, benchmark: { run_id: "r1", status: "running", stop_requested: reads > 1 } });
  });
  const { wrapper: provider } = wrapper();
  const { result, unmount } = renderHook(() => useBenchmarkEvidence("r1"), { wrapper: provider });
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(result.current.evidence.kind).toBe("complete");
  await act(async () => { await vi.advanceTimersByTimeAsync(4998); });
  expect(reads).toBe(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(2); });
  expect(reads).toBe(2);
  await act(async () => { focusManager.setFocused(false); focusManager.setFocused(true); await vi.advanceTimersByTimeAsync(10000); });
  expect(reads).toBe(2);
  await act(async () => { release?.(); await vi.advanceTimersByTimeAsync(1); });
  expect(result.current.query.data?.stopRequested).toBe(true);
  await act(async () => { focusManager.setFocused(false); focusManager.setFocused(true); await vi.advanceTimersByTimeAsync(1); });
  expect(reads).toBe(3);
  unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
  expect(reads).toBe(3);
  focusManager.setFocused(undefined);
});
it("does not treat partial persisted records as complete evidence", async () => {
  vi.stubGlobal("fetch", async (path: string) => response(path.endsWith("/r1") ? { ...run, completed: 1 } : room));
  const { wrapper: provider } = wrapper();
  const { result } = renderHook(() => useBenchmarkEvidence("r1"), { wrapper: provider });
  await waitFor(() => expect(result.current.evidence.kind).toBe("incomplete"));
});
