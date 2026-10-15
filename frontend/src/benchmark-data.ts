import { useQuery } from "@tanstack/react-query";
import { api, ApiProblem, type AdminRoomDetail, type BenchmarkRun } from "./api";
import { deriveRunAnalytics } from "./benchmark-analytics";
import type { ReplayAvailability } from "./benchmark-types";

export function validateLoadedRuns(value: unknown): BenchmarkRun[] {
  const ids = new Set<string>();
  if (!Array.isArray(value) || value.some(run => {
    if (!run || typeof run.run_id !== "string" || !run.run_id || ids.has(run.run_id)
      || typeof run.room_code !== "string" || !/^\d{6}$/.test(run.room_code)
      || typeof run.game_mode !== "string" || typeof run.status !== "string"
      || !Number.isInteger(run.target) || !Number.isInteger(run.completed)
      || !Array.isArray(run.roster) || run.roster.some((p: BenchmarkRun["roster"][number]) => !p || typeof p.display_name !== "string")) return true;
    ids.add(run.run_id);
    return false;
  })) throw new ApiProblem({ detail: "Invalid loaded Run history. Refresh to try again." });
  return value;
}

// shortcut: legacy index loads full Runs, replace with server summaries when the owner publishes pagination.
export function filterLoadedRuns(runs: BenchmarkRun[], search: string, status: string, mode: string, offset: number) {
  const term = search.trim().toLowerCase();
  const filtered = runs.filter(run => (!status || run.status === status) && (!mode || run.game_mode === mode)
    && [run.run_id, run.room_code, ...run.roster.map(p => p.display_name)].some(value => value.toLowerCase().includes(term)));
  return { items: filtered.slice(offset, offset + 25), total: filtered.length };
}

export function benchmarkReadiness(room: AdminRoomDetail, target: number): string[] {
  const seats = room.game_mode.startsWith("3p") ? 3 : 4;
  const selected = room.participants.filter(p => p.selected);
  const blockers: string[] = [];
  if (room.phase !== "lobby" || room.benchmark) blockers.push("Return to an available Lobby before starting a new Run.");
  if (!Number.isInteger(target) || target < 1 || target > 1000) blockers.push("Choose 1–1,000 whole Matches.");
  if (selected.length !== seats || room.selected_count !== seats) blockers.push(`Explicitly select exactly ${seats} bots. No automatic fill.`);
  for (const participant of selected) {
    if (!["mjai", "mcp", "built_in_bot"].includes(participant.kind)) blockers.push(`${participant.display_name}: only software Participants may play.`);
    if (participant.presence !== "connected") blockers.push(`${participant.display_name}: must be connected.`);
    if (seats === 3 && participant.kind === "mjai") blockers.push(`${participant.display_name}: MJAI is not supported in 3p.`);
  }
  return blockers;
}

export function matchingStopRequested(run: Pick<BenchmarkRun, "run_id" | "room_code" | "status">, room?: AdminRoomDetail): boolean | null {
  if (run.status !== "running") return false;
  if (room?.join_code !== run.room_code || room.benchmark?.run_id !== run.run_id
    || room.benchmark.status !== "running" || typeof room.benchmark.stop_requested !== "boolean") return null;
  return room.benchmark.stop_requested;
}

export function replayAvailability(matchId: string, page: unknown): ReplayAvailability {
  if (!page || typeof page !== "object" || !("replays" in page) || !Array.isArray(page.replays)) return "unknown";
  const records = page.replays.filter(record => record && record.match_id === matchId);
  if (records.length !== 1) return "unknown";
  const record = records[0];
  if (record.availability === "available" && record.replay_available === true) return "available";
  if (record.replay_available === false && ["unavailable", "too_large"].includes(record.availability)) return record.availability;
  return "unknown";
}

export const benchmarkRunKey = (runId: string) => ["admin", "benchmark", "run", runId] as const;
export function useBenchmarkEvidence(runId: string) {
  const query = useQuery({
    queryKey: benchmarkRunKey(runId),
    queryFn: async ({ signal }) => {
      const run = await api.getBenchmarkRun(runId, signal);
      if (!run || run.run_id !== runId || !/^\d{6}$/.test(run.room_code)
        || !["running", "completed", "stopped", "interrupted", "failed"].includes(run.status)
        || !Number.isInteger(run.target) || run.target < 1 || run.target > 1000 || run.completed > run.target) {
        throw new ApiProblem({ detail: "Invalid or mismatched Run detail." });
      }
      const analytics = deriveRunAnalytics(run);
      let room: AdminRoomDetail | undefined;
      let roomError: unknown;
      if (run.status === "running") {
        try { room = await api.getAdminRoom(run.room_code, signal); }
        catch (error) {
          if (signal.aborted || error instanceof ApiProblem && error.status === 401) throw error;
          roomError = error;
        }
      }
      return { run, analytics, stopRequested: matchingStopRequested(run, room), roomError };
    },
    retry: false,
    refetchOnMount: "always",
    refetchOnWindowFocus: "always",
    refetchInterval: query => query.state.data?.run.status === "running" ? 5000 : false,
  });
  const evidence = query.isError ? { kind: "error" as const, error: query.error }
    : !query.data ? { kind: "loading" as const }
    : query.data.analytics.kind === "invalid" ? { kind: "incomplete" as const, reason: query.data.analytics.reason }
    : { kind: "complete" as const, run: query.data.run, analytics: query.data.analytics };
  // Explicit refresh and future event invalidations share this one non-overlapping query.
  const refresh = () => query.refetch({ cancelRefetch: false });
  return { query, evidence, refresh };
}
