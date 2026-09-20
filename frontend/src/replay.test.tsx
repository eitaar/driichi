import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";

const mockPlayVoices = vi.hoisted(() => vi.fn());
const mockDecodeCharacterAsset = vi.hoisted(() => vi.fn());
vi.mock("./game/audio", () => ({
  AudioManager: class {
    settings = { master: 1, sfx: 1, voice: 1, voiceEnabled: true };
    playVoices = mockPlayVoices;
    unlock = vi.fn(() => Promise.resolve());
    destroy = vi.fn();
  },
}));
vi.mock("./game/assets", () => ({ decodeCharacterAsset: mockDecodeCharacterAsset }));
vi.mock("./game/three-table", () => ({
  ThreeTable: ({ projection, portraitEffect, surface }: { projection: { players?: unknown[] }; portraitEffect?: { characterId: string } | null; surface?: string }) => (
    <div data-testid="replay-three-table" data-portrait={portraitEffect?.characterId ?? "none"} data-surface={surface}>{projection.players?.length ?? 0} players</div>
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
  players: [0, 1, 2, 3].map((seat) => ({ participant_id: `P${seat}`, display_name: `Seat ${seat}`, participant_kind: "built_in_bot", seat, character_id: "ordinary-pack", final_points: 25000 })),
  frames: [
    frame(0, "start_game", [{ event: { Disconnected: { seat: 1 } }, line_index: 0, phase: "before", sequence: 1 }]),
    frame(1, "start_kyoku", [{ event: { Reconnected: { seat: 1 } }, line_index: 1, phase: "after", sequence: 2 }]),
    frame(2, "end_game"),
  ],
};

describe("Replay Admin workspace", () => {
  beforeEach(() => {
    mockPlayVoices.mockReset();
    mockDecodeCharacterAsset.mockReset().mockResolvedValue(undefined);
    window.history.replaceState({}, "", "/admin/replays");
    vi.stubGlobal("fetch", vi.fn().mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/replays") && (!init || !init.method)) return Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    }));
  });

  it("renders shared four- and three-player overlays from scene positions", async () => {
    const { TablePlayerOverlay } = await vi.importActual<typeof import("./game/three-table")>(
      "./game/three-table",
    );
    const players = [0, 1, 2, 3].map((seat) => ({
      seat,
      participant_id: `P${seat}`,
      display_name: seat === 0 ? "Mika" : `Seat ${seat}`,
      score: 25_000 - seat * 1_000,
      hand: seat === 0 ? [0, 1, 2] : undefined,
      concealed_count: 3,
      discards: [],
      melds: [],
      riichi: seat === 1,
    }));
    const overlayRoom = {
      roster: [{ participant_id: "P0", character_id: "ordinary-pack" }],
      match_players: [],
    } as never;
    const view = render(
      <TablePlayerOverlay
        projection={{ mode: "4p-red-east", audience: "replay_admin", players }}
        room={overlayRoom}
        surface="replay"
      />,
    );

    expect(view.container.querySelectorAll(".table-player-frame")).toHaveLength(4);
    expect(view.container.querySelector('[data-position="top"]')).not.toBeNull();
    expect(view.container.querySelector('[data-position="bottom"]')).toHaveTextContent("Mika");
    expect(view.container.querySelector('[data-position="bottom"]')).toHaveTextContent("25,000");
    expect(view.container.querySelector('[data-position="right"]')).toHaveTextContent("Riichi");
    expect(screen.getByAltText("Mika portrait")).toHaveAttribute(
      "src",
      "/assets/characters/ordinary-pack/portrait.webp",
    );

    view.rerender(
      <TablePlayerOverlay
        projection={{ mode: "3p-red-east", audience: "replay_admin", players }}
        room={overlayRoom}
        surface="replay"
      />,
    );
    expect(view.container.querySelectorAll(".table-player-frame")).toHaveLength(3);
    expect(view.container.querySelector('[data-position="top"]')).toBeNull();
    expect(view.container).not.toHaveTextContent("Seat 3");
  });

  it("lists newest replays with View and Delete actions", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: /replay library/i })).toBeVisible();
    expect(await screen.findByText("Night Market")).toBeVisible();
    expect(screen.getByRole("link", { name: /view replay match15/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /delete replay match15/i })).toBeVisible();
    expect(screen.queryByText(/download mjson/i)).not.toBeInTheDocument();
  });

  it("keeps pagination in the URL and follows browser-sized pages", async () => {
    const fetchMock = vi.mocked(fetch);
    window.history.replaceState({}, "", "/admin/replays?offset=50");
    fetchMock.mockImplementation((input, init) => {
      const url = new URL(String(input), window.location.origin);
      if (url.pathname === "/api/v1/admin/replays" && (!init || !init.method)) {
        const offset = Number(url.searchParams.get("offset") ?? 0);
        return Promise.resolve(response({
          replays: offset === 50 ? [{ ...summary, match_id: "MATCH51" }] : [summary],
          offset,
          limit: 50,
          total: 51,
          has_more: offset === 0,
        }));
      }
      if (url.pathname.endsWith("/admin/tokens") || url.pathname.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    expect(await screen.findByText("MATCH51")).toBeVisible();
    expect(fetchMock.mock.calls.some(([input]) => input === "/api/v1/admin/replays?offset=50&limit=50")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: /previous/i }));
    await waitFor(() => expect(window.location.search).toBe(""));
    expect(await screen.findByText("Night Market")).toBeVisible();
  });

  it("offers a working retry when the replay list request fails", async () => {
    const fetchMock = vi.mocked(fetch);
    let attempts = 0;
    fetchMock.mockImplementation((input, init) => {
      const url = new URL(String(input), window.location.origin);
      if (url.pathname === "/api/v1/admin/replays" && (!init || !init.method)) {
        attempts += 1;
        return attempts === 1
          ? Promise.reject(new Error("temporary failure"))
          : Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      }
      if (url.pathname.endsWith("/admin/tokens") || url.pathname.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    expect(await screen.findByRole("heading", { name: /replay library unavailable/i })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /^retry$/i }));
    expect(await screen.findByText("Night Market")).toBeVisible();
    expect(attempts).toBe(2);
  });

  it("clamps a deleted later page to the previous replay page", async () => {
    const fetchMock = vi.mocked(fetch);
    let deleted = false;
    window.history.replaceState({}, "", "/admin/replays?offset=50");
    fetchMock.mockImplementation((input, init) => {
      const url = new URL(String(input), window.location.origin);
      if (url.pathname === "/api/v1/admin/replays" && (!init || !init.method)) {
        const offset = Number(url.searchParams.get("offset") ?? 0);
        if (offset === 50 && !deleted) return Promise.resolve(response({ replays: [{ ...summary, match_id: "MATCH51" }], offset, limit: 50, total: 51, has_more: false }));
        return Promise.resolve(response({ replays: offset === 0 ? [summary] : [], offset, limit: 50, total: deleted ? 50 : 51, has_more: false }));
      }
      if (url.pathname.endsWith("/admin/replays/MATCH51") && init?.method === "DELETE") {
        deleted = true;
        return Promise.resolve(new Response(null, { status: 204 }));
      }
      if (url.pathname.endsWith("/admin/tokens") || url.pathname.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /delete replay match51/i }));
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: /confirm delete/i }));
    expect(await screen.findByText("Night Market")).toBeVisible();
    await waitFor(() => expect(window.location.search).toBe(""));
    expect(screen.queryByText("No Replays yet.")).not.toBeInTheDocument();
  });

  it("uses loaded Room assets for voice and portrait effects", async () => {
    const fetchMock = vi.mocked(fetch);
    const horaReplay = {
      ...replay,
      frames: [
        replay.frames[0],
        { ...replay.frames[1], visible_event: { type: "hora", actor: 0, target: 0, han: 5, fu: 30 } },
      ],
    };
    fetchMock.mockImplementation((input, init) => {
      const url = new URL(String(input), window.location.origin);
      if (url.pathname === "/api/v1/admin/replays" && (!init || !init.method)) return Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      if (url.pathname.endsWith("/admin/replays/MATCH15")) return Promise.resolve(response(horaReplay));
      if (url.pathname.endsWith("/admin/tokens") || url.pathname.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("link", { name: /view replay match15/i }));
    expect(await screen.findByText("ROOM ASSETS")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /next event/i }));
    await waitFor(() => expect(mockPlayVoices).toHaveBeenCalledWith([{ characterId: "ordinary-pack", kind: "tsumo" }]));
    expect(screen.getByTestId("replay-three-table")).toHaveAttribute("data-portrait", "ordinary-pack");
  });

  it("switches to generic silent playback when a Room pack cannot load", async () => {
    mockDecodeCharacterAsset.mockRejectedValue(new Error("asset_unavailable"));
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockImplementation((input, init) => {
      const url = new URL(String(input), window.location.origin);
      if (url.pathname === "/api/v1/admin/replays" && (!init || !init.method)) return Promise.resolve(response({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }));
      if (url.pathname.endsWith("/admin/replays/MATCH15")) return Promise.resolve(response(replay));
      if (url.pathname.endsWith("/admin/tokens") || url.pathname.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("link", { name: /view replay match15/i }));
    expect(await screen.findByText("GENERIC / SILENT")).toBeVisible();
    expect(screen.getByTestId("replay-three-table")).toHaveAttribute("data-portrait", "none");
    expect(mockPlayVoices).not.toHaveBeenCalled();
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
    fireEvent.click((await screen.findByRole("link", { name: /view replay match15/i })));
    expect(await screen.findByRole("heading", { name: /night market replay/i })).toBeVisible();
    expect(screen.getByTestId("replay-three-table")).toHaveTextContent("4 players");
    expect(screen.getByTestId("replay-three-table")).toHaveAttribute("data-surface", "replay");
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
    expect(screen.getByRole("link", { name: /back to replay library/i })).toBeVisible();
  });
});
