import { createElement } from "react";
import { createRoot, extend, unmountComponentAtNode } from "@react-three/fiber";
import {
  Group, Mesh, InstancedMesh, Color, HemisphereLight, DirectionalLight,
  BoxGeometry, CylinderGeometry, PlaneGeometry, MeshBasicMaterial, MeshPhongMaterial,
  CanvasTexture, LinearFilter, SRGBColorSpace, NoToneMapping,
  type WebGLRenderer, type Texture,
} from "three";
import { MatchTableScene } from "./three-table-scene";
import { CAMERA, TABLE_WIDTH_STRETCH } from "./three-table-layout";
import { doraSheenFrame, type DoraSheenClock } from "./dora-sheen";
import type { TileAtlas } from "./tile-atlas";
import type { TableRenderInput, TableRenderReply, TableRenderQuality } from "./three-table-render-messages";

extend({ Group, Mesh, InstancedMesh, Color, HemisphereLight, DirectionalLight,
  BoxGeometry, CylinderGeometry, PlaneGeometry, MeshBasicMaterial, MeshPhongMaterial });
let canvas: OffscreenCanvas | null = null;
let root: ReturnType<typeof createRoot> | null = null;
let gl: WebGLRenderer | null = null;
let current: TableRenderInput | null = null;
let scene: TableRenderInput["scene"];
let atlas: TileAtlas | null = null;
let center: Texture | null = null;
const images: { surface: Texture | null; surround: Texture | null } = { surface: null, surround: null };
const owned = new Map<Texture, ImageBitmap>();
let created = false;
let disposed = false;
let creationTimer: ReturnType<typeof setTimeout> | null = null;
let sheenFrame: number | null = null;
let sheenTimer: ReturnType<typeof setTimeout> | null = null;
const sheen: DoraSheenClock = { sweep: { value: -10 }, strength: { value: 0 }, listeners: new Set() };
const reply = (message: TableRenderReply) => self.postMessage(message);
const fatal = () => { if (!disposed) reply({ type: "fatal" }); };

