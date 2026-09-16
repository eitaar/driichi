import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";

vi.mock("./pixi-vignette", () => ({
  TileVignette: () => <div data-testid="tile-vignette" aria-hidden="true" />,
}));

function setPath(path: string) {
  window.history.replaceState({}, "", path);
}

function response(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": status >= 400 ? "application/problem+json" : "application/json" },
  });
}

describe("entry shell", () => {
  beforeEach(() => {
    setPath("/");
    vi.stubGlobal("fetch", vi.fn());
  });

  it("keeps the room action visible and keyboard reachable", async () => {
    render(<App />);

    await waitFor(() => expect(screen.getByRole("heading", { name: /your table is live/i })).toBeVisible());
    expect(screen.getByRole("textbox", { name: /room code/i })).toHaveFocus();
    await waitFor(() => expect(screen.getByRole("button", { name: /open room/i })).toBeVisible());
    expect(screen.getByRole("link", { name: /admin sign in/i })).toBeVisible();
  });

  it("uses a static entry mode when reduced motion is requested", () => {
    const matchMedia = window.matchMedia as unknown as ReturnType<typeof vi.fn>;
    matchMedia.mockImplementation((query: string) => ({
      matches: query === "(prefers-reduced-motion: reduce)",
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));

    render(<App />);

    expect(screen.getByTestId("entry-shell")).toHaveAttribute("data-motion", "static");
  });
});

describe("public room join", () => {
  beforeEach(() => {
    setPath("/room/123456");
  });

  it("renders lookup loading and then the exact public room fields", async () => {
    let resolveLookup!: (value: Response) => void;
    const lookup = new Promise<Response>((resolve) => {
      resolveLookup = resolve;
    });
    vi.stubGlobal("fetch", vi.fn().mockReturnValueOnce(lookup));

    render(<App />);
    expect(screen.getByText(/loading room/i)).toBeVisible();

    resolveLookup(response({
      room_name: "Night Market",
      game_mode: "4p-red-east",
      phase: "Lobby",
      join_allowed: true,
      participant_count: 1,
      participant_limit: 4,
    }));

    expect(await screen.findByRole("heading", { name: /join night market/i })).toBeVisible();
    expect(screen.getByText("4p-red-east")).toBeVisible();
    expect(screen.getByText("1 / 4 participants")).toBeVisible();
  });

  it("preserves problem details in an actionable error state", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(response({
        type: "about:blank",
        title: "Room not found",
        status: 404,
        detail: "The requested Room does not exist.",
        code: "room_not_found",
        request_id: "01TESTREQUESTID000000000000",
      }, 404)),
    );

    render(<App />);

    expect(await screen.findByRole("heading", { name: /room unavailable/i })).toBeVisible();
    expect(screen.getByText("The requested Room does not exist.")).toBeVisible();
    expect(screen.getByRole("link", { name: /try another room/i })).toHaveAttribute("href", "/");
  });

  it("retries a failed character request before enabling the join flow", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn()
        .mockResolvedValueOnce(response({ room_name: "Night Market", game_mode: "4p-red-east", phase: "Lobby", join_allowed: true, participant_count: 1, participant_limit: 4 }))
        .mockResolvedValueOnce(response({ title: "Service unavailable", detail: "Characters could not be loaded.", code: "characters_unavailable" }, 503))
        .mockResolvedValueOnce(response({ room_name: "Night Market", game_mode: "4p-red-east", phase: "Lobby", join_allowed: true, participant_count: 1, participant_limit: 4 }))
        .mockResolvedValueOnce(response([{ id: "player-red", name: "Red Player" }])),
    );

    render(<App />);
    await screen.findByRole("heading", { name: /join night market/i });
    expect(await screen.findByRole("alert")).toHaveTextContent("Characters could not be loaded.");
    fireEvent.click(screen.getByRole("button", { name: /retry character list/i }));
    expect(await screen.findByRole("radio", { name: /red player/i })).toBeVisible();
  });

  it("joins with nickname and selected character without exposing the guest credential", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn()
        .mockResolvedValueOnce(response({
          room_name: "Night Market",
          game_mode: "4p-red-east",
          phase: "Lobby",
          join_allowed: true,
          participant_count: 1,
          participant_limit: 4,
        }))
        .mockResolvedValueOnce(response([
          { id: "player-red", name: "Red Player" },
          { id: "tsumogiri-bot", name: "Tsumogiri" },
        ]))
        .mockResolvedValueOnce(response({
          participant_id: "01PARTICIPANT",
          websocket_url: "/ws/v1/rooms/123456/human",
        }, 201)),
    );

    render(<App />);
    await screen.findByRole("heading", { name: /join night market/i });
    fireEvent.change(screen.getByRole("textbox", { name: /display name/i }), {
      target: { value: "Mika" },
    });
    const redPlayer = await screen.findByRole("radio", { name: /red player/i });
    fireEvent.click(redPlayer);
    expect(redPlayer).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: /join room/i }));

    expect(await screen.findByRole("heading", { name: /you're in night market/i })).toBeVisible();
    expect(screen.queryByText(/websocket|cookie|token/i)).not.toBeInTheDocument();
    const calls = vi.mocked(fetch).mock.calls;
    expect(calls.at(-1)?.[0]).toBe("/api/v1/rooms/123456/join");
    expect(JSON.parse(String(calls.at(-1)?.[1] && (calls.at(-1)?.[1] as RequestInit).body))).toEqual({
      nickname: "Mika",
      character_id: "player-red",
    });
  });
});

