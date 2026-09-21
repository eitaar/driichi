/// <reference types="node" />

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

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
});
