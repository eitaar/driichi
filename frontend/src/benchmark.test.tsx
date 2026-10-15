import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { App } from "./app";
import { focusManager } from "@tanstack/react-query";

const room = {
  join_code: "123456", room_name: "Bot benchmark", game_mode: "4p-red-east", phase: "lobby", benchmark_mode: true, benchmark: null,
  selected_count: 0, connected_count: 4, participant_count: 4, created_at: "2026-09-29T00:00:00Z",
  time_control: "casual", replay_save: true, participant_limit: 32,
  participants: Array.from({ length: 4 }, (_, id) => ({ participant_id: `bot-${id}`, display_name: `Bot ${id}`, kind: "built_in_bot", presence: "connected", selected: false, ready: false, character_id: "tsumogiri-bot", role: "none", controller: "interactive" })),
  match_players: [], roster: [], result: null, revision: 1, persistence_degraded: false, replay_available: true,
};
const run = {
  run_id: "run-1", room_code: "123456", game_mode: "4p-red-east", target: 3, completed: 0, status: "interrupted", reason: "server restarted", failed_match_id: "aborted", roster: Array.from({ length: 4 }, (_, initial_seat) => ({ participant_id: `p${initial_seat}`, display_name: "Same bot", initial_seat, participant_kind: "built_in_bot", character_id: null })), matches: [], statistics: [],
};
const live = { revision: 5, benchmark: { run_id: "run-1", target: 3, completed: 0, stop_requested: false, status: "running" }, projection: { audience: "benchmark_admin", mode: "4p-red-east", round: "east", kyoku: 1, dora_indicators: [0], players: Array.from({ length: 4 }, (_, seat) => ({ seat, participant_id: `p${seat}`, display_name: `Bot ${seat}`, score: 25000, hand: Array(13).fill(0), concealed_count: 13, discards: [], melds: [], riichi: false })), decision: null } };
function response(data: unknown, status = 200) { return new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json" } }); }
function stubFetch(handler: (path: string, init?: RequestInit) => Response) {
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => handler(String(input), init)));
}
beforeEach(() => { window.history.replaceState({}, "", "/admin/benchmark"); });

it("omits the private observer opener on phone while preserving Run status and Room controls", async () => {
  const width = window.innerWidth;
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 390 });
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  stubFetch(path => response(path.endsWith("/live") ? live : { ...run, status: "running" }));
  try {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Benchmark Run" })).toBeVisible();
    expect(screen.getByRole("link", { name: "Room controls" })).toBeVisible();
    expect(screen.getByText("No completed Matches yet.")).toBeVisible();
    expect(screen.queryByText("Live table — all current hands")).not.toBeInTheDocument();
    expect(vi.mocked(fetch).mock.calls.some(([path]) => String(path).endsWith("/live"))).toBe(false);
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1280 });
    act(() => window.dispatchEvent(new Event("resize")));
    await screen.findByText("Live table — all current hands");
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([path]) => String(path).endsWith("/live"))).toBe(true));
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 390 });
    act(() => window.dispatchEvent(new Event("resize")));
    expect(screen.queryByText("Live table — all current hands")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Benchmark Run" })).toBeVisible();
  } finally { Object.defineProperty(window, "innerWidth", { configurable: true, value: width }); }
});