describe("admin login", () => {
  beforeEach(() => {
    setPath("/admin/login");
  });

  it("shows an inline Problem Details error and keeps password content out of the page", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(response({
        type: "about:blank",
        title: "Unauthorized",
        status: 401,
        detail: "The username or password is invalid.",
        code: "invalid_credentials",
        request_id: "01TESTREQUESTID000000000000",
      }, 401)),
    );

    render(<App />);
    const password = screen.getByLabelText(/password/i);
    expect(password).toHaveFocus();
    fireEvent.change(screen.getByLabelText(/username/i), { target: { value: "admin" } });
    fireEvent.change(password, { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: /sign in/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("The username or password is invalid.");
    expect(screen.queryByText("secret")).not.toBeInTheDocument();
  });

  it("renders a non-dashboard success state after login", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(response({ expires_at: "2026-09-16T00:00:00Z" })),
    );

    render(<App />);
    fireEvent.change(screen.getByLabelText(/username/i), { target: { value: "admin" } });
    fireEvent.change(screen.getByLabelText(/password/i), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: /sign in/i }));

    expect(await screen.findByRole("heading", { name: /admin session active/i })).toBeVisible();
    expect(screen.getByRole("link", { name: /open admin/i })).toHaveAttribute("href", "/admin");
  });
});

describe("admin room workspace", () => {
  beforeEach(() => {
    setPath("/admin");
    vi.stubGlobal("fetch", vi.fn());
  });

  it("renders the room list and authoritative detail with a flat task workspace", async () => {
    const detail = {
      join_code: "123456",
      room_name: "Night Market",
      game_mode: "4p-red-east",
      phase: "lobby",
      connected_count: 1,
      participant_count: 1,
      selected_count: 1,
      created_at: "2026-09-16T00:00:00Z",
      participants: [{ participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" }],
      match_players: [],
      roster: [],
      result: null,
      revision: 3,
      persistence_degraded: false,
      replay_available: true,
    };
    vi.mocked(fetch).mockImplementation((input) => {
      const path = String(input);
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response(detail));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([detail]));
      if (path.endsWith("/characters/human")) return Promise.resolve(response([{ id: "player-red", name: "Red Player" }]));
      return Promise.resolve(response([]));
    });

    render(<App />);

    expect(await screen.findByRole("heading", { name: /admin rooms/i })).toBeVisible();
    expect(await screen.findByRole("link", { name: /night market/i })).toBeVisible();
    expect(await screen.findByRole("heading", { name: /night market/i })).toBeVisible();
    expect(screen.getByText(/identity/i)).toBeVisible();
    expect(screen.getByText(/presence/i)).toBeVisible();
    expect(screen.getByText(/selection/i)).toBeVisible();
    expect(screen.getByText(/controller/i)).toBeVisible();
  });

  it("runs a Room mutation and refetches the exact Room detail key", async () => {
    const detail = {
      join_code: "123456", room_name: "Night Market", game_mode: "4p-red-east", phase: "lobby",
      connected_count: 0, participant_count: 0, selected_count: 0, created_at: "2026-09-16T00:00:00Z",
      participants: [], match_players: [], roster: [], result: null, revision: 1, persistence_degraded: false, replay_available: true,
    };
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/rooms/123456/fill-with-bots")) return Promise.resolve(response({ ...detail, selected_count: 4, revision: 2 }));
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response(detail));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([detail]));
      return Promise.resolve(response([]));
    });

    render(<App />);
    await screen.findByRole("heading", { name: /night market/i });
    fireEvent.click(screen.getByRole("button", { name: /fill with bots/i }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/admin/rooms/123456/fill-with-bots",
      expect.objectContaining({ method: "POST" }),
    ));
    await waitFor(() => expect(fetchMock.mock.calls.filter(([input]) => String(input).endsWith("/admin/rooms/123456")).length).toBeGreaterThan(1));
  });

  it("keeps a created Bot Token one-time and asks before revocation", async () => {
    const list = [{ token_id: "tok_1", name: "runner", state: "active", created_at: "2026-09-16T00:00:00Z", revoked_at: null }];
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/tokens") && (!init || !init.method || init.method === "GET")) return Promise.resolve(response(list));
      if (path.endsWith("/admin/tokens") && init?.method === "POST") return Promise.resolve(response({ ...list[0], token: "driichi_secret_once" }, 201));
      if (path.endsWith("/admin/tokens/tok_1/revoke")) return Promise.resolve(response({ ...list[0], state: "revoked" }));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });

    render(<App />);
    await screen.findByRole("heading", { name: /bot tokens/i });
    fireEvent.change(screen.getByRole("textbox", { name: /token name/i }), { target: { value: "runner" } });
    fireEvent.click(screen.getByRole("button", { name: /create token/i }));
    expect(await screen.findByText("driichi_secret_once")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /close token/i }));
    expect(screen.queryByText("driichi_secret_once")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /revoke runner/i }));
    expect(await screen.findByRole("dialog")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /confirm revoke/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/admin/tokens/tok_1/revoke",
      expect.objectContaining({ method: "POST" }),
    ));
  });
});