function release(texture: Texture | null) {
  if (!texture) return;
  texture.dispose();
  owned.get(texture)?.close();
  owned.delete(texture);
}
function texture(bitmap: ImageBitmap, unclaimed: Set<ImageBitmap>, anisotropy = 1) {
  const next = new CanvasTexture(bitmap);
  next.colorSpace = SRGBColorSpace;
  next.flipY = false;
  next.generateMipmaps = false;
  next.minFilter = next.magFilter = LinearFilter;
  next.anisotropy = anisotropy;
  owned.set(next, bitmap);
  unclaimed.delete(bitmap);
  return next;
}
function cover(map: Texture | null, aspect: number) {
  if (!map) return;
  const bitmap = owned.get(map)!;
  const imageAspect = bitmap.width / bitmap.height;
  const x = imageAspect > aspect ? aspect / imageAspect : 1;
  const y = imageAspect > aspect ? 1 : imageAspect / aspect;
  map.repeat.set(x, y);
  map.offset.set((1 - x) / 2, (1 - y) / 2);
}
function quality(): TableRenderQuality {
  const renderer = gl!;
  return { antialias: renderer.getContext().getContextAttributes()?.antialias === true,
    clearAlpha: renderer.getClearAlpha(), pixelRatio: renderer.getPixelRatio(),
    width: renderer.domElement.width, height: renderer.domElement.height,
    precision: renderer.capabilities.precision, colorSpace: renderer.outputColorSpace,
    toneMapping: renderer.toneMapping, shadowType: renderer.shadowMap.type, shaderChecks: renderer.debug.checkShaderErrors };
}
function cancelSheen() {
  if (sheenFrame !== null) cancelAnimationFrame(sheenFrame);
  if (sheenTimer !== null) clearTimeout(sheenTimer);
  sheenFrame = sheenTimer = null;
}
function tickSheen() {
  cancelSheen();
  if (disposed || !current) return;
  const flags = current.sheen;
  const now = performance.timeOrigin + performance.now() - current.windowTimeOrigin;
  const frame = doraSheenFrame(now, flags.enabled, flags.reduced, flags.visible);
  sheen.sweep.value = frame.phase;
  sheen.strength.value = frame.strength;
  for (const notify of sheen.listeners) notify();
  if (frame.waitMs === 0) sheenFrame = requestAnimationFrame(tickSheen);
  else if (frame.waitMs !== null) sheenTimer = setTimeout(tickSheen, Math.ceil(frame.waitMs));
}
function dispose() {
  if (disposed) return;
  disposed = true;
  cancelSheen();
  if (creationTimer !== null) clearTimeout(creationTimer);
  const finish = () => {
    for (const resource of [...owned.keys()]) release(resource);
    gl?.dispose();
    reply({ type: "disposed" });
  };
  // Fiber invokes this callback after its real root/resource teardown, not on receipt.
  if (canvas && root) unmountComponentAtNode(canvas, finish);
  else finish();
}
self.addEventListener("error", fatal);
self.addEventListener("unhandledrejection", fatal);
self.onmessage = async ({ data }: MessageEvent<TableRenderInput | { type: "dispose" }>) => {
  if (data.type === "dispose") { dispose(); return; }
  if (disposed) {
    data.assets.atlas?.bitmap.close(); data.assets.center?.close();
    data.assets.surface?.close(); data.assets.surround?.close();
    return;
  }
  const unclaimed = new Set([data.assets.atlas?.bitmap, data.assets.center, data.assets.surface, data.assets.surround]
    .filter((bitmap): bitmap is ImageBitmap => bitmap != null));
  try {
    if (!["init", "render"].includes(data.type) || !Number.isSafeInteger(data.revision) || data.revision <= (current?.revision ?? 0)
      || ![data.width, data.height, data.dpr, data.windowTimeOrigin].every(Number.isFinite)
      || data.width <= 0 || data.height <= 0 || data.dpr < 1 || data.dpr > 2) throw new Error("Invalid presentation input");
    current = data;
    if (data.scene) scene = data.scene;
    if (data.assets.atlas) {
      release(atlas?.texture ?? null);
      const asset = data.assets.atlas;
      atlas = { texture: texture(asset.bitmap, unclaimed), columns: asset.columns, rows: asset.rows,
        cellFor: tile => asset.cells[tile + 1] ?? asset.cells[0], dispose: () => release(atlas?.texture ?? null) };
    }
    if (data.assets.center) { release(center); center = texture(data.assets.center, unclaimed, 16); }
    for (const key of ["surface", "surround"] as const) {
      const bitmap = data.assets[key];
      if (bitmap !== undefined) { release(images[key]); images[key] = bitmap ? texture(bitmap, unclaimed) : null; }
    }
    cover(images.surface, 13.2 * TABLE_WIDTH_STRETCH / 11.8);
    cover(images.surround, data.width / data.height);
    if (data.type === "init") {
      if (root || !data.canvas) throw new Error("Canvas already owned");
      canvas = data.canvas;
      canvas.addEventListener("webglcontextlost", event => { event.preventDefault(); fatal(); });
      root = createRoot(canvas);
      creationTimer = setTimeout(() => { if (!created && !disposed) fatal(); }, 3000);
    }
    if (!root || !scene || !atlas || !center) throw new Error("Incomplete presentation input");
    await root.configure({
      size: { width: data.width, height: data.height, top: 0, left: 0 },
      dpr: data.dpr, frameloop: data.motion ? "always" : "demand", flat: true,
      shadows: { enabled: true },
      camera: { fov: CAMERA.fov, position: [...CAMERA.position], near: CAMERA.near, far: CAMERA.far },
      gl: { antialias: true, alpha: false, depth: true, stencil: false, precision: "highp", powerPreference: "high-performance" },
      onCreated: state => {
        created = true;
        if (creationTimer !== null) clearTimeout(creationTimer);
        creationTimer = null;
        gl = state.gl;
        gl.outputColorSpace = SRGBColorSpace;
        gl.toneMapping = NoToneMapping;
        gl.toneMappingExposure = 1;
        reply({ type: "created" });
      },
    });
    if (disposed) return;
    const revision = data.revision;
    const offset = performance.timeOrigin - data.windowTimeOrigin;
    const motion = data.motion ? { ...data.motion, startedAt: data.motion.startedAt - offset } : null;
    root.render(createElement(MatchTableScene, {
      layout: scene.layout, projection: scene.projection, atlas, centerTexture: center,
      appearanceTextures: images, frameRevision: revision, sheen, motion,
      onMotionComplete: itemId => { if (!disposed) reply({ type: "motion-complete", itemId }); },
      onMotionFrame: (now, pixelRatio) => { if (!disposed && motion) reply({ type: "motion-frame", itemId: motion.itemId, now: now + offset, pixelRatio }); },
      onRenderReady: stats => { if (!disposed && revision === current?.revision) reply({ type: "ready", revision, stats, quality: quality() }); },
    }));
    reply({ type: "processed", revision });
    tickSheen();
  } catch { fatal(); }
  finally { for (const bitmap of unclaimed) bitmap.close(); }
};