it("creates a bot-only room through the benchmark endpoint and reuses explicit participant selection", async () => {
  const requests: string[] = [];
  stubFetch((path, init) => {
    if (init?.method === "POST") requests.push(path);
    if (path === "/api/v1/admin/benchmark/rooms") return response(room);
    if (path === "/api/v1/admin/benchmark/runs") return response([]);
    if (path === "/api/v1/admin/rooms") return response([room]);
    if (path === "/api/v1/admin/rooms/123456") return response(room);
    if (path === "/api/v1/admin/tokens") return response([]);
    if (path === "/api/v1/admin/benchmark/rooms/123456/bots") return response(room);
    return response({});
  });
  render(<App />);
  await screen.findByRole("heading", { name: "Benchmarks" });
  fireEvent.click(screen.getByRole("button", { name: "New Run" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Room name" }), { target: { value: "Bot benchmark" } });
  fireEvent.click(screen.getByRole("button", { name: "Create benchmark room" }));
  await screen.findByRole("heading", { name: "Bot benchmark" });
  expect(screen.queryByRole("link", { name: "Open lobby" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Fill with Bots" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Rematch" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Add Built-in Bot" }));
  await waitFor(() => expect(requests).toContain("/api/v1/admin/benchmark/rooms/123456/bots"));
  expect(requests).toContain("/api/v1/admin/benchmark/rooms");
  const row = screen.getByText("Bot 0").closest("article")!;
  fireEvent.click(within(row).getByRole("button", { name: "Select" }));
  await waitFor(() => expect(requests).toContain("/api/v1/admin/rooms/123456/participants/bot-0/select"));
  expect(screen.getByRole("button", { name: "Start Run" })).toBeDisabled();
});

it("validates target boundaries and sends a selected roster Run without autofill", async () => {
  window.history.replaceState({}, "", "/admin/rooms/123456");
  const selected = { ...room, selected_count: 4, participants: room.participants.map(p => ({ ...p, selected: true })) };
  let startBody: unknown;
  stubFetch((path, init) => {
    if (path.endsWith("/runs")) { startBody = JSON.parse(String(init?.body)); return response({ ...selected, benchmark: { run_id: "run-1", target: 1000, completed: 0, stop_requested: false, status: "running" } }); }
    if (path === "/api/v1/admin/rooms") return response([selected]);
    if (path === "/api/v1/admin/rooms/123456") return response(selected);
    if (path === "/api/v1/admin/tokens") return response([]);
    return response({});
  });
  render(<App />);
  const target = await screen.findByRole("spinbutton", { name: "Matches in Run" });
  expect(target).toHaveAttribute("min", "1");
  expect(target).toHaveAttribute("max", "1000");
  fireEvent.change(target, { target: { value: "0" } });
  expect(screen.getByRole("button", { name: "Start Run" })).toBeDisabled();
  for (const value of ["1001", "1.5"]) {
    fireEvent.change(target, { target: { value } });
    expect(screen.getByRole("button", { name: "Start Run" })).toBeDisabled();
  }
  fireEvent.change(target, { target: { value: "1000" } });
  fireEvent.click(screen.getByRole("button", { name: "Start Run" }));
  await waitFor(() => expect(startBody).toEqual({ target: 1000 }));
});

it("keeps interrupted history read-only with null statistics and no fabricated graph points", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  stubFetch(() => response(run));
  render(<App />);
  expect(await screen.findByText("server restarted")).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent("interrupted");
  expect(screen.getByText("No completed Matches yet.")).toBeVisible();
  expect(screen.queryByRole("img", { name: /cumulative score/i })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Start Run" })).not.toBeInTheDocument();
});

it("derives charts from persisted evidence and opens the identified Match without unverified Replay navigation", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  const matches = [1, 2, 3].map(sequence => ({ match_id: `m${sequence}`, sequence, results: run.roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: [40000, 30000, 20000, 10000][seat] })) }));
  stubFetch(() => response({ ...run, status: "completed", completed: 3, matches, statistics: [] }));
  render(<App />);
  const table = await screen.findByRole("table", { name: "Participant statistics" }, { timeout: 20000 });
  const rows = within(table).getAllByRole("row");
  expect(rows).toHaveLength(5);
  expect(rows[1]).toHaveTextContent("Same bot (p0)");
  expect(rows[1]).toHaveTextContent("1.00");
  expect(rows[1]).toHaveTextContent("100.0%");
  expect(rows[1]).toHaveTextContent("45,000");
  expect(screen.getByRole("heading", { name: "Cumulative net score" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "View Match 2 details" }));
  expect(await screen.findByRole("dialog")).toHaveTextContent("m2");
  expect(screen.queryByRole("link", { name: "Open Replay" })).not.toBeInTheDocument();
}, 30000);

it("recovers a private observer read without joining a participant or submitting actions", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let reads = 0;
  stubFetch(path => {
    if (!path.endsWith("/live")) return response({ ...run, status: "running", reason: null, failed_match_id: null });
    if (++reads === 1) return response({ detail: "Observer connection lost" }, 503);
    return response(live);
  });
  render(<App />);
  await screen.findByText("Live table — all current hands");
  expect(await screen.findByRole("alert")).toHaveTextContent("Observer connection lost");
  fireEvent.click(screen.getByRole("button", { name: "Refresh table" }));
  expect(await screen.findByText(/Read-only Admin view/)).toBeVisible();
  const paths = vi.mocked(fetch).mock.calls.map(([path]) => String(path));
  expect(paths.some(path => path.endsWith("/join") || path.endsWith("/actions"))).toBe(false);
});

it("purges private snapshots on sign-out before back navigation can reuse them", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let signedOut = false;
  vi.stubGlobal("fetch", vi.fn(async (path: string) => {
    if (path.endsWith("/logout")) { signedOut = true; return response({}); }
    if (signedOut) return new Promise<Response>(() => {}); // unauthorized read has not resolved yet
    return response(path.endsWith("/live") ? live : { ...run, status: "running" });
  }));
  render(<App />);
  await screen.findByText(/Read-only Admin view/);
  fireEvent.click(screen.getByText("Current hands as text"));
  expect(screen.getByText(/Bot 0 \(p0\):/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
  await screen.findByRole("heading", { name: "Sign in to host." });
  window.history.pushState({}, "", "/admin/benchmark/runs/run-1");
  fireEvent.popState(window);
  expect(screen.queryByText("Live table — all current hands")).not.toBeInTheDocument();
  expect(screen.queryByText(/Bot 0 \(p0\):/)).not.toBeInTheDocument();
});

it("waits for a fresh authorized observer read before rendering a cached hand on reentry", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let holdLiveRead = false;
  vi.stubGlobal("fetch", vi.fn(async (path: string) => {
    if (path.endsWith("/live")) return holdLiveRead ? new Promise<Response>(() => {}) : response(live);
    return response(path === "/api/v1/admin/benchmark/runs" ? [{ ...run, status: "running" }] : { ...run, status: "running" });
  }));
  render(<App />);
  await screen.findByText(/Read-only Admin view/);
  fireEvent.click(within(screen.getByRole("navigation", { name: "Admin" })).getByRole("link", { name: "Benchmarks" }));
  await screen.findByRole("heading", { name: "Benchmarks" });
  holdLiveRead = true;
  fireEvent.click(await screen.findByRole("link", { name: /^123456/ }));
  await screen.findByText("Live table — all current hands");
  expect(await screen.findByText("Connecting to live table…")).toBeVisible();
  expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument();
});

it("removes private snapshots and leaves the Admin surface after a live read returns 401", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let expired = false;
  stubFetch(path => {
    if (!path.endsWith("/live")) return response({ ...run, status: "running" });
    return expired ? response({ detail: "Admin session expired" }, 401) : response(live);
  });
  render(<App />);
  const summary = await screen.findByText("Live table — all current hands");
  await screen.findByText(/Read-only Admin view/);
  expired = true;
  fireEvent.click(summary);
  await waitFor(() => expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument());
  fireEvent.click(summary);
  expect(await screen.findByRole("heading", { name: "Sign in to host." })).toBeVisible();
  expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument();
});

