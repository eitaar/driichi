import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { App } from "./app";

const room = {
  join_code: "123456", room_name: "Bot benchmark", game_mode: "4p-red-east", phase: "lobby", benchmark_mode: true, benchmark: null,
  selected_count: 0, connected_count: 4, participant_count: 4, created_at: "2026-09-29T00:00:00Z",
  time_control: "casual", replay_save: true, participant_limit: 32,
  participants: Array.from({ length: 4 }, (_, id) => ({ participant_id: `bot-${id}`, display_name: `Bot ${id}`, kind: "built_in_bot", presence: "connected", selected: false, ready: false, character_id: "tsumogiri-bot", role: "none", controller: "interactive" })),
  match_players: [], roster: [], result: null, revision: 1, persistence_degraded: false, replay_available: true,
};
const run = {
  run_id: "run-1", room_code: "123456", game_mode: "4p-red-east", target: 3, completed: 0, status: "interrupted", reason: "server restarted", failed_match_id: "aborted", roster: [], matches: [], statistics: [],
};
const live = { revision: 5, benchmark: null, projection: { audience: "benchmark_admin", mode: "4p-red-east", round: "east", kyoku: 1, dora_indicators: [0], players: Array.from({ length: 4 }, (_, seat) => ({ seat, participant_id: `p${seat}`, display_name: `Bot ${seat}`, score: 25000, hand: Array(13).fill(0), concealed_count: 13, discards: [], melds: [], riichi: false })), decision: null } };
function response(data: unknown, status = 200) { return new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json" } }); }
function stubFetch(handler: (path: string, init?: RequestInit) => Response) {
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => handler(String(input), init)));
}
beforeEach(() => { window.history.replaceState({}, "", "/admin/benchmark"); });

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

it("shows stored rank/rate and centered series separately for same-name participants", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  const statistics = [
    { participant_id: "p0", display_name: "Same bot", average_rank: 7 / 3, first_place_rate: 1 / 3, cumulative_net_scores: [15000, 0, 5000] },
    { participant_id: "p1", display_name: "Same bot", average_rank: 2, first_place_rate: 1 / 3, cumulative_net_scores: [5000, 0, 5000] },
  ];
  stubFetch(() => response({ ...run, status: "completed", completed: 3, statistics }));
  render(<App />);
  const table = await screen.findByRole("table", { name: "Participant statistics" });
  const rows = within(table).getAllByRole("row");
  expect(rows).toHaveLength(3);
  expect(within(rows[1]).getByText("p0")).toBeVisible();
  expect(within(rows[1]).getByText("2.33")).toBeVisible();
  expect(within(rows[1]).getByText("33.3%")).toBeVisible();
  expect(within(rows[2]).getByText("p1")).toBeVisible();
  expect(screen.getByRole("img", { name: "Cumulative score relative to table average" })).toBeVisible();
});

it("recovers a private observer read without joining a participant or submitting actions", async () => {
  window.history.replaceState({}, "", "/admin/benchmark/runs/run-1");
  let reads = 0;
  stubFetch(path => {
    if (!path.endsWith("/live")) return response({ ...run, status: "running", reason: null, failed_match_id: null });
    if (++reads === 1) return response({ detail: "Observer connection lost" }, 503);
    return response(live);
  });
  render(<App />);
  fireEvent.click(await screen.findByText("Live table — all current hands"));
  expect(await screen.findByRole("alert")).toHaveTextContent("Observer connection lost");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
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
  fireEvent.click(await screen.findByText("Live table — all current hands"));
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
  fireEvent.click(await screen.findByText("Live table — all current hands"));
  await screen.findByText(/Read-only Admin view/);
  fireEvent.click(screen.getByRole("link", { name: "Benchmarks" }));
  await screen.findByRole("heading", { name: "Benchmarks" });
  holdLiveRead = true;
  fireEvent.click(await screen.findByRole("link", { name: /^123456/ }));
  fireEvent.click(await screen.findByText("Live table — all current hands"));
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
  fireEvent.click(summary);
  await screen.findByText(/Read-only Admin view/);
  expired = true;
  fireEvent.click(summary);
  await waitFor(() => expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument());
  fireEvent.click(summary);
  expect(await screen.findByRole("heading", { name: "Sign in to host." })).toBeVisible();
  expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument();
});

it("exposes request failures with an explicit retry action", async () => {
  stubFetch(() => response({ detail: "Host unavailable" }, 503));
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Host unavailable");
  expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
});
