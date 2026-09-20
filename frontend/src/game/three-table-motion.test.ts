import { describe, expect, it } from "vitest";

import type { AnimationItem, AnimationKind } from "./animation";
import { cameraAccentAt, nextSceneMotion, sceneMotionProgress } from "./three-table-motion";
import { CAMERA } from "./three-table-layout";

function item(id: number, kind: AnimationKind): AnimationItem {
  return { id, kind, event: { type: kind } };
}

describe("nextSceneMotion", () => {
  it.each([
    ["draw", "draw", 140, 220],
    ["discard", "discard", 140, 220],
    ["call", "call", 180, 300],
    ["riichi", "riichi", 180, 300],
    ["score_change", "score", 180, 300],
    ["win", "win", 350, 500],
  ] as const)(
    "maps %s into bounded %s motion",
    (animationKind, motionKind, minimum, maximum) => {
      const motion = nextSceneMotion([item(7, animationKind)], false);

      expect(motion).toMatchObject({ itemId: 7, kind: motionKind, startedAt: 0 });
      expect(motion?.durationMs).toBeGreaterThanOrEqual(minimum);
      expect(motion?.durationMs).toBeLessThanOrEqual(maximum);
    },
  );

  it("ignores animation kinds without an approved scene mapping", () => {
    expect(
      nextSceneMotion(
        [item(1, "kyoku_start"), item(2, "kyoku_end"), item(3, "discard")],
        false,
      ),
    ).toMatchObject({ itemId: 3, kind: "discard" });
  });

  it("returns no motion when Reduced Motion is enabled", () => {
    expect(nextSceneMotion([item(4, "win")], true)).toBeNull();
  });
});

describe("cameraAccentAt", () => {
  it("adds one bounded win accent and restores the fixed camera exactly", () => {
    expect(cameraAccentAt("win", 0)).toEqual({ fov: CAMERA.fov, target: CAMERA.target });
    expect(cameraAccentAt("win", 0.5)).toEqual({
      fov: CAMERA.fov - 1.5,
      target: [CAMERA.target[0], CAMERA.target[1] + 0.08, CAMERA.target[2] + 0.22],
    });
    expect(cameraAccentAt("win", 1)).toEqual({ fov: CAMERA.fov, target: CAMERA.target });
  });

  it("leaves the fixed camera unchanged for non-win motion", () => {
    expect(cameraAccentAt("discard", 0.5)).toEqual({ fov: CAMERA.fov, target: CAMERA.target });
  });
});

describe("sceneMotionProgress", () => {
  const motion = {
    itemId: 9,
    startedAt: 100,
    durationMs: 200,
    kind: "discard" as const,
  };

  it.each([
    [50, 0],
    [100, 0],
    [200, 0.5],
    [300, 1],
    [500, 1],
  ])("clamps progress at now=%s", (now, expected) => {
    expect(sceneMotionProgress(motion, now)).toBe(expected);
  });

  it.each([Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY])(
    "returns bounded progress for malformed now=%s",
    (now) => {
      const progress = sceneMotionProgress(motion, now);
      expect(Number.isFinite(progress)).toBe(true);
      expect(progress).toBeGreaterThanOrEqual(0);
      expect(progress).toBeLessThanOrEqual(1);
    },
  );

  it.each([
    { ...motion, startedAt: Number.NaN },
    { ...motion, durationMs: Number.NaN },
    { ...motion, durationMs: 0 },
    { ...motion, durationMs: -1 },
  ])("returns bounded progress for malformed motion time %#", (malformed) => {
    const progress = sceneMotionProgress(malformed, 200);
    expect(Number.isFinite(progress)).toBe(true);
    expect(progress).toBeGreaterThanOrEqual(0);
    expect(progress).toBeLessThanOrEqual(1);
  });
});
