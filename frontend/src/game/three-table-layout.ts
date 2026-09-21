import { tableSeatGeometry, wallTileCount } from "./table-geometry";
import type { ProjectedPlayer, ProjectedState, RoomSnapshot } from "./types";

export type Vec3 = readonly [number, number, number];
export type SceneSeat = "bottom" | "right" | "top" | "left";

export interface SceneTile {
  key: string;
  tile: number | null;
  position: Vec3;
  rotation: Vec3;
  scale: number;
  face: "front" | "back";
  group: "hand" | "discard" | "meld" | "wall" | "dora";
}

export interface ScenePlayer {
  participantId: string;
  seat: number;
  position: SceneSeat;
  isLocal: boolean;
}

export interface MatchSceneLayout {
  players: ScenePlayer[];
  tiles: SceneTile[];
  wallCount: number;
}

export const TABLE_SIZE = { width: 13.6, depth: 11 } as const;
export const TABLE_RENDER_OFFSET: readonly [number, number, number] = [0, 0, -0.38];
export const CAMERA = {
  // The fixed lens stays inside the approved envelope while the authored table
  // depth keeps the complete world frame within the 16:9 safe composition.
  fov: 34,
  position: [0, 12.8, 12.3] as Vec3,
  target: [0, 0.15, 1] as Vec3,
  near: 0.1,
  far: 60,
} as const;
export const LOCAL_TILE_SIZE = 1;
export const REMOTE_TILE_SIZE = 0.72;
export const TILE_BODY_SIZE = { width: 0.6, depth: 0.82 } as const;
export const CENTER_DEVICE_AABB = {
  minX: -1.65,
  maxX: 1.65,
  minZ: -1.34,
  maxZ: 1.34,
} as const;

export interface SceneAabb {
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
}

export function sceneTileAabb(tile: Pick<SceneTile, "position" | "rotation" | "scale">): SceneAabb {
  const halfWidth = (TILE_BODY_SIZE.width * tile.scale) / 2;
  const halfDepth = (TILE_BODY_SIZE.depth * tile.scale) / 2;
  const cosine = Math.cos(tile.rotation[1]);
  const sine = Math.sin(tile.rotation[1]);
  const extentX = Math.abs(cosine) * halfWidth + Math.abs(sine) * halfDepth;
  const extentZ = Math.abs(sine) * halfWidth + Math.abs(cosine) * halfDepth;
  return {
    minX: tile.position[0] - extentX,
    maxX: tile.position[0] + extentX,
    minZ: tile.position[2] - extentZ,
    maxZ: tile.position[2] + extentZ,
  };
}

export function aabbIntersects(left: SceneAabb, right: SceneAabb): boolean {
  return left.minX < right.maxX
    && left.maxX > right.minX
    && left.minZ < right.maxZ
    && left.maxZ > right.minZ;
}

const HAND_ANCHORS: Record<SceneSeat, Vec3> = {
  bottom: [0, 0.28, 4.72],
  right: [5.35, 0.2, 0],
  top: [0, 0.2, -4.13],
  left: [-5.35, 0.2, 0],
};

const SEAT_ROTATIONS: Record<SceneSeat, Vec3> = {
  bottom: [0, 0, 0],
  right: [0, -Math.PI / 2, 0],
  top: [0, Math.PI, 0],
  left: [0, Math.PI / 2, 0],
};

const WALL_EDGE_ORDER: readonly SceneSeat[] = ["bottom", "right", "top", "left"];
const WALL_ANCHORS: Record<SceneSeat, Vec3> = {
  bottom: [-4.25, 0.18, 3.17],
  right: [4.65, 0.18, 2.69],
  top: [4.25, 0.18, -3.17],
  left: [-4.65, 0.18, -2.69],
};

function nonNegativeInteger(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.max(0, Math.floor(value));
}

