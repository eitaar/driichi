import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
  Box3,
  BoxGeometry,
  BufferGeometry,
  Float32BufferAttribute,
  Color,
  Euler,
  ExtrudeGeometry,
  Path,
  Shape,
  InstancedBufferAttribute,
  Group,
  InstancedMesh,
  Matrix4,
  MeshBasicMaterial,
  MeshPhongMaterial,
  PerspectiveCamera,
  PlaneGeometry,
  Quaternion,
  Scene,
  Vector3,
} from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import {
  ATLAS_CELL_HEIGHT,
  ATLAS_CELL_INSET_TEXELS,
  ATLAS_CELL_WIDTH,
  type TileAtlas,
} from "./tile-atlas";
import { isDora } from "./dora";
import type { DoraSheenClock } from "./dora-sheen";
import { createTableImageTexture, type LocalTableImage, type TableAppearance } from "./table-appearance";
import { TableCenter } from "./table-center";
import { dockLocalCalls } from "./three-table-dock";
import type { ProjectedState } from "./types";
import {
  CAMERA,
  LOCAL_TILE_SIZE,
  TABLE_RENDER_OFFSET,
  TABLE_WIDTH_STRETCH,
  TILE_BODY_HEIGHTS,
  TILE_BODY_SIZE,
  type MatchSceneLayout,
  type SceneTile,
} from "./three-table-layout";
import {
  cameraAccentAt,
  sceneMotionProgress,
  sceneMotionTarget,
  type SceneMotion,
} from "./three-table-motion";

export interface SceneRenderStats {
  tileCount: number;
  primitiveCount: number;
  tableHeightRatio?: number;
  tableWidthRatio?: number;
  pixelRatio?: number;
  triangleCount?: number;
}

interface MatchTableSceneProps {
  layout: MatchSceneLayout;
  projection: ProjectedState;
  atlas: TileAtlas;
  sheen: DoraSheenClock;
  appearance?: TableAppearance;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number, pixelRatio: number): void;
  onRenderReady(stats: SceneRenderStats): void;
}

// This is a framing translation only. Geometry remains in the authored world
// dimensions; in particular, no axis is scaled at runtime.
const BODY_SIZE = [TILE_BODY_SIZE.width, TILE_BODY_SIZE.height, TILE_BODY_SIZE.depth] as const;
// The vendored 300:400 source face is preserved on a smaller plane so the
// shared white body reads as an intentional ceramic rim instead of a hairline.
export const FACE_SIZE = [TILE_BODY_SIZE.width, TILE_BODY_SIZE.depth] as const;
export const BACK_FACE_SIZE = [0.54, 0.78] as const;
const MAX_TILE_INSTANCES = 256;

export function createBackFaceGeometry(): PlaneGeometry {
  return new PlaneGeometry(...BACK_FACE_SIZE);
}

/**
 * Keep one shared top-rim and four vertical sidewalls for every tile. The
 * face plane is intentionally inset, so the top rim is part of the visible
 * white material; the occluded bottom surface remains omitted.
 */
export function createTileSideGeometry(): BufferGeometry {
  const halfWidth = BODY_SIZE[0] / 2;
  const halfHeight = BODY_SIZE[1] / 2;
  const halfDepth = BODY_SIZE[2] / 2;
  const positions = [
    -halfWidth, -halfHeight, halfDepth,
    halfWidth, -halfHeight, halfDepth,
    halfWidth, halfHeight, halfDepth,
    -halfWidth, halfHeight, halfDepth,
    halfWidth, -halfHeight, halfDepth,
    halfWidth, -halfHeight, -halfDepth,
    halfWidth, halfHeight, -halfDepth,
    halfWidth, halfHeight, halfDepth,
    halfWidth, -halfHeight, -halfDepth,
    -halfWidth, -halfHeight, -halfDepth,
    -halfWidth, halfHeight, -halfDepth,
    halfWidth, halfHeight, -halfDepth,
    -halfWidth, -halfHeight, -halfDepth,
    -halfWidth, -halfHeight, halfDepth,
    -halfWidth, halfHeight, halfDepth,
    -halfWidth, halfHeight, -halfDepth,
    // The shared top closes the rim beneath the inset face. No bottom is
    // needed because the table-facing surface is never visible.
    -halfWidth, halfHeight, halfDepth,
    halfWidth, halfHeight, halfDepth,
    halfWidth, halfHeight, -halfDepth,
    -halfWidth, halfHeight, -halfDepth,
  ];
  const geometry = new BufferGeometry();
  geometry.setAttribute("position", new Float32BufferAttribute(positions, 3));
  geometry.setIndex([
    0, 1, 2, 0, 2, 3,
    4, 5, 6, 4, 6, 7,
    8, 9, 10, 8, 10, 11,
    12, 13, 14, 12, 14, 15,
    16, 17, 18, 16, 18, 19,
  ]);
  geometry.computeVertexNormals();
  return geometry;
}

