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
import { PCFSoftShadowMap, SRGBColorSpace } from "three";

import type { AnimationItem } from "./animation";
import { MatchTableScene } from "./three-table-scene";
import { nextSceneMotion, type SceneMotion } from "./three-table-motion";
import { createTileAtlas, type TileAtlas } from "./tile-atlas";
import { buildMatchSceneLayout, CAMERA } from "./three-table-layout";
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
  surface?: "live" | "replay";
}

export interface TablePlayerOverlayProps {
  projection: ProjectedState;
  room: RoomSnapshot | null;
  surface: "live" | "replay";
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
  const layout = useMemo(() => buildMatchSceneLayout(projection, room), [projection, room]);
  return (
    <div className="table-player-overlays" data-surface={surface}>
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
  const [motion, setMotion] = useState<SceneMotion | null>(null);
  const [lastConsumedAnimationId, setLastConsumedAnimationId] = useState<number | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const activeMotionRef = useRef<{ motion: SceneMotion; layout: typeof layout } | null>(null);
  const blockedAnimationIdsRef = useRef(new Set<number>());
  const onAnimationConsumedRef = useRef(_onAnimationConsumed);
  const onAnimationCancelledRef = useRef(_onAnimationCancelled);
  onAnimationConsumedRef.current = _onAnimationConsumed;
  onAnimationCancelledRef.current = _onAnimationCancelled;
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
    const item = animations.find(({ id }) => !blockedAnimationIdsRef.current.has(id));
    if (!item) return;
    const candidate = nextSceneMotion([item], false);
    if (!candidate) {
      blockedAnimationIdsRef.current.add(item.id);
      reportConsumed(item.id);
      return;
    }
    const nextMotion = { ...candidate, startedAt: performance.now() };
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
    blockedAnimationIdsRef.current.add(id);
    activeMotionRef.current = null;
    setMotion(null);
    reportConsumed(id);
  }, [reportConsumed]);

  const recordMotionFrame = useCallback(() => {
    const host = hostRef.current;
    if (!host) return;
    const count = Number(host.dataset.animationFrameCount ?? 0);
    host.dataset.animationFrameCount = String(Number.isFinite(count) ? count + 1 : 1);
  }, []);

  const animationState = reducedMotion ? "static" : motion ? "active" : "idle";

  return (
    <div
      ref={hostRef}
      className="three-table"
      data-testid="three-table"
      data-surface={surface}
      data-render-ready={String(ready)}
      data-rendered-tile-count={layout?.tiles.length ?? 0}
      data-rendered-scene-primitives={primitiveCount}
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
      {!projection ? (
        <div role="status" aria-label="Table synchronization">
          Synchronizing table
        </div>
      ) : isFallback ? (
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
              />
            }
            onCreated={({ gl }) => {
              gl.outputColorSpace = SRGBColorSpace;
              gl.shadowMap.enabled = true;
              gl.shadowMap.type = PCFSoftShadowMap;
            }}
          >
            <MatchTableScene
              layout={layout}
              atlas={atlas}
              motion={motion}
              onMotionComplete={completeMotion}
              onMotionFrame={recordMotionFrame}
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
