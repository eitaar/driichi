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
    expect(screen.getByRole("link", { name: /return to entry/i })).toHaveAttribute("href", "/");
  });
});
