import type { AnimationItem } from "./animation";
import {
  CAMERA,
  type MatchSceneLayout,
  type SceneTile,
  type Vec3,
} from "./three-table-layout";

export interface SceneMotion {
  itemId: number;
  startedAt: number;
  durationMs: number;
  kind: "draw" | "discard" | "call" | "riichi" | "score" | "win";
  /** The authoritative event is retained so a transient accent can target its resulting tile. */
  event?: unknown;
}

const MOTION_BY_KIND = {
  draw: { kind: "draw", durationMs: 180 },
  discard: { kind: "discard", durationMs: 180 },
  call: { kind: "call", durationMs: 240 },
  riichi: { kind: "riichi", durationMs: 240 },
  score_change: { kind: "score", durationMs: 240 },
  win: { kind: "win", durationMs: 420 },
} as const;

export function nextSceneMotion(
  items: AnimationItem[],
  reducedMotion: boolean,
): SceneMotion | null {
  if (reducedMotion) return null;
  for (const item of items) {
    const motion = MOTION_BY_KIND[item.kind as keyof typeof MOTION_BY_KIND];
    if (motion) return { itemId: item.id, startedAt: 0, event: item.event, ...motion };
  }
  return null;
}

export function cameraAccentAt(
  kind: SceneMotion["kind"],
  progress: number,
): { fov: number; target: Vec3 } {
  if (kind !== "win") return { fov: CAMERA.fov, target: CAMERA.target };
  const boundedProgress = Math.min(1, Math.max(0, progress));
  if (boundedProgress === 0 || boundedProgress === 1) {
    return { fov: CAMERA.fov, target: CAMERA.target };
  }
  const pulse = Math.sin(Math.PI * boundedProgress);
  return {
    fov: CAMERA.fov - 1.5 * pulse,
    target: [
      CAMERA.target[0],
      CAMERA.target[1] + 0.08 * pulse,
      CAMERA.target[2] + 0.22 * pulse,
    ],
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function eventPayload(event: unknown): Record<string, unknown> | null {
  if (!isRecord(event)) return null;
  if (isRecord(event.event)) return eventPayload(event.event);
  for (const [key, value] of Object.entries(event)) {
    if (
      [
        "tsumo",
        "dahai",
        "chi",
        "pon",
        "daiminkan",
        "kakan",
        "ankan",
        "hora",
        "reach",
        "reach_accepted",
        "score_change",
        "scores_changed",
      ].includes(key.toLowerCase())
      && isRecord(value)
    ) {
      return value;
    }
  }
  return event;
}

function integerField(payload: Record<string, unknown>, key: string): number | null {
  const value = payload[key];
  return typeof value === "number" && Number.isInteger(value) && value >= 0
    ? value
    : null;
}

function eventTile(payload: Record<string, unknown>): number | null {
  for (const key of ["tile", "pai", "called"]) {
    const value = payload[key];
    if (typeof value === "number" && Number.isInteger(value)) return value;
  }
  return null;
}

function tileBelongsToSeat(tile: SceneTile, seat: number): boolean {
  return tile.key.includes(`-seat-${seat}-`);
}

function lastMatchingTile(
  tiles: readonly SceneTile[],
  seat: number,
  groups: readonly SceneTile["group"][],
  tileId: number | null,
): SceneTile | null {
  const candidates = tiles.filter(
    (tile) => groups.includes(tile.group) && tileBelongsToSeat(tile, seat),
  );
  if (candidates.length === 0) return null;
  if (tileId !== null) {
    const matching = candidates.filter((tile) => tile.tile === tileId);
    return matching.at(-1) ?? null;
  }
  return candidates.at(-1) ?? null;
}

/**
 * Find only an authoritative resulting tile. The event stream gives us an actor,
 * and usually a tile identity; it does not provide a source/destination path.
 * Missing identity/location data intentionally produces no accent instead of a
 * guessed tile or a transform of the persistent population.
 */
export function sceneMotionTarget(
  layout: MatchSceneLayout,
  motion: Pick<SceneMotion, "kind" | "event">,
): SceneTile | null {
  const payload = eventPayload(motion.event);
  if (!payload) return null;
  const actor = integerField(payload, "actor");
  if (actor === null) return null;
  const eventTarget = integerField(payload, "target");
  const targetSeat =
    motion.kind === "win" && eventTarget !== null && eventTarget !== actor
      ? eventTarget
      : actor;
  const tileId = eventTile(payload);

  switch (motion.kind) {
    case "discard":
      return lastMatchingTile(layout.tiles, targetSeat, ["discard"], tileId);
    case "draw":
      return lastMatchingTile(layout.tiles, targetSeat, ["hand"], tileId);
    case "call":
      return lastMatchingTile(layout.tiles, targetSeat, ["meld"], tileId);
    case "riichi":
      return lastMatchingTile(layout.tiles, targetSeat, ["discard", "hand"], tileId);
    case "win":
      return lastMatchingTile(
        layout.tiles,
        targetSeat,
        targetSeat === actor ? ["hand", "discard", "meld"] : ["discard"],
        tileId,
      );
    case "score":
      return null;
  }
}

export function sceneMotionProgress(motion: SceneMotion, now: number): number {
  if (
    !Number.isFinite(motion.startedAt)
    || !Number.isFinite(motion.durationMs)
    || motion.durationMs <= 0
    || !Number.isFinite(now)
  ) return 0;
  return Math.min(1, Math.max(0, (now - motion.startedAt) / motion.durationMs));
}
