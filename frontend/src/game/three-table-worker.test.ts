import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import type { ComponentProps, ReactElement } from "react";
import { NoToneMapping, SRGBColorSpace, Texture } from "three";
import { MatchTableScene } from "./three-table-scene";
import { TableCenter } from "./table-center";
import { CAMERA, buildMatchSceneLayout } from "./three-table-layout";
import type { ProjectedState } from "./types";
import type { TableRenderInput, TableRenderReply } from "./three-table-render-messages";

// R3F root is the GPU/platform adapter; the actual Worker owner/message handler runs.
const rootAdapter = vi.hoisted(() => ({ configure: vi.fn(), render: vi.fn(), unmount: vi.fn(), options: {} as Record<string, unknown> }));
const scenePlatform = vi.hoisted(() => ({ invalidate: vi.fn(), gl: { shadowMap: { enabled: false, needsUpdate: false } }, size: { width: 1280, height: 720 } }));
vi.mock("@react-three/fiber", () => ({
  extend: () => {}, useThree: (select: (state: typeof scenePlatform) => unknown) => select(scenePlatform), useFrame: () => {},
  createRoot: () => ({ configure: rootAdapter.configure, render: rootAdapter.render }),
  unmountComponentAtNode: rootAdapter.unmount,
}));
let emitted: TableRenderReply[];
let handle: (event: MessageEvent) => Promise<void>;
const renderer = { outputColorSpace: "", toneMapping: 99, toneMappingExposure: 0, dispose: vi.fn() };
const state = { mode: "4p-red-east", audience: "player", viewer_seat: 0, players: [], remaining_wall: 42 } as ProjectedState;
const bitmap = () => ({ width: 2048, height: 2048, close: vi.fn() }) as unknown as ImageBitmap;
function input(): TableRenderInput {
  return { type: "init", revision: 1, canvas: { addEventListener: vi.fn() } as unknown as OffscreenCanvas,
    width: 1280, height: 720, dpr: 2, windowTimeOrigin: performance.timeOrigin,
    scene: { projection: state, layout: buildMatchSceneLayout(state, null, "live") },
    assets: { atlas: { bitmap: bitmap(), columns: 8, rows: 5, cells: [[0, 0]] }, center: bitmap() },
    motion: null, sheen: { enabled: false, reduced: false, visible: true } };
}
beforeEach(async () => {
  vi.resetModules(); vi.useFakeTimers(); emitted = [];
  rootAdapter.configure.mockReset(); rootAdapter.render.mockReset(); rootAdapter.unmount.mockReset(); renderer.dispose.mockClear();
  rootAdapter.configure.mockImplementation(async options => { rootAdapter.options = options; });
  rootAdapter.render.mockImplementation(() => {
    (rootAdapter.options.onCreated as (state: unknown) => void)({ gl: renderer });
  });
  const realm = { postMessage: (message: TableRenderReply) => emitted.push(message), addEventListener: vi.fn(), onmessage: null as unknown };
  vi.stubGlobal("self", realm);
  await import("./three-table-worker");
  handle = realm.onmessage as typeof handle;
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
const send = (data: TableRenderInput | { type: "dispose" }) => handle({ data } as MessageEvent);

describe("Dedicated renderer construction owner", () => {
  it("retains the original 2999/3000ms never-created guard inside the constructing realm", async () => {
    rootAdapter.render.mockImplementation(() => {});
    await send(input());
    vi.advanceTimersByTime(2999); expect(emitted.some(reply => reply.type === "fatal")).toBe(false);
    vi.advanceTimersByTime(1); expect(emitted.filter(reply => reply.type === "fatal")).toHaveLength(1);
  });
  it("does not classify a constructed renderer as never-created when reply dispatch is delayed", async () => {
    await send(input());
    // Parent message dispatch is intentionally absent in this platform adapter.
    vi.advanceTimersByTime(3000);
    expect(emitted.filter(reply => reply.type === "created")).toHaveLength(1);
    expect(emitted.some(reply => reply.type === "fatal")).toBe(false);
  });
  it("clears its old creation guard on teardown and acknowledges only actual root cleanup", async () => {
    rootAdapter.render.mockImplementation(() => {});
    const data = input(); await send(data);
    await send({ type: "dispose" }); vi.advanceTimersByTime(3000);
    expect(emitted.some(reply => reply.type === "fatal" || reply.type === "disposed")).toBe(false);
    const release = rootAdapter.unmount.mock.lastCall![1] as () => void;
    release();
    expect(data.assets.atlas!.bitmap.close).toHaveBeenCalledOnce();
    expect(data.assets.center!.close).toHaveBeenCalledOnce();
    expect(emitted.filter(reply => reply.type === "disposed")).toHaveLength(1);
  });
  it("does not revive a disposed root when pending configuration resolves", async () => {
    let finishConfigure!: () => void;
    rootAdapter.configure.mockImplementation(options => {
      rootAdapter.options = options;
      return new Promise<void>(resolve => { finishConfigure = resolve; });
    });
    const data = input();
    const configuring = send(data);
    await send({ type: "dispose" });
    expect(emitted).toEqual([]);
    expect(data.assets.atlas!.bitmap.close).not.toHaveBeenCalled();
    (rootAdapter.unmount.mock.lastCall![1] as () => void)();
    finishConfigure();
    await configuring;
    expect(rootAdapter.render).not.toHaveBeenCalled();
    expect(emitted).toEqual([{ type: "disposed" }]);
    expect(data.assets.atlas!.bitmap.close).toHaveBeenCalledOnce();
    expect(data.assets.center!.close).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(3000);
    expect(emitted).toEqual([{ type: "disposed" }]);
  });
  it.each(["revision", "width"] as const)("closes transferred pixels when an invalid %s rejects the input", async field => {
    const data = input();
    data[field] = field === "revision" ? 0 : Number.NaN;
    data.assets.surface = bitmap(); data.assets.surround = bitmap();
    await send(data);
    expect(emitted).toEqual([{ type: "fatal" }]);
    for (const pixel of [data.assets.atlas!.bitmap, data.assets.center!, data.assets.surface, data.assets.surround]) {
      expect(pixel.close).toHaveBeenCalledOnce();
    }
  });
  it("closes rejected stale pixels without destroying the current renderer's pixels", async () => {
    const original = input(); await send(original);
    const stale = input(); stale.type = "render";
    await send(stale);
    expect(emitted.at(-1)).toEqual({ type: "fatal" });
    expect(stale.assets.atlas!.bitmap.close).toHaveBeenCalledOnce();
    expect(stale.assets.center!.close).toHaveBeenCalledOnce();
    expect(original.assets.atlas!.bitmap.close).not.toHaveBeenCalled();
    expect(original.assets.center!.close).not.toHaveBeenCalled();
    await send({ type: "dispose" });
    (rootAdapter.unmount.mock.lastCall![1] as () => void)();
    expect(original.assets.atlas!.bitmap.close).toHaveBeenCalledOnce();
    expect(original.assets.center!.close).toHaveBeenCalledOnce();
    expect(stale.assets.atlas!.bitmap.close).toHaveBeenCalledOnce();
    expect(stale.assets.center!.close).toHaveBeenCalledOnce();
  });
  it("owns centered cover and replacement cleanup", async () => {
    const data = input();
    const first = bitmap(), replacement = bitmap(), last = bitmap();
    data.assets.surround = first;
    await send(data);
    const props = () => (rootAdapter.render.mock.lastCall![0] as ReactElement<ComponentProps<typeof MatchTableScene>>).props;
    const wide = props().appearanceTextures!.surround!;
    expect(wide.repeat.toArray()).toEqual([1, 0.5625]);
    expect(wide.offset.toArray()).toEqual([0, 0.21875]);
    expect(wide.flipY).toBe(false); expect(wide.generateMipmaps).toBe(false);
    const firstDispose = vi.spyOn(wide, "dispose");
    await send({ ...data, type: "render", revision: 2, width: 720, height: 1280, assets: {} });
    expect(props().appearanceTextures!.surround).toBe(wide);
    expect(wide.repeat.toArray()).toEqual([0.5625, 1]);
    expect(wide.offset.toArray()).toEqual([0.21875, 0]);
    await send({ ...data, type: "render", revision: 3, assets: { surround: replacement } });
    expect(firstDispose).toHaveBeenCalledOnce(); expect(first.close).toHaveBeenCalledOnce();
    const replacementDispose = vi.spyOn(props().appearanceTextures!.surround!, "dispose");
    await send({ ...data, type: "render", revision: 4, assets: { surround: null } });
    expect(props().appearanceTextures!.surround).toBeNull();
    expect(replacementDispose).toHaveBeenCalledOnce(); expect(replacement.close).toHaveBeenCalledOnce();
    await send({ ...data, type: "render", revision: 5, assets: { surround: last } });
    const lastDispose = vi.spyOn(props().appearanceTextures!.surround!, "dispose");
    await send({ type: "dispose" });
    expect(last.close).not.toHaveBeenCalled();
    (rootAdapter.unmount.mock.lastCall![1] as () => void)();
    expect(lastDispose).toHaveBeenCalledOnce(); expect(last.close).toHaveBeenCalledOnce();
    expect(first.close).toHaveBeenCalledOnce(); expect(replacement.close).toHaveBeenCalledOnce();
  });
  it("leaves supplied textures owned by the Worker across Scene unmount", async () => {
    await send(input());
    const props = (rootAdapter.render.mock.lastCall![0] as ReactElement<ComponentProps<typeof MatchTableScene>>).props;
    const surface = new Texture(), surround = new Texture();
    const supplied = { ...props, appearanceTextures: { surface, surround } };
    const disposals = [surface, surround, props.centerTexture!].map(texture => vi.spyOn(texture, "dispose"));
    const scene = renderHook(() => MatchTableScene(supplied));
    const center = renderHook(() => TableCenter({ texture: props.centerTexture }));
    scene.unmount(); center.unmount();
    for (const dispose of disposals) expect(dispose).not.toHaveBeenCalled();
    await send({ type: "dispose" });
    (rootAdapter.unmount.mock.lastCall![1] as () => void)();
    expect(disposals[2]).toHaveBeenCalledOnce();
    surface.dispose(); surround.dispose();
  });
  it("preserves the fixed native renderer configuration without inheriting root defaults", async () => {
    await send(input());
    expect(rootAdapter.options).toMatchObject({
      size: { width: 1280, height: 720 }, dpr: 2, frameloop: "demand", flat: true,
      shadows: { enabled: true }, camera: { fov: CAMERA.fov, position: CAMERA.position, near: CAMERA.near, far: CAMERA.far },
      gl: { antialias: true, alpha: false, depth: true, stencil: false, precision: "highp", powerPreference: "high-performance" },
    });
    expect(renderer.outputColorSpace).toBe(SRGBColorSpace); expect(renderer.toneMapping).toBe(NoToneMapping);
    expect(emitted.some(reply => reply.type === "ready")).toBe(false);
  });
});
