import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BenchmarkObserver as Observer } from "./benchmark-observer";
import { clearAdminSession } from "./admin-session";
import { deriveRunAnalytics, type RunAnalytics } from "./benchmark-analytics";

const emptyAnalytics = deriveRunAnalytics({ game_mode: "4p-red-east", completed: 0, matches: [], roster: Array.from({ length: 4 }, (_, initial_seat) => ({ participant_id: `p${initial_seat}`, display_name: `Bot ${initial_seat}`, initial_seat, participant_kind: "built_in_bot", character_id: null })) });

const progress = { run_id: "run-1", target: 3, completed: 0, stop_requested: false, status: "running" };
const snapshot = (revision = 7) => ({ revision, benchmark: progress, projection: {
  audience: "benchmark_admin", mode: "4p-red-east", viewer_seat: 0, dora_indicators: [],
  players: Array.from({ length: 4 }, (_, seat) => ({ seat, participant_id: `p${seat}`, display_name: `Bot ${seat}`, score: 25000 + seat, hand: [seat * 4], discards: [], melds: [] })),
} });
function response(data: unknown, status = 200) { return new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json" } }); }
async function flush() { await act(async () => { vi.advanceTimersByTime(0); for (let i = 0; i < 10; i++) await Promise.resolve(); }); }
async function advance(ms: number) { await act(async () => { vi.advanceTimersByTime(ms); }); await flush(); }
function mount(analytics: RunAnalytics = emptyAnalytics) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(<QueryClientProvider client={client}><Observer runId="run-1" roomCode="123456" analytics={analytics} /></QueryClientProvider>);
  return { ...view, client };
}
beforeEach(() => {
  vi.useFakeTimers(); vi.setSystemTime(new Date("2026-10-01T00:00:00Z"));
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it.each([emptyAnalytics, { kind: "invalid", reason: "Invalid persisted records." } as const])("never derives Run standings from live hands when persisted analytics are $kind", async analytics => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  mount(analytics); await flush();
  const leaderboard = screen.getByRole("region", { name: "Run leaderboard" });
  expect(leaderboard).toHaveTextContent(analytics.kind === "invalid" ? "Invalid persisted records." : "No completed Matches yet.");
  expect(within(leaderboard).queryByRole("list")).not.toBeInTheDocument();
  expect(screen.getByText("--:--:-- elapsed")).toBeVisible();
  const match = screen.getByRole("region", { name: "Current Match" });
  expect(within(match).getByText("Dealer").nextElementSibling).toHaveTextContent("Unknown");
  expect(within(match).getByText("Round").nextElementSibling).toHaveTextContent("Unknown");
});

it("orders persisted Run standings by average rank then stable ID, not current Match scores", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  if (emptyAnalytics.kind !== "ready") throw new Error("Invalid fixture");
  const participants = emptyAnalytics.participants.map((p, i) => ({ ...p, averageRank: i < 2 ? 1.5 : 3.5, firstPlaceRate: i < 2 ? 0.5 : 0, cumulativeNet: i < 2 ? 1500 : -1500 }));
  mount({ kind: "ready", completed: 2, provisional: true, participants: participants.reverse() }); await flush();
  const rows = within(screen.getByRole("region", { name: "Run leaderboard" })).getAllByRole("row").slice(1);
  expect(rows[0]).toHaveTextContent("Bot 0");
  expect(rows[1]).toHaveTextContent("Bot 1");
  expect(within(rows[0]).getAllByRole("cell").map(cell => cell.textContent)).toEqual(["Bot 0", "1.50", "50.0%", "1,500"]);
  expect(screen.getByRole("region", { name: "Current Match" })).toBeVisible();
});

