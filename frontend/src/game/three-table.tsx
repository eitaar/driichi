import {
  Component,
  type ErrorInfo,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Canvas } from "@react-three/fiber";
import { SRGBColorSpace } from "three";

import type { AnimationItem } from "./animation";
import { MatchTableScene, type SceneRenderStats } from "./three-table-scene";
import { nextSceneMotion, type SceneMotion } from "./three-table-motion";
import { createTileAtlas, type TileAtlas } from "./tile-atlas";
import { buildMatchSceneLayout, CAMERA, type TableSurface } from "./three-table-layout";
import { tileLabel } from "./tiles";
import type { ProjectedPlayer, ProjectedState, RoomSnapshot } from "./types";

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
  onAnimationCancelled?: (id: number) => void;
  surface?: TableSurface;
}

export interface TablePlayerOverlayProps {
  projection: ProjectedState;
  room: RoomSnapshot | null;
  surface: TableSurface;
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

function numericFact(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.floor(value))
    : 0;
}

function doraFact(projection: ProjectedState): string {
  const indicators = Array.isArray(projection.dora_indicators)
    ? projection.dora_indicators.filter((tile): tile is number => typeof tile === "number")
    : [];
  return indicators.length > 0
    ? `Dora ${indicators.map(tileLabel).join(", ")}`
    : "Dora unavailable";
}

function TableCenterFacts({
  projection,
  wallCount,
}: {
  projection: ProjectedState;
  wallCount: number;
}) {
  return (
    <dl className="table-center-facts" aria-label="Match facts">
      <div><dt>Kyoku</dt><dd>{roundFact(projection)}</dd></div>
      <div><dt>Honba</dt><dd>{numericFact(projection.honba)}</dd></div>
      <div><dt>Kyotaku</dt><dd>{numericFact(projection.kyotaku)}</dd></div>
      <div><dt>Wall</dt><dd>{wallCount}</dd></div>
      <div><dt>Dora</dt><dd>{doraFact(projection).replace(/^Dora\s*/, "")}</dd></div>
    </dl>
  );
}

function TableFallback({
  projection,
  wallCount,
}: {
  projection: ProjectedState;
  wallCount: number;
}) {
  return (
    <div className="three-table__fallback" role="status" aria-label="3D table unavailable">
      <strong>3D table unavailable.</strong>{" "}
      <span>{roundFact(projection)}.</span>{" "}
      <span>{wallCount} tiles left.</span>{" "}
      <span>{doraFact(projection)}.</span>
    </div>
  );
}

function canCreateWebGL(): boolean {
  if (typeof document === "undefined") return true;
  if (typeof WebGLRenderingContext === "undefined") return false;
  try {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
    context?.getExtension("WEBGL_lose_context")?.loseContext();
    return context !== null;
  } catch {
    return false;
  }
}

function characterIdFor(room: RoomSnapshot | null, participantId: string): string | null {
  const roomPlayers = room?.roster?.length ? room.roster : room?.match_players ?? [];
  return roomPlayers.find((player) => player.participant_id === participantId)?.character_id ?? null;
}

function playerInitial(player: ProjectedPlayer): string {
  return Array.from(player.display_name.trim())[0]?.toUpperCase()
    ?? Array.from((player.kind ?? "player").trim())[0]?.toUpperCase()
    ?? "?";
}

function PlayerPortrait({
  player,
  characterId,
}: {
  player: ProjectedPlayer;
  characterId: string | null;
}) {
  const [failed, setFailed] = useState(false);
  if (!characterId || failed) {
    return (
      <span className="table-player-portrait asset-fallback" aria-label={`${player.display_name} portrait unavailable`}>
        {playerInitial(player)}
      </span>
    );
  }
  return (
    <img
      className="table-player-portrait"
      src={`/assets/characters/${encodeURIComponent(characterId)}/portrait.webp`}
      alt={`${player.display_name} portrait`}
      onError={() => setFailed(true)}
    />
  );
}

