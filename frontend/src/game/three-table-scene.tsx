import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
  Box3,
  BoxGeometry,
  Color,
  DoubleSide,
  Euler,
  InstancedBufferAttribute,
  Group,
  InstancedMesh,
  Matrix4,
  MeshBasicMaterial,
  MeshStandardMaterial,
  PerspectiveCamera,
  PlaneGeometry,
  Quaternion,
  Scene,
  Texture,
  TextureLoader,
  Vector3,
} from "three";

import type { TileAtlas } from "./tile-atlas";
import {
  configureTableTexture,
  disposeTableTextures,
  TABLE_TEXTURE_SPECS,
  TABLE_TEXTURE_URLS,
  type TableTextureKey,
} from "./table-materials";
import { CAMERA, TABLE_SIZE, type MatchSceneLayout, type SceneTile } from "./three-table-layout";
import { cameraAccentAt, sceneMotionProgress, type SceneMotion } from "./three-table-motion";

export interface SceneRenderStats {
  tileCount: number;
  primitiveCount: number;
  tableHeightRatio?: number;
  tableWidthRatio?: number;
}

interface MatchTableSceneProps {
  layout: MatchSceneLayout;
  atlas: TileAtlas;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number): void;
  onRenderReady(stats: SceneRenderStats): void;
}

export const TABLE_RENDER_SCALE = {
  x: 0.9,
  y: 1,
  z: 1.1,
} as const;

const TABLE_RENDER_OFFSET: readonly [number, number, number] = [0, 0, -0.38];
const BODY_SIZE = [0.62, 0.18, 0.86] as const;
const FACE_SIZE = [0.58, 0.82] as const;
const FACE_Y = BODY_SIZE[1] / 2 + 0.003;
const MAX_TILE_INSTANCES = 256;

interface TableTextures {
  felt: Texture;
  rail: Texture;
  center: Texture;
  back: Texture;
}