it("maps Current Match facts, dealer and points by authoritative Seat without exposing identity keys", async () => {
  const current = { ...snapshot(), projection: { ...snapshot().projection, round: "East", kyoku: 2, dealer: 2, honba: 1, kyotaku: 3, remaining_wall: 42,
    players: snapshot().projection.players.map(p => ({ ...p, seat: (p.seat + 1) % 4 })) } };
  vi.stubGlobal("fetch", vi.fn(async () => response(current)));
  mount(); await flush();
  const match = screen.getByRole("region", { name: "Current Match" });
  expect(within(match).getByText("Dealer").nextElementSibling).toHaveTextContent("Bot 1");
  expect(match).toHaveTextContent("East 2");
  const rows = within(match).getAllByRole("row").slice(1);
  expect(within(rows[0]).getAllByRole("cell").map(cell => cell.textContent)).toEqual(["Bot 0", "2", "25,000"]);
  expect(document.body.textContent).not.toMatch(/\bp[0-4]\b/);
  fireEvent.click(screen.getByRole("button", { name: "Details" })); await flush();
  fireEvent.click(screen.getByText("Current hands as text"));
  expect(document.body.textContent).not.toMatch(/\bp[0-4]\b/);
});

it("closes private observation without replacing its fullscreen host and reopens without cached hands", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  mount(); await flush();
  const host = screen.getByRole("group", { name: "3D mahjong table" }).closest(".benchmark-observer");
  fireEvent.click(screen.getByRole("button", { name: "Details" })); await flush();
  fireEvent.click(screen.getByRole("button", { name: "Close observation" })); await flush();
  expect(document.querySelector(".benchmark-observer")).toBe(host);
  expect(screen.queryByRole("group", { name: "3D mahjong table" })).not.toBeInTheDocument();
  await advance(10000); expect(fetch).toHaveBeenCalledTimes(1);
  vi.mocked(fetch).mockImplementation(() => new Promise<Response>(() => {}));
  fireEvent.click(screen.getByRole("button", { name: "Open observation" })); await flush();
  expect(screen.getByText("Connecting to live table…")).toBeVisible();
  expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
});

it("coalesces refresh intents without overlapping private reads and reports Polling, never Live",  async () => {
  let release!: (response: Response) => void;
  vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { release = resolve; })));
  mount(); await flush();
  fireEvent.click(screen.getByRole("button", { name: "Refresh table" }));
  fireEvent.click(screen.getByRole("button", { name: "Refresh table" }));
  await advance(15000);
  expect(fetch).toHaveBeenCalledTimes(1);
  await act(async () => release(response(snapshot()))); await flush();
  expect(fetch).toHaveBeenCalledTimes(2);
  await act(async () => release(response(snapshot(8)))); await flush();
  fireEvent.click(screen.getByRole("button", { name: "Details" })); await flush();
  expect(screen.getByText(/Polling · Fresh/)).toBeVisible();
  expect(screen.queryByText(/^Live$/)).not.toBeInTheDocument();
});

it("discloses routine connection metadata without renewing or remounting observation", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  mount(); await flush();
  const table = screen.getByRole("group", { name: "3D mahjong table" });
  const host = table.closest(".benchmark-observer");
  const details = screen.getByRole("button", { name: "Details" });
  expect(screen.getByRole("heading", { name: "Observation" })).toBeVisible();
  expect(screen.getByText("--:--:-- elapsed")).toBeVisible();
  expect(details).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText(/Revision 7/)).not.toBeInTheDocument();
  expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument();
  expect(screen.queryByText(/Last successful update:/)).not.toBeInTheDocument();
  fireEvent.click(details); await flush();
  expect(screen.getByText(/Last successful update:/)).toHaveTextContent("2026-10-01T00:00:00.000Z");
  expect(screen.getByText(/Revision 7.*GET compatibility mode; SSE unavailable/)).toBeVisible();
  fireEvent.click(details); await flush();
  expect(details).toHaveAttribute("aria-expanded", "false");
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("group", { name: "3D mahjong table" })).toBe(table);
  expect(table.closest(".benchmark-observer")).toBe(host);
  expect(screen.queryByText(/Polling · Fresh/)).not.toBeInTheDocument();
});