export function TablePlayerOverlay({
  projection,
  room,
  surface,
}: TablePlayerOverlayProps) {
  const layout = useMemo(() => buildMatchSceneLayout(projection, room, surface), [projection, room, surface]);
  return (
    <div
      className="table-player-overlays"
      data-surface={surface}
      aria-hidden={surface === "live" ? "true" : undefined}
    >
      {layout.players.map((scenePlayer) => {
        const player = projection.players?.find((candidate) => candidate.seat === scenePlayer.seat);
        if (!player) return null;
        const characterId = characterIdFor(room, player.participant_id);
        return (
          <section
            className={`table-player-frame${scenePlayer.isLocal ? " is-local" : ""}`}
            data-position={scenePlayer.position}
            key={`${scenePlayer.position}:${player.participant_id}`}
            aria-label={`${player.display_name}, ${scenePlayer.position} player`}
          >
            <PlayerPortrait
              key={`${player.participant_id}:${characterId ?? "fallback"}`}
              player={player}
              characterId={characterId}
            />
            <span className="table-player-copy">
              <strong>{player.display_name}</strong>
              <span className="table-player-score">
                {typeof player.score === "number" ? player.score.toLocaleString() : "—"}
              </span>
              <span className="table-player-position">{scenePlayer.position.toUpperCase()}</span>
              {player.riichi && <span className="table-player-riichi">Riichi</span>}
            </span>
          </section>
        );
      })}
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
  onAnimationCancelled: _onAnimationCancelled,
  surface = "live",
}: ThreeTableProps) {
  const [atlas, setAtlas] = useState<TileAtlas | null>(null);
  const [atlasFailed, setAtlasFailed] = useState(false);
  const [webglFallback, setWebglFallback] = useState(() => !canCreateWebGL());
  const [rendererCreated, setRendererCreated] = useState(false);
  const [renderStats, setRenderStats] = useState<
    (SceneRenderStats & { layout: object; atlas: TileAtlas }) | null
  >(null);
  const [motion, setMotion] = useState<SceneMotion | null>(null);
  const [lastConsumedAnimationId, setLastConsumedAnimationId] = useState<number | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const contextCleanupRef = useRef<(() => void) | null>(null);
  const motionFrameTimesRef = useRef<number[]>([]);
  const motionPixelRatioRef = useRef<number | null>(null);
  const activeMotionRef = useRef<{
    motion: SceneMotion;
    layout: typeof layout;
  } | null>(null);
  // Animation ids are assigned monotonically by the authoritative queue. Keep
  // cancelled/consumed ids blocked through the rerender that removes them so a
  // stale scene callback cannot complete an active replacement.
  const blockedAnimationIdsRef = useRef(new Set<number>());
  const onAnimationConsumedRef = useRef(_onAnimationConsumed);
  const onAnimationCancelledRef = useRef(_onAnimationCancelled);
  onAnimationConsumedRef.current = _onAnimationConsumed;
  onAnimationCancelledRef.current = _onAnimationCancelled;
  const hasProjection = projection !== null;
  const layout = useMemo(
    () => (projection ? buildMatchSceneLayout(projection, room, surface) : null),
    [projection, surface],
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
      setAtlas(null);
    };
  }, [hasProjection]);

  const markFallback = useCallback(() => {
    contextCleanupRef.current?.();
    contextCleanupRef.current = null;
    setRendererCreated(false);
    setRenderStats(null);
    setWebglFallback(true);
  }, []);
  useEffect(() => () => contextCleanupRef.current?.(), []);
  useEffect(() => {
    if (!layout || !atlas || rendererCreated || webglFallback) return;
    const timeout = window.setTimeout(markFallback, 3_000);
    return () => window.clearTimeout(timeout);
  }, [atlas, layout, markFallback, rendererCreated, webglFallback]);
  const isFallback = atlasFailed || webglFallback;
  const ready = Boolean(
    layout
    && atlas
    && rendererCreated
    && renderStats?.layout === layout
    && renderStats.atlas === atlas
    && !isFallback,
  );
  const renderedTileCount = ready ? renderStats?.tileCount ?? 0 : 0;
  const primitiveCount = ready ? renderStats?.primitiveCount ?? 0 : 0;
  const tableHeightRatio = ready && typeof renderStats?.tableHeightRatio === "number"
    ? renderStats.tableHeightRatio
    : 0;
  const tableWidthRatio = ready && typeof renderStats?.tableWidthRatio === "number"
    ? renderStats.tableWidthRatio
    : 0;
  const rendererPixelRatio = ready && typeof renderStats?.pixelRatio === "number"
    ? renderStats.pixelRatio
    : 0;
  const renderedTriangleCount = ready && typeof renderStats?.triangleCount === "number"
    ? renderStats.triangleCount
    : 0;
  const recordRenderReady = useCallback((stats: SceneRenderStats) => {
    if (!layout || !atlas) return;
    setRenderStats({ ...stats, layout, atlas });
  }, [atlas, layout]);
  const reportConsumed = useCallback((id: number) => {
    setLastConsumedAnimationId(id);
    onAnimationConsumedRef.current?.(id);
  }, []);
  const reportCancelled = useCallback((id: number) => {
    onAnimationCancelledRef.current?.(id);
  }, []);

  useEffect(() => {
    const active = activeMotionRef.current;
    if (active && active.layout !== layout) {
      blockedAnimationIdsRef.current.add(active.motion.itemId);
      activeMotionRef.current = null;
      setMotion(null);
      reportCancelled(active.motion.itemId);
    }
    const presentIds = new Set(animations.map((item) => item.id));
    for (const id of blockedAnimationIdsRef.current) {
      if (!presentIds.has(id)) blockedAnimationIdsRef.current.delete(id);
    }
  }, [animations, layout, reportCancelled]);

  useEffect(() => {
    if (reducedMotion) {
      const active = activeMotionRef.current;
      if (active) {
        blockedAnimationIdsRef.current.add(active.motion.itemId);
        reportConsumed(active.motion.itemId);
      }
      activeMotionRef.current = null;
      setMotion(null);
      for (const item of animations) {
        if (blockedAnimationIdsRef.current.has(item.id)) continue;
        blockedAnimationIdsRef.current.add(item.id);
        reportConsumed(item.id);
      }
      return;
    }
    if (!ready || activeMotionRef.current) return;
    const item = animations.find((candidate) => !blockedAnimationIdsRef.current.has(candidate.id));
    if (!item) return;
    const candidate = nextSceneMotion([item], false);
    if (!candidate) {
      blockedAnimationIdsRef.current.add(item.id);
      reportConsumed(item.id);
      return;
    }
    const nextMotion = { ...candidate, startedAt: performance.now() };
    motionFrameTimesRef.current = [];
    motionPixelRatioRef.current = null;
    if (hostRef.current) hostRef.current.dataset.animationFrameCount = "0";
    activeMotionRef.current = { motion: nextMotion, layout };
    setMotion(nextMotion);
  }, [animations, layout, ready, reducedMotion, reportConsumed, motion]);

  useEffect(() => () => {
    const active = activeMotionRef.current;
    activeMotionRef.current = null;
    if (active) reportCancelled(active.motion.itemId);
  }, [reportCancelled]);

  const completeMotion = useCallback((id: number) => {
    const active = activeMotionRef.current;
    if (!active || active.motion.itemId !== id) return;
    blockedAnimationIdsRef.current.add(active.motion.itemId);
    const frameTimes = motionFrameTimesRef.current;
    if (typeof performance.measure === "function") {
      performance.measure("three-table-motion-event", {
        start: active.motion.startedAt,
        end: performance.now(),
        detail: { itemId: id, kind: active.motion.kind },
      });
      for (let index = 1; index < frameTimes.length; index += 1) {
        performance.measure("three-table-motion-frame", {
          start: frameTimes[index - 1],
          end: frameTimes[index],
          detail: { itemId: id, frameIndex: index - 1 },
        });
      }
    }
    activeMotionRef.current = null;
    const host = hostRef.current;
    if (host) {
      host.dataset.animationFrameCount = String(frameTimes.length);
      const pixelRatio = renderStats?.pixelRatio ?? motionPixelRatioRef.current;
      if (typeof pixelRatio === "number") host.dataset.rendererPixelRatio = String(pixelRatio);
    }
    motionFrameTimesRef.current = [];
    motionPixelRatioRef.current = null;
    setMotion(null);
    reportConsumed(id);
  }, [renderStats, reportConsumed]);

  const recordMotionFrame = useCallback((now: number, pixelRatio: number) => {
    // The effective DPR is static for a renderer, so publish it only when it
    // changes while retaining the numeric sample path for frame timings.
    if (motionPixelRatioRef.current !== pixelRatio) {
      motionPixelRatioRef.current = pixelRatio;
      const host = hostRef.current;
      if (host) host.dataset.rendererPixelRatio = String(pixelRatio);
    }
    motionFrameTimesRef.current.push(now);
  }, []);

  const animationState = reducedMotion ? "static" : motion ? "active" : "idle";

  return (
    <div
      ref={hostRef}
      className="three-table"
      data-testid="three-table"
      data-surface={surface}
      data-render-ready={String(ready)}
      data-rendered-tile-count={renderedTileCount}
      data-rendered-scene-primitives={primitiveCount}
      data-table-height-ratio={tableHeightRatio}
      data-table-width-ratio={tableWidthRatio}
      data-renderer-pixel-ratio={rendererPixelRatio || ""}
      data-rendered-scene-triangles={renderedTriangleCount}
      data-wall-tile-count={layout?.wallCount ?? 0}
      data-webgl-fallback={String(isFallback)}
      data-animation-state={animationState}
      data-animation-item-id={motion?.itemId ?? ""}
      data-last-consumed-animation-id={lastConsumedAnimationId ?? ""}
      data-animation-frame-count="0"
      data-player-frame-count={layout?.players.length ?? 0}
      role="group"
      aria-label="3D mahjong table"
    >
      {projection && (
        <TablePlayerOverlay projection={projection} room={room} surface={surface} />
      )}
      {projection && layout && !isFallback && (
        <TableCenterFacts projection={projection} wallCount={layout.wallCount} />
      )}
      {!projection ? null : isFallback ? (
        <TableFallback
          projection={projection}
          wallCount={layout?.wallCount ?? 0}
        />
      ) : atlas && layout ? (
        <TableErrorBoundary
          onError={markFallback}
          fallback={
            <TableFallback
              projection={projection}
              wallCount={layout.wallCount}
            />
          }
        >
          <Canvas
            aria-hidden="true"
            // Demand when idle; a bounded always loop only while an exact-target
            // motion is active, then it returns to demand in the completion commit.
            frameloop={motion ? "always" : "demand"}
            dpr={[1, 2]}
            camera={{
              fov: CAMERA.fov,
              position: [...CAMERA.position],
              near: CAMERA.near,
              far: CAMERA.far,
            }}
            gl={{
              antialias: true,
              alpha: false,
              depth: true,
              stencil: false,
              precision: "highp",
              powerPreference: "high-performance",
            }}
            fallback={
              <TableFallback
                projection={projection}
                wallCount={layout.wallCount}
              />
            }
            onCreated={({ gl }) => {
              gl.outputColorSpace = SRGBColorSpace;
              gl.shadowMap.enabled = false;
              contextCleanupRef.current?.();
              const onContextLost = (event: Event) => {
                event.preventDefault();
                markFallback();
              };
              gl.domElement.addEventListener("webglcontextlost", onContextLost);
              contextCleanupRef.current = () =>
                gl.domElement.removeEventListener("webglcontextlost", onContextLost);
              setRendererCreated(true);
            }}
          >
            <MatchTableScene
              layout={layout}
              atlas={atlas}
              motion={motion}
              onMotionComplete={completeMotion}
              onMotionFrame={recordMotionFrame}
              onRenderReady={recordRenderReady}
            />
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