function handValues(
  player: ProjectedPlayer,
  position: SceneSeat,
  audience: ProjectedState["audience"],
): { values: Array<number | null>; face: "front" | "back" } {
  if (position === "bottom") {
    if (Array.isArray(player.hand)) {
      return { values: player.hand, face: "front" };
    }
    return {
      values: Array.from({ length: nonNegativeInteger(player.concealed_count) }, () => null),
      face: "front",
    };
  }
  if (audience === "replay_admin" && Array.isArray(player.hand)) {
    return { values: player.hand, face: "front" };
  }
  return {
    values: Array.from({ length: nonNegativeInteger(player.concealed_count) }, () => null),
    face: "back",
  };
}

function tilePosition(
  position: SceneSeat,
  anchor: Vec3,
  offset: number,
): Vec3 {
  if (position === "bottom" || position === "top") {
    return [anchor[0] + offset, anchor[1], anchor[2]];
  }
  return [anchor[0], anchor[1], anchor[2] + offset];
}

function sceneTile(
  key: string,
  tile: number | null,
  position: Vec3,
  rotation: Vec3,
  scale: number,
  face: "front" | "back",
  group: SceneTile["group"],
): SceneTile {
  return { key, tile, position, rotation, scale, face, group };
}

function addHandTiles(
  tiles: SceneTile[],
  player: ProjectedPlayer,
  seat: number,
  position: SceneSeat,
  audience: ProjectedState["audience"],
): void {
  const { values, face } = handValues(player, position, audience);
  const spacing = position === "bottom" ? 0.66 : 0.51;
  const center = (values.length - 1) / 2;
  const anchor = HAND_ANCHORS[position];
  for (let index = 0; index < values.length; index += 1) {
    const drawOffset =
      position === "bottom" && values.length === 14 && index === values.length - 1
        ? 0.22
        : 0;
    const offset = (index - center) * spacing + drawOffset;
    tiles.push(
      sceneTile(
        `hand-${position}-seat-${seat}-${index}`,
        values[index],
        tilePosition(position, anchor, offset),
        SEAT_ROTATIONS[position],
        position === "bottom" ? LOCAL_TILE_SIZE : REMOTE_TILE_SIZE,
        face,
        "hand",
      ),
    );
  }
}

function discardPosition(position: SceneSeat, index: number): Vec3 {
  const column = index % 6;
  const row = Math.floor(index / 6);
  const offset = (column - 2.5) * 0.55;
  if (position === "bottom") return [offset, 0.2, 1.73 + row * 0.66];
  if (position === "top") return [offset, 0.2, -1.73 - row * 0.66];
  if (position === "right") return [2.55 + row * 0.55, 0.2, offset];
  return [-2.55 - row * 0.55, 0.2, offset];
}

function addDiscardTiles(
  tiles: SceneTile[],
  player: ProjectedPlayer,
  seat: number,
  position: SceneSeat,
): void {
  for (const [index, tile] of (player.discards ?? []).entries()) {
    tiles.push(
      sceneTile(
        `discard-${position}-seat-${seat}-${index}`,
        tile,
        discardPosition(position, index),
        SEAT_ROTATIONS[position],
        REMOTE_TILE_SIZE,
        "front",
        "discard",
      ),
    );
  }
}

const MELD_TILE_SPACING = 0.55;
const MELD_LANE_CENTERS = [-4, 4] as const;
const MELD_ROW_CENTERS = [3, 3.72] as const;

function meldPosition(
  position: SceneSeat,
  meldIndex: number,
  tileIndex: number,
  tileCount: number,
): Vec3 {
  // Keep each called group in one of two side lanes. A single horizontal meld
  // strip sits in the river's rows, so a long river can occupy the same world
  // AABB. Side lanes are outside the six-column river footprint for every seat;
  // the second row still clears the local hand's inner edge.
  const laneCenter = MELD_LANE_CENTERS[meldIndex % MELD_LANE_CENTERS.length];
  const rowCenter = MELD_ROW_CENTERS[Math.floor(meldIndex / MELD_LANE_CENTERS.length) % MELD_ROW_CENTERS.length];
  const offset = (tileIndex - (tileCount - 1) / 2) * MELD_TILE_SPACING;
  if (position === "bottom") return [laneCenter + offset, 0.2, rowCenter];
  if (position === "top") return [laneCenter + offset, 0.2, -rowCenter];
  if (position === "right") return [rowCenter, 0.2, laneCenter + offset];
  return [-rowCenter, 0.2, laneCenter + offset];
}