it("only successful current authorized reads renew freshness, including unchanged revisions", async () => {
  let fail = false;
  vi.stubGlobal("fetch", vi.fn(async () => fail ? response({ detail: "Offline" }, 503) : response(snapshot())));
  mount(); await flush();
  fireEvent.click(screen.getByRole("button", { name: "Details" })); await flush();
  expect(screen.getByText(/Polling · Fresh/)).toBeVisible();
  fail = true; await advance(5000);
  expect(screen.getByText(/Polling · Stale/)).toBeVisible();
  expect(screen.getByText(/Last successful update:/)).toHaveTextContent("00:00:00");
  fail = false; await advance(5000);
  expect(screen.getByText(/Polling · Fresh/)).toBeVisible();
  expect(screen.getByText(/Last successful update:/)).toHaveTextContent("00:00:10");
  vi.mocked(fetch).mockImplementation(() => new Promise<Response>(() => {}));
  await advance(45000);
  expect(screen.getByText(/Polling · Stale/)).toBeVisible();
});

it.each(["run", "revision", "audience", "duplicate Participant"]) ("rejects a wrong %s snapshot without restoring private current data", async kind => {
  let next: unknown = snapshot();
  vi.stubGlobal("fetch", vi.fn(async () => response(next)));
  mount(); await flush();
  next = kind === "run" ? { ...snapshot(8), benchmark: { ...progress, run_id: "other" } }
    : kind === "revision" ? snapshot(6) : kind === "audience" ? { ...snapshot(8), projection: { ...snapshot().projection, audience: "public" } }
    : { ...snapshot(8), projection: { ...snapshot().projection, players: snapshot().projection.players.map(p => ({ ...p, participant_id: "same" })) } };
  await advance(5000);
  expect(screen.getByText(/Polling · Stale/)).toBeVisible();
  expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
});

it("keeps Participant focus through Seat rotation and waiting without remounting the table host", async () => {
  let next: unknown = snapshot();
  vi.stubGlobal("fetch", vi.fn(async () => response(next)));
  mount(); await flush();
  const table = screen.getByRole("group", { name: "3D mahjong table" });
  fireEvent.click(screen.getByLabelText("Focus Participant")); await flush();
  const option = screen.getByRole("option", { name: "Bot 1 · Seat 2" });
  const host = table.closest(".benchmark-observer");
  expect(host).toContainElement(option);
  fireEvent.pointerDown(option, { pointerType: "mouse" }); fireEvent.click(option); await flush();
  expect(table.querySelector('[data-position="bottom"]')).toHaveTextContent("Bot 1");
  expect(within(screen.getByRole("group", { name: "Your concealed hand" })).getByRole("img")).toHaveAttribute("alt", "2m");
  next = { ...snapshot(8), projection: null }; await advance(5000);
  expect(screen.getByText("Waiting for next Match.")).toBeVisible();
  expect(table.querySelector(".table-replay-hand")).toBeNull();
  const rotated = snapshot(9); rotated.projection.players = rotated.projection.players.map(p => ({ ...p, seat: (p.seat + 1) % 4 }));
  next = rotated; await advance(5000);
  expect(screen.getByRole("group", { name: "3D mahjong table" })).toBe(table);
  expect(table.closest(".benchmark-observer")).toBe(host);
  expect(table.querySelector('[data-position="bottom"]')).toHaveTextContent("Bot 1");
  expect(screen.getByText(/Focused: Bot 1 · Seat 3/)).toBeVisible();
  expect(vi.mocked(fetch).mock.calls.every(([, init]) => !init?.method || init.method === "GET")).toBe(true);
});

