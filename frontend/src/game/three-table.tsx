import {
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import "./live-table.css";
import { ThreeTableRenderer } from "./three-table-renderer";

import type { AnimationItem } from "./animation";
import type { SceneRenderStats } from "./three-table-scene";
import { nextSceneMotion, type SceneMotion } from "./three-table-motion";
import { createTileAtlas, type TileAtlas } from "./tile-atlas";
import { buildMatchSceneLayout, type TableSurface } from "./three-table-layout";
import { tileAssetUrl, tileLabel } from "./tiles";
import { isDora } from "./dora";
import { startDoraSheen, type DoraSheenClock } from "./dora-sheen";
import type { TableAppearance } from "./table-appearance";
import type { ProjectedPlayer, ProjectedState, RoomSnapshot } from "./types";

export interface ThreeTableProps {
  projection: ProjectedState | null;
  /** Presentation only: never changes the authorized hands or Participant/Seat mapping. */
  viewSeat?: number;
  room: RoomSnapshot | null;
  animations?: AnimationItem[];
  reducedMotion?: boolean;
  onAnimationConsumed?: (id: number) => void;
  onAnimationCancelled?: (id: number) => void;
  surface?: TableSurface;
  sheenHost?: RefObject<HTMLElement | null>;
  appearance?: TableAppearance;
}

export interface TablePlayerOverlayProps {
  projection: ProjectedState;
  room: RoomSnapshot | null;
  surface: TableSurface;
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

function characterIdFor(room: RoomSnapshot | null, participantId: string): string | null {
  const roomPlayers = room?.roster?.length ? room.roster : room?.match_players ?? [];
  return roomPlayers.find((player) => player.participant_id === participantId)?.character_id ?? null;
}

function playerInitial(player: ProjectedPlayer): string {
  return Array.from(player.display_name.trim())[0]?.toUpperCase()
    ?? Array.from((player.kind ?? "player").trim())[0]?.toUpperCase()
    ?? "?";
}

function PlayerIcon({
  player,
  characterId,
}: {
  player: ProjectedPlayer;
  characterId: string | null;
}) {
  const [failed, setFailed] = useState(false);
  if (!characterId || failed) {
    return (
      <span className="table-player-icon asset-fallback" aria-label={`${player.display_name} icon unavailable`}>
        {playerInitial(player)}
      </span>
    );
  }
  return (
    <img
      className="table-player-icon"
      src={`/assets/characters/${encodeURIComponent(characterId)}/icon.webp`}
      alt={`${player.display_name} icon`}
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
            <PlayerIcon
              key={`${player.participant_id}:${characterId ?? "fallback"}`}
              player={player}
              characterId={characterId}
            />
            <span className="table-player-copy"><strong>{player.display_name}</strong></span>
          </section>
        );
      })}
    </div>
  );
}

