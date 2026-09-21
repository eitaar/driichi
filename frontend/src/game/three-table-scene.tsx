import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
  Box3,
  BoxGeometry,
  Color,
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
  Texture,
  TextureLoader,
  Vector3,
} from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";

import type { TileAtlas } from "./tile-atlas";
import {
  configureTableTexture,
  disposeTableTextures,
  FELT_MATERIAL_TINT,
  TABLE_TEXTURE_SPECS,
  TABLE_TEXTURE_URLS,
  type TableTextureKey,
} from "./table-materials";
import {
  CAMERA,
  TABLE_RENDER_OFFSET,
  TILE_BODY_SIZE,
  TABLE_SIZE,
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
  atlas: TileAtlas;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number, pixelRatio: number): void;
  onRenderReady(stats: SceneRenderStats): void;
}

// This is a framing translation only. Geometry remains in the authored world
// dimensions; in particular, no axis is scaled at runtime.
const BODY_SIZE = [TILE_BODY_SIZE.width, 0.2, TILE_BODY_SIZE.depth] as const;
const FACE_SIZE = [0.56, 0.78] as const;
export const BACK_FACE_SIZE = [0.48, 0.7] as const;
const FACE_Y = BODY_SIZE[1] / 2 + 0.003;
const MAX_TILE_INSTANCES = 256;

export function createBackFaceGeometry(): PlaneGeometry {
  return new PlaneGeometry(...BACK_FACE_SIZE);
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

interface TableTextures {
  felt: Texture;
}

function instanceMatrix(tile: SceneTile, face = false): Matrix4 {
  // Walls are deliberately a touch more separated than hands/rivers. The
  // shared tile geometry still does the work, but the ivory sidewalls can be
  // read as individual pieces instead of one continuous strip at distance.
  const tileScale = tile.group === "wall" ? tile.scale * 0.84 : tile.scale;
  const orientation = face
    ? tileFaceQuaternion(tile.rotation)
    : new Quaternion().setFromEuler(new Euler(...tile.rotation));
  return new Matrix4().compose(
    new Vector3(
      tile.position[0],
      tile.position[1] + (face ? FACE_Y * tileScale : 0),
      tile.position[2],
    ),
    orientation,
    new Vector3(tileScale, tileScale, tileScale),
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
          vec2 atlasUv = (vec2(vMapUv.x, 1.0 - vMapUv.y) + vAtlasCell) / vec2(${atlas.columns.toFixed(1)}, ${atlas.rows.toFixed(1)});
          vec4 sampledDiffuseColor = texture2D(map, atlasUv);
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      );
  };
  material.customProgramCacheKey = () => `tile-atlas-${atlas.columns}x${atlas.rows}`;
  return material;
}

function loadTexture(
  loader: TextureLoader,
  key: TableTextureKey,
): Promise<Texture> {
  return new Promise((resolve, reject) => {
    loader.load(
      TABLE_TEXTURE_URLS[key],
      (texture) => resolve(configureTableTexture(texture, TABLE_TEXTURE_SPECS[key])),
      undefined,
      reject,
    );
  });
}

