import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
  Box3,
  BoxGeometry,
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
  Quaternion,
  Scene,
  type Texture,
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
import { TableCenter } from "./table-center";
import { tableSeatGeometry } from "./table-geometry";
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
  centerTexture: Texture | null;
  appearanceTextures: { surface: Texture | null; surround: Texture | null };
  frameRevision?: number;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number, pixelRatio: number): void;
  onRenderReady(stats: SceneRenderStats): void;
}

// This is a framing translation only. Geometry remains in the authored world
// dimensions; in particular, no axis is scaled at runtime.
const BODY_SIZE = [TILE_BODY_SIZE.width, TILE_BODY_SIZE.height, TILE_BODY_SIZE.depth] as const;
// Keep the approved tile footprint while rounding its ceramic and ochre layers.
export const FACE_SIZE = [TILE_BODY_SIZE.width, TILE_BODY_SIZE.depth] as const;
export const BACK_FACE_SIZE = FACE_SIZE;
const MAX_TILE_INSTANCES = 256;

export function createBackFaceGeometry(): RoundedBoxGeometry {
  const [width, height, depth] = BODY_SIZE;
  const geometry = new RoundedBoxGeometry(width, height * 0.25, depth, 3, Math.min(width, height, depth) * 0.06);
  geometry.translate(0, -height * 0.375, 0);
  return geometry;
}

export function createTileSideGeometry(): RoundedBoxGeometry {
  const [width, height, depth] = BODY_SIZE;
  const geometry = new RoundedBoxGeometry(width, height * 0.79, depth, 3, Math.min(width, height, depth) * 0.1);
  geometry.translate(0, height * 0.105, 0);
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
  const bodyOrientation = new Quaternion().setFromEuler(new Euler(...tile.rotation));
  // Face-down walls expose the ochre rear by rotating the whole rounded body.
  if (tile.group === "wall" && backFace) bodyOrientation.multiply(new Quaternion().setFromEuler(new Euler(Math.PI, 0, 0)));
  const orientation = face
    ? bodyOrientation.clone().multiply(new Quaternion().setFromEuler(new Euler(-Math.PI / 2, 0, 0)))
    : bodyOrientation;
  if (face && backFace) orientation.multiply(new Quaternion().setFromEuler(new Euler(Math.PI, 0, 0)));
  const offset = new Vector3(0, face ? (bodyHeight / 2 + 0.003) * (backFace ? -1 : 1) : 0, 0)
    .applyQuaternion(bodyOrientation);
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
        "#include <common>\nattribute vec2 atlasCell;\nattribute float faceVisible;\nattribute float doraFace;\nvarying vec2 vAtlasCell;\nvarying float vFaceVisible;\nvarying float vDoraFace;\nvarying vec3 vTilePosition;\nvarying vec3 vTileNormal;",
      )
      .replace("#include <begin_vertex>", "vAtlasCell = atlasCell; vFaceVisible = faceVisible; vDoraFace = doraFace; vTilePosition = position; vTileNormal = normal;\n#include <begin_vertex>");
    shader.fragmentShader = shader.fragmentShader
      .replace("#include <common>", "#include <common>\nvarying vec2 vAtlasCell;\nvarying float vFaceVisible;\nvarying float vDoraFace;\nvarying vec3 vTilePosition;\nvarying vec3 vTileNormal;\nuniform float doraSweep;\nuniform float doraStrength;")
      .replace(
        "#include <map_fragment>",
        `#ifdef USE_MAP
          vec2 faceUv = vec2(vTilePosition.x / ${BODY_SIZE[0]} + 0.5, vTilePosition.z / ${BODY_SIZE[2]} + 0.5);
          vec2 atlasUv = (vAtlasCell + ${inset} + clamp(faceUv, 0.0, 1.0) * (vec2(1.0) - 2.0 * ${inset})) / vec2(${atlas.columns.toFixed(1)}, ${atlas.rows.toFixed(1)});
          vec4 ink = texture2D(map, atlasUv);
          float face = smoothstep(0.98, 0.999, vTileNormal.y) * vFaceVisible;
          diffuseColor.rgb *= mix(vec3(1.0), ink.rgb, ink.a * face);
        #endif`,
      )
      .replace("#include <opaque_fragment>", `
        float diagonal = vTilePosition.x / ${BODY_SIZE[0]} + vTilePosition.z / ${BODY_SIZE[2]};
        float band = exp(-pow((diagonal - mix(-1.15, 1.15, doraSweep)) / 0.085, 2.0));
        float gleam = band * smoothstep(0.35, 0.85, vTileNormal.y) * vDoraFace * doraStrength;
        outgoingLight = mix(outgoingLight, max(outgoingLight, vec3(2.4)), gleam);
        #include <opaque_fragment>
      `);
  };
  material.customProgramCacheKey = () => `tile-atlas-rounded-soft-sheen-${atlas.columns}x${atlas.rows}`;
  return material;
}

