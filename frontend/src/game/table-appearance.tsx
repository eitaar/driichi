import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Alert, AlertDescription } from "../components/ui/alert";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "../components/ui/field";

export interface LocalTableImage {
  image: ImageBitmap;
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
  // Blob decoding needs no blob: image request and preserves the existing self-only CSP.
  const image = await new Promise<ImageBitmap>((resolve, reject) => {
    const abort = () => reject(new DOMException("Image selection cancelled", "AbortError"));
    signal.addEventListener("abort", abort, { once: true });
    void createImageBitmap(file, { imageOrientation: "flipY", premultiplyAlpha: "none", colorSpaceConversion: "none" }).then(bitmap => {
      signal.removeEventListener("abort", abort);
      if (signal.aborted) { bitmap.close(); abort(); }
      else resolve(bitmap);
    }, () => {
      signal.removeEventListener("abort", abort);
      reject(new Error("The image file could not be decoded."));
    });
  });
  let disposed = false;
  const dispose = () => { if (!disposed) { disposed = true; image.close(); } };
  const pixels = image.width * image.height;
  if (signal.aborted) { dispose(); throw new DOMException("Image selection cancelled", "AbortError"); }
  if (!Number.isSafeInteger(pixels) || pixels <= 0 || pixels > 16_000_000) { dispose(); throw new Error("Choose an image up to 16 megapixels."); }
  return { image, dispose };
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
  const id = useId();
  return <FieldSet className="table-image-settings min-w-0">
    <FieldLegend>Table images — local only</FieldLegend>
    <FieldDescription id={`${id}-privacy`}>Images stay in this tab and are never uploaded. PNG, JPEG or WebP, up to 10 MB and 16 megapixels.</FieldDescription>
    <FieldGroup>
      {targets.map((target) => <Field key={target} data-invalid={Boolean(settings.errors[target])} aria-busy={settings.pending[target]}>
        <FieldLabel htmlFor={`${id}-${target}`}>Table {target} image</FieldLabel>
        <Input id={`${id}-${target}`} type="file" accept="image/png,image/jpeg,image/webp" aria-invalid={Boolean(settings.errors[target])} aria-describedby={`${id}-privacy${settings.errors[target] ? ` ${id}-${target}-error` : ''}`} onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          event.currentTarget.value = "";
          if (file) void settings.select(target, file);
        }} />
        <Button type="button" variant="secondary" className="min-h-11" disabled={!settings.appearance[target] && !settings.pending[target]} onClick={() => settings.clear(target)}>Clear {target} image</Button>
        {settings.errors[target] && <Alert variant="destructive" id={`${id}-${target}-error`}><AlertDescription>{settings.errors[target]}</AlertDescription></Alert>}
      </Field>)}
    </FieldGroup>
  </FieldSet>;
}
