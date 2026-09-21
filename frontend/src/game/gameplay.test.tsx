import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { GameplaySurface, handActionMap } from "./gameplay";
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
