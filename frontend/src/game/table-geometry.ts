import { seatPositions, type TableSeatPosition } from "./orientation";

export const TABLE_WIDTH = 1600;
export const TABLE_HEIGHT = 900;
export const TABLE_RATIO = TABLE_WIDTH / TABLE_HEIGHT;

export interface TablePoint {
  x: number;
  y: number;
  rotation: number;
}

export interface TableSeatGeometry {
  seat: number;
  position: TableSeatPosition;
  hand: TablePoint;
  frame: TablePoint;
}

const SEAT_POINTS: Record<
  TableSeatPosition,
  Omit<TableSeatGeometry, "seat" | "position">
> = {
  bottom: {
    hand: { x: 800, y: 808, rotation: 0 },
    frame: { x: 174, y: 730, rotation: 0 },
  },
  right: {
    hand: { x: 1460, y: 450, rotation: Math.PI / 2 },
    frame: { x: 1430, y: 132, rotation: 0 },
  },
  top: {
    hand: { x: 800, y: 82, rotation: Math.PI },
    frame: { x: 1180, y: 98, rotation: 0 },
  },
  left: {
    hand: { x: 140, y: 450, rotation: -Math.PI / 2 },
    frame: { x: 170, y: 132, rotation: 0 },
  },
};

const WALL_SLOTS_PER_EDGE = 34;
const WALL_EDGES: ReadonlyArray<{
  edge: TableSeatPosition;
  start: Pick<TablePoint, "x" | "y">;
  step: Pick<TablePoint, "x" | "y">;
  rotation: number;
}> = [
  {
    edge: "bottom",
    start: { x: 400, y: 760 },
    step: { x: 24, y: 0 },
    rotation: 0,
  },
  {
    edge: "right",
    start: { x: 1380, y: 700 },
    step: { x: 0, y: -16 },
    rotation: Math.PI / 2,
  },
  {
    edge: "top",
    start: { x: 1200, y: 140 },
    step: { x: -24, y: 0 },
    rotation: Math.PI,
  },
  {
    edge: "left",
    start: { x: 220, y: 200 },
    step: { x: 0, y: 16 },
    rotation: -Math.PI / 2,
  },
];

export function tableSeatGeometry(
  mode: string | undefined,
  viewerSeat?: number,
): TableSeatGeometry[] {
  return seatPositions(mode, viewerSeat).map(({ seat, position }) => ({
    seat,
    position,
    ...SEAT_POINTS[position],
  }));
}

export function discardPlacement(
  position: TableSeatPosition,
  index: number,
): TablePoint {
  const column = index % 6;
  const row = Math.floor(index / 6);
  const offset = (column - 2.5) * 48;
  if (position === "bottom")
    return { x: TABLE_WIDTH / 2 + offset, y: 548 + row * 58, rotation: 0 };
  if (position === "top")
    return {
      x: TABLE_WIDTH / 2 + offset,
      y: 244 - row * 58,
      rotation: Math.PI,
    };
  if (position === "right")
    return {
      x: 1260 - row * 58,
      y: TABLE_HEIGHT / 2 + offset,
      rotation: Math.PI / 2,
    };
  return {
    x: 340 + row * 58,
    y: TABLE_HEIGHT / 2 + offset,
    rotation: -Math.PI / 2,
  };
}

export function wallTileCount(value: unknown, maximum = 136): number {
  const boundedMaximum = Number.isFinite(maximum)
    ? Math.max(0, Math.floor(maximum))
    : 0;
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.min(boundedMaximum, Math.max(0, Math.floor(value)));
  }
  if (Array.isArray(value)) return Math.min(boundedMaximum, value.length);
  return 0;
}

export function wallPlacements(
  value: unknown,
): Array<TablePoint & { edge: TableSeatPosition }> {
  const count = wallTileCount(value);
  const placements: Array<TablePoint & { edge: TableSeatPosition }> = [];
  for (let index = 0; index < count; index += 1) {
    const lane = WALL_EDGES[index % WALL_EDGES.length];
    const slot = Math.floor(index / WALL_EDGES.length);
    if (slot >= WALL_SLOTS_PER_EDGE) break;
    placements.push({
      edge: lane.edge,
      x: lane.start.x + lane.step.x * slot,
      y: lane.start.y + lane.step.y * slot,
      rotation: lane.rotation,
    });
  }
  return placements;
}
