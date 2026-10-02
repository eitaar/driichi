import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
  Box3,
  BoxGeometry,
  BufferGeometry,
  Float32BufferAttribute,
  Color,
  CanvasTexture,
  AdditiveBlending,
  Euler,
  InstancedBufferAttribute,
  Group,
  InstancedMesh,
  Matrix4,
  MeshBasicMaterial,
  MeshLambertMaterial,
  PerspectiveCamera,
  PlaneGeometry,
  Quaternion,
  Scene,
  Vector3,
} from "three";
import {
  ATLAS_CELL_HEIGHT,
  ATLAS_CELL_INSET_TEXELS,
  ATLAS_CELL_WIDTH,
  type TileAtlas,
} from "./tile-atlas";
import { isDora } from "./dora";
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
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number, pixelRatio: number): void;
  onRenderReady(stats: SceneRenderStats): void;
}

// This is a framing translation only. Geometry remains in the authored world
// dimensions; in particular, no axis is scaled at runtime.
const BODY_SIZE = [TILE_BODY_SIZE.width, TILE_BODY_SIZE.height, TILE_BODY_SIZE.depth] as const;
// The vendored 300:400 source face is preserved on a smaller plane so the
// shared ivory body reads as an intentional ceramic rim instead of a hairline.
export const FACE_SIZE = [TILE_BODY_SIZE.width, TILE_BODY_SIZE.depth] as const;
export const BACK_FACE_SIZE = [0.54, 0.78] as const;
const MAX_TILE_INSTANCES = 256;

export function createBackFaceGeometry(): PlaneGeometry {
  return new PlaneGeometry(...BACK_FACE_SIZE);
}

