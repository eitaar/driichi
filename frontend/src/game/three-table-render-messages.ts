import type { SceneRenderStats } from "./three-table-scene";
import type { MatchSceneLayout } from "./three-table-layout";
import type { SceneMotion } from "./three-table-motion";
import type { ProjectedState } from "./types";

export interface TableRenderAssets {
  atlas?: { bitmap: ImageBitmap; columns: number; rows: number; cells: readonly (readonly [number, number])[] };
  center?: ImageBitmap;
  surface?: ImageBitmap | null;
  surround?: ImageBitmap | null;
}
export interface TableRenderInput {
  type: "init" | "render";
  revision: number;
  canvas?: OffscreenCanvas;
  scene?: { layout: MatchSceneLayout; projection: ProjectedState };
  assets: TableRenderAssets;
  width: number;
  height: number;
  dpr: number;
  windowTimeOrigin: number;
  motion: SceneMotion | null;
  sheen: { enabled: boolean; reduced: boolean; visible: boolean };
}
export interface TableRenderQuality {
  antialias: boolean;
  clearAlpha: number;
  pixelRatio: number;
  width: number;
  height: number;
  precision: string;
  colorSpace: string;
  toneMapping: number;
  shadowType: number;
  shaderChecks: boolean;
}
export type TableRenderReply =
  | { type: "created" }
  | { type: "processed"; revision: number }
  | { type: "ready"; revision: number; stats: SceneRenderStats; quality: TableRenderQuality }
  | { type: "motion-frame"; itemId: number; now: number; pixelRatio: number }
  | { type: "motion-complete"; itemId: number }
  | { type: "fatal" }
  | { type: "disposed" };
