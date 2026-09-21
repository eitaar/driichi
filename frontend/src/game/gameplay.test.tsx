import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import {
  GameplaySurface,
  ResultsPanel,
  RoundWinSurface,
  handActionMap,
  type RoundWinEffect,
} from "./gameplay";
import { useGameStore } from "./store";
import type { ProjectedState } from "./types";

function player(seat: number, display_name: string) {
  return {
    seat,
    participant_id: `participant-${seat}`,
    display_name,
    score: 25_000,
  };
}

describe("result surfaces", () => {
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
    expect(screen.getByText("Discard win")).toBeVisible();
    expect(screen.getByText("Riichi · 1 han")).toBeVisible();
    expect(screen.getAllByRole("img")).toHaveLength(14);
  });

  it("keeps final standings semantic while showing portraits, auto labels, and supplied deltas", () => {
    const room = {
      game_mode: "4p-red-east",
      replay_available: true,
      roster: [
        { participant_id: "p1", display_name: "Mika", kind: "human", seat: 0, character_id: "player-red", controller: "interactive" },
        { participant_id: "p2", display_name: "Nori", kind: "built_in_bot", seat: 1, character_id: "tsumogiri-bot", controller: "permanent_auto_built_in_bot" },
      ],
      match_players: [],
      result: {
        // Room snapshots use the normalized game_mode for display; MatchResult
        // mode remains the serialized engine enum on the live socket.
        mode: "FourPlayerRedEast",
        players: [
          { participant_id: "p1", display_name: "Mika", rank: 1, final_score: 45_000, delta: 5_000 },
          { participant_id: "p2", display_name: "Nori", rank: 2, final_score: 30_000, delta: -5_000 },
        ],
      },
    } as never;
    render(<ResultsPanel room={room} assets={{ "player-red": true, "tsumogiri-bot": true }} />);

    const standings = screen.getByRole("list", { name: "Final standings" });
    expect(within(standings).getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText("Permanent Auto")).toBeVisible();
    expect(screen.getByText("+5,000")).toBeVisible();
    expect(screen.getByText("-5,000")).toBeVisible();
    expect(screen.getAllByAltText("Mika portrait")).toHaveLength(2);
    expect(screen.getByAltText("Nori portrait")).toBeVisible();
    expect(screen.getByText("Available")).toBeVisible();
    expect(screen.getByText("4p-red-east")).toBeVisible();
  });

  it("maps seat-ordered final scores to ranked players without inventing deltas", () => {
    const room = {
      game_mode: "4p-red-east",
      roster: [
        { participant_id: "p1", display_name: "Mika", kind: "human", seat: 0, character_id: null, controller: "interactive" },
        { participant_id: "p2", display_name: "Nori", kind: "human", seat: 1, character_id: null, controller: "interactive" },
      ],
      match_players: [],
      result: {
        mode: "FourPlayerRedEast",
        players: [
          { participant_id: "p2", display_name: "Nori", seat: 1, rank: 1 },
          { participant_id: "p1", display_name: "Mika", seat: 0, rank: 2 },
        ],
        final_scores: [20_000, 40_000],
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
});