it("locally pages and filters loaded history without aggressive index polling", async () => {
  const runs = Array.from({ length: 27 }, (_, i) => ({ ...run, run_id: `run-${i}`, room_code: String(123456 + i), status: i === 26 ? "stopped" : "interrupted" }));
  stubFetch(() => response(runs));
  render(<App />);
  const table = await screen.findByRole("table", { name: "Loaded Run history" });
  expect(within(table).getAllByRole("row")).toHaveLength(26);
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  expect(within(table).getAllByRole("row")).toHaveLength(3);
  fireEvent.click(screen.getByRole("combobox", { name: "Status" }));
  const stopped = await screen.findByRole("option", { name: "stopped" });
  fireEvent.pointerDown(stopped, { pointerType: "mouse" }); fireEvent.click(stopped);
  expect(within(table).getAllByRole("row")).toHaveLength(2);
  expect(screen.getByRole("link", { name: /^123482/ })).toBeVisible();
  fireEvent.change(screen.getByRole("searchbox", { name: "Search loaded Runs" }), { target: { value: "missing" } });
  expect(screen.getByText("No matching loaded Runs.")).toBeVisible();
});

it("keeps an out-of-range local page empty after Refresh without inverted ranges", async () => {
  let runs = Array.from({ length: 26 }, (_, i) => ({ ...run, run_id: `r${i}` }));
  stubFetch(() => response(runs));
  render(<App />);
  await screen.findByRole("table", { name: "Loaded Run history" });
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  runs = runs.slice(0, 25);
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  expect(await screen.findByText("No matching loaded Runs.")).toBeVisible();
  expect(screen.getByText("0–0 of 25")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
  expect(screen.getByRole("table", { name: "Loaded Run history" })).toHaveTextContent("r0");
});

it.each([true, false])("restores authoritative Stop state on mobile (pending=%s), independent of the observer", async pending => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  stubFetch(path => response(path.endsWith("/live") ? { ...live, projection: null } : path === "/api/v1/admin/rooms/123456"
    ? { ...room, benchmark: { run_id: "run-1", target: 3, completed: 0, stop_requested: pending, status: "running" } }
    : { ...run, status: "running" }));
  render(<App />);
  await screen.findByRole("heading", { name: "Benchmark Run" });
  const stop = await screen.findByRole("button", { name: "Stop after current Match" });
  await waitFor(() => pending ? expect(stop).toBeDisabled() : expect(stop).toBeEnabled());
  if (pending) expect(screen.getByText("Stopping after current Match")).toBeVisible();
});

