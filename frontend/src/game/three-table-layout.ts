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

export type TableSurface = "live" | "replay";

export const TABLE_WIDTH_STRETCH = 1.06;
export const TABLE_SIZE = { width: 13.65 * TABLE_WIDTH_STRETCH, depth: 12.25 } as const;
export const TABLE_RENDER_OFFSET: readonly [number, number, number] = [0, 0, -0.38];
export const CAMERA = {
  // The fixed lens stays inside the approved envelope while the authored table
  // depth keeps the complete world frame within the 16:9 safe composition.
  fov: 33.8,
  position: [0, 12, 13.4] as Vec3,
  target: [0, 0.15, 0] as Vec3,
  near: 0.1,
  far: 60,
} as const;
export const LOCAL_TILE_SIZE = 1;
// Keep layout, hit-target projection, and the instanced renderer on one authored
// tile footprint. The remote scale remains the visual/replay value from the
// authoritative-motion integration.
export const REMOTE_TILE_SIZE = 0.78;
export const TILE_BODY_HEIGHTS = { local: 0.2, remote: 0.2 } as const;
export const TILE_BODY_SIZE = { width: 0.62, height: TILE_BODY_HEIGHTS.local, depth: 0.86 } as const;
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
  bottom: [0, 0.58, 4.72],
  right: [5.35, 0.48, 0],
  top: [0, 0.48, -4.13],
  left: [-5.35, 0.48, 0],
};

const SEAT_ROTATIONS: Record<SceneSeat, Vec3> = {
  bottom: [0, 0, 0],
  // A tile's physical glyph top points toward the table center. Side seats
  // therefore use the opposite yaw from a screen-upright presentation.
  right: [0, Math.PI / 2, 0],
  top: [0, Math.PI, 0],
  left: [0, -Math.PI / 2, 0],
};

const WALL_EDGE_ORDER: readonly SceneSeat[] = ["bottom", "right", "top", "left"];

function nonNegativeInteger(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.max(0, Math.floor(value));
}

