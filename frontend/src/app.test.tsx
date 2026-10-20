import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";
import { api, ApiProblem } from "./api";
import { navigate } from "./routes";
import { useGameStore } from "./game/store";

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

afterEach(() => { vi.restoreAllMocks(); });

describe("entry shell", () => {
  beforeEach(() => {
    setPath("/");
    vi.stubGlobal("fetch", vi.fn());
  });

  it("keeps the room action visible and keyboard reachable", async () => {
    render(<App />);

    await waitFor(() => expect(screen.getByRole("heading", { name: /join a room/i })).toBeVisible());
    expect(screen.getByRole("textbox", { name: /room code/i })).toHaveFocus();
    await waitFor(() => expect(screen.getByRole("button", { name: /open room/i })).toBeVisible());
    expect(screen.getByRole("link", { name: /admin sign in/i })).toBeVisible();
  });

  it("missing_room_stays_on_entry", async () => {
    vi.mocked(fetch).mockImplementation(input => Promise.resolve(String(input).endsWith('/characters/human') ? response([]) : response({ detail: 'Missing' }, 404)));
    render(<App />);
    const code = screen.getByLabelText('Room code');
    fireEvent.change(code, { target: { value: '123456' } });
    const historyLength = history.length;
    fireEvent.submit(code.closest('form')!);
    expect(await screen.findByText('Room not found. Check the code and try again.')).toHaveAttribute('id', 'room-code-error');
    expect(code).toHaveAttribute('aria-describedby', 'room-code-error');
    expect(code).toHaveValue('123456');
    expect(location.pathname).toBe('/');
    expect(history.length).toBe(historyLength);
    fireEvent.change(code, { target: { value: '123457' } });
    expect(screen.queryByText('Room not found. Check the code and try again.')).not.toBeInTheDocument();
  });

  it("transport_failure_is_not_missing_room", async () => {
    const lookup = vi.spyOn(api, 'lookupRoom').mockRejectedValueOnce(new ApiProblem({ detail: 'Unavailable' }, 503)).mockRejectedValueOnce(new Error('offline'));
    render(<App />);
    fireEvent.change(screen.getByLabelText('Room code'), { target: { value: '123456' } });
    fireEvent.click(screen.getByRole('button', { name: 'Open room' }));
    expect(await screen.findByText('Could not check the room. Try again.')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Open room' }));
    await waitFor(() => expect(lookup).toHaveBeenCalledTimes(2));
    expect(location.pathname).toBe('/');
    lookup.mockRestore();
  });

  it("pending_lookup_cannot_navigate_after_unmount", async () => {
    let resolve!: (value: Awaited<ReturnType<typeof api.lookupRoom>>) => void;
    const lookup = vi.spyOn(api, 'lookupRoom').mockReturnValue(new Promise(done => { resolve = done; }));
    render(<App />);
    const code = screen.getByLabelText('Room code');
    fireEvent.change(code, { target: { value: '123456' } });
    fireEvent.submit(code.closest('form')!);
    fireEvent.submit(code.closest('form')!);
    expect(lookup).toHaveBeenCalledTimes(1);
    expect(code).toBeDisabled();
    act(() => navigate('/admin/login'));
    await act(async () => resolve({ room_name: 'Night Market' } as Awaited<ReturnType<typeof api.lookupRoom>>));
    expect(location.pathname).toBe('/admin/login');
    lookup.mockRestore();
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
    fireEvent.click(screen.getByText('Room details'));
    expect(screen.getByText(/4p-red-east/)).toBeVisible();
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
    await screen.findByLabelText("Display name");
    fireEvent.change(screen.getByLabelText("Display name"), { target: { value: "Mika" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("Characters could not be loaded.");
    fireEvent.click(screen.getByRole("button", { name: /retry character list/i }));
    await screen.findByLabelText("Display name");
    fireEvent.change(screen.getByLabelText("Display name"), { target: { value: "Mika" } });
    expect(await screen.findByRole("radio", { name: /red player/i })).toBeVisible();
  });

  it("accepted_join_binds_before_lobby", async () => {
    let accept!: (value: Awaited<ReturnType<typeof api.joinHuman>>) => void;
    const join = vi.spyOn(api, 'joinHuman').mockReturnValue(new Promise(resolve => { accept = resolve; }));
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
        ])),
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

    fireEvent.submit(redPlayer.closest('form')!);
    expect(join).toHaveBeenCalledTimes(1);
    expect(join).toHaveBeenCalledWith('123456', 'Mika', 'player-red');
    const binding = vi.spyOn(Storage.prototype, 'setItem');
    let bindingAtPush: string | null | undefined;
    const push = vi.spyOn(history, 'pushState').mockImplementation(() => {
      bindingAtPush = sessionStorage.getItem('driichi:participant:123456');
    });
    await act(async () => accept({ participant_id: '01PARTICIPANT', websocket_url: '/ws/v1/rooms/123456/human' }));
    expect(binding).toHaveBeenCalled();
    expect(push).toHaveBeenCalledWith({}, '', '/room/123456/lobby');
    expect(bindingAtPush).toBe('01PARTICIPANT');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByText(/websocket|cookie|token/i)).not.toBeInTheDocument();
    expect(screen.queryByText('01PARTICIPANT')).not.toBeInTheDocument();
    expect(screen.queryByText('/ws/v1/rooms/123456/human')).not.toBeInTheDocument();
    expect(screen.queryByText(/you're in night market/i)).not.toBeInTheDocument();
    binding.mockRestore();
    push.mockRestore();
    join.mockRestore();
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

  it("accepted_login_clears_fields_then_opens_admin", async () => {
    let accept!: (value: Awaited<ReturnType<typeof api.loginAdmin>>) => void;
    const login = vi.spyOn(api, 'loginAdmin').mockReturnValue(new Promise(resolve => { accept = resolve; }));
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(response({ expires_at: "2026-09-16T00:00:00Z" })),
    );

    render(<App />);
    fireEvent.change(screen.getByLabelText(/username/i), { target: { value: "admin" } });
    fireEvent.change(screen.getByLabelText(/password/i), { target: { value: "secret" } });
    const username = screen.getByLabelText(/username/i) as HTMLInputElement;
    const password = screen.getByLabelText(/password/i) as HTMLInputElement;
    fireEvent.submit(password.closest('form')!);
    fireEvent.submit(password.closest('form')!);
    expect(login).toHaveBeenCalledTimes(1);
    expect(password).toBeDisabled();
    let credentialsAtPush: { username: string; password: string } | undefined;
    const push = vi.spyOn(history, 'pushState').mockImplementation(() => {
      credentialsAtPush = { username: username.value, password: password.value };
    });
    await act(async () => accept({ expires_at: '2026-09-16T00:00:00Z' }));
    expect(push).toHaveBeenCalledWith({}, '', '/admin');
    expect(credentialsAtPush).toEqual({ username: '', password: '' });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByText(/admin session active/i)).not.toBeInTheDocument();
    push.mockRestore();
    login.mockRestore();
  });
});

describe("entry transition safeguards", () => {
  const room = { room_name: 'Night Market', game_mode: '4p-red-east', phase: 'Lobby', join_allowed: true, participant_count: 1, participant_limit: 4 };
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn().mockImplementation(input => Promise.resolve(response(String(input).endsWith('/characters/human') ? [{ id: 'player-red', name: 'Red Player' }] : room))));
    sessionStorage.clear();
  });

  it('accepted_home_lookup_keeps_the_authoritative_route_lookup', async () => {
    setPath('/');
    render(<App />);
    fireEvent.change(screen.getByLabelText('Room code'), { target: { value: '123456' } });
    fireEvent.click(screen.getByRole('button', { name: 'Open room' }));
    await screen.findByLabelText('Display name');
    expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url) === '/api/v1/rooms/123456')).toHaveLength(2);
  });

  it.each(['storage', 'request', 'unmount'])('join_%s_failure_never_opens_lobby', async failure => {
    setPath('/room/123456');
    let accept!: (value: Awaited<ReturnType<typeof api.joinHuman>>) => void;
    let reject!: (error: Error) => void;
    const join = vi.spyOn(api, 'joinHuman').mockReturnValue(new Promise((yes, no) => { accept = yes; reject = no; }));
    render(<App />);
    fireEvent.change(await screen.findByLabelText('Display name'), { target: { value: 'Mika' } });
    fireEvent.click(screen.getByRole('radio', { name: 'Red Player' }));
    fireEvent.click(screen.getByRole('button', { name: 'Join room' }));
    expect(screen.getByLabelText('Display name')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Back' })).toBeDisabled();
    if (failure === 'storage') vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Storage unavailable'); });
    if (failure === 'unmount') act(() => navigate('/'));
    await act(async () => {
      if (failure === 'request') reject(new ApiProblem({ detail: 'Room is full' }, 409));
      else accept({ participant_id: 'P1', websocket_url: '/ws' });
    });
    expect(location.pathname).toBe(failure === 'unmount' ? '/' : '/room/123456');
    expect(sessionStorage.getItem('driichi:participant:123456')).toBeNull();
    if (failure !== 'unmount') {
      expect(screen.getByRole('alert')).toBeVisible();
      expect(screen.getByRole('button', { name: 'Join room' })).toBeEnabled();
      expect(screen.getByLabelText('Display name')).toHaveValue('Mika');
    }
    expect(join).toHaveBeenCalledTimes(1);
  });

  it('blank_login_is_linked_and_never_sent', () => {
    setPath('/admin/login');
    const login = vi.spyOn(api, 'loginAdmin');
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    expect(login).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Username')).toHaveAttribute('aria-describedby', 'admin-username-error');
    expect(screen.getByLabelText('Password')).toHaveAttribute('aria-describedby', 'admin-password-error');
  });

  it('late_login_cannot_navigate_after_unmount', async () => {
    setPath('/admin/login');
    let accept!: (value: Awaited<ReturnType<typeof api.loginAdmin>>) => void;
    vi.spyOn(api, 'loginAdmin').mockReturnValue(new Promise(resolve => { accept = resolve; }));
    render(<App />);
    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'admin' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'secret' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    act(() => navigate('/'));
    await act(async () => accept({ expires_at: '2026-09-16T00:00:00Z' }));
    expect(location.pathname).toBe('/');
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

    expect(await screen.findByRole("heading", { name: /^rooms$/i })).toBeVisible();
    expect(await screen.findByRole("link", { name: /night market/i })).toBeVisible();
    expect(await screen.findByRole("heading", { name: /night market/i })).toBeVisible();
    expect(screen.queryByText(/^identity$/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/^presence$/i)).not.toBeInTheDocument();
    expect(screen.getByText(/selection/i)).toBeVisible();
    expect(screen.getByText(/controller/i)).toBeVisible();
    expect(screen.queryByText(/seat open/i)).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Selected roster' })).not.toBeInTheDocument();
    expect(document.querySelector('.participants-section')).toHaveAttribute('data-slot', 'card');
    expect(document.querySelector('.lifecycle-section')).toHaveAttribute('data-slot', 'card');
    expect(screen.getByRole('button', { name: 'Start Match' })).toBeDisabled();
    fireEvent.click(screen.getByRole('link', { name: /night market/i }));
    expect(window.location.pathname).toBe('/admin/rooms/123456');
    fireEvent.click(screen.getByRole('link', { name: 'Back to Rooms' }));
    expect(window.location.pathname).toBe('/admin');
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
    await screen.findByRole("heading", { name: /choose a room/i });
    fireEvent.click(screen.getByText("Bot tokens"));
    fireEvent.change(screen.getByRole("textbox", { name: /token name/i }), { target: { value: "runner" } });
    fireEvent.click(screen.getByRole("button", { name: /create token/i }));
    expect(await screen.findByText("driichi_secret_once")).toBeVisible();
    const tokenDialog = await screen.findByRole("dialog");
    fireEvent.click(within(tokenDialog).getAllByRole("button", { name: /close token/i })[1]);
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

describe("admin mutation coverage", () => {
  const room = {
    join_code: "123456", room_name: "Night Market", game_mode: "4p-red-east", phase: "lobby", time_control: "casual", replay_save: true, participant_limit: 4,
    connected_count: 1, participant_count: 1, selected_count: 4, created_at: "2026-09-16T00:00:00Z",
    participants: [{ participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: true, character_id: "player-red", role: "player_0", controller: "interactive" }],
    match_players: [{ participant_id: "P1", display_name: "Mika", kind: "human", seat: 0, character_id: "player-red", controller: "interactive" }],
    roster: [{ participant_id: "P1", display_name: "Mika", kind: "human", seat: 0, character_id: "player-red", controller: "interactive" }],
    result: null, revision: 9, persistence_degraded: false, replay_available: true,
  };

  beforeEach(() => setPath("/admin/rooms/123456"));

  it("covers PATCH, rematch, back to Lobby, logout, and modal Escape focus", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/rooms/123456") && init?.method === "PATCH") return Promise.resolve(response(room));
      if (path.endsWith("/admin/rooms/123456/rematch")) return Promise.resolve(response({ ...room, phase: "playing" }));
      if (path.endsWith("/admin/rooms/123456/back-to-lobby")) return Promise.resolve(response({ ...room, phase: "lobby" }));
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response(room));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([room]));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      if (path.endsWith("/admin/logout")) return Promise.resolve(new Response(null, { status: 204 }));
      return Promise.resolve(response([]));
    });
    render(<App />);
    await screen.findByRole("heading", { name: /night market/i });
    fireEvent.click(screen.getByRole("button", { name: /save settings/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456", expect.objectContaining({ method: "PATCH" })));
    fireEvent.click(screen.getByRole("button", { name: /sign out/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/logout", expect.objectContaining({ method: "POST" })));
  });

  it("runs Rematch only for a post-match Room", async () => {
    const fetchMock = vi.fn().mockImplementation((input) => {
      const path = String(input);
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response({ ...room, phase: "post_match" }));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([room]));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      if (path.endsWith("/admin/rooms/123456/rematch")) return Promise.resolve(response({ ...room, phase: "playing" }));
      return Promise.resolve(response([]));
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByRole("heading", { name: /night market/i });
    fireEvent.click(screen.getByRole("button", { name: /rematch/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456/rematch", expect.objectContaining({ method: "POST" })));
    fireEvent.click(screen.getByRole("button", { name: /back to lobby/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456/back-to-lobby", expect.objectContaining({ method: "POST" })));
  });

  it("confirms destructive Room deletion with a focused dialog", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockImplementation((input) => {
      const path = String(input);
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response({ ...room, phase: "lobby" }));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([room]));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    render(<App />);
    await screen.findByRole("heading", { name: /night market/i });
    const deleteButton = screen.getByRole("button", { name: /delete room/i });
    deleteButton.focus();
    fireEvent.click(deleteButton);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeVisible();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /cancel/i }));
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(deleteButton).toHaveFocus();
    fireEvent.click(deleteButton);
    fireEvent.click(await screen.findByRole("button", { name: /confirm delete/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456", expect.objectContaining({ method: "DELETE" })));
  });

  it("covers participant selection, deselection, and match start mutations", async () => {
    let selected = false;
    const participant = { participant_id: "P2", display_name: "Nori", kind: "human", presence: "connected", selected: false, ready: false, character_id: "player-red", role: "none", controller: "interactive" };
    const fetchMock = vi.fn().mockImplementation((input, init) => {
      const path = String(input);
      const bots = [0, 1, 2].map((index) => ({ ...participant, participant_id: `B${index}`, display_name: `Bot ${index}`, kind: "built_in_bot", selected: true, ready: true }));
      const detail = { ...room, selected_count: selected ? 4 : 3, participants: [...bots, { ...participant, selected, ready: selected }] };
      if (path.endsWith("/participants/P2/select")) { selected = true; return Promise.resolve(response(detail)); }
      if (path.endsWith("/participants/P2/deselect")) { selected = false; return Promise.resolve(response(detail)); }
      if (path.endsWith("/start")) return Promise.resolve(response({ ...detail, phase: "playing" }));
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response(detail));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([detail]));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByRole("heading", { name: /night market/i });
    const nori = within(screen.getByText("Nori").closest("article") as HTMLElement);
    expect(nori.queryByText('Identity')).not.toBeInTheDocument();
    expect(nori.queryByText('P2')).not.toBeInTheDocument();
    expect(nori.queryByText('Presence')).not.toBeInTheDocument();
    expect(nori.queryByText('connected')).not.toBeInTheDocument();
    fireEvent.click(nori.getByRole("button", { name: /^select$/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456/participants/P2/select", expect.objectContaining({ method: "POST" })));
    await waitFor(() => expect(nori.getByRole("button", { name: /^deselect$/i })).toBeEnabled());
    fireEvent.click(nori.getByRole("button", { name: /^deselect$/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456/participants/P2/deselect", expect.objectContaining({ method: "POST" })));
    await waitFor(() => expect(nori.getByRole("button", { name: /^select$/i })).toBeEnabled());
    fireEvent.click(nori.getByRole("button", { name: /^select$/i }));
    await waitFor(() => expect(fetchMock.mock.calls.filter(([input]) => String(input).endsWith("/participants/P2/select")).length).toBeGreaterThan(1));
    await waitFor(() => expect(screen.getByRole("button", { name: /start match/i })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: /start match/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456/start", expect.objectContaining({ method: "POST" })));
  });

  it("confirms participant kick and creates rooms with the server participant default", async () => {
    const fetchMock = vi.fn().mockImplementation((input, init) => {
      const path = String(input);
      if (path.endsWith("/admin/rooms") && init?.method === "POST") return Promise.resolve(response(room, 201));
      if (path.endsWith("/admin/rooms/123456")) return Promise.resolve(response({ ...room, participants: [{ ...room.participants[0], selected: false }], selected_count: 3 }));
      if (path.endsWith("/admin/rooms")) return Promise.resolve(response([room]));
      if (path.endsWith("/admin/tokens")) return Promise.resolve(response([]));
      return Promise.resolve(response([]));
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByRole("heading", { name: /night market/i });
    fireEvent.click(screen.getByRole("button", { name: /kick/i }));
    fireEvent.click(await screen.findByRole("button", { name: /confirm kick/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms/123456/participants/P1/kick", expect.objectContaining({ method: "POST" })));
    fireEvent.click(screen.getByText("Create room"));
    fireEvent.click(screen.getByRole("button", { name: /set up room/i }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(/room name/i), { target: { value: "Second Room" } });
    fireEvent.click(within(dialog).getByRole("button", { name: /^create room$/i }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/v1/admin/rooms", expect.objectContaining({ method: "POST" })));
    const createCall = fetchMock.mock.calls.find(([input, init]) => String(input).endsWith("/admin/rooms") && init?.method === "POST");
    expect(JSON.parse(String(createCall?.[1]?.body))).toEqual({
      room_name: "Second Room",
      game_mode: "4p-red-east",
      time_control: "casual",
      replay_save: true,
    });
  });
});

describe("human lobby websocket", () => {
  beforeEach(() => {
    setPath("/room/123456/lobby");
    sessionStorage.setItem("driichi:participant:123456", "P1");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("", { status: 200 })));
    vi.useRealTimers();
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
    const socket = Object.assign(vi.fn(function (url: string) { return new FakeWebSocket(url); }), { OPEN: FakeWebSocket.OPEN });
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
        ],
        match_players: [
          { participant_id: "P1", display_name: "Mika", kind: "human", seat: 0, character_id: "player-red", controller: "interactive" },
          { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", seat: 1, character_id: "tsumogiri-bot", controller: "permanent_auto_built_in_bot" },
          { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", seat: 2, character_id: "tsumogiri-bot", controller: "permanent_auto_built_in_bot" },
        ], roster: [], result: null,
      },
      state: null,
    });
    expect(await screen.findByRole("heading", { name: /^lobby$/i })).toBeVisible();
    const cards = () => within(screen.getByRole('list', { name: 'Participants' })).getAllByRole('button').map(button => button.getAttribute('aria-label'));
    expect(cards()).toEqual(['Bot 1', 'Bot 2', 'Mika']);
    expect(screen.getByText('3 / 3 selected')).toBeVisible();
    expect(screen.getByText('2 / 3 ready')).toBeVisible();
    expect(screen.queryByText('3 connected')).not.toBeInTheDocument();
    expect(screen.queryByText(/^connected$/i)).not.toBeInTheDocument();
    expect(screen.queryByText('Room status')).not.toBeInTheDocument();
    expect(screen.queryByText('Match readiness blockers')).not.toBeInTheDocument();
    expect(screen.getByText('Room code')).toBeVisible();
    expect(screen.getByText('123456', { exact: true })).toBeVisible();
    expect(screen.getByText('Night Market', { exact: true })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Preloading roster' })).toBeDisabled();
    expect(screen.getByText('Preloading every selected Character.')).toBeVisible();
    expect(screen.queryByText(/Seat 1/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Mika' }));
    const dialog = await screen.findByRole('dialog', { name: 'Mika' });
    expect(dialog).toHaveTextContent('Your Participant');
    expect(dialog).not.toHaveTextContent('Seat');
    fireEvent.keyDown(dialog, { key: 'Escape' });
    act(() => useGameStore.setState(state => ({ room: { ...state.room!, participants: [...state.room!.participants].reverse().map(p => ({ ...p, ready: true, presence: 'disconnected' })) } })));
    expect(cards()).toEqual(['Bot 1', 'Bot 2', 'Mika']);
    expect(screen.getByText('3 / 3 ready')).toBeVisible();
    expect(screen.queryByText('0 connected')).not.toBeInTheDocument();
  });

  it.each([3, 4])("routes validated %ip final results before native boundary, retaining socket, notices and reopen", async (count) => {
    class FinalSocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose = null; onerror = null;
      send = vi.fn(); close = vi.fn();
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    const socket = new FinalSocket();
    const constructor = Object.assign(vi.fn(function () { return socket; }), { OPEN: 1 });
    vi.stubGlobal("WebSocket", constructor);
    const players = Array.from({ length: count }, (_, seat) => ({ participant_id: seat === 0 ? "P1" : `B${seat}`, display_name: `Player ${seat}`, seat, kind: "Human", character_id: null, controller: "interactive", final_score: 30000 - seat * 1000, rank: seat + 1 }));
    const final = { join_code: "123456", room_name: "Final Room", game_mode: `${count}p-red-east`, phase: "post_match", revision: 1, participants: [], match_players: [], roster: players, kyoku_end: null, result: { mode: count === 3 ? "ThreePlayerRedEast" : "FourPlayerRedEast", players, final_scores: players.map(p => p.final_score) } };
    render(<App />);
    await waitFor(() => expect(socket.onopen).not.toBeNull());
    act(() => { socket.onopen?.(); socket.emit({ type: "snapshot", room: final, state: null }); });
    expect(await screen.findByTestId("match-victory-page")).toBeVisible();
    expect(within(screen.getByRole("list", { name: "Final standings" })).getAllByRole("listitem")).toHaveLength(count);
    expect(screen.queryByTestId("gameplay-shell")).not.toBeInTheDocument();
    expect(screen.queryByText("Widen this window to play.")).not.toBeInTheDocument();
    const originalWidth = window.innerWidth;
    for (const width of [390, 1024, 390]) {
      act(() => { vi.stubGlobal("innerWidth", width); window.dispatchEvent(new Event("resize")); });
      expect(screen.getByTestId("match-victory-page")).toBeVisible();
      expect(document.querySelector("canvas")).toBeNull();
    }
    vi.stubGlobal("innerWidth", originalWidth);
    act(() => socket.emit({ type: "error", code: "not_ready" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("not_ready");
    fireEvent.click(screen.getByRole("button", { name: "Return to room" }));
    expect(screen.getByRole("heading", { name: "Lobby" })).toBeVisible();
    await waitFor(() => expect(screen.getByRole("button", { name: "Review final standings" })).toHaveFocus());
    fireEvent.click(screen.getByRole("button", { name: "Review final standings" }));
    expect(screen.getByTestId("match-victory-page")).toBeVisible();
    expect(screen.getByRole("button", { name: "Return to room" })).toHaveFocus();
    act(() => useGameStore.setState({ status: "reconnecting", commandError: "" }));
    expect(screen.getByRole("status")).toHaveTextContent("reconnecting");
    act(() => useGameStore.setState({ status: "error", reason: "network_error" }));
    expect(screen.getByText("The live Room connection failed. Reconnecting shortly.")).toBeVisible();
    act(() => useGameStore.setState({ status: "closed", reason: "token_revoked" }));
    expect(screen.getByRole("alert")).toHaveTextContent("This Guest Session token is no longer valid.");
    expect(screen.getByRole("link", { name: "Return to join" })).toHaveAttribute("href", "/room/123456");
    act(() => useGameStore.setState({ status: "connected", reason: "" }));
    act(() => socket.emit({ type: "room_update", room: { ...final, result: null }, state: null }));
    await waitFor(() => expect(screen.queryByTestId("match-victory-page")).not.toBeInTheDocument());
    expect(screen.queryByRole("list", { name: "Final standings" })).not.toBeInTheDocument();
    act(() => socket.emit({ type: "room_update", room: { ...final, phase: "lobby", result: null }, state: null }));
    expect(screen.getByRole("heading", { name: "Lobby" })).toBeVisible();
    expect(constructor).toHaveBeenCalledTimes(1);
    expect(socket.close).not.toHaveBeenCalled();
    expect(socket.send).not.toHaveBeenCalled();
  });

  it("keeps the WebSocket connected when the server rejects a normal command", async () => {
    class FakeWebSocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: (() => void) | null = null;
      send() {}
      close() {}
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    const socket = new FakeWebSocket();
    const webSocket = Object.assign(vi.fn(function () { return socket; }), { OPEN: FakeWebSocket.OPEN });
    vi.stubGlobal("WebSocket", webSocket);
    render(<App />);
    socket.onopen?.();
    socket.emit({ type: "snapshot", room: { join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 1, participants: [], match_players: [], roster: [], result: null }, state: null });
    socket.emit({ type: "error", code: "not_ready" });
    expect(await screen.findByText(/not_ready/i)).toBeVisible();
    expect(screen.queryByText(/^connected$/i)).not.toBeInTheDocument();
  });

  it("does not send an action after an error leaves the OPEN socket unusable", async () => {
    class FakeWebSocket {
      static OPEN = 1;
      readyState = FakeWebSocket.OPEN;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: (() => void) | null = null;
      sent: string[] = [];
      send(value: string) { this.sent.push(value); }
      close() {}
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    const instance = new FakeWebSocket();
    vi.stubGlobal("WebSocket", Object.assign(vi.fn(function () { return instance; }), { OPEN: FakeWebSocket.OPEN }));

    render(<App />);
    await waitFor(() => expect(instance.onopen).not.toBeNull());
    act(() => {
      instance.onopen?.();
      instance.emit({
        type: "snapshot",
        room: {
          join_code: "123456", room_name: "Night Market", game_mode: "4p-red-east", phase: "playing", revision: 1,
          participants: [], match_players: [], roster: [], result: null,
        },
        state: {
          audience: "player", viewer_seat: 0, mode: "4p-red-east", player_count: 4,
          players: [
            { seat: 0, participant_id: "P1", display_name: "Mika", hand: [0], score: 25_000 },
            { seat: 1, participant_id: "P2", display_name: "Nori", concealed_count: 1, score: 25_000 },
            { seat: 2, participant_id: "P3", display_name: "Ren", concealed_count: 1, score: 25_000 },
            { seat: 3, participant_id: "P4", display_name: "Sora", concealed_count: 1, score: 25_000 },
          ],
          decision: {
            decision_id: "decision-1", kind: "turn",
            actions: [{ action_id: "action-1", action: { discard: { tile: 0 } } }],
          },
        },
      });
    });

    const discard = await screen.findByRole("button", { name: /discard 1m/i });
    fireEvent.pointerDown(discard);
    act(() => instance.onerror?.());
    fireEvent.click(discard);

    expect(instance.readyState).toBe(FakeWebSocket.OPEN);
    expect(instance.sent).toEqual([]);
    expect(useGameStore.getState().pendingAction).toBeNull();
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
    vi.stubGlobal("WebSocket", Object.assign(vi.fn(function () { return instance; }), { OPEN: FakeWebSocket.OPEN }));
    const pendingImages: (() => void)[] = [];
    class FakeImage {
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      decode = () => Promise.resolve();
      set src(_value: string) { pendingImages.push(() => this.onload?.()); }
    }
    vi.stubGlobal("Image", FakeImage);
    render(<App />);
    instance.emit({ type: "snapshot", room: { join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 1, participants: [
      { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
      { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
      { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
    ], match_players: [], roster: [], result: null }, state: null });
    const ready = await screen.findByRole("button", { name: "Preloading roster" });
    expect(pendingImages.length).toBeGreaterThan(0);
    expect(ready).toBeDisabled();
    fireEvent.click(ready);
    expect(instance.sent).toEqual([]);
    await act(async () => { pendingImages.splice(0).forEach(complete => complete()); });
    await waitFor(() => expect(ready).toBeEnabled());
    fireEvent.click(ready);
    expect(JSON.parse(instance.sent.at(-1) ?? "{}")).toEqual({ type: "set_ready", preloaded_characters: ["player-red", "tsumogiri-bot", "tsumogiri-bot"] });
    const completeRoom = useGameStore.getState().room!;
    for (const missingId of ['P1', 'B1']) {
      act(() => useGameStore.setState({ room: { ...completeRoom, participants: completeRoom.participants.map(participant => participant.participant_id === missingId ? { ...participant, selected: false } : participant) } }));
      expect(ready).toBeDisabled();
      fireEvent.click(ready);
      expect(instance.sent).toHaveLength(1);
    }
    act(() => useGameStore.setState({ room: completeRoom }));
    await act(async () => { pendingImages.splice(0).forEach(complete => complete()); });
    await waitFor(() => expect(ready).toBeEnabled());
    act(() => instance.onerror?.());
    expect(ready).toBeDisabled();
    fireEvent.click(ready);
    expect(instance.sent).toHaveLength(1);
  });

  it("repeats selected asset preload for a fresh connection generation", async () => {
    class FakeWebSocket {
      static OPEN = 1;
      readyState = 0;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      send() {}
      close() {}
      open() { this.readyState = FakeWebSocket.OPEN; this.onopen?.(); }
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    const sockets: FakeWebSocket[] = [];
    vi.stubGlobal("WebSocket", Object.assign(vi.fn(function () { const socket = new FakeWebSocket(); sockets.push(socket); return socket; }), { OPEN: FakeWebSocket.OPEN }));
    let imageCount = 0;
    class FakeImage {
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      decode = () => Promise.resolve();
      set src(_value: string) { imageCount += 1; queueMicrotask(() => this.onload?.()); }
    }
    vi.stubGlobal("Image", FakeImage);
    vi.useFakeTimers();
    render(<App />);
    expect(sockets).toHaveLength(1);
    const snapshot = { type: "snapshot", room: { join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 1, participants: [
      { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
      { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
      { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
    ], match_players: [], roster: [], result: null }, state: null };
    act(() => { sockets[0].open(); sockets[0].emit(snapshot); });
    expect(imageCount).toBe(6);
    sockets[0].onclose?.({ code: 1006, reason: "" } as CloseEvent);
    act(() => { vi.advanceTimersByTime(500); });
    expect(sockets).toHaveLength(2);
    act(() => { sockets[1].open(); sockets[1].emit(snapshot); });
    expect(imageCount).toBe(12);
    sockets[0].emit({ ...snapshot, room: { ...snapshot.room, room_name: "Stale Room" } });
    expect(screen.queryByRole("heading", { name: /stale room lobby/i })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^lobby$/i })).toBeVisible();
    vi.useRealTimers();
  });

  it("keeps Ready disabled when a 200 asset cannot decode", async () => {
    class FakeWebSocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      send() {}
      close() {}
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) } as MessageEvent<string>); }
    }
    class BrokenImage {
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      decode = () => Promise.reject(new Error("malformed_image"));
      set src(_value: string) { queueMicrotask(() => this.onload?.()); }
    }
    const instance = new FakeWebSocket();
    vi.stubGlobal("WebSocket", Object.assign(vi.fn(function () { return instance; }), { OPEN: FakeWebSocket.OPEN }));
    vi.stubGlobal("Image", BrokenImage);
    render(<App />);
    instance.emit({ type: "snapshot", room: { join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 1, participants: [
      { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
      { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
      { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
    ], match_players: [], roster: [], result: null }, state: null });
    const ready = await screen.findByRole("button", { name: /set ready/i });
    await waitFor(() => expect(screen.getByText(/could not be preloaded/i)).toBeVisible());
    expect(ready).toBeDisabled();
  });
});
