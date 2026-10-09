import { useCallback, useEffect, useRef, useState } from "react";
import { LinearFilter, SRGBColorSpace, Texture } from "three";

export interface LocalTableImage {
  url: string;
  image: HTMLImageElement;
  dispose(): void;
}
export type TableImageTarget = "surface" | "surround";
export type TableAppearance = Record<TableImageTarget, LocalTableImage | null>;
const targets = ["surface", "surround"] as const;

export async function loadLocalTableImage(file: File, signal: AbortSignal): Promise<LocalTableImage> {
  if (!["image/png", "image/jpeg", "image/webp"].includes(file.type) || file.size > 10 * 1024 * 1024) {
    throw new Error("Choose a PNG, JPEG or WebP image up to 10 MB.");
  }
  const bytes = new Uint8Array(await file.slice(0, 12).arrayBuffer());
  const valid = file.type === "image/png" ? [137, 80, 78, 71, 13, 10, 26, 10].every((byte, index) => bytes[index] === byte)
    : file.type === "image/jpeg" ? bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255
    : String.fromCharCode(...bytes.slice(0, 4)) === "RIFF" && String.fromCharCode(...bytes.slice(8, 12)) === "WEBP";
  if (!valid) throw new Error("The image file could not be decoded.");
  if (signal.aborted) throw new DOMException("Image selection cancelled", "AbortError");
  const url = URL.createObjectURL(file);
  const image = new Image();
  let disposed = false;
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    image.src = "";
    URL.revokeObjectURL(url);
  };
  try {
    await new Promise<void>((resolve, reject) => {
      const cleanup = () => {
        image.onload = image.onerror = null;
        signal.removeEventListener("abort", abort);
      };
      const abort = () => { cleanup(); reject(new DOMException("Image selection cancelled", "AbortError")); };
      image.onload = () => { cleanup(); resolve(); };
      image.onerror = () => { cleanup(); reject(new Error("The image file could not be decoded.")); };
      signal.addEventListener("abort", abort, { once: true });
      image.src = url;
    });
    const pixels = image.naturalWidth * image.naturalHeight;
    if (signal.aborted) throw new DOMException("Image selection cancelled", "AbortError");
    if (!Number.isSafeInteger(pixels) || pixels <= 0 || pixels > 16_000_000) throw new Error("Choose an image up to 16 megapixels.");
    return { url, image, dispose };
  } catch (error) {
    dispose();
    throw error;
  }
}

export function createTableImageTexture(local: LocalTableImage | null | undefined, aspect: number): Texture | null {
  if (!local) return null;
  const texture = new Texture(local.image);
  const imageAspect = local.image.naturalWidth / local.image.naturalHeight;
  const x = imageAspect > aspect ? aspect / imageAspect : 1;
  const y = imageAspect > aspect ? 1 : imageAspect / aspect;
  texture.repeat.set(x, y);
  texture.offset.set((1 - x) / 2, (1 - y) / 2);
  texture.colorSpace = SRGBColorSpace;
  texture.generateMipmaps = false;
  texture.minFilter = texture.magFilter = LinearFilter;
  texture.needsUpdate = true;
  return texture;
}

export function useTableAppearance() {
  const [appearance, setAppearance] = useState<TableAppearance>({ surface: null, surround: null });
  const [errors, setErrors] = useState({ surface: "", surround: "" });
  const [pending, setPending] = useState({ surface: false, surround: false });
  const owned = useRef<TableAppearance>({ surface: null, surround: null });
  const loading = useRef<Record<TableImageTarget, AbortController | null>>({ surface: null, surround: null });
  const clear = useCallback((target: TableImageTarget) => {
    loading.current[target]?.abort();
    loading.current[target] = null;
    owned.current[target]?.dispose();
    owned.current[target] = null;
    setAppearance((current) => ({ ...current, [target]: null }));
    setErrors((current) => ({ ...current, [target]: "" }));
    setPending((current) => ({ ...current, [target]: false }));
  }, []);
  const select = useCallback(async (target: TableImageTarget, file: File) => {
    clear(target);
    const request = new AbortController();
    loading.current[target] = request;
    setPending((current) => ({ ...current, [target]: true }));
    try {
      const next = await loadLocalTableImage(file, request.signal);
      if (loading.current[target] !== request || request.signal.aborted) { next.dispose(); return; }
      owned.current[target] = next;
      setAppearance((current) => ({ ...current, [target]: next }));
    } catch (error) {
      if (!request.signal.aborted && loading.current[target] === request) {
        setErrors((current) => ({ ...current, [target]: error instanceof Error ? error.message : "The image file could not be decoded." }));
      }
    } finally {
      if (loading.current[target] === request) {
        loading.current[target] = null;
        setPending((current) => ({ ...current, [target]: false }));
      }
    }
  }, [clear]);
  useEffect(() => () => {
    for (const target of targets) {
      loading.current[target]?.abort();
      loading.current[target] = null;
      owned.current[target]?.dispose();
      owned.current[target] = null;
    }
  }, []);
  return { appearance, errors, pending, select, clear };
}

export function TableAppearanceSettings({ settings }: { settings: ReturnType<typeof useTableAppearance> }) {
  return <fieldset className="table-image-settings">
    <legend>Table images — local only</legend>
    {targets.map((target) => <div key={target}>
      <label>Table {target} image
        <input type="file" accept="image/png,image/jpeg,image/webp" onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          event.currentTarget.value = "";
          if (file) void settings.select(target, file);
        }} />
      </label>
      <button type="button" disabled={!settings.appearance[target] && !settings.pending[target]} onClick={() => settings.clear(target)}>Clear {target} image</button>
      {settings.errors[target] && <p role="alert">{settings.errors[target]}</p>}
    </div>)}
  </fieldset>;
}