describe("human lobby websocket", () => {
  beforeEach(() => {
    setPath("/room/123456/lobby");
    sessionStorage.setItem("driichi:participant:123456", "P1");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("", { status: 200 })));
  });

  it("renders authoritative snapshots, separated identity axes, and a three-seat roster", async () => {
    class FakeWebSocket {
      static OPEN = 1;
      readyState = 0;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: (() => void) | null = null;
      sent: string[] = [];
      constructor(public url: string) { setTimeout(() => { this.readyState = FakeWebSocket.OPEN; this.onopen?.(); }, 0); }
      send(value: string) { this.sent.push(value); }
      close() { this.onclose?.({ code: 1000, reason: "client_closed" } as CloseEvent); }
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    const socket = vi.fn((url: string) => new FakeWebSocket(url));
    vi.stubGlobal("WebSocket", socket);
    render(<App />);
    const instance = await waitFor(() => {
      expect(socket).toHaveBeenCalled();
      return socket.mock.results[0]?.value as FakeWebSocket;
    });
    instance.emit({
      type: "snapshot",
      room: {
        join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 7,
        participants: [
          { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
          { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
          { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
        ], match_players: [], roster: [], result: null,
      },
      state: null,
    });
    expect(await screen.findByRole("heading", { name: /night market lobby/i })).toBeVisible();
    expect(screen.getByText(/your participant/i)).toBeVisible();
    expect(screen.getByText(/presence/i)).toBeVisible();
    expect(screen.getByText(/selection/i)).toBeVisible();
    expect(screen.getByText(/controller/i)).toBeVisible();
    expect(screen.getByText(/3 seats/i)).toBeVisible();
  });

  it("does not send Ready until selected Character assets finish preloading", async () => {
    class FakeWebSocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose: (() => void) | null = null;
      onerror: (() => void) | null = null;
      sent: string[] = [];
      constructor(public url: string) { setTimeout(() => this.onopen?.(), 0); }
      send(value: string) { this.sent.push(value); }
      close() {}
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    const instance = new FakeWebSocket("ws://test");
    vi.stubGlobal("WebSocket", vi.fn(() => instance));
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(new Blob(["asset"]), { status: 200 })));
    render(<App />);
    instance.emit({ type: "snapshot", room: { join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 1, participants: [
      { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
      { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
      { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
    ], match_players: [], roster: [], result: null }, state: null });
    const ready = await screen.findByRole("button", { name: /set ready/i });
    expect(ready).toBeDisabled();
    await waitFor(() => expect(ready).toBeEnabled());
    fireEvent.click(ready);
    expect(JSON.parse(instance.sent.at(-1) ?? "{}")).toEqual({ type: "set_ready", preloaded_characters: ["player-red", "tsumogiri-bot", "tsumogiri-bot"] });
  });
});
