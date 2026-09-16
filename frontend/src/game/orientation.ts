import type { ProjectedPlayer } from "./types";

export type TableSeatPosition = "bottom" | "right" | "top" | "left";

export interface OrientedSeat {
  seat: number;
  position: TableSeatPosition;
}

function isThreePlayerMode(mode: string | undefined): boolean {
  return (
    mode?.startsWith("3p") === true ||
    mode === "ThreePlayerRedEast" ||
    mode === "ThreePlayerRedHalf"
  );
}

/**
 * Return the fixed broadcast orientation for a table. A spectator has no
 * viewer seat, so Seat 0 is deliberately the bottom seat.
 */
export function seatPositions(
  mode: string | undefined,
  viewerSeat?: number,
): OrientedSeat[] {
  const count = isThreePlayerMode(mode) ? 3 : 4;
  const anchor =
    Number.isInteger(viewerSeat) && (viewerSeat as number) >= 0
      ? (viewerSeat as number)
      : 0;
  const positions: TableSeatPosition[] =
    count === 3
      ? ["bottom", "right", "left"]
      : ["bottom", "right", "top", "left"];
  return positions.map((position, offset) => ({
    seat: (anchor + offset) % count,
    position,
  }));
}

export function seatPositionFor(
  mode: string | undefined,
  seat: number,
  viewerSeat?: number,
): TableSeatPosition | undefined {
  return seatPositions(mode, viewerSeat).find((entry) => entry.seat === seat)
    ?.position;
}

export function orientedPlayers(
  players: ProjectedPlayer[] | undefined,
  mode: string | undefined,
  viewerSeat?: number,
): Array<ProjectedPlayer & { position: TableSeatPosition }> {
  if (!players) return [];
  const positions = seatPositions(mode, viewerSeat);
  return positions.flatMap(({ seat, position }) => {
    const player = players.find((candidate) => candidate.seat === seat);
    return player ? [{ ...player, position }] : [];
  });
}

export const getSeatPositions = seatPositions;
export const tableSeatPositions = seatPositions;
