import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CanvasTexture, LinearFilter, SRGBColorSpace } from "three";

import {
  ATLAS_CELL_HEIGHT,
  ATLAS_CELL_INSET_TEXELS,
  ATLAS_CELL_WIDTH,
  ATLAS_COLUMNS,
  atlasCellUvBounds,
  createTileAtlas,
} from "./tile-atlas";
import { tileFileName } from "./tiles";

type ImageMode = "load" | "error" | "pending";

let imageMode: ImageMode;
let loadedUrls: string[];
let drawImage: ReturnType<typeof vi.fn>;
let fillRect: ReturnType<typeof vi.fn>;
let fillStyle: string;
let originalGetContext: typeof HTMLCanvasElement.prototype.getContext;

class MockImage {
  onload: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  private source = "";

  get src(): string {
    return this.source;
  }

  set src(value: string) {
    this.source = value;
    if (!value) return;
    loadedUrls.push(value);
    if (imageMode === "pending") return;
    queueMicrotask(() => {
      if (this.source !== value) return;
      if (imageMode === "error") this.onerror?.(new Event("error"));
      else this.onload?.(new Event("load"));
    });
  }
}

function uniqueRepresentativeTiles(): number[] {
  const tiles = [...Array.from({ length: 136 }, (_, tile) => tile), -1];
  return tiles.filter(
    (tile, index) =>
      tiles.findIndex((candidate) => tileFileName(candidate) === tileFileName(tile)) === index,
  );
}

beforeEach(() => {
  imageMode = "load";
  loadedUrls = [];
  drawImage = vi.fn();
  fillRect = vi.fn();
  fillStyle = "";
  vi.stubGlobal("Image", MockImage);
  originalGetContext = HTMLCanvasElement.prototype.getContext;
  Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
    configurable: true,
    value: vi.fn(() => ({
      drawImage,
      fillRect,
      get fillStyle() { return fillStyle; },
      set fillStyle(value: string) { fillStyle = value; },
    })),
  });
});

afterEach(() => {
  Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
    configurable: true,
    value: originalGetContext,
  });
  vi.unstubAllGlobals();
});

describe("createTileAtlas", () => {
  it("loads and draws each unique face exactly once into deterministic cells", async () => {
    const representatives = uniqueRepresentativeTiles();
    const atlas = await createTileAtlas();

    expect(atlas.columns).toBe(ATLAS_COLUMNS);
    expect(atlas.rows).toBe(Math.ceil(representatives.length / ATLAS_COLUMNS));
    expect(loadedUrls).toHaveLength(representatives.length);
    expect(new Set(loadedUrls).size).toBe(representatives.length);
    expect(fillStyle).toBe("#eee5d2");
    expect(fillRect).toHaveBeenCalledTimes(representatives.length);
    expect(fillRect).toHaveBeenNthCalledWith(1, 0, 0, 128, 171);
    expect(drawImage).toHaveBeenCalledTimes(representatives.length);
    expect(fillRect.mock.invocationCallOrder[0]).toBeLessThan(drawImage.mock.invocationCallOrder[0]);

    const cells = representatives.map((tile) => JSON.stringify(atlas.cellFor(tile)));
    expect(new Set(cells).size).toBe(representatives.length);
    expect(atlas.cellFor(0)).toEqual([0, 0]);
  });

  it("keeps every atlas UV sample inside its cell by a half-texel boundary", () => {
    const rows = 5;
    const bounds = atlasCellUvBounds([3, 2], rows);
    const atlasWidth = ATLAS_COLUMNS * ATLAS_CELL_WIDTH;
    const atlasHeight = rows * ATLAS_CELL_HEIGHT;
    expect(bounds.minU).toBeCloseTo((3 * ATLAS_CELL_WIDTH + ATLAS_CELL_INSET_TEXELS) / atlasWidth, 12);
    expect(bounds.maxU).toBeCloseTo((4 * ATLAS_CELL_WIDTH - ATLAS_CELL_INSET_TEXELS) / atlasWidth, 12);
    expect(bounds.minV).toBeCloseTo((2 * ATLAS_CELL_HEIGHT + ATLAS_CELL_INSET_TEXELS) / atlasHeight, 12);
    expect(bounds.maxV).toBeCloseTo((3 * ATLAS_CELL_HEIGHT - ATLAS_CELL_INSET_TEXELS) / atlasHeight, 12);
    expect(bounds.minU).toBeGreaterThan(3 / ATLAS_COLUMNS);
    expect(bounds.maxU).toBeLessThan(4 / ATLAS_COLUMNS);
    expect(bounds.minV).toBeGreaterThan(2 / rows);
    expect(bounds.maxV).toBeLessThan(3 / rows);
  });

  it("shares normal copies, separates red fives, and uses the fallback cell", async () => {
    const atlas = await createTileAtlas();

    expect(atlas.cellFor(0)).toEqual(atlas.cellFor(1));
    expect(atlas.cellFor(16)).not.toEqual(atlas.cellFor(17));
    expect(atlas.cellFor(-99)).toEqual(atlas.cellFor(Number.NaN));
    expect(atlas.cellFor(136)).toEqual(atlas.cellFor(-99));
  });

  it("configures one sRGB atlas texture and disposes it once", async () => {
    const dispose = vi.spyOn(CanvasTexture.prototype, "dispose").mockImplementation(() => undefined);
    const atlas = await createTileAtlas();

    expect(atlas.texture).toBeInstanceOf(CanvasTexture);
    expect(atlas.texture.colorSpace).toBe(SRGBColorSpace);
    expect(atlas.texture.flipY).toBe(false);
    expect(atlas.texture.generateMipmaps).toBe(false);
    expect(atlas.texture.minFilter).toBe(LinearFilter);
    expect(atlas.texture.magFilter).toBe(LinearFilter);

    atlas.dispose();
    atlas.dispose();
    expect(dispose).toHaveBeenCalledTimes(1);
  });

  it("rejects an already-aborted build without starting image work", async () => {
    const controller = new AbortController();
    controller.abort();

    await expect(createTileAtlas(controller.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(loadedUrls).toHaveLength(0);
  });

  it("cancels the active image when aborted during loading", async () => {
    imageMode = "pending";
    const controller = new AbortController();
    const promise = createTileAtlas(controller.signal);
    await Promise.resolve();
    expect(loadedUrls).toHaveLength(1);

    controller.abort();
    await expect(promise).rejects.toMatchObject({ name: "AbortError" });
  });

  it("rejects image and draw failures without returning a partial atlas", async () => {
    imageMode = "error";
    await expect(createTileAtlas()).rejects.toThrow("Failed to load tile face");

    imageMode = "load";
    drawImage.mockImplementationOnce(() => {
      throw new Error("draw failed");
    });
    await expect(createTileAtlas()).rejects.toThrow("draw failed");
  });
});
