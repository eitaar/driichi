import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  GameplaySurface,
  ResultsPanel,
  RoundWinSurface,
  handActionMap,
  type RoundWinEffect,
} from "./gameplay";
import { actionCandidates } from "./actions";
import { useGameStore } from "./store";
import type { ProjectedState, RoomSnapshot } from "./types";

function player(seat: number, display_name: string) {
  return {
    seat,
    participant_id: `participant-${seat}`,
    display_name,
    score: 25_000,
  };
}

const resultWin = {
  event_index: 4, winner: "p0", seat: 0, target: 1, tsumo: false,
  concealed: [0, 1, 16, 17], melds: [], winning_tile: 16, winning_tile_index: 2,
  yaku: [{ id: 1, name: "riichi", han: 1, yakuman_multiplier: null }],
  evaluation: { Ordinary: { han: 1, fu: 30, limit: null } }, win_points: 1000,
};
const resultSettlement = {
  changes: [
    { participant_id: "p0", seat: 0, before: 25000, delta: 6000, after: 31000 },
    { participant_id: "p1", seat: 1, before: 25000, delta: -6000, after: 19000 },
    { participant_id: "p2", seat: 2, before: 0, delta: 0, after: 0 },
    { participant_id: "p3", seat: 3, before: -100, delta: 0, after: -100 },
  ], pot_before: 0, pot_after: 0, draw_reason: null,
};
function liveRoom(content: unknown = { Win: resultWin }, remaining = 5000, index = 0) {
  return {
    game_mode: "4p-red-east", phase: "playing", revision: 9, participants: [], match_players: [],
    roster: [0, 1, 2, 3].map((seat) => ({ participant_id: `p${seat}`, display_name: `Seat ${seat}`, kind: "human", seat, character_id: null, controller: "interactive" })),
    kyoku_end: { id: { match_id: "m", kyoku_sequence: 1, step_index: index }, content, published_at_ms: 100, deadline_at_ms: 5100, remaining_ms: remaining, required: ["p0"], confirmed: [] },
    result: null, join_code: "123456", room_name: "Room",
  } as RoomSnapshot;
}
const liveProjection: ProjectedState = { audience: "player", viewer_seat: 0, mode: "4p-red-east", player_count: 4, players: [0, 1, 2, 3].map((seat) => player(seat, `Seat ${seat}`)), decision: null };
const liveProps = { projection: liveProjection, status: "connected" as const, reason: "", commandError: "", connectionGeneration: 0, send: () => true, reducedMotion: false, participantId: "p0" };

