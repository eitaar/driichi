import { useEffect, useState } from "react";

export const GAMEPLAY_VIEWPORT_QUERY =
  "(min-width: 1024px) and (min-height: 600px)";

export function isGameplayViewportSupported(width: number, height: number): boolean {
  return Number.isFinite(width)
    && Number.isFinite(height)
    && width >= 1024
    && height >= 600;
}

function readViewportSupport(): boolean {
  if (typeof window === "undefined") return true;
  return isGameplayViewportSupported(window.innerWidth, window.innerHeight);
}

type MediaQueryListWithLegacyListeners = MediaQueryList & {
  addListener?: (listener: () => void) => void;
  removeListener?: (listener: () => void) => void;
};

export function useGameplayViewportSupport(): boolean {
  const [supported, setSupported] = useState(readViewportSupport);

  useEffect(() => {
    if (typeof window === "undefined") return undefined;
    const query = window.matchMedia(GAMEPLAY_VIEWPORT_QUERY) as MediaQueryListWithLegacyListeners;
    const update = () => setSupported(readViewportSupport());
    const onResize = () => update();

    query.addEventListener?.("change", update);
    query.addListener?.(update);
    window.addEventListener("resize", onResize, { passive: true });
    update();
    return () => {
      query.removeEventListener?.("change", update);
      query.removeListener?.(update);
      window.removeEventListener("resize", onResize);
    };
  }, []);

  return supported;
}
