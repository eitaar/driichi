import {
  CanvasTexture,
  LinearFilter,
  SRGBColorSpace,
  type Texture,
} from "three";

import { tileAssetUrl, tileFileName } from "./tiles";

export const ATLAS_COLUMNS = 8;
export const ATLAS_CELL_WIDTH = 256;
export const ATLAS_CELL_HEIGHT = 342;
export const ATLAS_CELL_INSET_TEXELS = 0.5;
const CELL_WIDTH = ATLAS_CELL_WIDTH;
const CELL_HEIGHT = ATLAS_CELL_HEIGHT;
const PHYSICAL_TILE_IDS = Array.from({ length: 136 }, (_, tile) => tile);
const FALLBACK_TILE = -1;

interface AtlasEntry {
  readonly tile: number;
  readonly fileName: string;
}

const ATLAS_ENTRIES: readonly AtlasEntry[] = (() => {
  const seen = new Set<string>();
  const entries: AtlasEntry[] = [];

  for (const tile of [...PHYSICAL_TILE_IDS, FALLBACK_TILE]) {
    const fileName = tileFileName(tile);
    if (seen.has(fileName)) continue;
    seen.add(fileName);
    entries.push(Object.freeze({ tile, fileName }));
  }

  return Object.freeze(entries);
})();

const CELL_BY_FILE = new Map<string, readonly [number, number]>(
  ATLAS_ENTRIES.map((entry, index) => [
    entry.fileName,
    Object.freeze([
      index % ATLAS_COLUMNS,
      Math.floor(index / ATLAS_COLUMNS),
    ]) as readonly [number, number],
  ]),
);
const FALLBACK_CELL = CELL_BY_FILE.get(tileFileName(FALLBACK_TILE))!;

export interface TileAtlas {
  readonly texture: CanvasTexture;
  readonly columns: number;
  readonly rows: number;
  cellFor(tile: number): readonly [number, number];
  dispose(): void;
}

export interface AtlasUvBounds {
  readonly minU: number;
  readonly maxU: number;
  readonly minV: number;
  readonly maxV: number;
}

/**
 * Return the normalized top-left-origin bounds for one atlas cell after a
 * half-texel inset. Linear filtering can then never sample a neighboring
 * face, while the source image keeps its full 300:400 aspect inside the cell.
 */
export function atlasCellUvBounds(
  [column, row]: readonly [number, number],
  rows: number,
): AtlasUvBounds {
  const safeRows = Math.max(1, Math.floor(rows));
  const width = ATLAS_COLUMNS * ATLAS_CELL_WIDTH;
  const height = safeRows * ATLAS_CELL_HEIGHT;
  return {
    minU: (column * ATLAS_CELL_WIDTH + ATLAS_CELL_INSET_TEXELS) / width,
    maxU: ((column + 1) * ATLAS_CELL_WIDTH - ATLAS_CELL_INSET_TEXELS) / width,
    minV: (row * ATLAS_CELL_HEIGHT + ATLAS_CELL_INSET_TEXELS) / height,
    maxV: ((row + 1) * ATLAS_CELL_HEIGHT - ATLAS_CELL_INSET_TEXELS) / height,
  };
}

function abortError(): DOMException {
  return new DOMException("Tile atlas creation aborted", "AbortError");
}

function loadImage(url: string, signal?: AbortSignal): Promise<HTMLImageElement> {
  if (signal?.aborted) return Promise.reject(abortError());

  return new Promise((resolve, reject) => {
    const image = new Image();
    let settled = false;

    const cleanup = () => {
      image.onload = null;
      image.onerror = null;
      signal?.removeEventListener("abort", onAbort);
    };
    const succeed = () => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(image);
    };
    const fail = () => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new Error(`Failed to load tile face: ${url}`));
    };
    const onAbort = () => {
      if (settled) return;
      settled = true;
      cleanup();
      image.src = "";
      reject(abortError());
    };

    image.onload = succeed;
    image.onerror = fail;
    signal?.addEventListener("abort", onAbort, { once: true });
    image.src = url;
  });
}

function disposeOnce(texture: Texture): () => void {
  let disposed = false;
  return () => {
    if (disposed) return;
    disposed = true;
    texture.dispose();
  };
}

export async function createTileAtlas(signal?: AbortSignal): Promise<TileAtlas> {
  if (signal?.aborted) throw abortError();

  const rows = Math.ceil(ATLAS_ENTRIES.length / ATLAS_COLUMNS);
  const canvas = document.createElement("canvas");
  canvas.width = ATLAS_COLUMNS * CELL_WIDTH;
  canvas.height = rows * CELL_HEIGHT;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Canvas 2D context is unavailable");

  for (let index = 0; index < ATLAS_ENTRIES.length; index += 1) {
    if (signal?.aborted) throw abortError();
    const entry = ATLAS_ENTRIES[index];
    const image = await loadImage(tileAssetUrl(entry.tile), signal);
    if (signal?.aborted) throw abortError();
    const x = (index % ATLAS_COLUMNS) * CELL_WIDTH;
    const y = Math.floor(index / ATLAS_COLUMNS) * CELL_HEIGHT;
    context.fillStyle = "#eee5d2";
    context.fillRect(x, y, CELL_WIDTH, CELL_HEIGHT);
    context.drawImage(image, x, y, CELL_WIDTH, CELL_HEIGHT);
  }

  const texture = new CanvasTexture(canvas);
  texture.colorSpace = SRGBColorSpace;
  texture.flipY = false;
  texture.generateMipmaps = false;
  texture.minFilter = LinearFilter;
  texture.magFilter = LinearFilter;
  texture.needsUpdate = true;

  return Object.freeze({
    texture,
    columns: ATLAS_COLUMNS,
    rows,
    cellFor(tile: number): readonly [number, number] {
      return CELL_BY_FILE.get(tileFileName(tile)) ?? FALLBACK_CELL;
    },
    dispose: disposeOnce(texture),
  });
}
