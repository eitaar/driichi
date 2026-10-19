import { expect, it } from "vitest";
import { fitRadar, radarUpperBound } from "./benchmark-chart-geometry";

it.each([[33.3, 50], [20, 25], [50, 75], [100, 100]])("scales %s percent to the shared %s bound", (percentage, bound) => {
  expect(radarUpperBound([{ rankCounts: [percentage] }], 100)).toBe(bound);
});
it("uses the full roster regardless of filtered axes, with fallback and cap", () => {
  const roster = [{ rankCounts: [20, 20] }, { rankCounts: [50, 50] }];
  expect(radarUpperBound(roster, 100)).toBe(75);
  expect(radarUpperBound(roster, 0)).toBe(25);
  expect(radarUpperBound([], 100)).toBe(25);
  expect(radarUpperBound([{ rankCounts: [200] }], 100)).toBe(100);
});
it.each([[320, 280, 60, 18, 16], [280, 480, 70, 36, 32], [800, 300, 110, 20, 16]])("fits the measured rectangle at width %s", (width, legendTop, labelWidth, labelHeight, rem) => {
  const { cx, cy, radius } = fitRadar(width, legendTop, labelWidth, labelHeight, rem);
  expect([cx, cy, radius].every(Number.isFinite)).toBe(true);
  expect(radius).toBeGreaterThan(0);
  expect(cx - radius - labelWidth - 8).toBeGreaterThanOrEqual(rem / 2);
  expect(cx + radius + labelWidth + 8).toBeLessThanOrEqual(width - rem / 2);
  expect(cy - radius - labelHeight - 8).toBeGreaterThanOrEqual(rem / 2);
  expect(cy + radius + labelHeight + 8).toBeLessThanOrEqual(legendTop - rem / 2);
});