function instanceMatrix(tile: SceneTile, face = false): Matrix4 {
  const rotation = face
    ? new Euler(tile.rotation[0] - Math.PI / 2, tile.rotation[1], tile.rotation[2])
    : new Euler(...tile.rotation);
  return new Matrix4().compose(
    new Vector3(
      tile.position[0],
      tile.position[1] + (face ? FACE_Y * tile.scale : 0),
      tile.position[2],
    ),
    new Quaternion().setFromEuler(rotation),
    new Vector3(tile.scale, tile.scale, tile.scale),
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
    side: DoubleSide,
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
    const keys: readonly TableTextureKey[] = ["felt", "rail", "center", "back"];

    // Wait for every request before deciding ownership. Promise.all would reject
    // on the first failed request and leak textures that finish afterward.
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

      const next = {
        felt: loaded.felt!,
        rail: loaded.rail!,
        center: loaded.center!,
        back: loaded.back!,
      } satisfies TableTextures;
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
  textures,
}: Pick<MatchTableSceneProps, "layout" | "atlas"> & { textures: TableTextures | null }) {
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
    const bodyGeometry = new BoxGeometry(...BODY_SIZE, 3, 1, 3);
    const faceGeometry = new PlaneGeometry(...FACE_SIZE);
    const faceCells = new Float32Array(MAX_TILE_INSTANCES * 2);
    faceGeometry.setAttribute("atlasCell", new InstancedBufferAttribute(faceCells, 2));
    return {
      bodyGeometry,
      faceGeometry,
      backGeometry: new PlaneGeometry(...FACE_SIZE),
      ivoryMaterial: new MeshStandardMaterial({
        color: new Color("#c8b99d"),
        metalness: 0.02,
        roughness: 0.6,
      }),
      faceMaterial: atlasMaterial(atlas),
      backMaterial: (() => {
        const material = new MeshBasicMaterial({
          color: new Color("#ffffff"),
          side: DoubleSide,
          toneMapped: false,
        });
        if (textures?.back) material.map = textures.back;
        return material;
      })(),
    };
  }, [atlas, textures?.back]);

  useEffect(
    () => () => {
      resources.bodyGeometry.dispose();
      resources.faceGeometry.dispose();
      resources.backGeometry.dispose();
      resources.ivoryMaterial.dispose();
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
        args={[resources.bodyGeometry, resources.ivoryMaterial, MAX_TILE_INSTANCES]}
        castShadow
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
        args={[resources.bodyGeometry, resources.ivoryMaterial, MAX_TILE_INSTANCES]}
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

function resetMotionGroup(group: Group | null): void {
  if (!group) return;
  group.position.set(0, 0, 0);
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

function MotionController({
  groupRef,
  motion,
  onMotionComplete,
  onMotionFrame,
}: {
  groupRef: React.RefObject<Group | null>;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number): void;
}) {
  const startedAtRef = useRef<number | null>(null);
  const motionPixelRatioRef = useRef<number | null>(null);
  const motionPixelRatioAppliedRef = useRef(false);
  const invalidate = useThree((state) => state.invalidate);
  const camera = useThree((state) => state.camera) as PerspectiveCamera;
  const gl = useThree((state) => state.gl);

  useLayoutEffect(() => {
    if (!motion) {
      startedAtRef.current = null;
      if (motionPixelRatioRef.current !== null) {
        gl.setPixelRatio(motionPixelRatioRef.current);
        motionPixelRatioRef.current = null;
        motionPixelRatioAppliedRef.current = false;
      }
      resetMotionGroup(groupRef.current);
      invalidate();
      return undefined;
    }
    startedAtRef.current = null;
    motionPixelRatioRef.current ??= gl.getPixelRatio();
    // Keep the physically lit scene responsive during the short motion window;
    // the first frame establishes the active state before lowering authored DPR.
    invalidate();
    return () => {
      resetMotionGroup(groupRef.current);
      applyCameraFrame(camera, motion.kind, 1);
      if (motionPixelRatioRef.current !== null) {
        gl.setPixelRatio(motionPixelRatioRef.current);
        motionPixelRatioRef.current = null;
        motionPixelRatioAppliedRef.current = false;
      }
      invalidate();
    };
  }, [camera, gl, groupRef, invalidate, motion?.itemId, motion?.kind, motion?.startedAt]);

  useFrame(() => {
    const group = groupRef.current;
    if (!group || !motion) return;
    if (motionPixelRatioRef.current !== null && !motionPixelRatioAppliedRef.current) {
      gl.setPixelRatio(Math.min(motionPixelRatioRef.current, 0.1));
      motionPixelRatioAppliedRef.current = true;
    }
    const now = performance.now();
    startedAtRef.current ??= now - 32;
    const progress = sceneMotionProgress(
      { ...motion, startedAt: startedAtRef.current },
      now,
    );
    const pulse = Math.sin(Math.PI * progress);
    const translates = motion.kind === "draw" || motion.kind === "discard";
    group.position.y = translates ? pulse * 0.09 : 0;
    const scale = translates ? 1 : 1 + pulse * (motion.kind === "win" ? 0 : 0.012);
    group.scale.set(scale, scale, scale);
    applyCameraFrame(camera, motion.kind, progress);
    onMotionFrame(now);
    if (progress >= 1) {
      resetMotionGroup(group);
      applyCameraFrame(camera, motion.kind, 1);
      onMotionComplete(motion.itemId);
      return;
    }
    invalidate();
  });

  return null;
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

function TableRails({ textures }: { textures: TableTextures | null }) {
  const invalidate = useThree((state) => state.invalidate);
  const meshRef = useRef<InstancedMesh>(null);
  const resources = useMemo(() => {
    const material = new MeshStandardMaterial({
      color: new Color("#ffffff"),
      metalness: 0.18,
      roughness: 0.48,
    });
    if (textures?.rail) material.map = textures.rail;
    return { geometry: new BoxGeometry(1, 1, 1), material };
  }, [textures?.rail]);

  useLayoutEffect(() => {
    const mesh = meshRef.current;
    if (!mesh) return;
    const rails = [
      { position: [0, 0.2, 4.24] as const, scale: [12.62, 0.34, 0.62] as const },
      { position: [0, 0.2, -4.24] as const, scale: [12.62, 0.34, 0.62] as const },
      { position: [6.44, 0.2, 0] as const, scale: [0.62, 0.34, 7.62] as const },
      { position: [-6.44, 0.2, 0] as const, scale: [0.62, 0.34, 7.62] as const },
    ];
    rails.forEach((rail, index) => {
      mesh.setMatrixAt(index, new Matrix4().compose(
        new Vector3(...rail.position),
        new Quaternion(),
        new Vector3(...rail.scale),
      ));
    });
    mesh.instanceMatrix.needsUpdate = true;
    invalidate();
  }, [invalidate]);

  useEffect(() => () => {
    resources.geometry.dispose();
    resources.material.dispose();
  }, [resources]);

  return (
    <instancedMesh
      ref={meshRef}
      name="table-rails"
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
    material: new MeshStandardMaterial({
      color: new Color("#a77842"),
      metalness: 0.58,
      roughness: 0.34,
    }),
  }), []);

  useLayoutEffect(() => {
    const mesh = meshRef.current;
    if (!mesh) return;
    const trims = [
      { position: [0, 0.37, -0.73] as const, scale: [1.62, 0.035, 0.055] as const },
      { position: [0, 0.37, 0.73] as const, scale: [1.62, 0.035, 0.055] as const },
      { position: [-0.92, 0.37, 0] as const, scale: [0.055, 0.035, 1.32] as const },
      { position: [0.92, 0.37, 0] as const, scale: [0.055, 0.035, 1.32] as const },
    ];
    trims.forEach((trim, index) => {
      mesh.setMatrixAt(index, new Matrix4().compose(
        new Vector3(...trim.position),
        new Quaternion(),
        new Vector3(...trim.scale),
      ));
    });
    mesh.instanceMatrix.needsUpdate = true;
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
      args={[resources.geometry, resources.material, 4]}
      frustumCulled={false}
      dispose={null}
    />
  );
}

function FeltMaterial({ texture }: { texture: Texture | undefined }) {
  return texture ? (
    <meshStandardMaterial
      color="#3d7d61"
      map={texture}
      metalness={0.02}
      roughness={0.9}
    />
  ) : (
    <meshStandardMaterial color="#3d7d61" metalness={0.02} roughness={0.9} />
  );
}

function CenterMaterial({ texture }: { texture: Texture | undefined }) {
  return texture ? (
    <meshStandardMaterial
      color="#bd9867"
      map={texture}
      metalness={0.24}
      roughness={0.44}
      emissive="#2c506b"
      emissiveIntensity={0.9}
      side={DoubleSide}
      transparent
    />
  ) : (
    <meshStandardMaterial color="#151d22" metalness={0.3} roughness={0.44} />
  );
}

function ProceduralTable({ textures }: { textures: TableTextures | null }) {
  return (
    <group name="table-body-root">
      <mesh position={[0, -0.48, 0]}>
        <boxGeometry args={[TABLE_SIZE.width, 0.72, TABLE_SIZE.depth]} />
        <meshStandardMaterial color="#293337" metalness={0.34} roughness={0.58} />
      </mesh>
      <mesh position={[0, -0.09, 0]}>
        <boxGeometry args={[12.65, 0.18, 8.25]} />
        <meshStandardMaterial color="#65402d" metalness={0.08} roughness={0.62} />
      </mesh>
      <mesh position={[0, 0.015, 0]}>
        <boxGeometry args={[11.7, 0.16, 7.3]} />
        <FeltMaterial texture={textures?.felt} />
      </mesh>
      <mesh position={[0, 0.18, 0]}>
        <boxGeometry args={[2.55, 0.3, 2.05]} />
        <meshStandardMaterial color="#29353b" metalness={0.5} roughness={0.42} />
      </mesh>
      <mesh position={[0, 0.345, 0]}>
        <boxGeometry args={[2.16, 0.035, 1.66]} />
        <meshStandardMaterial color="#1c3546" metalness={0.28} roughness={0.48} />
      </mesh>
      <mesh position={[0, 0.366, 0]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[2.16, 1.66]} />
        <CenterMaterial texture={textures?.center} />
      </mesh>
      <mesh position={[0, 0.37, 0]}>
        <ringGeometry args={[0.36, 0.43, 48]} />
        <meshStandardMaterial color="#a77842" metalness={0.58} roughness={0.34} />
      </mesh>
      <CenterTrim />
      <TableRails textures={textures} />
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
  const tileGroupRef = useRef<Group>(null);

  return (
    <>
      <color attach="background" args={["#050709"]} />
      <hemisphereLight args={["#dce8e0", "#17231f", 0.48]} />
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
      <pointLight position={[6.5, 5.2, -2.8]} color="#a7c7dc" intensity={0.46} />
      <spotLight
        position={[0, 3.8, 8.5]}
        color="#d8a873"
        intensity={0.64}
        angle={0.48}
        penumbra={0.9}
        distance={18}
      />
      <FixedCamera />
      <group scale={[TABLE_RENDER_SCALE.x, TABLE_RENDER_SCALE.y, TABLE_RENDER_SCALE.z]} position={TABLE_RENDER_OFFSET}>
        <ProceduralTable textures={textures} />
        <group ref={tileGroupRef}>
          <InstancedTiles layout={layout} atlas={atlas} textures={textures} />
        </group>
      </group>
      <SceneReadiness
        layout={layout}
        materialsReady={textures !== null}
        onRenderReady={onRenderReady}
      />
      <MotionController
        groupRef={tileGroupRef}
        motion={motion}
        onMotionComplete={onMotionComplete}
        onMotionFrame={onMotionFrame}
      />
    </>
  );
}
