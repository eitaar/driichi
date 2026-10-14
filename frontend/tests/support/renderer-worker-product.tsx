import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import fixture from "../fixtures/task12-projection.json";
import { ThreeTable } from "../../src/game/three-table";
import { TableAppearanceSettings, useTableAppearance } from "../../src/game/table-appearance";
import type { ProjectedState, RoomSnapshot } from "../../src/game/types";
import "../../src/styles.css";
import "./renderer-worker.css";

function Product() {
  const [mode, setMode] = useState<"4p-red-east" | "3p-red-east">("4p-red-east");
  const [surface, setSurface] = useState<"live" | "replay">("live");
  const [mounted, setMounted] = useState(true);
  const settings = useTableAppearance();
  return <div className="gameplay-shell">
    <div className="product-table">{mounted && <ThreeTable projection={fixture.projections[mode] as ProjectedState} room={fixture.room_envelopes[mode] as RoomSnapshot} surface={surface} reducedMotion appearance={settings.appearance} />}</div>
    <div className="product-controls">
      <button onClick={() => setMode(mode === "4p-red-east" ? "3p-red-east" : "4p-red-east")}>Change mode</button>
      <button onClick={() => setSurface(surface === "live" ? "replay" : "live")}>Change surface</button>
      <button onClick={() => setMounted(value => !value)}>Toggle table</button>
      <TableAppearanceSettings settings={settings} />
    </div>
  </div>;
}
createRoot(document.querySelector("#root")!).render(<StrictMode><Product /></StrictMode>);
