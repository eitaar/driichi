import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";

vi.mock("./game/pixi-table", () => ({
  PixiTable: ({ projection }: { projection: { players?: unknown[] } }) => (
    <div data-testid="replay-pixi-table">{projection.players?.length ?? 0} players</div>
  ),
}));

function response(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": status >= 400 ? "application/problem+json" : "application/json" },
  });
}

const summary = {
  match_id: "MATCH15",
  source: "room",
  room_name: "Night Market",
  game_mode: "4p-red-east",
  started_at: "2026-09-16T00:00:00Z",
  completed_at: "2026-09-16T00:10:00Z",
  file_size: 512,
  availability: "available",
  replay_available: true,
};

const frame = (index: number, event: string, auxiliary_events: Array<Record<string, unknown>> = []) => ({
  event_index: index,
  visible_event: { type: event },
  visible_state: {
    audience: "replay_admin",
    mode: "FourPlayerRedEast",
    round: "East",
    kyoku: index < 2 ? 1 : 2,
    players: [0, 1, 2, 3].map((seat) => ({ seat, participant_id: `P${seat}`, display_name: `Seat ${seat}`, kind: "BuiltInBot", score: 25000, hand: [0, 1, 2], concealed_count: 3, discards: [], melds: [], riichi: false })),
    dora_indicators: [0],
    decision: null,
  },
  auxiliary_events,
});

const replay = {
  ...summary,
  players: [0, 1, 2, 3].map((seat) => ({ participant_id: `P${seat}`, display_name: `Seat ${seat}`, participant_kind: "built_in_bot", seat, character_id: "missing-pack", final_points: 25000 })),
  frames: [
    frame(0, "start_game", [{ event: { Disconnected: { seat: 1 } }, line_index: 0, phase: "before", sequence: 1 }]),
    frame(1, "start_kyoku", [{ event: { Reconnected: { seat: 1 } }, line_index: 1, phase: "after", sequence: 2 }]),
    frame(2, "end_game"),
  ],
};

describe("Replay Admin workspace", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/admin/replays");
    vi.stubGlobal("fetch", vi.fn().mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/replays") && (!init || !init.method)) return Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    }));
  });

  it("lists newest replays with View and Delete actions", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: /replay library/i })).toBeVisible();
    expect(await screen.findByText("Night Market")).toBeVisible();
    expect(screen.getByRole("button", { name: /view replay match15/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /delete replay match15/i })).toBeVisible();
    expect(screen.queryByText(/download mjson/i)).not.toBeInTheDocument();
  });

  it("uses server frames, pauses on navigation, and exposes auxiliary ordering", async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/replays") && (!init || !init.method)) return Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      if (path.endsWith("/admin/replays/MATCH15")) return Promise.resolve(response(replay));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    fireEvent.click((await screen.findByRole("button", { name: /view replay match15/i })));
    expect(await screen.findByRole("heading", { name: /night market replay/i })).toBeVisible();
    expect(screen.getByTestId("replay-pixi-table")).toHaveTextContent("4 players");
    expect(screen.getByRole("button", { name: /^play$/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /previous event/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /next event/i })).toBeVisible();
    expect(screen.getByRole("button", { name: "0.5x" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: "1x" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("combobox", { name: /jump to kyoku/i })).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent(/disconnected/i);
    const eventLog = screen.getByRole("log", { name: /replay event log/i });
    expect(within(eventLog).getAllByRole("listitem")[0]).toHaveTextContent(/before.*disconnected.*start game/i);
    fireEvent.click(screen.getByRole("button", { name: /^play$/i }));
    expect(screen.getByRole("button", { name: /^pause$/i })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /next event/i }));
    expect(screen.getByRole("button", { name: /^play$/i })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "2x" }));
    expect(screen.getByRole("button", { name: "2x" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.change(screen.getByRole("combobox", { name: /jump to kyoku/i }), { target: { value: "2" } });
    expect(screen.getByRole("button", { name: /^play$/i })).toBeVisible();
  });

  it("confirms safe deletion and refreshes the list", async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/replays") && (!init || !init.method)) return Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      if (path.endsWith("/admin/replays/MATCH15") && init?.method === "DELETE") return Promise.resolve(new Response(null, { status: 204 }));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /delete replay match15/i }));    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/delete this replay/i)).toBeVisible();
    fireEvent.click(within(dialog).getByRole("button", { name: /confirm delete/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/replays/MATCH15", expect.objectContaining({ method: "DELETE" })));
  });

  it("keeps unavailable and oversized states actionable", async () => {
    window.history.replaceState({}, "", "/admin/replays/MATCH15");
    vi.stubGlobal("fetch", vi.fn().mockImplementation((input) => {
      const path = String(input);
      if (path.endsWith("/admin/replays/MATCH15")) return Promise.resolve(response({ code: "replay_too_large", detail: "The Replay timeline exceeds the maximum size." }, 413));
      return Promise.resolve(response([]));
    }));
    render(<App />);
    expect(await screen.findByRole("heading", { name: /replay unavailable/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /back to replay library/i })).toBeVisible();
  });
});