function updateAtlasCells(
  geometry: RoundedBoxGeometry,
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
  const body = useRef<InstancedMesh>(null);
  const back = useRef<InstancedMesh>(null);

  const resources = useMemo(() => {
    const bodyGeometry = createTileSideGeometry();
    bodyGeometry.setAttribute("atlasCell", new InstancedBufferAttribute(new Float32Array(MAX_TILE_INSTANCES * 2), 2));
    bodyGeometry.setAttribute("faceVisible", new InstancedBufferAttribute(new Float32Array(MAX_TILE_INSTANCES), 1));
    bodyGeometry.setAttribute("doraFace", new InstancedBufferAttribute(new Float32Array(MAX_TILE_INSTANCES), 1));
    return {
      bodyGeometry,
      backGeometry: createBackFaceGeometry(),
      bodyMaterial: atlasMaterial(atlas, sheen),
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
      resources.backGeometry.dispose();
      resources.bodyMaterial.dispose();
      resources.backMaterial.dispose();
    },
    [resources],
  );

  useLayoutEffect(() => {
    updateAtlasCells(resources.bodyGeometry, layout.tiles, atlas);
    const visible = resources.bodyGeometry.getAttribute("faceVisible") as InstancedBufferAttribute;
    const doraFaces = resources.bodyGeometry.getAttribute("doraFace") as InstancedBufferAttribute;
    layout.tiles.forEach((tile, index) => {
      const publicFace = tile.face === "front" && tile.tile !== null;
      visible.setX(index, publicFace ? 1 : 0);
      doraFaces.setX(index, publicFace && tile.group !== "dora" && tile.group !== "wall" && isDora(tile.tile, projection.dora_indicators ?? []) ? 1 : 0);
    });
    visible.needsUpdate = doraFaces.needsUpdate = true;
    applyMatrices(body.current, layout.tiles, false);
    applyMatrices(back.current, layout.tiles, false);
    invalidate();
  }, [atlas, layout.tiles, projection.dora_indicators, invalidate, resources.bodyGeometry]);

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
        userData={{ localTileBodies: true, tilePopulation: true }}
        args={[resources.bodyGeometry, resources.bodyMaterial, MAX_TILE_INSTANCES]}
        frustumCulled={false}
        dispose={null}
      />
      <instancedMesh
        ref={back}
        castShadow
        receiveShadow
        name="tile-back-bodies"
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
  frameRevision,
  onRenderReady,
}: Pick<MatchTableSceneProps, "layout" | "frameRevision" | "onRenderReady">) {
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
  }, [gl, invalidate, layout, frameRevision]);

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
      const felt = scene.getObjectByName("table-felt") as unknown as { material: MeshPhongMaterial } | undefined;
      if (felt) {
        const material = felt.material;
        console.info("[native-clear-material]", JSON.stringify({ revision: frameRevision, frame: gl.info.render.frame, map: Boolean(material.map), color: material.color.getHexString(), version: material.version }));
      }
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

