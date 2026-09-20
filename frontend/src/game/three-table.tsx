import {
  Component,
  type ErrorInfo,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Canvas } from "@react-three/fiber";
import { PCFSoftShadowMap, SRGBColorSpace } from "three";

import type { AnimationItem } from "./animation";
import { MatchTableScene } from "./three-table-scene";
import { createTileAtlas, type TileAtlas } from "./tile-atlas";
import { buildMatchSceneLayout, CAMERA } from "./three-table-layout";
import { tileLabel } from "./tiles";
import type { ProjectedState, RoomSnapshot } from "./types";

export interface PortraitEffect {
  characterId: string;
  displayName: string;
  result: "Ron" | "Tsumo";
  han?: number;
  fu?: number;
  limit?: string;
  points?: number;
}

export interface ThreeTableProps {
  projection: ProjectedState | null;
  room: RoomSnapshot | null;
  animations?: AnimationItem[];
  reducedMotion?: boolean;
  portraitEffect?: PortraitEffect | null;
  onAnimationConsumed?: (id: number) => void;
  surface?: "live" | "replay";
}

interface BoundaryProps {
  children: ReactNode;
  fallback: ReactNode;
  onError(): void;
}

class TableErrorBoundary extends Component<BoundaryProps, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo): void {
    this.props.onError();
  }

  render(): ReactNode {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

function roundFact(projection: ProjectedState): string {
  if (typeof projection.kyoku === "string") return projection.kyoku;
  if (typeof projection.round === "string" && typeof projection.kyoku === "number") {
    return `${projection.round} ${Math.max(1, Math.floor(projection.kyoku))}`;
  }
  if (typeof projection.kyoku === "number") {
    return `Kyoku ${Math.max(1, Math.floor(projection.kyoku))}`;
  }
  return "Round unavailable";
}

function doraFact(projection: ProjectedState): string {
  const indicators = Array.isArray(projection.dora_indicators)
    ? projection.dora_indicators.filter((tile): tile is number => typeof tile === "number")
    : [];
  return indicators.length > 0
    ? `Dora ${indicators.map(tileLabel).join(", ")}`
    : "Dora unavailable";
}

function TableFallback({
  projection,
  wallCount,
  onFallback,
}: {
  projection: ProjectedState;
  wallCount: number;
  onFallback(): void;
}) {
  useEffect(onFallback, [onFallback]);
  return (
    <div className="three-table__fallback" role="status" aria-label="3D table unavailable">
      <strong>3D table unavailable.</strong>{" "}
      <span>{roundFact(projection)}.</span>{" "}
      <span>{wallCount} tiles left.</span>{" "}
      <span>{doraFact(projection)}.</span>
    </div>
  );
}

export function ThreeTable({
  projection,
  room,
  animations = [],
  reducedMotion = false,
  portraitEffect: _portraitEffect = null,
  onAnimationConsumed: _onAnimationConsumed,
  surface = "live",
}: ThreeTableProps) {
  const [atlas, setAtlas] = useState<TileAtlas | null>(null);
  const [atlasFailed, setAtlasFailed] = useState(false);
  const [webglFallback, setWebglFallback] = useState(false);
  const hasProjection = projection !== null;
  const layout = useMemo(
    () => (projection ? buildMatchSceneLayout(projection, room) : null),
    [projection, room],
  );

  useEffect(() => {
    if (!hasProjection) return;
    const controller = new AbortController();
    let owned: TileAtlas | null = null;
    let active = true;
    setAtlasFailed(false);

    void createTileAtlas(controller.signal)
      .then((nextAtlas) => {
        if (!active) {
          nextAtlas.dispose();
          return;
        }
        owned = nextAtlas;
        setAtlas(nextAtlas);
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted && active) setAtlasFailed(true);
        if (error instanceof DOMException && error.name === "AbortError") return;
      });

    return () => {
      active = false;
      controller.abort();
      owned?.dispose();
    };
  }, [hasProjection]);

  const markFallback = useCallback(() => setWebglFallback(true), []);
  const isFallback = atlasFailed || webglFallback;
  const ready = Boolean(layout && atlas && !isFallback);
  const primitiveCount = layout
    ? 10 + new Set(layout.tiles.map((tile) => `${tile.face}:${tile.group}`)).size
    : 0;
  const animationState = reducedMotion
    ? "reduced"
    : animations.length > 0
      ? "queued"
      : "idle";

  return (
    <div
      className="three-table"
      data-testid="three-table"
      data-surface={surface}
      data-render-ready={String(ready)}
      data-rendered-tile-count={layout?.tiles.length ?? 0}
      data-rendered-scene-primitives={primitiveCount}
      data-wall-tile-count={layout?.wallCount ?? 0}
      data-webgl-fallback={String(isFallback)}
      data-animation-state={animationState}
      data-player-frame-count={layout?.players.length ?? 0}
      role="group"
      aria-label="3D mahjong table"
    >
      {!projection ? (
        <div role="status" aria-label="Table synchronization">
          Synchronizing table
        </div>
      ) : atlasFailed ? (
        <TableFallback
          projection={projection}
          wallCount={layout?.wallCount ?? 0}
          onFallback={markFallback}
        />
      ) : atlas && layout ? (
        <TableErrorBoundary
          onError={markFallback}
          fallback={
            <TableFallback
              projection={projection}
              wallCount={layout.wallCount}
              onFallback={markFallback}
            />
          }
        >
          <Canvas
            aria-hidden="true"
            frameloop="demand"
            dpr={[1, 1.5]}
            camera={{
              fov: CAMERA.fov,
              position: [...CAMERA.position],
              near: CAMERA.near,
              far: CAMERA.far,
            }}
            gl={{ antialias: true, alpha: true, powerPreference: "high-performance" }}
            shadows={{ type: PCFSoftShadowMap }}
            fallback={
              <TableFallback
                projection={projection}
                wallCount={layout.wallCount}
                onFallback={markFallback}
              />
            }
            onCreated={({ gl }) => {
              gl.outputColorSpace = SRGBColorSpace;
              gl.shadowMap.enabled = true;
              gl.shadowMap.type = PCFSoftShadowMap;
            }}
          >
            <MatchTableScene layout={layout} atlas={atlas} />
          </Canvas>
        </TableErrorBoundary>
      ) : (
        <div role="status" aria-label="3D table loading">
          Preparing 3D table
        </div>
      )}
    </div>
  );
}