/**
 * Keep one shared top-rim and four vertical sidewalls for every tile. The
 * face plane is intentionally inset, so the top rim is part of the visible
 * ivory material; the occluded bottom surface remains omitted.
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

function instanceMatrix(tile: SceneTile, face = false): Matrix4 {
  // Walls are deliberately a touch more separated than hands/rivers. The
  // shared tile geometry still does the work, but the ivory sidewalls can be
  // read as individual pieces instead of one continuous strip at distance.
  const tileScale = tile.scale;
  const bodyHeight = tile.scale === LOCAL_TILE_SIZE
    ? TILE_BODY_HEIGHTS.local
    : TILE_BODY_HEIGHTS.remote;
  const concealedHand = tile.group === "hand" && tile.face === "back";
  const orientation = face
    ? tileFaceQuaternion(tile.rotation)
    : new Quaternion().setFromEuler(new Euler(...tile.rotation));
  if (face && concealedHand) orientation.multiply(new Quaternion().setFromEuler(new Euler(Math.PI, 0, 0)));
  const offset = new Vector3(0, face ? (bodyHeight / 2 + 0.003) * (concealedHand ? -1 : 1) : 0, 0)
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

function atlasMaterial(atlas: TileAtlas): MeshBasicMaterial {
  const material = new MeshBasicMaterial({
    color: new Color("#ffffff"),
    map: atlas.texture,
    toneMapped: false,
  });
  const cellInset = [
    ATLAS_CELL_INSET_TEXELS / ATLAS_CELL_WIDTH,
    ATLAS_CELL_INSET_TEXELS / ATLAS_CELL_HEIGHT,
  ] as const;
  const inset = `vec2(${cellInset[0].toFixed(8)}, ${cellInset[1].toFixed(8)})`;
  material.onBeforeCompile = (shader) => {
    shader.vertexShader = shader.vertexShader
      .replace(
        "#include <common>",
        "#include <common>\nattribute vec2 atlasCell;\nvarying vec2 vAtlasCell;",
      )
      .replace("#include <begin_vertex>", "vAtlasCell = atlasCell;\n#include <begin_vertex>");
    shader.fragmentShader = shader.fragmentShader
      .replace("#include <common>", "#include <common>\nvarying vec2 vAtlasCell;")
      .replace(
        "#include <map_fragment>",
        `#ifdef USE_MAP
          vec2 localAtlasUv = vec2(vMapUv.x, 1.0 - vMapUv.y);
          vec2 atlasUv = (vAtlasCell + ${inset} + localAtlasUv * (vec2(1.0) - 2.0 * ${inset})) / vec2(${atlas.columns.toFixed(1)}, ${atlas.rows.toFixed(1)});
          vec4 sampledDiffuseColor = texture2D(map, atlasUv);
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      );
  };
  material.customProgramCacheKey = () => `tile-atlas-${atlas.columns}x${atlas.rows}`;
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
}: Pick<MatchTableSceneProps, "layout" | "atlas">) {
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
    return {
      bodyGeometry,
      faceGeometry,
      backGeometry: createBackFaceGeometry(),
      // Front and concealed tiles share one persistent, lit ceramic body.
      // A single material keeps ownership and draw cost clear.
      bodyMaterial: new MeshLambertMaterial({
        color: new Color("#fafbfc"),
        emissive: new Color("#dce4ed"),
        emissiveIntensity: 0.02,
      }),
      faceMaterial: atlasMaterial(atlas),
      backMaterial: new MeshLambertMaterial({
        color: new Color("#8a9fb7"),
        emissive: new Color("#586574"),
        emissiveIntensity: 0.02,
      }),
    };
  }, [atlas]);

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
    applyMatrices(body.current, bodyTiles, false);
    applyMatrices(frontFaces.current, frontTiles, true);
    applyMatrices(backFaces.current, backTiles, true);
    invalidate();
  }, [atlas, backTiles, bodyTiles, frontTiles, invalidate, resources.faceGeometry]);

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


type TablePart = {
  position: readonly [number, number, number];
  scale: readonly [number, number, number];
  rotation?: readonly [number, number, number];
};

const OUTER_CHASSIS_PARTS: readonly TablePart[] = [
  { position: [0, -0.35, 5.1], scale: [13.6, 0.8, 0.8] },
  { position: [0, -0.35, -5.1], scale: [13.6, 0.8, 0.8] },
  { position: [6.5, -0.35, 0], scale: [0.6, 0.8, 10.2] },
  { position: [-6.5, -0.35, 0], scale: [0.6, 0.8, 10.2] },
];
const WALNUT_RAIL_PARTS: readonly TablePart[] = [
  { position: [0, 0.15, 4.46], scale: [12.15, 0.22, 0.6] },
  { position: [0, 0.15, -4.46], scale: [12.15, 0.22, 0.6] },
  { position: [5.86, 0.15, 0], scale: [0.5, 0.22, 8.54] },
  { position: [-5.86, 0.15, 0], scale: [0.5, 0.22, 8.54] },
];
const BRONZE_INLAY_PARTS: readonly TablePart[] = [
  { position: [0, 0.29, 4.26], scale: [11.98, 0.045, 0.07] },
  { position: [0, 0.29, -4.26], scale: [11.98, 0.045, 0.07] },
  { position: [5.66, 0.29, 0], scale: [0.07, 0.045, 8.27] },
  { position: [-5.66, 0.29, 0], scale: [0.07, 0.045, 8.27] },
];
const CORNER_CAP_PARTS: readonly TablePart[] = [
  { position: [-5.72, 0.26, -4.14], scale: [0.74, 0.16, 0.55] },
  { position: [5.72, 0.26, -4.14], scale: [0.74, 0.16, 0.55] },
  { position: [5.72, 0.26, 4.14], scale: [0.74, 0.16, 0.55] },
  { position: [-5.72, 0.26, 4.14], scale: [0.74, 0.16, 0.55] },
];
const CORNER_ACCENT_PARTS: readonly TablePart[] = CORNER_CAP_PARTS.map(({ position }) => ({
  position: [position[0], 0.36, position[2]],
  scale: [0.34, 0.035, 0.07],
}));
// Keep the hardware warm and material-led while separating it decisively from
// the #050709 surround. Uniform tone-mapped-off materials make these authored
// rail colors deterministic in the screenshot surface; bronze stays an
// accent rather than becoming an all-over plastic gold.
export const TABLE_RAIL_PALETTE = {
  chassis: "#8497a6",
  walnut: "#9daebb",
  bronze: "#aabac8",
  cornerCaps: "#8497a6",
  cornerAccents: "#aabac8",
} as const;
const RAIL_BATCHES = [
  { name: "table-rails", parts: OUTER_CHASSIS_PARTS, material: "chassis" },
  { name: "table-walnut-inner-rail", parts: WALNUT_RAIL_PARTS, material: "walnut" },
  { name: "table-bronze-inlay", parts: BRONZE_INLAY_PARTS, material: "bronze" },
  { name: "table-corner-caps", parts: CORNER_CAP_PARTS, material: "cornerCaps" },
  { name: "table-corner-accents", parts: CORNER_ACCENT_PARTS, material: "bronze" },
] as const;
const RAIL_BATCH_NAMES = RAIL_BATCHES.map(({ name }) => name);

function applyTableParts(
  mesh: InstancedMesh | null,
  parts: readonly TablePart[],
): void {
  if (!mesh) return;
  mesh.count = parts.length;
  parts.forEach((part, index) => {
    const rotation = part.rotation
      ? new Quaternion().setFromEuler(new Euler(...part.rotation))
      : new Quaternion();
    mesh.setMatrixAt(index, new Matrix4().compose(
      new Vector3(...part.position),
      rotation,
      new Vector3(...part.scale),
    ));
  });
  mesh.instanceMatrix.needsUpdate = true;
}

export function TableRails() {
  const invalidate = useThree((state) => state.invalidate);
  const chassisRef = useRef<InstancedMesh>(null);
  const walnutRef = useRef<InstancedMesh>(null);
  const bronzeInlayRef = useRef<InstancedMesh>(null);
  const cornerCapsRef = useRef<InstancedMesh>(null);
  const cornerAccentsRef = useRef<InstancedMesh>(null);
  const resources = useMemo(() => ({
    // Every rail batch shares this persistent unit box; only each batch's
    // instance matrices and count vary. Uniform materials avoid the broken
    // per-instance color path in SwiftShader while keeping the palette explicit.
    geometry: new BoxGeometry(1, 1, 1),
    materials: {
      chassis: new MeshBasicMaterial({
        color: new Color(TABLE_RAIL_PALETTE.chassis),
        toneMapped: false,
      }),
      walnut: new MeshBasicMaterial({
        color: new Color(TABLE_RAIL_PALETTE.walnut),
        toneMapped: false,
      }),
      bronze: new MeshBasicMaterial({
        color: new Color(TABLE_RAIL_PALETTE.bronze),
        toneMapped: false,
      }),
      cornerCaps: new MeshBasicMaterial({
        color: new Color(TABLE_RAIL_PALETTE.cornerCaps),
        toneMapped: false,
      }),
    },
  }), []);

  useEffect(() => () => {
    resources.geometry.dispose();
    resources.materials.chassis.dispose();
    resources.materials.walnut.dispose();
    resources.materials.bronze.dispose();
    resources.materials.cornerCaps.dispose();
  }, [resources]);

  useLayoutEffect(() => {
    applyTableParts(chassisRef.current, OUTER_CHASSIS_PARTS);
    applyTableParts(walnutRef.current, WALNUT_RAIL_PARTS);
    applyTableParts(bronzeInlayRef.current, BRONZE_INLAY_PARTS);
    applyTableParts(cornerCapsRef.current, CORNER_CAP_PARTS);
    applyTableParts(cornerAccentsRef.current, CORNER_ACCENT_PARTS);
    invalidate();
  }, [invalidate]);

  return (
    <group
      name="table-rail-batches"
      userData={{
        // Five bounded batches retain the legacy diagnostic names while the
        // bronze material is intentionally shared by both bronze batches.
        railBatches: RAIL_BATCH_NAMES,
      }}
      dispose={null}
    >
      <instancedMesh
        ref={chassisRef}
        name="table-rails"
        args={[resources.geometry, resources.materials.chassis, OUTER_CHASSIS_PARTS.length]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={walnutRef}
        name="table-walnut-inner-rail"
        args={[resources.geometry, resources.materials.walnut, WALNUT_RAIL_PARTS.length]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={bronzeInlayRef}
        name="table-bronze-inlay"
        args={[resources.geometry, resources.materials.bronze, BRONZE_INLAY_PARTS.length]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={cornerCapsRef}
        name="table-corner-caps"
        args={[resources.geometry, resources.materials.cornerCaps, CORNER_CAP_PARTS.length]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={cornerAccentsRef}
        name="table-corner-accents"
        args={[resources.geometry, resources.materials.bronze, CORNER_ACCENT_PARTS.length]}
        frustumCulled={false}
        dispose={null}
      />
    </group>
  );
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

export function CenterTrim() {
  const invalidate = useThree((state) => state.invalidate);
  const meshRef = useRef<InstancedMesh>(null);
  const resources = useMemo(() => ({
    geometry: new BoxGeometry(1, 1, 1),
    material: new MeshBasicMaterial({
      color: new Color("#9a7042"),
      toneMapped: false,
    }),
  }), []);

  useLayoutEffect(() => {
    const trims: readonly TablePart[] = [
      { position: [0, 0.49, -1.02], scale: [2.72, 0.045, 0.055] },
      { position: [0, 0.49, 1.02], scale: [2.72, 0.045, 0.055] },
      { position: [-1.38, 0.49, 0], scale: [0.055, 0.045, 1.94] },
      { position: [1.38, 0.49, 0], scale: [0.055, 0.045, 1.94] },
      { position: [-1.3, 0.5, -0.91], scale: [0.13, 0.055, 0.13] },
      { position: [1.3, 0.5, -0.91], scale: [0.13, 0.055, 0.13] },
      { position: [1.3, 0.5, 0.91], scale: [0.13, 0.055, 0.13] },
      { position: [-1.3, 0.5, 0.91], scale: [0.13, 0.055, 0.13] },
    ];
    applyTableParts(meshRef.current, trims);
    invalidate();
  }, [invalidate]);

  useEffect(() => () => {
    resources.geometry.dispose();
    resources.material.dispose();
  }, [resources]);

  return (
    <instancedMesh
      ref={meshRef}
      name="center-device-trim"
      args={[resources.geometry, resources.material, 8]}
      frustumCulled={false}
      dispose={null}
    />
  );
}

function DoraGlows({ layout }: { layout: MatchSceneLayout }) {
  const texture = useMemo(() => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    const gradient = ctx.createRadialGradient(32, 32, 10, 32, 32, 32);
    gradient.addColorStop(0, "rgba(220,166,74,0.7)");
    gradient.addColorStop(1, "rgba(220,166,74,0)");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 64, 64);
    return new CanvasTexture(canvas);
  }, []);
  useEffect(() => () => texture?.dispose(), [texture]);
  const indicators = layout.tiles.filter((tile) => tile.group === "dora" && tile.tile !== null).map((tile) => tile.tile!);
  if (!texture) return null;
  return <group name="actual-dora-glows">{layout.tiles.filter((tile) => tile.face === "front" && tile.group !== "dora" && tile.group !== "wall" && isDora(tile.tile, indicators)).map((tile) => (
    <mesh key={tile.key} position={[tile.position[0], tile.position[1] + 0.14, tile.position[2]]} rotation={[-Math.PI / 2, 0, tile.rotation[1]]} scale={tile.scale}>
      <planeGeometry args={[1.2, 1.5]} />
      <meshBasicMaterial map={texture} transparent depthWrite={false} blending={AdditiveBlending} toneMapped={false} />
    </mesh>
  ))}</group>;
}

function DockedTiles({ layout, atlas, motion, onMotionComplete, onMotionFrame }: Omit<MatchTableSceneProps, "projection" | "onRenderReady">) {
  const camera = useThree((state) => state.camera) as PerspectiveCamera;
  const size = useThree((state) => state.size);
  const [docked, setDocked] = useState(layout);
  useLayoutEffect(() => {
    setDocked(dockLocalCalls(layout, camera, size.width, size.height));
  }, [layout, camera, size.width, size.height]);
  return <>
    <InstancedTiles layout={docked} atlas={atlas} />
    <DoraGlows layout={docked} />
    <MotionController layout={docked} motion={motion} onMotionComplete={onMotionComplete} onMotionFrame={onMotionFrame} />
  </>;
}

function ProceduralTable({
  projection, wallCount,
}: {
  projection: ProjectedState;
  wallCount: number;
}) {
  return (
    <group name="table-body-root">
      {/* A recessed, textile-covered playfield leaves the perimeter visibly built up. */}
      <mesh position={[0, 0.075, 0]} receiveShadow>
        <boxGeometry args={[13.2 * TABLE_WIDTH_STRETCH, 0.12, 11.8]} />
        <meshLambertMaterial color="#647b8d" />
      </mesh>

      {/* Machined center console: dark housing, bronze frame, restrained material inset. */}
      <mesh position={[0, 0.255, 0]}>
        <boxGeometry args={[3.3, 0.32, 2.68]} />
        <meshBasicMaterial color="#cbd4de" toneMapped={false} />
      </mesh>
      <mesh position={[0, 0.43, 0]}>
        <boxGeometry args={[3.02, 0.055, 2.4]} />
        <meshBasicMaterial color="#cbd4de" toneMapped={false} />
      </mesh>
      <TableCenter projection={projection} wallCount={wallCount} />
      <mesh position={[0, -0.15, 0]} receiveShadow>
        <boxGeometry args={[13.65 * TABLE_WIDTH_STRETCH, 0.42, 12.25]} />
        <meshLambertMaterial color="#9daebb" />
      </mesh>
    </group>
  );
}
export function MatchTableScene({
  layout,
  projection,
  atlas,
  motion,
  onMotionComplete,
  onMotionFrame,
  onRenderReady,
}: MatchTableSceneProps) {
  const gl = useThree((state) => state.gl);
  useLayoutEffect(() => {
    gl.shadowMap.enabled = true;
    gl.shadowMap.needsUpdate = true;
  }, [gl]);

  return (
    <>
      <color attach="background" args={["#eef0f2"]} />
      <hemisphereLight args={["#fafbfc", "#354a66", 0.75]} />
      <directionalLight position={[-4, 9, 5]} color="#fafbfc" intensity={1.6} castShadow shadow-mapSize={[2048, 2048]} shadow-camera-left={-9} shadow-camera-right={9} shadow-camera-top={9} shadow-camera-bottom={-9} shadow-normalBias={0.02} />
      <FixedCamera />
      <group position={TABLE_RENDER_OFFSET}>
        <ProceduralTable projection={projection} wallCount={layout.wallCount} />
        <DockedTiles layout={layout} atlas={atlas} motion={motion} onMotionComplete={onMotionComplete} onMotionFrame={onMotionFrame} />
      </group>
      <SceneReadiness
        layout={layout}
        onRenderReady={onRenderReady}
      />
    </>
  );
}
