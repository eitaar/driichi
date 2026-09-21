/// <reference types="node" />

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Group } from "three";
import { describe, expect, it } from "vitest";

import { applyMotionAccentFrame } from "./three-table-scene";

const sceneSource = readFileSync(
  resolve(process.cwd(), "src/game/three-table-scene.tsx"),
  "utf8",
);

function componentSource(start: string, end: string): string {
  const startIndex = sceneSource.indexOf(start);
  const endIndex = sceneSource.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing scene source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing scene source marker: ${end}`).toBeGreaterThan(startIndex);
  return sceneSource.slice(startIndex, endIndex);
}

describe("persistent motion scene contents", () => {
  it("does not use motion state to hide or remount visible scene resources", () => {
    const tileSource = componentSource("function InstancedTiles", "function resetMotionGroup");
    const railsSource = componentSource("function TableRails", "function FeltSeams");
    const tableSource = componentSource("function ProceduralTable", "export function MatchTableScene");

    expect(sceneSource).not.toContain("motionActive");
    expect(sceneSource).not.toMatch(/\bvisible\s*=/);
    expect(tileSource).toContain('name="tile-front-bodies"');
    expect(tileSource).toContain('name="tile-back-bodies"');
    expect(railsSource).toContain('name="table-rails"');
    expect(railsSource).toContain('name="table-walnut-inner-rail"');
    expect(railsSource).toContain('name="table-bronze-inlay"');
    expect(railsSource).toContain('name="table-corner-caps"');
    expect(railsSource).toContain('name="table-corner-accents"');
    expect(tableSource).toContain("<FeltSeams />");
    expect(tableSource).toContain("<CenterTrim />");
    expect(sceneSource).toContain("<ProceduralTable textures={textures} />");
    expect(sceneSource).toContain("<InstancedTiles layout={layout} atlas={atlas} />");
  });

  it("keeps the felt map independent of the active motion prop", () => {
    const tableSource = componentSource("function ProceduralTable", "export function MatchTableScene");

    expect(tableSource).toContain("<FeltMaterial texture={textures?.felt} />");
    expect(tableSource).not.toMatch(/FeltMaterial[\s\S]*motion/);
    expect(tableSource).not.toMatch(/textures\?\.felt[\s\S]*motion/);
  });

  it("moves only the transient discard accent while persistent tile groups stay stationary", () => {
    const persistentTiles = new Group();
    persistentTiles.position.set(0, 0, 0);
    persistentTiles.scale.set(1, 1, 1);
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
    expect(persistentTiles.position.toArray()).toEqual([0, 0, 0]);
    expect(persistentTiles.scale.toArray()).toEqual([1, 1, 1]);
    expect(accent.position.y).toBeGreaterThan(target.position[1]);
    const motionSource = componentSource("function MotionController", "function FixedCamera");
    expect(motionSource).not.toMatch(/\\bgroup\\.(position|scale)\\b/);
    expect(sceneSource).toContain("sceneMotionTarget");
  });
});