it.each(["3p-red-east", "ThreePlayerRedEast"])("resolves %s focus without creating a fourth Seat", async mode => {
  const three = { ...snapshot(), projection: { ...snapshot().projection, mode, players: snapshot().projection.players.slice(0, 3) } };
  vi.stubGlobal("fetch", vi.fn(async () => response(three)));
  mount(); await flush();
  fireEvent.click(screen.getByLabelText("Focus Participant")); await flush();
  const option = screen.getByRole("option", { name: "Bot 2 · Seat 3" });
  fireEvent.pointerDown(option, { pointerType: "mouse" }); fireEvent.click(option); await flush();
  expect(screen.getByText("Focused: Bot 2 · Seat 3")).toBeVisible();
  const table = screen.getByRole("group", { name: "3D mahjong table" });
  expect(table.querySelector('[data-position="bottom"]')).toHaveTextContent("Bot 2");
  expect(table.querySelector('[data-position="top"]')).toBeNull();
  expect(table).toHaveAttribute("data-player-frame-count", "3");
});

it("failed waiting-state renewal remains Stale, rather than implying current waiting evidence", async () => {
  let fail = false;
  vi.stubGlobal("fetch", vi.fn(async () => fail ? response({ detail: "Offline" }, 503) : response({ ...snapshot(), projection: null })));
  mount(); await flush();
  expect(screen.getByText(/Polling · Waiting/)).toBeVisible();
  fail = true; await advance(5000);
  expect(screen.getByText(/Polling · Stale/)).toBeVisible();
});

it("ignores an old Run/Room response after a new observer lifetime starts", async () => {
  const releases: Array<(response: Response) => void> = [];
  vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { releases.push(resolve); })));
  const { rerender, client } = mount(); await flush();
  rerender(<QueryClientProvider client={client}><Observer runId="run-2" roomCode="654321" analytics={emptyAnalytics} /></QueryClientProvider>);
  await flush();
  expect(vi.mocked(fetch).mock.calls[0][1]?.signal?.aborted).toBe(true);
  await act(async () => releases[0](response(snapshot()))); await flush();
  expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
  await act(async () => releases[1](response({ ...snapshot(1), benchmark: { ...progress, run_id: "run-2" } }))); await flush();
  fireEvent.click(screen.getByRole("button", { name: "Details" })); await flush();
  expect(screen.getByText(/Polling · Fresh/)).toBeVisible();
});

it("terminal evidence removes concealed hands and stops renewal without a Live claim", async () => {
  let ended = false;
  vi.stubGlobal("fetch", vi.fn(async () => response(ended ? { ...snapshot(8), benchmark: { ...progress, status: "completed" } } : snapshot())));
  mount(); await flush(); ended = true; await advance(5000);
  expect(screen.getByText(/Polling · completed/)).toBeVisible();
  expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
  await advance(60000); expect(fetch).toHaveBeenCalledTimes(2);
});

it("restores fullscreen trigger focus after denied-fullscreen in-page expansion without remount", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  mount(); await flush();
  const table = screen.getByRole("group", { name: "3D mahjong table" });
  const trigger = screen.getByRole("button", { name: "Fullscreen table" });
  trigger.focus(); fireEvent.click(trigger); await flush();
  expect(screen.getByText(/Browser fullscreen unavailable/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Exit fullscreen" })); await flush();
  expect(screen.getByRole("button", { name: "Fullscreen table" })).toHaveFocus();
  expect(screen.getByRole("group", { name: "3D mahjong table" })).toBe(table);
});

it("keeps expanded observation open when Escape reaches its host while Select is open", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  mount(); await flush();
  fireEvent.click(screen.getByRole("button", { name: "Fullscreen table" })); await flush();
  const trigger = screen.getByLabelText("Focus Participant");
  fireEvent.click(trigger); await flush();
  expect(screen.getByRole("listbox")).toBeVisible();
  trigger.focus();
  fireEvent.keyDown(document.querySelector(".benchmark-observer")!, { key: "Escape" }); await flush();
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Exit fullscreen" })).toBeVisible();
  expect(trigger).toHaveFocus();
  fireEvent.keyDown(trigger, { key: "Escape" }); await flush();
  expect(screen.getByRole("button", { name: "Fullscreen table" })).toHaveFocus();
});

