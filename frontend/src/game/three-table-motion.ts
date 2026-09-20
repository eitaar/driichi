import type { AnimationItem } from "./animation";

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

export function sceneMotionProgress(motion: SceneMotion, now: number): number {
  if (
    !Number.isFinite(motion.startedAt)
    || !Number.isFinite(motion.durationMs)
    || motion.durationMs <= 0
    || !Number.isFinite(now)
  ) return 0;
  return Math.min(1, Math.max(0, (now - motion.startedAt) / motion.durationMs));
}
