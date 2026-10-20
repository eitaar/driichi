import { Suspense, type ReactNode } from "react";
import { useGameplayViewportSupport } from "../game/viewport";
import { navigate } from "../routes";

export function DesktopTableBoundary({ children, fallback }: { children: ReactNode; fallback: ReactNode }) {
  const supported = useGameplayViewportSupport();
  return supported ? <Suspense fallback={<p role="status">Loading table…</p>}>{children}</Suspense> : fallback;
}

export function DesktopTableFallback({ backHref }: { backHref: string }) {
  return <section className="flex flex-col gap-3 p-4" aria-label="Desktop table limitation">
    <h2>Desktop required for the 3D table</h2>
    <p>Use a viewport of at least 1024 × 600. Room controls and results remain available.</p>
    <a href={backHref} onClick={event => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault(); navigate(backHref);
    }}>Back</a>
  </section>;
}