describe("result surfaces", () => {
  afterEach(() => { vi.useRealTimers(); });

  it("does not substitute delta or rank for missing authoritative result facts", () => {
    const room = { ...liveRoom(), result: { mode: "FourPlayerRedEast", players: [{ participant_id: "p0", display_name: "Seat 0", delta: 99999 }], final_scores: [99999] } } as never;
    render(<ResultsPanel room={room} assets={{}} />);
    expect(screen.queryByRole("list", { name: "Final standings" })).not.toBeInTheDocument();
    expect(screen.queryByText("99,999")).not.toBeInTheDocument();
  });

  it("reconnect uses server remaining time and resends or Pause never extend it", () => {
    vi.useFakeTimers();
    const view = render(<GameplaySurface {...liveProps} room={liveRoom({ Settlement: resultSettlement }, 3400)} />);
    const transfer = screen.getByLabelText("Score transfer");
    expect(transfer).toHaveTextContent("+3,200");
    act(() => { vi.advanceTimersByTime(600); });
    expect(transfer).toHaveTextContent("+4,400");
    fireEvent.click(screen.getByRole("button", { name: "Pause for review" }));
    view.rerender(<GameplaySurface {...liveProps} room={liveRoom({ Settlement: resultSettlement }, 5000)} />);
    act(() => { vi.advanceTimersByTime(600); });
    expect(screen.getByTestId("result-timer")).toHaveAttribute("data-remaining-ms", "2200");
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    expect(transfer).toHaveTextContent("+5,600");
    act(() => { vi.advanceTimersByTime(3000); });
    expect(transfer).toHaveTextContent("31,000");
    expect(screen.queryByTestId("action-deck")).not.toBeInTheDocument();
    expect(transfer).toBeVisible(); // expiry cannot infer next content
    view.rerender(<GameplaySurface {...liveProps} room={{ ...liveRoom(), kyoku_end: null } as never} />);
    expect(screen.queryByLabelText("Score transfer")).not.toBeInTheDocument();
  });
  it("counts from snapshot receipt even when rendering or reconnect is delayed", () => {
    vi.useFakeTimers();
    useGameStore.getState().reset();
    const room = liveRoom({ Settlement: resultSettlement });
    useGameStore.getState().receiveSnapshot(room, liveProjection);
    act(() => { vi.advanceTimersByTime(3000); });
    const view = render(<GameplaySurface {...liveProps} room={room} />);
    expect(screen.getByLabelText("Score transfer")).toHaveTextContent("+6,000");
    expect(screen.getByTestId("result-timer")).toHaveAttribute("data-remaining-ms", "2000");
    view.unmount();
    act(() => { useGameStore.getState().resetForReconnect(); vi.advanceTimersByTime(1000); });
    const resent = liveRoom({ Settlement: resultSettlement });
    useGameStore.getState().receiveSnapshot(resent, liveProjection);
    render(<GameplaySurface {...liveProps} room={resent} connectionGeneration={1} />);
    expect(screen.getByTestId("result-timer")).toHaveAttribute("data-remaining-ms", "1000");
  });

  it("focuses confirmation when a result arrives after the table is already mounted", () => {
    vi.useFakeTimers();
    useGameStore.getState().reset();
    useGameStore.getState().setStatus("connected");
    const initial = { ...liveRoom(), kyoku_end: null };
    useGameStore.getState().receiveSnapshot(initial, liveProjection);
    const view = render(<GameplaySurface {...liveProps} room={initial} />);
    const arrived = liveRoom();
    act(() => { useGameStore.getState().receiveSnapshot(arrived, liveProjection); });
    view.rerender(<GameplaySurface {...liveProps} room={arrived} />);
    expect(screen.getByRole("button", { name: "Continue" })).toHaveFocus();
  });

  it("confirms only the displayed step, waits on it, and reopens each new server step", () => {
    useGameStore.getState().reset();
    useGameStore.getState().setStatus("connected");
    const sent: unknown[] = [];
    const room = liveRoom();
    useGameStore.getState().receiveSnapshot(room, liveProjection);
    const send = (message: unknown) => { sent.push(message); return true; };
    const view = render(<GameplaySurface {...liveProps} send={send} room={room} reducedMotion />);
    const confirm = screen.getByRole("button", { name: "Continue" });
    expect(confirm).toHaveFocus();
    fireEvent.click(confirm);
    expect(sent).toEqual([{ type: "confirm_result", step_id: { match_id: "m", kyoku_sequence: 1, step_index: 0 } }]);
    expect(screen.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
    expect(screen.getByTestId("round-win-surface")).toBeVisible();
    const second = liveRoom({ Win: { ...resultWin, winner: "p2", seat: 2, event_index: 5 } }, 4500, 1);
    act(() => { useGameStore.getState().receiveSnapshot(second, liveProjection); });
    view.rerender(<GameplaySurface {...liveProps} room={second} send={send} reducedMotion />);
    expect(screen.getByRole("heading", { name: "Seat 2" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    const settlement = liveRoom({ Settlement: resultSettlement }, 1500, 2);
    act(() => { useGameStore.getState().receiveSnapshot(settlement, liveProjection); });
    view.rerender(<GameplaySurface {...liveProps} room={settlement} send={send} reducedMotion />);
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Score transfer")).toHaveTextContent("+6,000");
  });

  it("keeps the result and remaining player's confirmation when a confirmed player disconnects", () => {
    vi.useFakeTimers();
    useGameStore.getState().reset(); useGameStore.getState().setStatus("connected");
    const initial = liveRoom();
    initial.kyoku_end = { ...initial.kyoku_end!, required: ["p0", "p1", "p2"], confirmed: ["p0"] };
    const projection = { ...liveProjection, viewer_seat: 1 };
    const sent: unknown[] = [];
    const props = { ...liveProps, projection, participantId: "p1", send: (message: unknown) => { sent.push(message); return true; } };
    useGameStore.getState().receiveSnapshot(initial, projection);
    const view = render(<GameplaySurface {...props} room={initial} />);
    act(() => { vi.advanceTimersByTime(600); });
    const shrunk = { ...initial, kyoku_end: { ...initial.kyoku_end!, required: ["p1", "p2"] } };
    const deadline = useGameStore.getState().resultDeadline;
    act(() => { useGameStore.getState().receiveSnapshot(shrunk, projection); });
    view.rerender(<GameplaySurface {...props} room={shrunk} />);
    expect(screen.getByTestId("round-win-surface")).toBeVisible();
    expect(screen.getByTestId("result-timer")).toHaveAttribute("data-remaining-ms", "4400");
    expect(useGameStore.getState().resultDeadline).toBe(deadline);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(sent).toEqual([{ type: "confirm_result", step_id: { match_id: "m", kyoku_sequence: 1, step_index: 0 } }]);
    expect(screen.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
  });

  it("keeps gameplay closed when a result packet is malformed", () => {
    const projection = { ...liveProjection, decision: { decision_id: "stale", kind: "turn", actions: [{ action_id: "discard", action: { Discard: { tile: 0 } } }] } };
    render(<GameplaySurface {...liveProps} projection={projection} room={liveRoom({ Unknown: resultWin })} />);
    expect(screen.queryByTestId("action-deck")).not.toBeInTheDocument();
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
  });

  it("rejects partial or mismatched recorded rosters without displaying result facts", () => {
    const room = liveRoom() as unknown as { roster: Array<Record<string, unknown>> };
    const view = render(<GameplaySurface {...liveProps} room={{ ...room, roster: room.roster.slice(0, 1) } as never} />);
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
    view.rerender(<GameplaySurface {...liveProps} room={{ ...room, roster: [room.roster[0], room.roster[0], ...room.roster.slice(2)] } as never} />);
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
    view.rerender(<GameplaySurface {...liveProps} room={liveRoom({ Settlement: { ...resultSettlement, changes: resultSettlement.changes.map((change) => ({ ...change, participant_id: "foreign" + change.seat })) } })} />);
    expect(screen.queryByLabelText("Score transfer")).not.toBeInTheDocument();
  });

  it("spectator dismiss is local and a new result step reopens", () => {
    const sent: unknown[] = [];
    const props = { ...liveProps, projection: { ...liveProjection, audience: "public" }, participantId: "spectator", send: (message: unknown) => { sent.push(message); } };
    const view = render(<GameplaySurface {...props} room={liveRoom()} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
    expect(screen.getByText("Waiting for players to confirm")).toBeVisible();
    view.rerender(<GameplaySurface {...props} room={liveRoom({ Settlement: resultSettlement }, 4000, 1)} />);
    expect(screen.getByLabelText("Score transfer")).toBeVisible();
    expect(sent).toEqual([]);
  });

  it("displays the supplied draw reason with settlement and omits it when absent", () => {
    const settlement = { ...resultSettlement, draw_reason: "suucha_riichi", changes: resultSettlement.changes.map((change) => ({ ...change, delta: 0, after: change.before })) };
    const view = render(<GameplaySurface {...liveProps} room={liveRoom({ Draw: settlement })} reducedMotion />);
    const transfer = screen.getByLabelText("Score transfer");
    expect(within(transfer).getByRole("heading", { name: "suucha_riichi" })).toBeVisible();
    expect(transfer).toHaveTextContent("+0");
    expect(transfer).toHaveTextContent("25,000");
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
    view.rerender(<GameplaySurface {...liveProps} room={liveRoom({ Settlement: { ...settlement, draw_reason: null } }, 5000, 1)} reducedMotion />);
    expect(screen.queryByText("suucha_riichi")).not.toBeInTheDocument();
    expect(within(screen.getByLabelText("Score transfer")).getByRole("heading", { name: "Score transfer" })).toBeVisible();
  });

  it("distinguishes authoritative closed and open kans without inferring from their tiles", () => {
    const meld = { kind: "Ankan", tiles: [52, 53, 54, 55], opened: false, from_who: null, called_tile: null };
    const view = render(<GameplaySurface {...liveProps} room={liveRoom({ Win: { ...resultWin, melds: [meld] } })} reducedMotion />);
    const closed = screen.getByRole("group", { name: "Closed Ankan" });
    const closedTiles = within(closed).getAllByRole("img");
    expect(closedTiles).toHaveLength(4);
    expect(closedTiles[0]).toHaveAttribute("src", expect.stringMatching(/\/Back\.svg$/));
    expect(closedTiles[3]).toHaveAttribute("src", expect.stringMatching(/\/Back\.svg$/));
    expect(closedTiles[1]).not.toHaveAttribute("src", expect.stringMatching(/\/Back\.svg$/));
    view.rerender(<GameplaySurface {...liveProps} room={liveRoom({ Win: { ...resultWin, melds: [{ ...meld, kind: "Daiminkan", opened: true, from_who: 1, called_tile: 52 }] } }, 5000, 1)} reducedMotion />);
    expect(screen.queryByRole("group", { name: "Closed Ankan" })).not.toBeInTheDocument();
    const openTiles = within(screen.getByRole("group", { name: "Open Daiminkan" })).getAllByRole("img");
    expect(openTiles).toHaveLength(4);
    for (const tile of openTiles) expect(tile).not.toHaveAttribute("src", expect.stringMatching(/\/Back\.svg$/));
  });

  it("renders a compact accessible round win with a neutral portrait fallback", () => {
    const effect: RoundWinEffect = {
      characterId: "missing-character",
      displayName: "Mika",
      result: "Ron",
      han: 3,
      fu: 40,
      points: 8_000,
      winningTile: 16,
      hand: [0, 4, 8, 12, 20, 24, 28, 32, 36, 40, 44, 48, 52],
      yaku: [{ name: "riichi", han: 1 }],
    };
    render(<RoundWinSurface effect={effect} assets={{ "missing-character": false }} reducedMotion />);

    expect(screen.getByTestId("round-win-surface")).toHaveAttribute("data-motion", "static");
    expect(screen.getByRole("heading", { name: "Mika" })).toBeVisible();
    expect(screen.getByRole("img", { name: "Mika portrait unavailable" })).toBeVisible();
    expect(screen.getByLabelText("Ron win details")).toBeVisible();
    expect(screen.getByText("Riichi")).toBeVisible();
    expect(screen.getByText("1 han")).toBeVisible();
    expect(screen.getAllByRole("img")).toHaveLength(14);
  });

  it("keeps final standings semantic and highlights the winner portrait", () => {
    const room = {
      game_mode: "4p-red-east",
      replay_available: true,
      roster: [
        { participant_id: "p1", display_name: "Mika", kind: "human", seat: 0, character_id: "player-red", controller: "interactive" },
        { participant_id: "p2", display_name: "Nori", kind: "built_in_bot", seat: 1, character_id: "tsumogiri-bot", controller: "permanent_auto_built_in_bot" },
        { participant_id: "p3", display_name: "Ren", kind: "human", seat: 2, character_id: null, controller: "interactive" },
        { participant_id: "p4", display_name: "Aya", kind: "human", seat: 3, character_id: null, controller: "interactive" },
      ],
      match_players: [],
      result: {
        // Room snapshots use the normalized game_mode for display; MatchResult
        // mode remains the serialized engine enum on the live socket.
        mode: "FourPlayerRedEast",
        players: [
          { participant_id: "p1", display_name: "Mika", kind: "Human", seat: 0, rank: 1, final_score: 45_000 },
          { participant_id: "p2", display_name: "Nori", kind: "BuiltInBot", seat: 1, rank: 2, final_score: 30_000 },
          { participant_id: "p3", display_name: "Ren", kind: "Human", seat: 2, rank: 3, final_score: 15_000 },
          { participant_id: "p4", display_name: "Aya", kind: "Human", seat: 3, rank: 4, final_score: 10_000 },
        ],
        final_scores: [45_000, 30_000, 15_000, 10_000],
      },
    } as never;
    render(<ResultsPanel room={room} assets={{ "player-red": true, "tsumogiri-bot": true }} />);

    const standings = screen.getByRole("list", { name: "Final standings" });
    expect(within(standings).getAllByRole("listitem")).toHaveLength(4);
    expect(within(standings).getAllByRole("listitem")[0]).toHaveTextContent("Mika");
    expect(within(standings).getAllByRole("listitem")[0]).toHaveTextContent("45,000");
    expect(screen.getByAltText("Mika portrait")).toBeVisible();
  });

  it("uses certified ranks and scores without inventing deltas",  () => {
    const room = {
      game_mode: "4p-red-east",
      roster: [
        { participant_id: "p1", display_name: "Mika", kind: "human", seat: 0, character_id: null, controller: "interactive" },
        { participant_id: "p2", display_name: "Nori", kind: "human", seat: 1, character_id: null, controller: "interactive" },
        { participant_id: "p3", display_name: "Ren", kind: "human", seat: 2, character_id: null, controller: "interactive" },
        { participant_id: "p4", display_name: "Aya", kind: "human", seat: 3, character_id: null, controller: "interactive" },
      ],
      match_players: [],
      result: {
        mode: "FourPlayerRedEast",
        players: [
          { participant_id: "p1", display_name: "Mika", kind: "Human", seat: 0, rank: 2, final_score: 20_000 },
          { participant_id: "p2", display_name: "Nori", kind: "Human", seat: 1, rank: 1, final_score: 40_000 },
          { participant_id: "p3", display_name: "Ren", kind: "Human", seat: 2, rank: 3, final_score: 0 },
          { participant_id: "p4", display_name: "Aya", kind: "Human", seat: 3, rank: 4, final_score: -100 },
        ],
        final_scores: [20_000, 40_000, 0, -100],
      },
    } as never;
    render(<ResultsPanel room={room} assets={{}} />);

    const standings = screen.getByRole("list", { name: "Final standings" });
    const rows = within(standings).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("Nori");
    expect(rows[0]).toHaveTextContent("40,000");
    expect(rows[1]).toHaveTextContent("Mika");
    expect(rows[1]).toHaveTextContent("20,000");
    expect(screen.queryByText(/DELTA/)).not.toBeInTheDocument();
  });
});

describe("GameplayPlayerStatus", () => {
  beforeEach(() => {
    useGameStore.getState().reset();
  });

  it("keeps duplicate physical tile identities mapped to their own action ids", () => {
    const hand = [0, 1, 16, 17, 52, 53, 88, 89];
    const actions = hand.map((tile, index) => ({
      action_id: `discard-${tile}`,
      action: { Discard: { tile, tsumogiri: index === hand.length - 1 } },
    }));
    const mapped = handActionMap(hand, actions);

    expect(mapped.map((action) => action?.action_id)).toEqual(
      hand.map((tile) => `discard-${tile}`),
    );
    expect(mapped.filter(Boolean)).toHaveLength(actions.length);
  });

  it("keeps ordinary and Riichi discard candidates separate for duplicate tile values", () => {
    const hand = [16, 17, 29]; // red 5m, regular 5m, and 8m: duplicate value plus red identity.
    const grouped = actionCandidates({
      decision_id: "tenpai-turn",
      kind: "turn",
      actions: [
        { action_id: "ordinary-red-five", action: { Discard: { tile: 16 } } },
        { action_id: "ordinary-five-copy", action: { Discard: { tile: 17 } } },
        { action_id: "ordinary-eight", action: { Discard: { tile: 29 } } },
        { action_id: "riichi-red-five", action: { riichi_discard: { tile: 16 } } },
      ],
    });

    expect(grouped.discard.map(({ action_id }) => action_id)).toEqual([
      "ordinary-red-five",
      "ordinary-five-copy",
      "ordinary-eight",
    ]);
    expect(grouped.riichiDiscard.map(({ action_id }) => action_id)).toEqual([
      "riichi-red-five",
    ]);
    expect(handActionMap(hand, grouped.discard).map((action) => action?.action_id)).toEqual([
      "ordinary-red-five",
      "ordinary-five-copy",
      "ordinary-eight",
    ]);
  });

  it("maps every physical tile id and leaves unavailable candidates unbound", () => {
    // Tile ids identify physical copies, so equal-looking tiles must remain distinct.
    for (let tile = 0; tile < 136; tile += 1) {
      const action = {
        action_id: `discard-${tile}`,
        action: { Discard: { tile, tsumogiri: false } },
      };
      expect(handActionMap([tile], [action])[0]?.action_id).toBe(`discard-${tile}`);
    }

    const duplicateTypeHand = [
      0, 1, 2, 3, 16, 17, 18, 19, 52, 53, 54, 55, 88, 89,
    ];
    const duplicateTypeActions = duplicateTypeHand
      .slice()
      .reverse()
      .map((tile) => ({
        action_id: `duplicate-${tile}`,
        action: { riichi_discard: { tile } },
      }));
    expect(
      handActionMap(duplicateTypeHand, duplicateTypeActions).map(
        (action) => action?.action_id,
      ),
    ).toEqual(duplicateTypeHand.map((tile) => `duplicate-${tile}`));

    expect(
      handActionMap(
        [0, 1, 2],
        [{ action_id: "discard-0", action: { Discard: { tile: 0 } } }],
      ).map((action) => action?.action_id ?? null),
    ).toEqual(["discard-0", null, null]);
  });

  it("keeps a stale fourth three-player seat out of the assistive player list", () => {
    const projection: ProjectedState = {
      audience: "player",
      viewer_seat: 0,
      mode: "3p-red-east",
      player_count: 3,
      players: [
        player(3, "Stale fourth player"),
        player(1, "Right player"),
        player(0, "Local player"),
        player(2, "Left player"),
      ],
      decision: null,
    };

    render(
      <GameplaySurface
        room={null}
        projection={projection}
        status="closed"
        reason="room_deleted"
        commandError=""
        connectionGeneration={0}
        send={() => false}
        reducedMotion
      />,
    );

    const list = screen.getByRole("list", { name: "Player status" });
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(3);
    expect(items.map((item) => item.textContent)).toEqual([
      expect.stringContaining("Local player"),
      expect.stringContaining("Right player"),
      expect.stringContaining("Left player"),
    ]);
    expect(within(list).queryByText("Stale fourth player")).not.toBeInTheDocument();
  });

  it("renders only the authoritative winning position", () => {
    const room = {
      game_mode: "4p-red-east",
      phase: "playing",
      revision: 9,
      roster: [0, 1, 2, 3].map((seat) => ({
        participant_id: `p${seat}`,
        display_name: seat === 0 ? "Mika" : `Seat ${seat}`,
        kind: seat === 0 ? "human" : "built_in_bot",
        seat,
        character_id: null,
        controller: "interactive",
      })),
      match_players: [],
      participants: [],
      kyoku_end: {
        id: { match_id: "m", kyoku_sequence: 1, step_index: 0 },
        content: { Win: {
          event_index: 4,
          winner: "p0",
          seat: 0,
          target: 1,
          tsumo: false,
          concealed: [0, 4, 8, 12, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56],
          melds: [],
          winning_tile: 56,
          winning_tile_index: 13,
          yaku: [{ id: 1, name: "riichi", han: 1, yakuman_multiplier: null }],
          evaluation: { Ordinary: { han: 1, fu: 30, limit: null } },
          win_points: 8_000,
        } },
        published_at_ms: 100,
        deadline_at_ms: 5_100,
        remaining_ms: 5_000,
        required: ["p0"],
        confirmed: [],
      },
      result: null,
    };
    const projection: ProjectedState = {
      audience: "player",
      viewer_seat: 0,
      mode: "4p-red-east",
      player_count: 4,
      players: [0, 1, 2, 3].map((seat) => player(seat, seat === 0 ? "Mika" : `Seat ${seat}`)),
      decision: null,
    };

    render(<GameplaySurface room={room as never} projection={projection} status="connected" reason="" commandError="" connectionGeneration={0} send={() => true} reducedMotion />);

    expect(screen.getAllByTestId("winning-tile")).toHaveLength(1);
    expect(screen.queryByText("0 fu")).not.toBeInTheDocument();
    expect(screen.queryByText("99999")).not.toBeInTheDocument();
  });
});
