export const ASSET_LOAD_TIMEOUT_MS = 3_000;

export type CharacterAssetKind = "portrait" | "icon";

/** Decode a character asset without allowing a broken or stalled browser load to block the UI. */
export function decodeCharacterAsset(id: string, kind: CharacterAssetKind): Promise<void> {
  return new Promise((resolve, reject) => {
    if (typeof Image === "undefined") {
      reject(new Error("image_unavailable"));
      return;
    }
    const image = new Image();
    let settled = false;
    const timer = globalThis.setTimeout(() => finish(new Error("asset_timeout")), ASSET_LOAD_TIMEOUT_MS);
    const cleanup = () => {
      globalThis.clearTimeout(timer);
      image.onload = null;
      image.onerror = null;
    };
    const finish = (error?: Error) => {
      if (settled) return;
      settled = true;
      cleanup();
      if (error) {
        try { image.src = ""; } catch { /* A timed-out image may already be detached. */ }
        reject(error);
      } else {
        resolve();
      }
    };
    image.onload = () => {
      try {
        const decoded = image.decode?.();
        if (decoded) void decoded.then(() => finish()).catch(() => finish(new Error("asset_unavailable")));
        else finish();
      } catch {
        finish(new Error("asset_unavailable"));
      }
    };
    image.onerror = () => finish(new Error("asset_unavailable"));
    try {
      image.src = `/assets/characters/${encodeURIComponent(id)}/${kind}.webp`;
    } catch {
      finish(new Error("asset_unavailable"));
    }
  });
}
