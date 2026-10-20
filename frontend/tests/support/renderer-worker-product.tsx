/// <reference types="vite/client" />

import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import fixture from "../fixtures/task12-projection.json";
import { ThemeProvider, useTheme } from "../../src/components/theme-provider";
import { ThreeTable } from "../../src/game/three-table";
import { paintTableCenter } from "../../src/game/table-center-pixels";
import { TableAppearanceSettings, useTableAppearance } from "../../src/game/table-appearance";
import type { ProjectedState, RoomSnapshot } from "../../src/game/types";
import "../../src/styles.css";
import "./renderer-worker.css";

function Product() {
  const { setPreference } = useTheme();
  const [mode, setMode] = useState<"4p-red-east" | "3p-red-east">("4p-red-east");
  const [surface, setSurface] = useState<"live" | "replay">("live");
  const [mounted, setMounted] = useState(true);
  const settings = useTableAppearance();
  const [scoreInput, setScoreInput] = useState<{ projection: ProjectedState; viewSeat: number; surface: "live" | "replay" } | null>(null);
  const raster = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const update = (event: Event) => setScoreInput((event as CustomEvent).detail);
    window.addEventListener("seat-score-input", update);
    return () => window.removeEventListener("seat-score-input", update);
  }, []);
  useEffect(() => {
    if (!scoreInput || !raster.current) return;
    const canvas = paintTableCenter({ ...scoreInput.projection, viewer_seat: scoreInput.viewSeat }, Number(scoreInput.projection.remaining_wall));
    if (canvas) { canvas.style.width = "256px"; canvas.style.height = "256px"; raster.current.replaceChildren(canvas); }
  }, [scoreInput]);
  return <div className="gameplay-shell">
    <div className="product-table">{mounted && <ThreeTable projection={scoreInput?.projection ?? fixture.projections[mode] as ProjectedState} viewSeat={scoreInput?.viewSeat} room={fixture.room_envelopes[mode] as RoomSnapshot} surface={scoreInput?.surface ?? surface} reducedMotion appearance={settings.appearance} />}</div>
    <div className="product-controls">
      <button onClick={() => setPreference("light")}>Light theme</button>
      <button onClick={() => setPreference("dark")}>Dark theme</button>
      <button onClick={() => setPreference("system")}>System theme</button>
      <button onClick={() => setMode(mode === "4p-red-east" ? "3p-red-east" : "4p-red-east")}>Change mode</button>
      <button onClick={() => setSurface(surface === "live" ? "replay" : "live")}>Change surface</button>
      <button onClick={() => setMounted(value => !value)}>Toggle table</button>
      <TableAppearanceSettings settings={settings} />
      <div ref={raster} data-testid="center-raster" />
    </div>
  </div>;
}
createRoot(document.querySelector("#root")!).render(<StrictMode><ThemeProvider><Product /></ThemeProvider></StrictMode>);
