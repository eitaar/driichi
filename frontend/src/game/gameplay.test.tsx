import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { GameplaySurface } from "./gameplay";
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