it("confirms Stop and recovers a lost response by reading matching ordinary Room detail", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let accepted = false;
  let posts = 0;
  stubFetch((path, init) => {
    if (path.endsWith("/live")) return response({ ...live, projection: null });
    if (path.endsWith("/stop")) { ++posts; accepted = true; return response({ detail: "Response lost" }, 503); }
    if (path === "/api/v1/admin/rooms/123456") return response({ ...room, benchmark: { run_id: "run-1", target: 3, completed: 0, stop_requested: accepted, status: "running" } });
    expect(init?.method).not.toBe("POST");
    return response({ ...run, status: "running" });
  });
  render(<App />);
  const stop = await screen.findByRole("button", { name: "Stop after current Match" });
  fireEvent.click(stop);
  expect(posts).toBe(0);
  const confirm = await screen.findByRole("alertdialog");
  expect(confirm).toHaveTextContent("Finish current Match, then stop");
  fireEvent.click(within(confirm).getByRole("button", { name: "Confirm Stop" }));
  expect(await screen.findByText("Stopping after current Match")).toBeVisible();
  expect(posts).toBe(1);
  expect(stop).toBeDisabled();
  expect(screen.getByRole("link", { name: "Room controls" })).toBeEnabled();
});

it("keeps uncertain Stop disabled when recovery fails and never treats a different Run as acceptance", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let attempted = false;
  stubFetch(path => {
    if (path.endsWith("/live")) return response({ ...live, projection: null });
    if (path.endsWith("/stop")) { attempted = true; return response({ ...room, benchmark: { run_id: "other", stop_requested: true, status: "running" } }); }
    if (path === "/api/v1/admin/rooms/123456") return attempted ? response({ detail: "Read failed" }, 503) : response({ ...room, benchmark: { run_id: "run-1", stop_requested: false, status: "running" } });
    return response({ ...run, status: "running" });
  });
  render(<App />);
  const stop = await screen.findByRole("button", { name: "Stop after current Match" });
  fireEvent.click(stop);
  fireEvent.click(await screen.findByRole("button", { name: "Confirm Stop" }));
  expect(await screen.findByText(/Stop state unknown/)).toBeVisible();
  expect(screen.queryByText("Stopping after current Match")).not.toBeInTheDocument();
  expect(stop).toBeDisabled();
});

