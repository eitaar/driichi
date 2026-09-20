import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
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
  Vector3,
} from "three";

import type { TileAtlas } from "./tile-atlas";
import { CAMERA, TABLE_SIZE, type MatchSceneLayout, type SceneTile } from "./three-table-layout";
import { cameraAccentAt, sceneMotionProgress, type SceneMotion } from "./three-table-motion";

export interface SceneRenderStats {
  tileCount: number;
  primitiveCount: number;
}

interface MatchTableSceneProps {
  layout: MatchSceneLayout;
  atlas: TileAtlas;
  motion: SceneMotion | null;
  onMotionComplete(itemId: number): void;
  onMotionFrame(now: number): void;
  onRenderReady(stats: SceneRenderStats): void;
}

const BODY_SIZE = [0.62, 0.18, 0.86] as const;
const FACE_SIZE = [0.58, 0.82] as const;
const FACE_Y = BODY_SIZE[1] / 2 + 0.003;

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
    const bodyGeometry = new BoxGeometry(...BODY_SIZE, 3, 1, 3);
    const faceGeometry = new PlaneGeometry(...FACE_SIZE);
    const faceCells = new Float32Array(frontTiles.length * 2);
    for (let index = 0; index < frontTiles.length; index += 1) {
      const [column, row] = atlas.cellFor(frontTiles[index].tile ?? -1);
      faceCells[index * 2] = column;
      faceCells[index * 2 + 1] = row;
    }
    faceGeometry.setAttribute("atlasCell", new InstancedBufferAttribute(faceCells, 2));
    return {
      bodyGeometry,
      faceGeometry,
      backGeometry: new PlaneGeometry(...FACE_SIZE),
      ivoryMaterial: new MeshStandardMaterial({
        color: new Color("#eee5d2"),
        metalness: 0.02,
        roughness: 0.6,
      }),
      faceMaterial: atlasMaterial(atlas),
      backMaterial: new MeshStandardMaterial({
        color: new Color("#173a33"),
        metalness: 0.04,
        roughness: 0.72,
        side: DoubleSide,
      }),
    };
  }, [atlas, frontTiles]);

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
    applyMatrices(frontBody.current, frontTiles, false);
    applyMatrices(frontFaces.current, frontTiles, true);
    applyMatrices(backBody.current, backTiles, false);
    applyMatrices(backFaces.current, backTiles, true);
    invalidate();
  }, [backTiles, frontTiles, invalidate]);

  return (
    <group dispose={null}>
      {frontTiles.length > 0 ? (
        <>
          <instancedMesh
            ref={frontBody}
            name="tile-front-bodies"
            userData={{ tileBodies: true }}
            args={[resources.bodyGeometry, resources.ivoryMaterial, frontTiles.length]}
            castShadow
            receiveShadow
            frustumCulled={false}
            dispose={null}
          />
          <instancedMesh
            ref={frontFaces}
            name="tile-front-faces"
            args={[resources.faceGeometry, resources.faceMaterial, frontTiles.length]}
            receiveShadow
            frustumCulled={false}
            dispose={null}
          />
        </>
      ) : null}
      {backTiles.length > 0 ? (
        <>
          <instancedMesh
            ref={backBody}
            name="tile-back-bodies"
            userData={{ tileBodies: true }}
            args={[resources.bodyGeometry, resources.ivoryMaterial, backTiles.length]}
            receiveShadow
            frustumCulled={false}
            dispose={null}
          />
          <instancedMesh
            ref={backFaces}
            name="tile-back-faces"
            args={[resources.backGeometry, resources.backMaterial, backTiles.length]}
            receiveShadow
            frustumCulled={false}
            dispose={null}
          />
        </>
      ) : null}
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
  camera.fov = frame.fov;
  camera.position.set(...CAMERA.position);
  camera.lookAt(...frame.target);
  camera.updateProjectionMatrix();
}

