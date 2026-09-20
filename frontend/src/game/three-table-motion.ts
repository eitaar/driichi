import type { AnimationItem } from "./animation";
import { CAMERA, type Vec3 } from "./three-table-layout";

export interface SceneMotion {
  itemId: number;
  startedAt: number;
  durationMs: number;
  kind: "draw" | "discard" | "call" | "riichi" | "score" | "win";
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
    if (motion) return { itemId: item.id, startedAt: 0, ...motion };
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

export function sceneMotionProgress(motion: SceneMotion, now: number): number {
  if (
    !Number.isFinite(motion.startedAt)
    || !Number.isFinite(motion.durationMs)
    || motion.durationMs <= 0
    || !Number.isFinite(now)
  ) return 0;
  return Math.min(1, Math.max(0, (now - motion.startedAt) / motion.durationMs));
}