function useTableTextures(): TableTextures | null {
  const [textures, setTextures] = useState<TableTextures | null>(null);
  const ownedRef = useRef<TableTextures | null>(null);

  useEffect(() => {
    let active = true;
    const loader = new TextureLoader();
    const keys: readonly TableTextureKey[] = ["felt"];

    // Readiness owns only the texture sampled by the felt material. Keeping the
    // request set narrow prevents unused table art from blocking the scene.
    void Promise.allSettled(
      keys.map(async (key) => ({ key, texture: await loadTexture(loader, key) })),
    ).then((results) => {
      const loaded: Partial<Record<TableTextureKey, Texture>> = {};
      let failed = false;
      for (const result of results) {
        if (result.status === "fulfilled") {
          loaded[result.value.key] = result.value.texture;
        } else {
          failed = true;
        }
      }

      if (failed || !active) {
        disposeTableTextures(loaded);
        return;
      }

      const next = { felt: loaded.felt! } satisfies TableTextures;
      ownedRef.current = next;
      setTextures(next);
    });

    return () => {
      active = false;
      disposeTableTextures(ownedRef.current);
      ownedRef.current = null;
    };
  }, []);

  return textures;
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
  const frontBody = useRef<InstancedMesh>(null);
  const frontFaces = useRef<InstancedMesh>(null);
  const backBody = useRef<InstancedMesh>(null);
  const backFaces = useRef<InstancedMesh>(null);

  const resources = useMemo(() => {
    // One shared, low-segment bevel supplies a real highlight on every tile
    // body without multiplying geometry or draw calls during motion.
    const bodyGeometry = new RoundedBoxGeometry(...BODY_SIZE, 1, 0.045);
    const faceGeometry = new PlaneGeometry(...FACE_SIZE);
    const faceCells = new Float32Array(MAX_TILE_INSTANCES * 2);
    faceGeometry.setAttribute("atlasCell", new InstancedBufferAttribute(faceCells, 2));
    return {
      bodyGeometry,
      faceGeometry,
      backGeometry: createBackFaceGeometry(),
      // Front and concealed tiles share one warm ivory ceramic sidewall.
      // Keeping one material also makes ownership/disposal unambiguous.
      bodyMaterial: new MeshBasicMaterial({
        color: new Color("#ead9bd"),
        toneMapped: false,
      }),
      faceMaterial: atlasMaterial(atlas),
      backMaterial: new MeshBasicMaterial({
        color: new Color("#c38c4b"),
        toneMapped: false,
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
    applyMatrices(frontBody.current, frontTiles, false);
    applyMatrices(frontFaces.current, frontTiles, true);
    applyMatrices(backBody.current, backTiles, false);
    applyMatrices(backFaces.current, backTiles, true);
    invalidate();
  }, [atlas, backTiles, frontTiles, invalidate, resources.faceGeometry]);

  return (
    <group name="persistent-tile-renderer" dispose={null}>
      <instancedMesh
        ref={frontBody}
        name="tile-front-bodies"
        userData={{ tileBodies: true }}
        args={[resources.bodyGeometry, resources.bodyMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={frontFaces}
        name="tile-front-faces"
        args={[resources.faceGeometry, resources.faceMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={backBody}
        name="tile-back-bodies"
        userData={{ tileBodies: true }}
        args={[resources.bodyGeometry, resources.bodyMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={backFaces}
        name="tile-back-faces"
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
  const frame = cameraAccentAt(kind, progress);
  if (kind !== "win") return;
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
    geometry: new RoundedBoxGeometry(...BODY_SIZE, 1, 0.045),
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
    const progress = sceneMotionProgress(
      { ...motion, startedAt: startedAtRef.current },
      now,
    );
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
    invalidate();
  });

  return motion && target ? (
    <group ref={accentRef} name="transient-motion-accent">
      <mesh geometry={resources.geometry} material={resources.material} dispose={null} />
    </group>
  ) : null;
}

function FixedCamera() {
  const camera = useThree((state) => state.camera);
  const invalidate = useThree((state) => state.invalidate);

  useLayoutEffect(() => {
    camera.position.set(...CAMERA.position);
    if (camera instanceof PerspectiveCamera) camera.fov = CAMERA.fov;
    camera.lookAt(...CAMERA.target);
    camera.updateProjectionMatrix();
    invalidate();
  }, [camera, invalidate]);

  return null;
}

type TablePart = {
  position: readonly [number, number, number];
  scale: readonly [number, number, number];
  rotation?: readonly [number, number, number];
};

const OUTER_CHASSIS_PARTS: readonly TablePart[] = [
  { position: [0, 0.02, 4.84], scale: [12.85, 0.2, 0.81] },
  { position: [0, 0.02, -4.84], scale: [12.85, 0.2, 0.81] },
  { position: [6.18, 0.02, 0], scale: [0.68, 0.2, 9.24] },
  { position: [-6.18, 0.02, 0], scale: [0.68, 0.2, 9.24] },
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

function applyTableParts(mesh: InstancedMesh | null, parts: readonly TablePart[]): void {
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

function TableRails() {
  const invalidate = useThree((state) => state.invalidate);
  const resources = useMemo(() => {
    const geometry = new BoxGeometry(1, 1, 1);
    const chassisMaterial = new MeshBasicMaterial({
      color: new Color("#687679"),
      toneMapped: false,
    });
    return {
      geometry,
      chassisMaterial,
      walnutMaterial: new MeshBasicMaterial({
        color: new Color("#75462e"),
        toneMapped: false,
      }),
      bronzeMaterial: new MeshLambertMaterial({
        color: new Color("#c38a49"),
        toneMapped: false,
      }),
      capMaterial: new MeshBasicMaterial({
        color: new Color("#697477"),
        toneMapped: false,
      }),
      capAccentMaterial: new MeshBasicMaterial({
        color: new Color("#d0a15b"),
        toneMapped: false,
      }),
    };
  }, []);

  useEffect(() => () => {
    resources.geometry.dispose();
    resources.chassisMaterial.dispose();
    resources.walnutMaterial.dispose();
    resources.bronzeMaterial.dispose();
    resources.capMaterial.dispose();
    resources.capAccentMaterial.dispose();
  }, [resources]);

  return (
    <group name="layered-table-perimeter">
      <instancedMesh
        name="table-rails"
        onUpdate={(mesh) => {
          applyTableParts(mesh, OUTER_CHASSIS_PARTS);
          invalidate();
        }}
        args={[resources.geometry, resources.chassisMaterial, 4]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        name="table-walnut-inner-rail"
        onUpdate={(mesh) => {
          applyTableParts(mesh, WALNUT_RAIL_PARTS);
          invalidate();
        }}
        args={[resources.geometry, resources.walnutMaterial, 4]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        name="table-bronze-inlay"
        onUpdate={(mesh) => {
          applyTableParts(mesh, BRONZE_INLAY_PARTS);
          invalidate();
        }}
        args={[resources.geometry, resources.bronzeMaterial, 4]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        name="table-corner-caps"
        onUpdate={(mesh) => {
          applyTableParts(mesh, CORNER_CAP_PARTS);
          invalidate();
        }}
        args={[resources.geometry, resources.capMaterial, 4]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        name="table-corner-accents"
        onUpdate={(mesh) => {
          applyTableParts(mesh, CORNER_ACCENT_PARTS);
          invalidate();
        }}
        args={[resources.geometry, resources.capAccentMaterial, 4]}
        frustumCulled={false}
        dispose={null}
      />
    </group>
  );
}
function FeltSeams() {
  const invalidate = useThree((state) => state.invalidate);
  const meshRef = useRef<InstancedMesh>(null);
  const resources = useMemo(() => ({
    geometry: new BoxGeometry(1, 1, 1),
    material: new MeshBasicMaterial({
      color: new Color("#2d634b"),
      toneMapped: false,
    }),
  }), []);

  useLayoutEffect(() => {
    const seams: readonly TablePart[] = [
      { position: [-3.3, 0.16, -2.42], scale: [4.65, 0.018, 0.038], rotation: [0, -0.53, 0] },
      { position: [3.3, 0.16, -2.42], scale: [4.65, 0.018, 0.038], rotation: [0, 0.53, 0] },
      { position: [-3.3, 0.16, 2.42], scale: [4.65, 0.018, 0.038], rotation: [0, 0.53, 0] },
      { position: [3.3, 0.16, 2.42], scale: [4.65, 0.018, 0.038], rotation: [0, -0.53, 0] },
    ];
    applyTableParts(meshRef.current, seams);
    invalidate();
  }, [invalidate]);

  useEffect(() => () => {
    resources.geometry.dispose();
    resources.material.dispose();
  }, [resources]);

  return (
    <instancedMesh
      ref={meshRef}
      name="felt-directional-seams"
      args={[resources.geometry, resources.material, 4]}
      frustumCulled={false}
      dispose={null}
    />
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
  materialsReady,
  onRenderReady,
}: Pick<MatchTableSceneProps, "layout" | "onRenderReady"> & { materialsReady: boolean }) {
  const invalidate = useThree((state) => state.invalidate);
  const scene = useThree((state) => state.scene);
  const gl = useThree((state) => state.gl);
  const camera = useThree((state) => state.camera) as PerspectiveCamera;
  const scheduled = useRef(false);
  const reported = useRef(false);

  useEffect(() => {
    scheduled.current = false;
    reported.current = false;
    if (materialsReady) invalidate();
  }, [gl, invalidate, layout, materialsReady]);

  useFrame(() => {
    if (!materialsReady || scheduled.current || reported.current) return;
    scheduled.current = true;
    queueMicrotask(() => {
      scheduled.current = false;
      if (reported.current) return;
      let tileCount = 0;
      scene.traverse((object) => {
        if (object instanceof InstancedMesh && object.userData.tileBodies === true) {
          tileCount += object.count;
        }
      });
      const primitiveCount = gl.info.render.calls;
      if (tileCount <= 0 || primitiveCount <= 0) {
        invalidate();
        return;
      }
      reported.current = true;
      gl.shadowMap.autoUpdate = false;
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

function CenterTrim() {
  const invalidate = useThree((state) => state.invalidate);
  const meshRef = useRef<InstancedMesh>(null);
  const resources = useMemo(() => ({
    geometry: new BoxGeometry(1, 1, 1),
    material: new MeshBasicMaterial({
      color: new Color("#c28a4c"),
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

function FeltMaterial({ texture }: { texture: Texture | undefined }) {
  return texture ? (
    <meshBasicMaterial color={FELT_MATERIAL_TINT} map={texture} toneMapped={false} />
  ) : (
    <meshBasicMaterial color={FELT_MATERIAL_TINT} toneMapped={false} />
  );
}

function CenterMaterial() {
  // The approved center reads as machined graphite with bronze edges. Keep
  // the console material procedural so readiness depends only on sampled art.
  return <meshBasicMaterial color="#6d777a" toneMapped={false} />;
}

function ProceduralTable({
  textures,
}: {
  textures: TableTextures | null;
}) {
  return (
    <group name="table-body-root">
      {/* The deep chassis is the structural shadow line beneath the assembled rails. */}
      <mesh position={[0, -0.48, 0]}>
        <boxGeometry args={[TABLE_SIZE.width, 0.72, TABLE_SIZE.depth]} />
        <meshBasicMaterial color="#3c4b4e" toneMapped={false} />
      </mesh>
      <mesh position={[0, -0.075, 0]}>
        <boxGeometry args={[13.28, 0.17, 8.88]} />
        <meshBasicMaterial color="#5e6c6d" toneMapped={false} />
      </mesh>
      {/* A recessed, textile-covered playfield leaves the perimeter visibly built up. */}
      <mesh position={[0, 0.075, 0]}>
        <boxGeometry args={[11.58, 0.12, 8.58]} />
        <FeltMaterial texture={textures?.felt} />
      </mesh>
      <FeltSeams />
      {/* Machined center console: dark housing, bronze frame, restrained material inset. */}
      <mesh position={[0, 0.255, 0]}>
        <boxGeometry args={[3.3, 0.32, 2.68]} />
        <meshBasicMaterial color="#627074" toneMapped={false} />
      </mesh>
      <mesh position={[0, 0.43, 0]}>
        <boxGeometry args={[3.02, 0.055, 2.4]} />
        <meshLambertMaterial color="#86785d" toneMapped={false} />
      </mesh>
      <mesh position={[0, 0.464, 0]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[2.5, 1.88]} />
        <CenterMaterial />
      </mesh>
      <CenterTrim />
      <TableRails />
    </group>
  );
}
export function MatchTableScene({
  layout,
  atlas,
  motion,
  onMotionComplete,
  onMotionFrame,
  onRenderReady,
}: MatchTableSceneProps) {
  const textures = useTableTextures();

  return (
    <>
      <color attach="background" args={["#050709"]} />
      <hemisphereLight args={["#dce8e0", "#17231f", 0.62]} />
      <directionalLight
        position={[-5.5, 10.5, 6.5]}
        color="#ffe2bd"
        intensity={1.9}
        castShadow
        shadow-mapSize-width={512}
        shadow-mapSize-height={512}
        shadow-camera-near={1}
        shadow-camera-far={32}
        shadow-camera-left={-9}
        shadow-camera-right={9}
        shadow-camera-top={7}
        shadow-camera-bottom={-7}
      />
      <pointLight position={[6.5, 5.2, -2.8]} color="#a7c7dc" intensity={0.62} />
      <spotLight
        position={[0, 3.8, 8.5]}
        color="#d8a873"
        intensity={0.82}
        angle={0.48}
        penumbra={0.9}
        distance={18}
      />
      <FixedCamera />
      <group position={TABLE_RENDER_OFFSET}>
        <ProceduralTable textures={textures} />
        <InstancedTiles layout={layout} atlas={atlas} />
        <MotionController
          layout={layout}
          motion={motion}
          onMotionComplete={onMotionComplete}
          onMotionFrame={onMotionFrame}
        />
      </group>
      <SceneReadiness
        layout={layout}
        materialsReady={textures !== null}
        onRenderReady={onRenderReady}
      />
    </>
  );
}
