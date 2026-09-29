import { Group } from "three";
import { describe, expect, it } from "vitest";
import { applyMotionAccentFrame } from "./three-table-scene";

describe("table motion accent", () => {
  it("moves the transient discard accent without changing the persistent tile group", () => {
    const persistentTiles = new Group();
    persistentTiles.updateMatrix();
    const before = persistentTiles.matrix.clone();
    const accent = new Group();
    const target = {
      key: "discard-bottom-seat-0-0",
      tile: 0,
      position: [0, 0.2, 1.73] as const,
      rotation: [0, 0, 0] as const,
      scale: 0.72,
      face: "front" as const,
      group: "discard" as const,
    };

    applyMotionAccentFrame(accent, target, "discard", 0.5);

    expect(persistentTiles.matrix.equals(before)).toBe(true);
    expect(accent.position.y).toBeGreaterThan(target.position[1]);
  });
});
