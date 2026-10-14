import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadLocalTableImage, useTableAppearance } from "./table-appearance";

const header = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0]);
function file(type = "image/png", size = 12, bytes = header) {
  const value = new File([bytes], "local-image", { type });
  Object.defineProperty(value, "size", { value: size });
  Object.defineProperty(value, "slice", { value: () => ({ arrayBuffer: async () => bytes.buffer }) });
  return value;
}
const pending: Array<{ resolve(bitmap: ImageBitmap): void; reject(error: Error): void }> = [];
const bitmap = (width = 4000, height = 4000) => ({ width, height, close: vi.fn() }) as unknown as ImageBitmap;
const decode = vi.fn(() => new Promise<ImageBitmap>((resolve, reject) => pending.push({ resolve, reject })));
beforeEach(() => { pending.length = 0; decode.mockClear(); vi.stubGlobal("createImageBitmap", decode); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe("local table images", () => {
  it("rejects unsupported, forged and oversized files before any pixel decode", async () => {
    for (const value of [file("image/svg+xml"), file("image/png", 10 * 1024 * 1024 + 1), file("image/png", 12, new Uint8Array(12))]) {
      await expect(loadLocalTableImage(value, new AbortController().signal)).rejects.toThrow();
    }
    expect(decode).not.toHaveBeenCalled();
  });
  it("accepts inclusive byte/pixel limits, uses native Blob decode and closes the owned bitmap once", async () => {
    const input = file("image/png", 10 * 1024 * 1024);
    const loaded = loadLocalTableImage(input, new AbortController().signal);
    await waitFor(() => expect(pending).toHaveLength(1));
    const image = bitmap(); pending[0].resolve(image);
    const local = await loaded;
    expect(local.image.width * local.image.height).toBe(16_000_000);
    expect(decode).toHaveBeenCalledWith(input, { imageOrientation: "flipY", premultiplyAlpha: "none", colorSpaceConversion: "none" });
    local.dispose(); local.dispose(); expect(image.close).toHaveBeenCalledOnce();
  });
  it.each(["pixels", "decode", "abort"] as const)("reverts to standard and releases a failed %s decode", async reason => {
    const controller = new AbortController();
    const loaded = loadLocalTableImage(file(), controller.signal);
    const failure = expect(loaded).rejects.toThrow();
    await waitFor(() => expect(pending).toHaveLength(1));
    const image = bitmap(reason === "pixels" ? 4001 : 4000);
    if (reason === "decode") pending[0].reject(new Error("invalid pixels"));
    else if (reason === "abort") { controller.abort(); await failure; pending[0].resolve(image); }
    else pending[0].resolve(image);
    await failure; await Promise.resolve();
    expect(image.close).toHaveBeenCalledTimes(reason === "decode" ? 0 : 1);
  });
  it("ignores stale replacement pixels, clears/reselects and releases pending decode after navigation", async () => {
    const view = renderHook(() => useTableAppearance());
    let first!: Promise<void>, second!: Promise<void>;
    act(() => { first = view.result.current.select("surface", file()); });
    await waitFor(() => expect(pending).toHaveLength(1));
    act(() => { second = view.result.current.select("surface", file()); });
    await waitFor(() => expect(pending).toHaveLength(2));
    const stale = bitmap(), image = bitmap();
    await act(async () => { pending[1].resolve(image); await second; pending[0].resolve(stale); await first; });
    expect(view.result.current.appearance.surface?.image).toBe(image);
    expect(stale.close).toHaveBeenCalledOnce();
    act(() => view.result.current.clear("surface"));
    expect(view.result.current.appearance.surface).toBeNull(); expect(image.close).toHaveBeenCalledOnce();
    let loading!: Promise<void>;
    act(() => { loading = view.result.current.select("surround", file()); });
    await waitFor(() => expect(pending).toHaveLength(3));
    view.unmount(); await loading;
    const late = bitmap(); pending[2].resolve(late); await Promise.resolve();
    expect(late.close).toHaveBeenCalledOnce();
  });
});
