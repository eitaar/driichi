import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { App } from "./app";

vi.mock("./pixi-vignette", () => ({ TileVignette: () => null }));

const players = [
  { participant_id: "P1", display_name: "Mika", seat: 0, final_score: 30_000, rank: 2 },
  { participant_id: "B1", display_name: "Bot One", seat: 1, final_score: 45_000, rank: 1 },
  { participant_id: "B2", display_name: "Bot Two", seat: 2, final_score: 0, rank: 3 },
];
const room = {
  join_code: "123456", room_name: "Post-Match Room", game_mode: "3p-red-east", phase: "post_match", revision: 1,
  participants: players.map((player) => ({ ...player, kind: player.seat === 0 ? "human" : "built_in_bot", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" })),
  roster: players.map((player) => ({ ...player, kind: "human", character_id: "player-red", controller: "interactive" })), match_players: [],
  result: { mode: "ThreePlayerRedEast", players: players.map((player) => ({ ...player, kind: player.seat === 0 ? "Human" : "BuiltInBot" })), final_scores: [30_000, 45_000, 0] },
};

class Socket {
  static OPEN = 1;
  readyState = 1;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  send = vi.fn();
  close() {}
  snapshot(value = room) { this.onmessage?.({ data: JSON.stringify({ type: "snapshot", room: value, state: null }) } as MessageEvent); }
}

function resize(width: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 844 });
  window.dispatchEvent(new Event("resize"));
}

beforeEach(() => {
  window.history.replaceState({}, "", "/room/123456/lobby");
  sessionStorage.setItem("driichi:participant:123456", "P1");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("[]", { headers: { "content-type": "application/json" } })));
  class Image {
    onload: (() => void) | null = null;
    decode = () => Promise.resolve();
    set src(_value: string) { queueMicrotask(() => this.onload?.()); }
  }
  vi.stubGlobal("Image", Image);
});

it.each([false, true])("preserves HumanLobby Post-Match standings on phone (resize: %s) without renderer resources", async (fromDesktop) => {
  resize(fromDesktop ? 1280 : 390);
  const worker = vi.fn();
  vi.stubGlobal("Worker", worker);
  const socket = new Socket();
  vi.stubGlobal("WebSocket", Object.assign(vi.fn(function () { return socket; }), { OPEN: 1 }));
  render(<App />);
  act(() => { socket.onopen?.(); socket.snapshot(); });
  if (fromDesktop) {
    expect(await screen.findByRole("heading", { name: "Final standings" }, { timeout: 10_000 })).toBeVisible();
    act(() => resize(390));
  }
  const standings = await screen.findByRole("list", { name: "Final standings" });
  const rows = within(standings).getAllByRole("listitem");
  expect(rows.map((row) => row.textContent)).toEqual(["1Bot One45,000", "2Mika30,000", "3Bot Two0"]);
  expect(within(rows[0]).getByLabelText("Rank 1")).toBeVisible();
  expect(document.querySelector("canvas")).toBeNull();
  expect(worker).not.toHaveBeenCalled();
  expect(screen.getByRole('button', { name: 'Mika' })).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Mika' }));
  const detail = await screen.findByRole('dialog', { name: 'Mika' });
  expect(detail).toHaveTextContent('Your Participant');
  expect(detail).toHaveTextContent('Previous Match Seat');
  fireEvent.keyDown(detail, { key: 'Escape' });
  expect(document.querySelector(".connection-state")).toHaveTextContent("connected");
  const ready = await screen.findByRole("button", { name: "Set Ready" });
  await waitFor(() => expect(ready).toBeEnabled());
  fireEvent.click(ready);
  expect(socket.send).toHaveBeenCalledWith(JSON.stringify({ type: "set_ready", preloaded_characters: ["player-red", "player-red", "player-red"] }));
  const sends = socket.send.mock.calls.length;
  fireEvent.click(screen.getByRole("button", { name: "Return to Room" }));
  expect(socket.send).toHaveBeenCalledTimes(sends);
  expect(screen.getByRole("heading", { name: "Lobby" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Review final standings" }));
  expect(screen.getByRole("list", { name: "Final standings" })).toBeVisible();
  act(() => socket.snapshot({ ...room, revision: 2, roster: room.roster.map((player) => ({ ...player, participant_id: player.seat === 1 ? "wrong-id" : player.participant_id })) }));
  expect(screen.queryByRole("list", { name: "Final standings" })).not.toBeInTheDocument();
});
