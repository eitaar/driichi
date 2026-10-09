import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import { createTableImageTexture, loadLocalTableImage, useTableAppearance } from "./table-appearance";

const header = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0]);
function file(type = "image/png", size = 12, bytes = header) {
  const value = new File([bytes], "local-image", { type });
  Object.defineProperty(value, "size", { value: size });
  Object.defineProperty(value, "slice", { value: () => ({ arrayBuffer: async () => bytes.buffer }) });
  return value;
}
let images: TestImage[];
let revoke: Mock<(url: string) => void>;
let create: Mock<(object: Blob | MediaSource) => string>;
class TestImage {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  naturalWidth = 4000;
  naturalHeight = 4000;
  source = "";
  set src(value: string) { this.source = value; if (value) images.push(this); }
  get src() { return this.source; }
}
beforeEach(() => {
  images = [];
  let id = 0;
  revoke = vi.fn();
  create = vi.fn(() => `blob:local/${++id}`);
  const NativeURL = URL;
  vi.stubGlobal("URL", class extends NativeURL {
    static createObjectURL(object: Blob | MediaSource): string { return create(object); }
    static revokeObjectURL(url: string): void { revoke(url); }
  });
  vi.stubGlobal("Image", TestImage);
});
afterEach(() => { vi.unstubAllGlobals(); });

describe("local table images", () => {
  it("uses centered aspect-preserving cover on surface and surround textures", () => {
    const image = new TestImage();
    const local = { url: "blob:local", image: image as unknown as HTMLImageElement, dispose: () => {} };
    const wide = createTableImageTexture(local, 2)!;
    expect(wide.repeat.toArray()).toEqual([1, 0.5]);
    expect(wide.offset.toArray()).toEqual([0, 0.25]);
    const narrow = createTableImageTexture(local, 0.5)!;
    expect(narrow.repeat.toArray()).toEqual([0.5, 1]);
    expect(narrow.offset.toArray()).toEqual([0.25, 0]);
    expect(wide.image).toBe(image);
    expect(wide.generateMipmaps).toBe(false);
    expect(createTableImageTexture(null, 1)).toBeNull();
    wide.dispose(); narrow.dispose();
  });
  it("rejects unsupported, forged or oversized files before allocating a URL", async () => {
    for (const value of [file("image/svg+xml"), file("image/png", 10 * 1024 * 1024 + 1), file("image/png", 12, new Uint8Array(12))]) {
      await expect(loadLocalTableImage(value, new AbortController().signal)).rejects.toThrow();
    }
    expect(create).not.toHaveBeenCalled();
  });
  it("accepts inclusive byte/pixel limits and releases its URL once", async () => {
    const loaded = loadLocalTableImage(file("image/png", 10 * 1024 * 1024), new AbortController().signal);
    await waitFor(() => expect(images).toHaveLength(1));
    images[0].onload?.();
    const image = await loaded;
    expect(image.image.naturalWidth * image.image.naturalHeight).toBe(16_000_000);
    image.dispose(); image.dispose();
    expect(revoke.mock.calls).toEqual([["blob:local/1"]]);
    expect(images[0].src).toBe("");
  });
  it.each(["pixels", "decode", "abort"] as const)("reverts to standard and releases failed %s loads", async (reason) => {
    const controller = new AbortController();
    const loaded = loadLocalTableImage(file(), controller.signal);
    const failure = expect(loaded).rejects.toThrow();
    await waitFor(() => expect(images).toHaveLength(1));
    if (reason === "pixels") { images[0].naturalWidth = 4001; images[0].onload?.(); }
    else if (reason === "decode") images[0].onerror?.();
    else controller.abort();
    await failure;
    expect(revoke).toHaveBeenCalledOnce();
    expect(images[0].src).toBe("");
    expect(images[0].onload).toBeNull();
  });
  it("ignores stale replacement callbacks, clears/reselects, and disposes pending loads on navigation", async () => {
    const view = renderHook(() => useTableAppearance());
    let first!: Promise<void>, second!: Promise<void>;
    act(() => { first = view.result.current.select("surface", file()); });
    await waitFor(() => expect(images).toHaveLength(1));
    const stale = images[0].onload;
    act(() => { second = view.result.current.select("surface", file()); });
    await waitFor(() => expect(images).toHaveLength(2));
    await act(async () => { images[1].onload?.(); await second; stale?.(); await first; });
    expect(view.result.current.appearance.surface?.url).toBe("blob:local/2");
    expect(revoke.mock.calls).toEqual([["blob:local/1"]]);
    act(() => view.result.current.clear("surface"));
    expect(view.result.current.appearance.surface).toBeNull();
    expect(revoke.mock.calls).toEqual([["blob:local/1"], ["blob:local/2"]]);
    let pending!: Promise<void>;
    act(() => { pending = view.result.current.select("surround", file()); });
    await waitFor(() => expect(images).toHaveLength(3));
    view.unmount();
    await pending;
    expect(revoke.mock.calls).toEqual([["blob:local/1"], ["blob:local/2"], ["blob:local/3"]]);
  });
});