function DockedTiles({ layout, atlas, projection, sheen, motion, onMotionComplete, onMotionFrame }: Pick<MatchTableSceneProps, "layout" | "atlas" | "projection" | "sheen" | "motion" | "onMotionComplete" | "onMotionFrame">) {
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

function ProceduralTable({ centerTexture, surfaceTexture }: {
  centerTexture: Texture | null;
  surfaceTexture: Texture | null;
}) {
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
        <meshPhongMaterial color={surfaceTexture ? "#ffffff" : "#647f96"} map={surfaceTexture} specular="#182a3b" shininess={8} onUpdate={material => { material.needsUpdate = true; }} />
      </mesh>
      <mesh name="center-housing" geometry={resources.console} position={[0, 0.285, 0]} castShadow>
        <meshPhongMaterial color="#263947" specular="#637887" shininess={24} />
      </mesh>
      <mesh name="center-bezel" geometry={resources.bezel} position={[0, 0.435, 0]}>
        <meshPhongMaterial color="#8ea7be" specular="#8ea7be" shininess={16} />
      </mesh>
      <TableCenter texture={centerTexture} />
    </group>
  );
}
export function MatchTableScene({
  layout,
  projection,
  atlas,
  sheen,
  centerTexture,
  appearanceTextures,
  frameRevision,
  motion,
  onMotionComplete,
  onMotionFrame,
  onRenderReady,
}: MatchTableSceneProps) {
  const gl = useThree((state) => state.gl);
  const surroundTexture = appearanceTextures.surround;
  useLayoutEffect(() => {
    gl.shadowMap.enabled = true;
    gl.shadowMap.needsUpdate = true;
  }, [gl]);

  return (
    <>
      {surroundTexture ? <primitive attach="background" object={surroundTexture} /> : <color attach="background" args={["#eef0f2"]} />}
      <hemisphereLight args={["#fafbfc", "#354a66", 0.9]} />
      <directionalLight position={[-4, 9, 5]} color="#fafbfc" intensity={2.7} castShadow shadow-mapSize={[2048, 2048]} shadow-camera-left={-9} shadow-camera-right={9} shadow-camera-top={9} shadow-camera-bottom={-9} shadow-normalBias={0.02} />
      <FixedCamera />
      <group position={TABLE_RENDER_OFFSET}>
        <ProceduralTable centerTexture={centerTexture} surfaceTexture={appearanceTextures.surface} />
        {tableSeatGeometry(projection.mode, projection.viewer_seat).filter(({ seat }) => projection.players?.some((player) => player.seat === seat && player.riichi)).map(({ seat, position }) => {
          const yaw = { bottom: 0, right: Math.PI / 2, top: Math.PI, left: -Math.PI / 2 }[position];
          const x = position === "right" ? 1.55 : position === "left" ? -1.55 : 0;
          const z = position === "bottom" ? 1.25 : position === "top" ? -1.25 : 0;
          return <group key={seat} name={`riichi-stick-${seat}`} position={[x, 0.507, z]} rotation={[0, yaw, 0]}>
            <mesh castShadow><boxGeometry args={[0.65, 0.035, 0.04]} /><meshPhongMaterial color="#fafbfc" /></mesh>
            <mesh position={[0, 0.019, 0]}><cylinderGeometry args={[0.014, 0.014, 0.003, 12]} /><meshBasicMaterial color="#b83243" /></mesh>
          </group>;
        })}
        <DockedTiles layout={layout} atlas={atlas} projection={projection} sheen={sheen} motion={motion} onMotionComplete={onMotionComplete} onMotionFrame={onMotionFrame} />
      </group>
      <SceneReadiness
        layout={layout}
        frameRevision={frameRevision}
        onRenderReady={onRenderReady}
      />
    </>
  );
}