it.each([
  { native: false, closed: false }, { native: false, closed: true },
  { native: true, closed: false }, { native: true, closed: true },
])("restores enabled focus after fullscreen exit (native=$native, closed=$closed)", async ({ native, closed }) => {
  vi.stubGlobal("fetch", vi.fn(async () => response(snapshot())));
  mount(); await flush();
  const table = screen.getByRole("group", { name: "3D mahjong table" });
  const host = table.closest(".benchmark-observer") as HTMLDivElement;
  const trigger = screen.getByRole("button", { name: "Fullscreen table" });
  let fullscreenElement: Element | null = null;
  const originalFullscreen = Object.getOwnPropertyDescriptor(document, "fullscreenElement");
  if (native) {
    Object.defineProperty(document, "fullscreenElement", { configurable: true, get: () => fullscreenElement });
    host.requestFullscreen = vi.fn(async () => {
      fullscreenElement = host;
      document.dispatchEvent(new Event("fullscreenchange"));
    });
  }
  try {
    trigger.focus(); fireEvent.click(trigger); await flush();
    expect(host).toHaveAttribute("data-expanded", "true");
    fireEvent.click(screen.getByRole("button", { name: "Details" })); await flush();
    if (closed) {
      fireEvent.click(screen.getByRole("button", { name: "Close observation" })); await flush();
      expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
    }
    const triggerFocus = vi.spyOn(trigger, "focus");
    if (native) {
      act(() => { fullscreenElement = null; document.dispatchEvent(new Event("fullscreenchange")); });
    } else {
      fireEvent.click(screen.getByRole("button", { name: "Exit fullscreen" }));
    }
    await flush();
    const target = screen.getByRole("button", { name: closed ? "Open observation" : "Fullscreen table" });
    expect(target).toBeEnabled();
    expect(target).toHaveFocus();
    expect(host).toHaveAttribute("data-expanded", "false");
    expect(document.querySelector(".benchmark-observer")).toBe(host);
    if (closed) {
      expect(trigger).toBeDisabled();
      expect(triggerFocus).not.toHaveBeenCalled();
      await advance(10000); expect(fetch).toHaveBeenCalledTimes(1);
    } else {
      expect(screen.getByRole("group", { name: "3D mahjong table" })).toBe(table);
      expect(fetch).toHaveBeenCalledTimes(1);
    }
  } finally {
    if (native) {
      if (originalFullscreen) Object.defineProperty(document, "fullscreenElement", originalFullscreen);
      else Reflect.deleteProperty(document, "fullscreenElement");
    }
  }
});

it.each([401, 403])("authorization failure %s purges private observation and stops renewal", async status => {
  let denied = false;
  vi.stubGlobal("fetch", vi.fn(async () => denied ? response({ detail: "Denied" }, status) : response(snapshot())));
  mount(); await flush();
  expect(screen.getByRole("group", { name: "Your concealed hand" })).toBeVisible();
  denied = true; await advance(5000);
  expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Current Match" })).toHaveTextContent("No current Match.");
  await advance(60000); expect(fetch).toHaveBeenCalledTimes(2);
});

it("session clearing aborts reads and immediately purges local private state and renewal work", async () => {
  let release!: (response: Response) => void;
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(response(snapshot())).mockImplementation(() => new Promise<Response>(resolve => { release = resolve; })));
  const { client } = mount(); await flush(); await advance(5000);
  const signal = vi.mocked(fetch).mock.calls.at(-1)?.[1]?.signal;
  act(() => clearAdminSession(client));
  expect(screen.queryByText(/Read-only Admin view/)).not.toBeInTheDocument();
  expect(signal?.aborted).toBe(true);
  await act(async () => release(response(snapshot(8)))); await advance(60000);
  expect(fetch).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("group", { name: "Your concealed hand" })).not.toBeInTheDocument();
});
