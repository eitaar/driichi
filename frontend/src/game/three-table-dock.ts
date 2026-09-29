import { PerspectiveCamera, Vector3 } from "three";
import { TABLE_RENDER_OFFSET, TILE_BODY_SIZE, type MatchSceneLayout } from "./three-table-layout";

// Move local calls on the table plane so their projected bounds meet the hand's right edge.
export function dockLocalCalls(layout: MatchSceneLayout, camera: PerspectiveCamera, width: number, height: number): MatchSceneLayout {
  const local = layout.tiles.filter((tile) => tile.group === "meld" && tile.key.startsWith("meld-bottom-"));
  if (!local.length || width < 700) return layout;
  const padding = 20;
  let dx = 0;
  let dz = 0;
  const onPlane = (x: number, y: number) => {
    const ray = new Vector3(x, y, 0.5).unproject(camera).sub(camera.position);
    return camera.position.clone().addScaledVector(ray, (local[0].position[1] - camera.position.y) / ray.y);
  };
  for (let iteration = 0; iteration < 5; iteration += 1) {
    let right = -Infinity;
    let bottom = Infinity;
    for (const tile of local) for (const x of [-1, 1]) for (const z of [-1, 1]) for (const y of [-1, 1]) {
      const point = new Vector3(
        tile.position[0] + dx + x * TILE_BODY_SIZE.width * tile.scale / 2,
        tile.position[1] + y * TILE_BODY_SIZE.height * tile.scale / 2,
        tile.position[2] + dz + z * TILE_BODY_SIZE.depth * tile.scale / 2 + TABLE_RENDER_OFFSET[2],
      ).project(camera);
      right = Math.max(right, point.x);
      bottom = Math.min(bottom, point.y);
    }
    const delta = onPlane(1 - 2 * padding / width, -1 + 2 * padding / height).sub(onPlane(right, bottom));
    dx += delta.x;
    dz += delta.z;
  }
  return { ...layout, tiles: layout.tiles.map((tile) => local.includes(tile)
    ? { ...tile, position: [tile.position[0] + dx, tile.position[1], tile.position[2] + dz] }
    : tile) };
}