it.each(["/admin/benchmark/runs/run-1", "/admin/rooms/123456"])("rechecks open Stop confirmation after refreshed pending, unknown or terminal state at %s", async route => {
  window.history.replaceState({}, "", route);
  let state = "eligible";
  let reads = 0;
  let posts = 0;
  stubFetch((path, init) => {
    if (init?.method === "POST") posts++;
    if (path.endsWith("/live")) return response({ ...live, projection: null });
    const currentRoom = { ...room, revision: reads, benchmark: { run_id: "run-1", stop_requested: state === "unknown" ? undefined : state === "pending", status: state === "terminal" ? "stopped" : "running" } };
    if (path === "/api/v1/admin/rooms") return response([currentRoom]);
    if (path.endsWith("/tokens")) return response([]);
    if (path === "/api/v1/admin/rooms/123456") { reads++; return response(currentRoom); }
    return response({ ...run, status: state === "terminal" ? "stopped" : "running" });
  });
  render(<App />);
  for (const next of ["pending", "unknown", "terminal"]) {
    fireEvent.click(await screen.findByRole("button", { name: "Stop after current Match" }));
    const confirm = await screen.findByRole("button", { name: "Confirm Stop" });
    state = next;
    const previous = reads;
    act(() => { focusManager.setFocused(false); focusManager.setFocused(true); });
    if (next !== "terminal" || !route.includes("/benchmark/")) await waitFor(() => expect(reads).toBeGreaterThan(previous));
    await waitFor(() => next === "terminal" ? expect(confirm).not.toBeInTheDocument() : expect(confirm).toBeDisabled());
    fireEvent.click(confirm);
    expect(posts).toBe(0);
    if (next !== "terminal") {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      state = "eligible";
      act(() => { focusManager.setFocused(false); focusManager.setFocused(true); });
      await waitFor(() => expect(screen.getByRole("button", { name: "Stop after current Match" })).toBeEnabled());
      if (route.includes("/benchmark/")) await waitFor(() => expect(screen.getByRole("button", { name: "Refresh Run" })).toBeEnabled());
    }
  }
  focusManager.setFocused(undefined);
});

it.each(["/admin/benchmark/runs/run-1", "/admin/rooms/123456"].flatMap(route => ["pending", "failed", "revoked", "old-revoked", "mismatched", "left"].map(outcome => [route, outcome])))("does not recover a lost Stop using a pre-outcome snapshot at %s (%s)", async (route, outcome) => {
  window.history.replaceState({}, "", route);
  let posts = 0;
  let reads = 0;
  let releaseOld!: (value: Response) => void;
  let releaseFresh!: (value: Response) => void;
  const snapshot = (pending: boolean) => response({ ...room, benchmark: { run_id: "run-1", status: "running", stop_requested: pending } });
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    if (input.endsWith("/live")) return response({ ...live, projection: null });
    if (input.endsWith("/stop")) { posts++; return response({ detail: "Response lost" }, 503); }
    if (input === "/api/v1/admin/rooms/123456") {
      reads++;
      if (reads === 2) return new Promise<Response>(resolve => { releaseOld = resolve; });
      if (reads === 3) return new Promise<Response>(resolve => { releaseFresh = resolve; });
      return snapshot(false);
    }
    if (input === "/api/v1/admin/rooms") return response([room]);
    if (input.endsWith("/tokens")) return response([]);
    return response({ ...run, status: "running" });
  }));
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Stop after current Match" }));
  act(() => { focusManager.setFocused(false); focusManager.setFocused(true); });
  await waitFor(() => expect(reads).toBe(2));
  fireEvent.click(screen.getByRole("button", { name: "Confirm Stop" }));
  await screen.findByText(/Stop state unknown/);
  if (outcome === "left") {
    fireEvent.click(within(screen.getByRole("navigation", { name: "Admin" })).getByRole("link", { name: "Replays" }));
    await act(async () => { releaseOld(snapshot(false)); });
    expect(reads).toBe(2);
    expect(screen.queryByRole("button", { name: "Stop after current Match" })).not.toBeInTheDocument();
  } else if (outcome === "old-revoked") {
    await act(async () => { releaseOld(response({ detail: "Session expired" }, 401)); });
    await screen.findByRole("heading", { name: "Sign in to host." });
    expect(reads).toBe(2);
  } else {
    await act(async () => { releaseOld(snapshot(false)); });
    await waitFor(() => expect(reads).toBe(3));
    expect(screen.getByRole("button", { name: "Stop after current Match" })).toBeDisabled();
    expect(screen.getByText(/Stop state unknown/)).toBeVisible();
    await act(async () => { releaseFresh(outcome === "failed" ? response({ detail: "Read failed" }, 503)
      : outcome === "revoked" ? response({ detail: "Session expired" }, 401)
      : outcome === "mismatched" ? response({ ...room, benchmark: { run_id: "other", status: "running", stop_requested: false } }) : snapshot(true)); });
    if (outcome === "revoked") await screen.findByRole("heading", { name: "Sign in to host." });
    else if (outcome === "pending") expect(await screen.findByText("Stopping after current Match")).toBeVisible();
    else if (outcome === "mismatched" && route.startsWith("/admin/rooms/")) {
      expect(screen.queryByText("Stopping after current Match")).not.toBeInTheDocument();
      await waitFor(() => expect(screen.getByRole("link", { name: "View Run and live table" })).toHaveAttribute("href", "/admin/benchmark/runs/other"));
    } else {
      await waitFor(() => expect(screen.getByRole("button", { name: "Stop after current Match" })).toBeDisabled());
      expect(screen.getByText(/Stop state unknown/)).toBeVisible();
    }
  }
  expect(posts).toBe(1);
  focusManager.setFocused(undefined);
});