function MotionTiles({
  layout,
  atlas,
  motion,
  onMotionComplete,
  onMotionFrame,
}: Pick<
  MatchTableSceneProps,
  "layout" | "atlas" | "onMotionComplete" | "onMotionFrame"
> & { motion: SceneMotion }) {
  const groupRef = useRef<Group>(null);
  const startedAtRef = useRef<number | null>(null);
  const invalidate = useThree((state) => state.invalidate);
  const camera = useThree((state) => state.camera) as PerspectiveCamera;

  useLayoutEffect(() => {
    startedAtRef.current = null;
    invalidate();
    return () => {
      resetMotionGroup(groupRef.current);
      applyCameraFrame(camera, motion.kind, 1);
      invalidate();
    };
  }, [camera, invalidate, motion.itemId, motion.kind]);

  useFrame(() => {
    const group = groupRef.current;
    if (!group) return;
    const now = performance.now();
    startedAtRef.current ??= now;
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

  return (
    <group ref={groupRef}>
      <InstancedTiles layout={layout} atlas={atlas} />
    </group>
  );
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

function TableRails() {
  const invalidate = useThree((state) => state.invalidate);
  const meshRef = useRef<InstancedMesh>(null);
  const resources = useMemo(() => ({
    geometry: new BoxGeometry(1, 1, 1),
    material: new MeshStandardMaterial({
      color: new Color("#171c20"),
      metalness: 0.58,
      roughness: 0.46,
    }),
  }), []);

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
      receiveShadow
      frustumCulled={false}
      dispose={null}
    />
  );
}

function SceneReadiness({
  layout,
  onRenderReady,
}: Pick<MatchTableSceneProps, "layout" | "onRenderReady">) {
  const invalidate = useThree((state) => state.invalidate);
  const scene = useThree((state) => state.scene);
  const gl = useThree((state) => state.gl);
  const scheduled = useRef(false);
  const reported = useRef(false);

  useEffect(() => {
    scheduled.current = false;
    reported.current = false;
    invalidate();
  }, [invalidate, layout]);

  useFrame(() => {
    if (scheduled.current || reported.current) return;
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
      onRenderReady({ tileCount, primitiveCount });
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
      color: new Color("#9a7042"),
      metalness: 0.72,
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
      receiveShadow
      frustumCulled={false}
      dispose={null}
    />
  );
}

function ProceduralTable() {
  return (
    <group scale={[0.94, 1, 1]}>
      <mesh position={[0, -0.48, 0]} receiveShadow>
        <boxGeometry args={[TABLE_SIZE.width, 0.72, TABLE_SIZE.depth]} />
        <meshStandardMaterial color="#171c20" metalness={0.42} roughness={0.48} />
      </mesh>
      <mesh position={[0, -0.09, 0]} receiveShadow>
        <boxGeometry args={[12.65, 0.18, 8.25]} />
        <meshStandardMaterial color="#32251e" metalness={0.2} roughness={0.62} />
      </mesh>
      <mesh position={[0, 0.015, 0]} receiveShadow>
        <boxGeometry args={[11.7, 0.16, 7.3]} />
        <meshStandardMaterial color="#21483f" metalness={0.02} roughness={0.94} />
      </mesh>
      <mesh position={[0, 0.18, 0]} receiveShadow>
        <boxGeometry args={[2.55, 0.3, 2.05]} />
        <meshStandardMaterial color="#30393b" metalness={0.64} roughness={0.38} />
      </mesh>
      <mesh position={[0, 0.345, 0]} receiveShadow>
        <boxGeometry args={[2.16, 0.035, 1.66]} />
        <meshStandardMaterial color="#1d292c" emissive="#0c1214" emissiveIntensity={0.35} metalness={0.3} roughness={0.45} />
      </mesh>
      <mesh position={[0, 0.37, 0]} receiveShadow>
        <ringGeometry args={[0.36, 0.43, 48]} />
        <meshStandardMaterial color="#9a7042" metalness={0.72} roughness={0.34} />
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
  return (
    <>
      <color attach="background" args={["#050709"]} />
      <hemisphereLight args={["#e7ede8", "#17231f", 0.72]} />
      <directionalLight
        position={[-5.5, 10.5, 6.5]}
        color="#ffe2bd"
        intensity={2.6}
        castShadow
        shadow-mapSize-width={1024}
        shadow-mapSize-height={1024}
        shadow-camera-near={1}
        shadow-camera-far={32}
        shadow-camera-left={-9}
        shadow-camera-right={9}
        shadow-camera-top={7}
        shadow-camera-bottom={-7}
      />
      <pointLight position={[6.5, 5.2, -2.8]} color="#a7c7dc" intensity={0.8} />
      <spotLight
        position={[0, 3.8, 8.5]}
        color="#d8a873"
        intensity={0.85}
        angle={0.48}
        penumbra={0.9}
        distance={18}
      />
      <FixedCamera />
      <ProceduralTable />
      <SceneReadiness layout={layout} onRenderReady={onRenderReady} />
      {motion ? (
        <MotionTiles
          layout={layout}
          atlas={atlas}
          motion={motion}
          onMotionComplete={onMotionComplete}
          onMotionFrame={onMotionFrame}
        />
      ) : (
        <InstancedTiles layout={layout} atlas={atlas} />
      )}
    </>
  );
}