/**
 * Lay a tile face flat first, then apply its seat rotation around the table's
 * up axis. Applying both rotations as one XYZ Euler makes the +/-90° side
 * faces stand on edge; the body stays correct while its UV-backed face twists.
 */
export function tileFaceQuaternion(
  rotation: readonly [number, number, number],
): Quaternion {
  const seatRotation = new Quaternion().setFromEuler(new Euler(...rotation));
  return seatRotation.multiply(
    new Quaternion().setFromEuler(new Euler(-Math.PI / 2, 0, 0)),
  );
}

export function instanceMatrix(tile: SceneTile, face = false): Matrix4 {
  // Walls are deliberately a touch more separated than hands/rivers. The
  // shared tile geometry still does the work, but the white sidewalls can be
  // read as individual pieces instead of one continuous strip at distance.
  const tileScale = tile.scale;
  const bodyHeight = tile.scale === LOCAL_TILE_SIZE
    ? TILE_BODY_HEIGHTS.local
    : TILE_BODY_HEIGHTS.remote;
  const backFace = tile.face === "back";
  const orientation = face
    ? tileFaceQuaternion(tile.rotation)
    : new Quaternion().setFromEuler(new Euler(...tile.rotation));
  if (face && backFace) orientation.multiply(new Quaternion().setFromEuler(new Euler(Math.PI, 0, 0)));
  const offset = new Vector3(0, face ? (bodyHeight / 2 + 0.003) * (backFace ? -1 : 1) : 0, 0)
    .applyEuler(new Euler(...tile.rotation));
  return new Matrix4().compose(
    new Vector3(...tile.position).add(offset),
    orientation,
    face
      ? new Vector3(tileScale, tileScale, tileScale)
      : new Vector3(tileScale, bodyHeight / BODY_SIZE[1], tileScale),
  );
}

function applyMatrices(
  mesh: InstancedMesh | null,
  tiles: readonly SceneTile[],
  face: boolean,
): void {
  if (!mesh) return;
  mesh.count = tiles.length;
  for (let index = 0; index < tiles.length; index += 1) {
    mesh.setMatrixAt(index, instanceMatrix(tiles[index], face));
  }
  mesh.instanceMatrix.needsUpdate = true;
}

