import { Euler, PerspectiveCamera, Vector3 } from "three";

import {
  CAMERA,
  TABLE_RENDER_OFFSET,
  TILE_BODY_SIZE,
  type MatchSceneLayout,
  type SceneTile,
} from "./three-table-layout";

export const TABLE_ASPECT_RATIO = 16 / 9;

export interface SceneRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface HandHitTarget {
  index: number;
  tileKey: string;
  rect: SceneRect;
}

export interface HandHitTargetLayout {
  targets: HandHitTarget[];
  bounds: SceneRect;
}

function sceneCamera(aspect: number): PerspectiveCamera {
  const camera = new PerspectiveCamera(
    CAMERA.fov,
    Number.isFinite(aspect) && aspect > 0 ? aspect : TABLE_ASPECT_RATIO,
    CAMERA.near,
    CAMERA.far,
  );
  camera.position.set(...CAMERA.position);
  camera.lookAt(...CAMERA.target);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld(true);
  return camera;
}

function projectedTileRect(tile: SceneTile, camera: PerspectiveCamera): SceneRect {
  const halfWidth = (TILE_BODY_SIZE.width * tile.scale) / 2;
  const halfHeight = (TILE_BODY_SIZE.height * tile.scale) / 2;
  const halfDepth = (TILE_BODY_SIZE.depth * tile.scale) / 2;
  const rotation = new Euler(...tile.rotation);
  const center = new Vector3(
    tile.position[0] + TABLE_RENDER_OFFSET[0],
    tile.position[1] + TABLE_RENDER_OFFSET[1],
    tile.position[2] + TABLE_RENDER_OFFSET[2],
  );
  const corners: Vector3[] = [];
  for (const x of [-halfWidth, halfWidth]) {
    for (const y of [-halfHeight, halfHeight]) {
      for (const z of [-halfDepth, halfDepth]) {
        corners.push(new Vector3(x, y, z).applyEuler(rotation).add(center));
      }
    }
  }
  const projected = corners.map((corner) => corner.project(camera));
  const minX = Math.min(...projected.map(({ x }) => x));
  const maxX = Math.max(...projected.map(({ x }) => x));
  const minY = Math.min(...projected.map(({ y }) => y));
  const maxY = Math.max(...projected.map(({ y }) => y));
  const rawLeft = (minX + 1) / 2;
  const rawTop = (1 - maxY) / 2;
  const rawRight = (maxX + 1) / 2;
  const rawBottom = (1 - minY) / 2;
  const left = Math.max(0, Math.min(1, rawLeft));
  const top = Math.max(0, Math.min(1, rawTop));
  const right = Math.max(left, Math.min(1, rawRight));
  const bottom = Math.max(top, Math.min(1, rawBottom));
  return {
    left,
    top,
    width: right - left,
    height: bottom - top,
  };
}

export function projectSceneTileRect(
  tile: SceneTile,
  aspect = TABLE_ASPECT_RATIO,
): SceneRect {
  return projectedTileRect(tile, sceneCamera(aspect));
}

export function projectLocalHandHitTargets(
  layout: MatchSceneLayout,
  aspect = TABLE_ASPECT_RATIO,
): HandHitTargetLayout {
  const camera = sceneCamera(aspect);
  const handTiles = layout.tiles.filter(
    (tile) => tile.group === "hand" && tile.key.startsWith("hand-bottom-seat-"),
  );
  const targets = handTiles.map((tile, index) => ({
    index,
    tileKey: tile.key,
    rect: projectedTileRect(tile, camera),
  }));
  if (!targets.length) {
    return { targets, bounds: { left: 0, top: 0, width: 0, height: 0 } };
  }
  const left = Math.min(...targets.map(({ rect }) => rect.left));
  const top = Math.min(...targets.map(({ rect }) => rect.top));
  const right = Math.max(...targets.map(({ rect }) => rect.left + rect.width));
  const bottom = Math.max(...targets.map(({ rect }) => rect.top + rect.height));
  return {
    targets,
    bounds: { left, top, width: right - left, height: bottom - top },
  };
}
