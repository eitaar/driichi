import { DoubleSide, Group, Mesh, MeshBasicMaterial, Raycaster, Vector3 } from "three";
import { describe, expect, it } from "vitest";
import { applyMotionAccentFrame, createTableFrameGeometry } from "./three-table-scene";

describe("rounded table frame", () => {
  it("forms a continuous perimeter with an actual recessed opening", () => {
    const geometry = createTableFrameGeometry(14, 12, 13.1, 11.1, 0.48, 0.15);
    const material = new MeshBasicMaterial({ side: DoubleSide });
    const frame = new Mesh(geometry, material);
    frame.updateMatrixWorld();
    const ray = new Raycaster(new Vector3(0, 5, 0), new Vector3(0, -1, 0));
    expect(ray.intersectObject(frame)).toHaveLength(0);
    for (const [x, z] of [[6.8, 0], [-6.8, 0], [0, 5.8], [0, -5.8]]) {
      ray.set(new Vector3(x, 5, z), new Vector3(0, -1, 0));
      expect(ray.intersectObject(frame).length).toBeGreaterThan(0);
    }
    geometry.dispose();
    material.dispose();
  });
});

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