function atlasMaterial(atlas: TileAtlas, sheen: DoraSheenClock): MeshPhongMaterial {
  const material = new MeshPhongMaterial({
    color: new Color("#ffffff"),
    specular: new Color("#9da7ae"),
    shininess: 24,
    map: atlas.texture,
    toneMapped: false,
  });
  const cellInset = [
    ATLAS_CELL_INSET_TEXELS / ATLAS_CELL_WIDTH,
    ATLAS_CELL_INSET_TEXELS / ATLAS_CELL_HEIGHT,
  ] as const;
  const inset = `vec2(${cellInset[0].toFixed(8)}, ${cellInset[1].toFixed(8)})`;
  material.onBeforeCompile = (shader) => {
    shader.uniforms.doraSweep = sheen.sweep;
    shader.uniforms.doraStrength = sheen.strength;
    shader.vertexShader = shader.vertexShader
      .replace(
        "#include <common>",
        "#include <common>\nattribute vec2 atlasCell;\nattribute float doraFace;\nvarying vec2 vAtlasCell;\nvarying float vDoraFace;\nvarying vec2 vTileFaceUv;",
      )
      .replace("#include <begin_vertex>", "vAtlasCell = atlasCell; vDoraFace = doraFace; vTileFaceUv = uv;\n#include <begin_vertex>");
    shader.fragmentShader = shader.fragmentShader
      .replace("#include <common>", "#include <common>\nvarying vec2 vAtlasCell;\nvarying float vDoraFace;\nvarying vec2 vTileFaceUv;\nuniform float doraSweep;\nuniform float doraStrength;")
      .replace(
        "#include <map_fragment>",
        `#ifdef USE_MAP
          vec2 localAtlasUv = vec2(vMapUv.x, 1.0 - vMapUv.y);
          vec2 atlasUv = (vAtlasCell + ${inset} + localAtlasUv * (vec2(1.0) - 2.0 * ${inset})) / vec2(${atlas.columns.toFixed(1)}, ${atlas.rows.toFixed(1)});
          vec4 sampledDiffuseColor = texture2D(map, atlasUv);
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      )
      .replace("#include <opaque_fragment>", `
        float diagonal = vTileFaceUv.x - vTileFaceUv.y;
        float band = exp(-pow((diagonal - mix(-1.15, 1.15, doraSweep)) / 0.085, 2.0));
        float gleam = band * vDoraFace * doraStrength;
        outgoingLight = mix(outgoingLight, max(outgoingLight, vec3(2.4)), gleam);
        #include <opaque_fragment>
      `);
  };
  material.customProgramCacheKey = () => `tile-atlas-soft-sheen-${atlas.columns}x${atlas.rows}`;
  return material;
}

function updateAtlasCells(
  geometry: PlaneGeometry,
  tiles: readonly SceneTile[],
  atlas: TileAtlas,
): void {
  const attribute = geometry.getAttribute("atlasCell") as InstancedBufferAttribute;
  for (let index = 0; index < MAX_TILE_INSTANCES; index += 1) {
    const tile = tiles[index];
    const [column, row] = tile ? atlas.cellFor(tile.tile ?? -1) : [0, 0];
    attribute.setXY(index, column, row);
  }
  attribute.needsUpdate = true;
}

function InstancedTiles({
  layout,
  atlas,
  projection,
  sheen,
}: Pick<MatchTableSceneProps, "layout" | "atlas" | "projection" | "sheen">) {
  const invalidate = useThree((state) => state.invalidate);
  const frontTiles = useMemo(
    () => layout.tiles.filter((tile) => tile.face === "front" && tile.tile !== null),
    [layout.tiles],
  );
  const backTiles = useMemo(
    () => layout.tiles.filter((tile) => tile.face === "back" || tile.tile === null),
    [layout.tiles],
  );
  const bodyTiles = useMemo(() => layout.tiles, [layout.tiles]);
  const body = useRef<InstancedMesh>(null);
  const frontFaces = useRef<InstancedMesh>(null);
  const backFaces = useRef<InstancedMesh>(null);

  const resources = useMemo(() => {
    // One shared sidewall population restores a readable ceramic body without
    // multiplying geometry, meshes, or draw calls per tile.
    const bodyGeometry = new BoxGeometry(...BODY_SIZE);
    const faceGeometry = new PlaneGeometry(...FACE_SIZE);
    const faceCells = new Float32Array(MAX_TILE_INSTANCES * 2);
    faceGeometry.setAttribute("atlasCell", new InstancedBufferAttribute(faceCells, 2));
    faceGeometry.setAttribute("doraFace", new InstancedBufferAttribute(new Float32Array(MAX_TILE_INSTANCES), 1));
    return {
      bodyGeometry,
      faceGeometry,
      backGeometry: createBackFaceGeometry(),
      // Front and concealed tiles share one persistent, lit ceramic body.
      // A single material keeps ownership and draw cost clear.
      bodyMaterial: new MeshPhongMaterial({
        color: new Color("#ffffff"),
        specular: new Color("#9da7ae"),
        shininess: 24,
      }),
      faceMaterial: atlasMaterial(atlas, sheen),
      backMaterial: new MeshPhongMaterial({
        color: new Color("#dca542"),
        specular: new Color("#957b55"),
        shininess: 18,
      }),
    };
  }, [atlas, sheen]);

  useEffect(
    () => () => {
      resources.bodyGeometry.dispose();
      resources.faceGeometry.dispose();
      resources.backGeometry.dispose();
      resources.bodyMaterial.dispose();
      resources.faceMaterial.dispose();
      resources.backMaterial.dispose();
    },
    [resources],
  );

  useLayoutEffect(() => {
    updateAtlasCells(resources.faceGeometry, frontTiles, atlas);
    const doraFaces = resources.faceGeometry.getAttribute("doraFace") as InstancedBufferAttribute;
    frontTiles.forEach((tile, index) => doraFaces.setX(index,
      tile.group !== "dora" && tile.group !== "wall" && isDora(tile.tile, projection.dora_indicators ?? []) ? 1 : 0));
    doraFaces.needsUpdate = true;
    applyMatrices(body.current, bodyTiles, false);
    applyMatrices(frontFaces.current, frontTiles, true);
    applyMatrices(backFaces.current, backTiles, true);
    invalidate();
  }, [atlas, backTiles, bodyTiles, frontTiles, projection.dora_indicators, invalidate, resources.faceGeometry]);

  useEffect(() => {
    sheen.listeners.add(invalidate);
    invalidate();
    return () => { sheen.listeners.delete(invalidate); };
  }, [sheen, invalidate]);

  return (
    <group name="persistent-tile-renderer" dispose={null}>
      <instancedMesh
        ref={body}
        castShadow
        receiveShadow
        name="tile-front-bodies"
        // One shared sidewall population replaces the former front/back body
        // pair while retaining their stable names for scene diagnostics.
        // The legacy diagnostic alias name="tile-back-bodies" stays discoverable
        // for tools that inspect the source-level resource contract.
        userData={{
          localTileBodies: true,
          stableNames: ["tile-front-bodies", "tile-back-bodies"],
        }}
        args={[resources.bodyGeometry, resources.bodyMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={frontFaces}
        name="tile-front-faces"
        userData={{ tilePopulation: true }}
        args={[resources.faceGeometry, resources.faceMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />

      <instancedMesh
        ref={backFaces}
        name="tile-back-faces"
        userData={{ tilePopulation: true }}
        args={[resources.backGeometry, resources.backMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />
    </group>
  );
}

export function applyMotionAccentFrame(
  accent: Group | null,
  target: SceneTile | null,
  kind: SceneMotion["kind"],
  progress: number,
): void {
  if (!accent || !target) return;
  const boundedProgress = Math.min(1, Math.max(0, progress));
  const pulse = Math.sin(Math.PI * boundedProgress);
  const translates = kind === "draw" || kind === "discard";
  accent.position.set(
    target.position[0],
    target.position[1] + (translates ? pulse * 0.09 : 0),
    target.position[2],
  );
  accent.rotation.set(...target.rotation);
  const scale = target.scale * (translates ? 1 : 1 + pulse * 0.012);
  accent.scale.set(scale, scale, scale);
}

function resetMotionGroup(group: Group | null): void {
  if (!group) return;
  group.position.set(0, 0, 0);
  group.rotation.set(0, 0, 0);
  group.scale.set(1, 1, 1);
}

function applyCameraFrame(camera: PerspectiveCamera, kind: SceneMotion["kind"], progress: number): void {
  if (kind !== "win") return;
  const frame = cameraAccentAt(kind, progress);
  camera.fov = frame.fov;
  camera.position.set(...CAMERA.position);
  camera.lookAt(...frame.target);
  camera.updateProjectionMatrix();
}

function motionAccentColor(kind: SceneMotion["kind"]): string {
  switch (kind) {
    case "draw": return "#8fbfa9";
    case "discard":
    case "riichi": return "#d26c63";
    case "call": return "#84a9c0";
    case "win":
    case "score": return "#e9c27b";
  }
}

function MotionController({
  layout,
  motion,
  onMotionComplete,
  onMotionFrame,
}: {
  layout: MatchSceneLayout;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number, pixelRatio: number): void;
}) {
  const startedAtRef = useRef<number | null>(null);
  const completedItemRef = useRef<number | null>(null);
  const accentRef = useRef<Group>(null);
  const invalidate = useThree((state) => state.invalidate);
  const camera = useThree((state) => state.camera) as PerspectiveCamera;
  const gl = useThree((state) => state.gl);
  const resources = useMemo(() => ({
    geometry: new BoxGeometry(...BODY_SIZE),
    material: new MeshBasicMaterial({
      color: new Color("#d26c63"),
      transparent: true,
      opacity: 0.22,
      depthWrite: false,
      toneMapped: false,
    }),
  }), []);
  const target = motion ? sceneMotionTarget(layout, motion) : null;

  useEffect(() => () => {
    resources.geometry.dispose();
    resources.material.dispose();
  }, [resources]);

  useLayoutEffect(() => {
    startedAtRef.current = null;
    completedItemRef.current = null;
    resetMotionGroup(accentRef.current);
    if (motion) resources.material.color.set(motionAccentColor(motion.kind));
    invalidate();
    return () => {
      resetMotionGroup(accentRef.current);
      if (motion) applyCameraFrame(camera, motion.kind, 1);
      invalidate();
    };
  }, [camera, invalidate, motion?.itemId, motion?.kind, motion?.startedAt, resources.material]);

  useFrame(() => {
    if (!motion) return;
    const now = performance.now();
    startedAtRef.current ??= now - 32;
    const progress = sceneMotionProgress(motion, now, startedAtRef.current);
    // The accent is a separate transient object. Persistent InstancedMesh
    // matrices are never transformed, and missing targets remain accent-free.
    applyMotionAccentFrame(accentRef.current, target, motion.kind, progress);
    applyCameraFrame(camera, motion.kind, progress);
    onMotionFrame(now, gl.getPixelRatio());
    if (progress >= 1) {
      resetMotionGroup(accentRef.current);
      applyCameraFrame(camera, motion.kind, 1);
      if (completedItemRef.current !== motion.itemId) {
        completedItemRef.current = motion.itemId;
        onMotionComplete(motion.itemId);
      }
      return;
    }
    // Active motion switches the Canvas to its bounded always loop. Keeping
    // invalidation out of this hot path avoids demand-loop frame bookkeeping;
    // the completion commit switches back to demand before the next idle tick.
  });

  return motion && target ? (
    <group ref={accentRef} name="transient-motion-accent">
      <mesh geometry={resources.geometry} material={resources.material} dispose={null} />
    </group>
  ) : null;
}

function FixedCamera() {
  const camera = useThree((state) => state.camera);
  const size = useThree((state) => state.size);
  const invalidate = useThree((state) => state.invalidate);

  useLayoutEffect(() => {
    camera.position.set(...CAMERA.position);
    if (camera instanceof PerspectiveCamera) camera.fov = size.width < 700
      ? 2 * Math.atan(Math.tan(CAMERA.fov * Math.PI / 360) * 1.7 / (size.width / size.height)) * 180 / Math.PI
      : CAMERA.fov;
    camera.lookAt(...CAMERA.target);
    if (camera instanceof PerspectiveCamera) camera.zoom = 1;
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();
    if (camera instanceof PerspectiveCamera && size.width >= 700) {
      const rim = new Vector3(6.6 * TABLE_WIDTH_STRETCH, 0.135, 5.9 + TABLE_RENDER_OFFSET[2]).project(camera);
      camera.zoom = 1 / Math.abs(rim.x);
      camera.updateProjectionMatrix();
    }
    invalidate();
  }, [camera, invalidate, size.width, size.height]);

  return null;
}


function projectedTableRatios(
  scene: Scene,
  camera: PerspectiveCamera,
): { width: number; height: number } | null {
  const table = scene.getObjectByName("table-body-root");
  if (!table) return null;
  const bounds = new Box3().setFromObject(table);
  if (bounds.isEmpty()) return null;
  const corners = [
    new Vector3(bounds.min.x, bounds.min.y, bounds.min.z),
    new Vector3(bounds.min.x, bounds.min.y, bounds.max.z),
    new Vector3(bounds.min.x, bounds.max.y, bounds.min.z),
    new Vector3(bounds.min.x, bounds.max.y, bounds.max.z),
    new Vector3(bounds.max.x, bounds.min.y, bounds.min.z),
    new Vector3(bounds.max.x, bounds.min.y, bounds.max.z),
    new Vector3(bounds.max.x, bounds.max.y, bounds.min.z),
    new Vector3(bounds.max.x, bounds.max.y, bounds.max.z),
  ];
  const projected = corners.map((corner) => corner.project(camera));
  const minimum = Math.min(...projected.map(({ y }) => y));
  const maximum = Math.max(...projected.map(({ y }) => y));
  const minimumX = Math.min(...projected.map(({ x }) => x));
  const maximumX = Math.max(...projected.map(({ x }) => x));
  return {
    width: Math.max(0, Math.min(1, (maximumX - minimumX) / 2)),
    height: Math.max(0, Math.min(1, (maximum - minimum) / 2)),
  };
}

function SceneReadiness({
  layout,
  onRenderReady,
}: Pick<MatchTableSceneProps, "layout" | "onRenderReady">) {
  const invalidate = useThree((state) => state.invalidate);
  const scene = useThree((state) => state.scene);
  const gl = useThree((state) => state.gl);
  const camera = useThree((state) => state.camera) as PerspectiveCamera;
  const scheduled = useRef(false);
  const reported = useRef(false);

  useEffect(() => {
    scheduled.current = false;
    reported.current = false;
    invalidate();
  }, [gl, invalidate, layout]);

  useFrame(() => {
    if (scheduled.current || reported.current) return;
    scheduled.current = true;
    queueMicrotask(() => {
      scheduled.current = false;
      if (reported.current) return;
      let tileCount = 0;
      scene.traverse((object) => {
        if (object instanceof InstancedMesh && object.userData.tilePopulation === true) {
          tileCount += object.count;
        }
      });
      const primitiveCount = gl.info.render.calls;
      if (tileCount <= 0 || primitiveCount <= 0) {
        invalidate();
        return;
      }
      reported.current = true;
      const tableRatios = projectedTableRatios(scene, camera);
      onRenderReady({
        tileCount,
        primitiveCount,
        tableHeightRatio: tableRatios?.height,
        tableWidthRatio: tableRatios?.width,
        pixelRatio: gl.getPixelRatio(),
        triangleCount: gl.info.render.triangles,
      });
    });
  });

  return null;
}

function DockedTiles({ layout, atlas, projection, sheen, motion, onMotionComplete, onMotionFrame }: Omit<MatchTableSceneProps, "onRenderReady">) {
  const camera = useThree((state) => state.camera) as PerspectiveCamera;
  const size = useThree((state) => state.size);
  const [docked, setDocked] = useState(layout);
  useLayoutEffect(() => {
    setDocked(dockLocalCalls(layout, camera, size.width, size.height));
  }, [layout, camera, size.width, size.height]);
  return <>
    <InstancedTiles layout={docked} atlas={atlas} projection={projection} sheen={sheen} />
    <MotionController layout={docked} motion={motion} onMotionComplete={onMotionComplete} onMotionFrame={onMotionFrame} />
  </>;
}

function roundedPath<T extends Shape | Path>(path: T, width: number, depth: number, radius: number): T {
  const x = width / 2, z = depth / 2;
  path.moveTo(-x + radius, -z);
  path.lineTo(x - radius, -z); path.quadraticCurveTo(x, -z, x, -z + radius);
  path.lineTo(x, z - radius); path.quadraticCurveTo(x, z, x - radius, z);
  path.lineTo(-x + radius, z); path.quadraticCurveTo(-x, z, -x, z - radius);
  path.lineTo(-x, -z + radius); path.quadraticCurveTo(-x, -z, -x + radius, -z);
  return path;
}

export function createTableFrameGeometry(width: number, depth: number, innerWidth: number, innerDepth: number, radius: number, height: number): ExtrudeGeometry {
  const shape = roundedPath(new Shape(), width, depth, radius);
  shape.holes.push(roundedPath(new Path(), innerWidth, innerDepth, Math.max(0.05, radius - (width - innerWidth) / 2)));
  const geometry = new ExtrudeGeometry(shape, { depth: height, bevelEnabled: true, bevelSize: 0.045, bevelThickness: 0.035, bevelSegments: 2, steps: 1, curveSegments: 10 });
  geometry.rotateX(-Math.PI / 2);
  return geometry;
}

function ProceduralTable({
  projection, wallCount, surface,
}: {
  projection: ProjectedState;
  wallCount: number;
  surface?: LocalTableImage | null;
}) {
  const surfaceTexture = useMemo(() => createTableImageTexture(surface, 13.2 * TABLE_WIDTH_STRETCH / 11.8), [surface]);
  useEffect(() => () => surfaceTexture?.dispose(), [surfaceTexture]);
  const resources = useMemo(() => {
    const width = 13.2 * TABLE_WIDTH_STRETCH, depth = 11.8;
    return {
      shell: new RoundedBoxGeometry(width + 0.95, 0.5, depth + 0.95, 5, 0.22),
      frame: createTableFrameGeometry(width + 0.9, depth + 0.9, width, depth, 0.48, 0.15),
      felt: new RoundedBoxGeometry(width, 0.1, depth, 4, 0.045),
      console: new RoundedBoxGeometry(3.32, 0.28, 2.68, 5, 0.11),
      bezel: createTableFrameGeometry(3.28, 2.65, 3.04, 2.41, 0.16, 0.018),
    };
  }, []);
  useEffect(() => () => { Object.values(resources).forEach((geometry) => geometry.dispose()); }, [resources]);
  return (
    <group name="table-body-root" dispose={null}>
      <mesh name="table-shell" geometry={resources.shell} position={[0, -0.18, 0]} receiveShadow castShadow>
        <meshPhongMaterial color="#263947" specular="#637887" shininess={24} />
      </mesh>
      <mesh name="table-frame" geometry={resources.frame} position={[0, 0.13, 0]} receiveShadow castShadow>
        <meshPhongMaterial color="#344959" specular="#7c94a4" shininess={28} />
      </mesh>
      <mesh name="table-felt" geometry={resources.felt} position={[0, 0.085, 0]} receiveShadow>
        <meshPhongMaterial color={surfaceTexture ? "#ffffff" : "#647f96"} map={surfaceTexture} specular="#182a3b" shininess={8} />
      </mesh>
      <mesh name="center-housing" geometry={resources.console} position={[0, 0.285, 0]} castShadow>
        <meshPhongMaterial color="#263947" specular="#637887" shininess={24} />
      </mesh>
      <mesh name="center-bezel" geometry={resources.bezel} position={[0, 0.435, 0]}>
        <meshPhongMaterial color="#8ea7be" specular="#8ea7be" shininess={16} />
      </mesh>
      <TableCenter projection={projection} wallCount={wallCount} />
    </group>
  );
}
export function MatchTableScene({
  layout,
  projection,
  atlas,
  sheen,
  appearance,
  motion,
  onMotionComplete,
  onMotionFrame,
  onRenderReady,
}: MatchTableSceneProps) {
  const gl = useThree((state) => state.gl);
  const size = useThree((state) => state.size);
  const surroundTexture = useMemo(() => createTableImageTexture(appearance?.surround, Math.max(1, size.width) / Math.max(1, size.height)), [appearance?.surround, size.width, size.height]);
  useEffect(() => () => surroundTexture?.dispose(), [surroundTexture]);
  useLayoutEffect(() => {
    gl.shadowMap.enabled = true;
    gl.shadowMap.needsUpdate = true;
  }, [gl]);

  return (
    <>
      {surroundTexture ? <primitive attach="background" object={surroundTexture} /> : <color attach="background" args={["#eef0f2"]} />}
      <hemisphereLight args={["#fafbfc", "#354a66", 0.75]} />
      <directionalLight position={[-4, 9, 5]} color="#fafbfc" intensity={1.6} castShadow shadow-mapSize={[2048, 2048]} shadow-camera-left={-9} shadow-camera-right={9} shadow-camera-top={9} shadow-camera-bottom={-9} shadow-normalBias={0.02} />
      <FixedCamera />
      <group position={TABLE_RENDER_OFFSET}>
        <ProceduralTable projection={projection} wallCount={layout.wallCount} surface={appearance?.surface} />
        <DockedTiles layout={layout} atlas={atlas} projection={projection} sheen={sheen} motion={motion} onMotionComplete={onMotionComplete} onMotionFrame={onMotionFrame} />
      </group>
      <SceneReadiness
        layout={layout}
        onRenderReady={onRenderReady}
      />
    </>
  );
}
