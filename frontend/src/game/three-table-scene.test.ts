/// <reference types="node" />

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Group } from "three";
import { TILE_BODY_SIZE } from "./three-table-layout";
import { describe, expect, it } from "vitest";

import { applyMotionAccentFrame, TABLE_RAIL_PALETTE } from "./three-table-scene";

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

  it("restores a shared dimensional body resource with lighting-responsive material", () => {
    const tileSource = componentSource("function InstancedTiles", "function resetMotionGroup");
    expect(TILE_BODY_SIZE.height).toBeGreaterThanOrEqual(0.18);
    expect(tileSource).toContain("createTileSideGeometry()");
    expect(tileSource).toContain("new MeshLambertMaterial");
    expect(tileSource).toContain("args={[resources.bodyGeometry, resources.bodyMaterial, MAX_TILE_INSTANCES]}");
    expect(tileSource).not.toMatch(/new\s+Mesh\s*\(/);
  });

  it("uses persistent uniform rail batches instead of per-instance colors", () => {
    const railsSource = componentSource("function TableRails", "function FeltSeams");
    const colors = Object.values(TABLE_RAIL_PALETTE);
    const channelLuminance = (hex: string) => {
      const channels = [0, 2, 4].map((offset) => Number.parseInt(hex.slice(offset + 1, offset + 3), 16) / 255);
      const linear = channels.map((channel) => (
        channel <= 0.04045
          ? channel / 12.92
          : ((channel + 0.055) / 1.055) ** 2.4
      ));
      return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    };
    const channelDelta = (foreground: string, background: string) =>
      [0, 2, 4].map((offset) =>
        Number.parseInt(foreground.slice(offset + 1, offset + 3), 16)
        - Number.parseInt(background.slice(offset + 1, offset + 3), 16),
      );
    const background = "#050709";
    const backgroundLuminance = channelLuminance(background);
    const minimumOuterLuminance = channelLuminance("#2b211a") - backgroundLuminance;
    const minimumBronzeLuminance = channelLuminance("#8f653f") - backgroundLuminance;

    expect(colors).toHaveLength(5);
    expect(new Set(colors)).toHaveLength(4);
    expect(colors.every((color) => channelLuminance(color) - backgroundLuminance >= minimumOuterLuminance)).toBe(true);
    expect(channelLuminance(TABLE_RAIL_PALETTE.bronze) - backgroundLuminance).toBeGreaterThanOrEqual(minimumBronzeLuminance);
    expect(channelDelta(TABLE_RAIL_PALETTE.walnut, background)).toEqual([85, 46, 25]);
    expect(channelDelta(TABLE_RAIL_PALETTE.bronze, background)).toEqual([172, 119, 70]);
    expect(railsSource).toContain("const resources = useMemo");
    expect(railsSource).toContain("geometry: new BoxGeometry(1, 1, 1)");
    expect(railsSource.match(/new MeshBasicMaterial/g)).toHaveLength(4);
    expect(railsSource.match(/toneMapped: false/g)).toHaveLength(4);
    expect(railsSource.match(/<instancedMesh\b/g)).toHaveLength(5);
    expect(railsSource.match(/args=\{\[resources\.geometry,/g)).toHaveLength(5);
    expect(railsSource).toContain("resources.materials.bronze");
    expect(railsSource).not.toContain("vertexColors");
    expect(railsSource).not.toContain("setColorAt");
    expect(railsSource).not.toContain("instanceColor");
    expect(railsSource).not.toContain("onUpdate");
  });

  it("updates rail matrices once and disposes the shared geometry/materials once", () => {
    const railsSource = componentSource("function TableRails", "function FeltSeams");

    expect(railsSource).toContain("useLayoutEffect");
    expect(railsSource).not.toContain("useFrame");
    expect(railsSource.match(/applyTableParts\([^)]*Ref\.current/g)).toHaveLength(5);
    expect(railsSource.match(/resources\.geometry\.dispose\(\)/g)).toHaveLength(1);
    expect(railsSource.match(/resources\.materials\.[a-zA-Z]+\.dispose\(\)/g)).toHaveLength(4);
    expect(railsSource).toContain("resources.materials.bronze");
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