it("shows an honest unavailable Resume for Interrupted only", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  stubFetch(() => response(run));
  render(<App />);
  expect(await screen.findByRole("button", { name: "Resume unavailable" })).toBeDisabled();
  expect(screen.getByText(/server does not publish safe Resume eligibility/)).toBeVisible();
});

it("requires a fresh explicit Stop intent after conflict and never retries the POST blindly", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let posts = 0;
  stubFetch(path => {
    if (path.endsWith("/live")) return response({ ...live, projection: null });
    if (path.endsWith("/stop")) { posts++; return response({ detail: "Changed" }, 409); }
    if (path === "/api/v1/admin/rooms/123456") return response({ ...room, benchmark: { run_id: "run-1", stop_requested: false, status: "running" } });
    return response({ ...run, status: "running" });
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Stop after current Match" }));
  fireEvent.click(await screen.findByRole("button", { name: "Confirm Stop" }));
  expect(await screen.findByText(/Run changed. Review refreshed state and confirm a new Stop intent/)).toBeVisible();
  expect(posts).toBe(1);
  expect(screen.queryByText("Stopping after current Match")).not.toBeInTheDocument();
});

it("uses bounded exact-Match metadata retry without downloading Replay or changing persisted results", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  const match = { match_id: "m1", sequence: 1, results: run.roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: [40000, 30000, 20000, 10000][seat] })) };
  let reads = 0;
  const paths: string[] = [];
  stubFetch(path => {
    paths.push(path);
    if (path === "/api/v1/admin/replays") {
      if (++reads === 1) return response({ detail: "Metadata failed" }, 503);
      return response({ replays: [{ match_id: "m1", availability: "available", replay_available: true }], offset: 0, limit: 50, total: 1, has_more: false });
    }
    return response({ ...run, status: "completed", completed: 1, matches: [match] });
  });
  render(<App />);
  const table = await screen.findByRole("table", { name: "Participant statistics" }, { timeout: 20000 });
  expect(paths).not.toContain("/api/v1/admin/replays");
  fireEvent.click(screen.getByRole("button", { name: "View Match 1 details" }));
  const detail = await screen.findByRole("dialog");
  expect(await within(detail).findByText(/Replay status not verified/)).toBeVisible();
  expect(screen.queryByRole("link", { name: "Open Replay" })).not.toBeInTheDocument();
  fireEvent.click(within(detail).getByRole("button", { name: "Retry Replay status" }));
  expect(await within(detail).findByRole("link", { name: "Open Replay" })).toHaveAttribute("href", "/admin/replays/m1");
  fireEvent.click(within(detail).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(within(table).getAllByRole("row")[1]).toHaveTextContent("15,000"));
  expect(paths.filter(path => path.includes("replays"))).toEqual(["/api/v1/admin/replays", "/api/v1/admin/replays"]);
}, 30000);

it("leaves the Admin surface on ordinary detail authorization loss", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  stubFetch(path => path === "/api/v1/admin/rooms/123456" ? response({ detail: "Expired" }, 401) : response(path.endsWith("/live") ? { ...live, projection: null } : { ...run, status: "running" }));
  render(<App />);
  expect(await screen.findByRole("heading", { name: "Sign in to host." })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop after current Match" })).not.toBeInTheDocument();
});

it("exposes request failures with an explicit retry action", async () => {
  stubFetch(() => response({ detail: "Host unavailable" }, 503));
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Host unavailable");
  expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
});