function addMeldTiles(
  tiles: SceneTile[],
  player: ProjectedPlayer,
  seat: number,
  position: SceneSeat,
): void {
  for (const [meldIndex, meld] of (player.melds ?? []).entries()) {
    const meldTiles = (meld.tiles ?? []).filter((tile) => Number.isInteger(tile));
    for (const [tileIndex, tile] of meldTiles.entries()) {
      tiles.push(
        sceneTile(
          `meld-${position}-seat-${seat}-${meldIndex}-${tileIndex}`,
          tile,
          meldPosition(position, meldIndex, tileIndex, meldTiles.length),
          SEAT_ROTATIONS[position],
          position === "bottom" ? LOCAL_TILE_SIZE : REMOTE_TILE_SIZE,
          "front",
          "meld",
        ),
      );
    }
  }
}

function addDoraTiles(tiles: SceneTile[], indicators: unknown): void {
  if (!Array.isArray(indicators)) return;
  const center = (indicators.length - 1) / 2;
  for (const [index, tile] of indicators.entries()) {
    if (typeof tile !== "number") continue;
    tiles.push(
      sceneTile(
        `dora-${index}`,
        tile,
        [(index - center) * 0.55, 0.28, -0.18],
        [0, 0, 0],
        REMOTE_TILE_SIZE,
        "front",
        "dora",
      ),
    );
  }
}

function wallPosition(index: number): { edge: SceneSeat; position: Vec3 } {
  const edge = WALL_EDGE_ORDER[index % WALL_EDGE_ORDER.length];
  const slot = Math.floor(index / WALL_EDGE_ORDER.length);
  const anchor = WALL_ANCHORS[edge];
  if (edge === "bottom") return { edge, position: [anchor[0] + slot * 0.25, anchor[1], anchor[2]] };
  if (edge === "right") return { edge, position: [anchor[0], anchor[1], anchor[2] - slot * 0.25] };
  if (edge === "top") return { edge, position: [anchor[0] - slot * 0.25, anchor[1], anchor[2]] };
  return { edge, position: [anchor[0], anchor[1], anchor[2] + slot * 0.25] };
}

function addWallTiles(tiles: SceneTile[], count: number): void {
  for (let index = 0; index < count; index += 1) {
    const { edge, position } = wallPosition(index);
    const slot = Math.floor(index / WALL_EDGE_ORDER.length);
    tiles.push(
      sceneTile(
        `wall-${edge}-${slot}-${index}`,
        null,
        position,
        SEAT_ROTATIONS[edge],
        REMOTE_TILE_SIZE,
        "back",
        "wall",
      ),
    );
  }
}

export function buildMatchSceneLayout(
  projection: ProjectedState,
  room: RoomSnapshot | null,
): MatchSceneLayout {
  void room;
  const geometry = tableSeatGeometry(projection.mode, projection.viewer_seat);
  const projectedPlayers = projection.players ?? [];
  const players: ScenePlayer[] = [];
  const tiles: SceneTile[] = [];

  for (const { seat, position } of geometry) {
    const player = projectedPlayers.find((candidate) => candidate.seat === seat);
    if (!player) continue;
    players.push({
      participantId: player.participant_id,
      seat,
      position,
      isLocal: position === "bottom",
    });
    addHandTiles(tiles, player, seat, position, projection.audience);
    addDiscardTiles(tiles, player, seat, position);
    addMeldTiles(tiles, player, seat, position);
  }

  addDoraTiles(tiles, projection.dora_indicators);
  const wallCount = wallTileCount(projection.remaining_wall);
  addWallTiles(tiles, wallCount);

  return { players, tiles, wallCount };
}
