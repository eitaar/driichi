import { describe, expect, it } from "vitest";
import {
  TABLE_HEIGHT,
  TABLE_RATIO,
  TABLE_WIDTH,
  tableSeatGeometry,
  wallPlacements,
  wallTileCount,
} from "./table-geometry";

describe("immersive table geometry", () => {
  it("keeps the established 1600 by 900 scene", () => {
    expect([TABLE_WIDTH, TABLE_HEIGHT, TABLE_RATIO]).toEqual([
      1600,
      900,
      1600 / 900,
    ]);
  });

  it("puts the viewer at bottom and never invents a fourth sanma Seat", () => {
    expect(tableSeatGeometry("4p-red-east", 2).map(({ seat, position }) => [seat, position])).toEqual([
      [2, "bottom"],
      [3, "right"],
      [0, "top"],
      [1, "left"],
    ]);
    expect(tableSeatGeometry("3p-red-east", 1).map(({ seat, position }) => [seat, position])).toEqual([
      [1, "bottom"],
      [2, "right"],
      [0, "left"],
    ]);
  });

  it("normalizes malformed wall counts without exceeding a full wall", () => {
    expect(wallTileCount(undefined)).toBe(0);
    expect(wallTileCount(-2)).toBe(0);
    expect(wallTileCount(3.9)).toBe(3);
    expect(wallTileCount(999)).toBe(136);
  });

  it("creates one deterministic in-bounds placement per remaining tile", () => {
    const placements = wallPlacements(69);
    expect(placements).toHaveLength(69);
    expect(new Set(placements.map(({ x, y }) => `${x}:${y}`)).size).toBe(69);
    for (const point of placements) {
      expect(point.x).toBeGreaterThanOrEqual(0);
      expect(point.x).toBeLessThanOrEqual(TABLE_WIDTH);
      expect(point.y).toBeGreaterThanOrEqual(0);
      expect(point.y).toBeLessThanOrEqual(TABLE_HEIGHT);
    }
  });
});