function handValues(
  player: ProjectedPlayer,
  position: SceneSeat,
  surface: TableSurface,
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
  // The render surface, not the payload's audience field, is the privacy
  // boundary. A live/spectator surface may receive a malformed or stale
  // replay-shaped projection, but it must never turn opponent hands face-up.
  // Replay consumes only the exact hand array persisted in the current frame;
  // a missing array remains concealed rather than being reconstructed.
  if (surface === "replay" && Array.isArray(player.hand)) {
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
  surface: TableSurface,
): void {
  if (position === "bottom") return; // Both Live and Replay show the local hand in DOM.
  const { values, face } = handValues(player, position, surface);
  const spacing = TILE_BODY_SIZE.width * REMOTE_TILE_SIZE + 0.006;
  const center = (values.length - 1) / 2;
  const anchor = HAND_ANCHORS[position];
  for (let index = 0; index < values.length; index += 1) {
    const offset = (index - center) * spacing;
    tiles.push(
      sceneTile(
        `hand-${position}-seat-${seat}-${index}`,
        values[index],
        tilePosition(position, anchor, offset),
        [Math.PI / 2, 0, -SEAT_ROTATIONS[position][1]],
        REMOTE_TILE_SIZE,
        face,
        "hand",
      ),
    );
  }
}

function discardPosition(position: SceneSeat, index: number, declaration: number | null): Vec3 {
  const column = index % 6;
  const row = Math.floor(index / 6);
  let offset = (column - 2.5) * 0.55;
  if (declaration !== null && Math.floor(declaration / 6) === row) {
    const extra = (TILE_BODY_SIZE.depth - TILE_BODY_SIZE.width) * REMOTE_TILE_SIZE;
    offset += index === declaration ? extra / 2 : index > declaration ? extra : 0;
  }
  if (position === "bottom") return [offset, 0.2, 1.73 + row * 0.66];
  if (position === "top") return [-offset, 0.2, -1.73 - row * 0.66];
  if (position === "right") return [2.55 + row * 0.55, 0.2, -offset];
  return [-2.55 - row * 0.55, 0.2, offset];
}

function addDiscardTiles(
  tiles: SceneTile[],
  player: ProjectedPlayer,
  seat: number,
  position: SceneSeat,
): void {
  const discards = player.discards ?? [];
  const known = player.riichi_discard_index;
  const declaration = typeof known === "number" && Number.isInteger(known) && known >= 0 && known < discards.length ? known : null;
  for (const [index, tile] of discards.entries()) {
    const rotation: Vec3 = [0, SEAT_ROTATIONS[position][1] + (index === declaration ? Math.PI / 2 : 0), 0];
    tiles.push(
      sceneTile(
        `discard-${position}-seat-${seat}-${index}`,
        tile,
        discardPosition(position, index, declaration),
        rotation,
        REMOTE_TILE_SIZE,
        "front",
        "discard",
      ),
    );
  }
}

function meldPosition(position: SceneSeat, index: number, scale: number): Vec3 {
  const edge = position === "bottom" || position === "top" ? 6 * TABLE_WIDTH_STRETCH : 5.25;
  const offset = index * (TILE_BODY_SIZE.width * scale + 0.02);
  if (position === "bottom") return [edge - offset, 0.3, 4.5];
  if (position === "top") return [-edge + offset, 0.3, -5.3];
  if (position === "right") return [6 * TABLE_WIDTH_STRETCH, 0.3, -edge + offset];
  return [-5.35 * TABLE_WIDTH_STRETCH - 0.1, 0.3, edge - offset];
}

function addMeldTiles(
  tiles: SceneTile[],
  player: ProjectedPlayer,
  seat: number,
  position: SceneSeat,
): void {
  let offset = 0;
  for (const [meldIndex, meld] of (player.melds ?? []).entries()) {
    const meldTiles = (meld.tiles ?? []).filter((tile) => Number.isInteger(tile));
    const scale = (position === "bottom" ? LOCAL_TILE_SIZE * 0.85 : REMOTE_TILE_SIZE) * 0.9;
    for (const [tileIndex, tile] of meldTiles.entries()) {
      tiles.push(
        sceneTile(
          `meld-${position}-seat-${seat}-${meldIndex}-${tileIndex}`,
          tile,
          meldPosition(position, offset++, scale),
          SEAT_ROTATIONS[position],
          scale,
          "front",
          "meld",
        ),
      );
    }
  }
}

function addWallTiles(tiles: SceneTile[], count: number, threePlayer: boolean, indicators: unknown): void {
  const edges = threePlayer ? ["bottom", "right", "left"] as const : WALL_EDGE_ORDER;
  const stacksPerEdge = threePlayer ? 18 : 17;
  const total = edges.length * stacksPerEdge * 2;
  const liveCapacity = total - 14;
  const markers = Array.isArray(indicators) ? indicators.filter((tile) => Number.isInteger(tile) && tile >= 0 && tile < 136).slice(0, 5) : [];
  // shortcut: illustrative fixed cut, upgrade when authoritative cut/rinshan positions exist.
  for (let index = liveCapacity - count; index < total; index += 1) {
    const edge = edges[Math.floor(index / (stacksPerEdge * 2))];
    const slot = index % (stacksPerEdge * 2);
    const stack = Math.floor(slot / 2);
    const upper = slot % 2 === 0;
    const dead = index >= liveCapacity;
    const offset = (stack - (stacksPerEdge - 1) / 2) * (TILE_BODY_SIZE.width * 0.65 + 0.008) + (dead ? 0.22 : 0);
    const height = upper ? 0.45 : 0.24;
    const position: Vec3 = edge === "bottom" ? [offset, height, 3.17]
      : edge === "right" ? [4.25, height, -offset]
      : edge === "top" ? [-offset, height, -3.17]
      : [-4.25, height, offset];
    const markerIndex = stacksPerEdge - 3 - stack;
    const marker = dead && upper && markerIndex >= 0 && markerIndex < markers.length ? markers[markerIndex] : null;
    tiles.push(sceneTile(
      marker !== null ? `dora-${markerIndex}` : `${dead ? "dead-wall" : "wall"}-${edge}-${stack}-${upper ? "upper" : "lower"}`,
      marker, position, SEAT_ROTATIONS[edge], 0.65,
      marker !== null ? "front" : "back", marker !== null ? "dora" : "wall",
    ));
  }
}

export function buildMatchSceneLayout(
  projection: ProjectedState,
  room: RoomSnapshot | null,
  surface: TableSurface = "live",
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
    addHandTiles(tiles, player, seat, position, surface);
    addDiscardTiles(tiles, player, seat, position);
    addMeldTiles(tiles, player, seat, position);
  }

  const wallCount = wallTileCount(projection.remaining_wall, geometry.length === 3 ? 94 : 122);
  addWallTiles(tiles, wallCount, geometry.length === 3, projection.dora_indicators);

  return { players, tiles, wallCount };
}