export function ThreeTable({
  projection: authorizedProjection,
  viewSeat,
  room,
  animations = [],
  reducedMotion = false,
  onAnimationConsumed: _onAnimationConsumed,
  onAnimationCancelled: _onAnimationCancelled,
  surface = "live",
  sheenHost,
  appearance,
}: ThreeTableProps) {
  const projection = useMemo(() => authorizedProjection && Number.isInteger(viewSeat)
    && authorizedProjection.players?.some(player => player.seat === viewSeat)
    ? { ...authorizedProjection, viewer_seat: viewSeat } : authorizedProjection,
  [authorizedProjection, viewSeat]);
  const [atlas, setAtlas] = useState<TileAtlas | null>(null);
  const [atlasFailed, setAtlasFailed] = useState(false);
  const [webglFallback, setWebglFallback] = useState(false);
  const [rendererCreated, setRendererCreated] = useState(false);
  const [renderStats, setRenderStats] = useState<
    (SceneRenderStats & { layout: object; atlas: TileAtlas; appearance: TableAppearance | undefined }) | null
  >(null);
  const [motion, setMotion] = useState<SceneMotion | null>(null);
  const [lastConsumedAnimationId, setLastConsumedAnimationId] = useState<number | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const sheen = useMemo<DoraSheenClock>(() => ({ sweep: { value: -10 }, strength: { value: 0 }, listeners: new Set() }), []);
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
  // Decisions/clocks belong to Window; they neither change geometry nor enter the Worker.
  const renderProjection = projection ? { ...projection, decision: undefined } : null;
  const projectionKey = JSON.stringify(renderProjection);
  const sceneProjection = useMemo(() => renderProjection, [projectionKey]);
  const layout = useMemo(
    () => (sceneProjection ? buildMatchSceneLayout(sceneProjection, room, surface) : null),
    [sceneProjection, surface],
  );

  const localHand = projection?.players?.find((player) => player.seat === layout?.players.find((seat) => seat.isLocal)?.seat)?.hand ?? [];
  const indicators = projection?.dora_indicators ?? [];
  const hasSheenTargets = Boolean(layout?.tiles.some((tile) => tile.face === "front" && tile.group !== "dora" && tile.group !== "wall" && isDora(tile.tile, indicators)))
    || ((surface === "replay" || projection?.audience === "player") && localHand.some((tile) => isDora(tile, indicators)));
  useEffect(() => {
    const host = sheenHost?.current ?? hostRef.current;
    if (host) return startDoraSheen(host, sheen, hasSheenTargets, reducedMotion);
  }, [sheenHost, sheen, hasSheenTargets, reducedMotion]);

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
    setRendererCreated(false);
    setRenderStats(null);
    setWebglFallback(true);
  }, []);
  const invalidateRender = useCallback(() => setRenderStats(null), []);
  const recordCreated = useCallback(() => setRendererCreated(true), []);
  const isFallback = atlasFailed || webglFallback;
  const ready = Boolean(
    layout
    && atlas
    && rendererCreated
    && renderStats?.layout === layout
    && renderStats.atlas === atlas
    && renderStats.appearance?.surface === appearance?.surface
    && renderStats.appearance?.surround === appearance?.surround
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
    setRenderStats({ ...stats, layout, atlas, appearance });
  }, [atlas, layout, appearance]);
  const reportConsumed = useCallback((id: number) => {
    setLastConsumedAnimationId(id);
    onAnimationConsumedRef.current?.(id);
  }, []);
  const reportCancelled = useCallback((id: number) => {
    onAnimationCancelledRef.current?.(id);
  }, []);

  useEffect(() => {
    const active = activeMotionRef.current;
    const presentIds = new Set(animations.map((item) => item.id));
    if (active && (active.layout !== layout || !presentIds.has(active.motion.itemId) || isFallback)) {
      blockedAnimationIdsRef.current.add(active.motion.itemId);
      activeMotionRef.current = null;
      motionFrameTimesRef.current = [];
      motionPixelRatioRef.current = null;
      setMotion(null);
      reportCancelled(active.motion.itemId);
    }
    for (const id of blockedAnimationIdsRef.current) {
      if (!presentIds.has(id)) blockedAnimationIdsRef.current.delete(id);
    }
  }, [animations, isFallback, layout, reportCancelled]);

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
      data-wall-layout="count-only-diagram"
      data-dead-wall-tile-count={layout?.tiles.filter(({ key, group }) => key.startsWith("dead-wall-") || group === "dora").length ?? 0}
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
      {projection && layout && (
        <div className="visually-hidden"><TableCenterFacts projection={projection} wallCount={layout.wallCount} />
          <ul aria-label="Table scores">{layout.players.map(({ seat }) => {
            const player = projection.players?.find((candidate) => candidate.seat === seat);
            return player && <li key={seat}>{player.display_name}: {typeof player.score === "number" ? player.score.toLocaleString() : "unavailable"}{player.riichi ? ", Riichi" : ""}</li>;
          })}</ul>
        </div>
      )}
      {surface === "replay" && projection?.players?.find((player) => player.seat === layout?.players.find((seat) => seat.position === "bottom")?.seat)?.hand && (
        <div className="table-replay-hand" role="group" aria-label="Your concealed hand">
          {localHand.map((tile, index) => (
            <span key={`${tile}-${index}`} className={`table-replay-tile${isDora(tile, indicators) ? " is-dora" : ""}`}>
              <img src={tileAssetUrl(tile)} alt={`${tileLabel(tile)}${isDora(tile, indicators) ? ", Dora" : ""}`} />
            </span>
          ))}
        </div>
      )}
      {!projection ? null : isFallback ? (
        <TableFallback
          projection={projection}
          wallCount={layout?.wallCount ?? 0}
        />
      ) : atlas && layout ? (
        <ThreeTableRenderer
          layout={layout}
          projection={sceneProjection!}
          atlas={atlas}
          appearance={appearance}
          motion={motion}
          sheenEnabled={hasSheenTargets}
          reducedMotion={reducedMotion}
          onCreated={recordCreated}
          onFailure={markFallback}
          onInvalidate={invalidateRender}
          onMotionComplete={completeMotion}
          onMotionFrame={recordMotionFrame}
          onRenderReady={recordRenderReady}
        />
      ) : (
        <div role="status" aria-label="3D table loading">
          Preparing 3D table
        </div>
      )}
    </div>
  );
}
