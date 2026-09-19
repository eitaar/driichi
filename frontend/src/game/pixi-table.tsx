import { useEffect, useRef } from "react";
import type { AnimationItem } from "./animation";
import { ASSET_LOAD_TIMEOUT_MS } from "./assets";
import {
  TABLE_HEIGHT,
  TABLE_RATIO,
  TABLE_WIDTH,
  discardPlacement,
  tableSeatGeometry,
} from "./table-geometry";
import {
  drawCenterDevice,
  drawPlayerFrame,
  drawTableSkin,
  drawWall,
  loadTableArtAssets,
  type TableArtAssets,
  type TextureLoader,
} from "./table-art";
import {
  drawResultPortrait,
  runTableAnimation,
  type PortraitEffect,
} from "./table-effects";
import type { ProjectedPlayer, ProjectedState, RoomSnapshot } from "./types";
import { tileAssetUrl } from "./tiles";
import { actionTile } from "./actions";

const TILE_FRAMES = {
  hand: { width: 42, height: 56 },
  discard: { width: 31, height: 42 },
  meld: { width: 38, height: 50 },
  dora: { width: 36, height: 48 },
} as const;
export type { PortraitEffect } from "./table-effects";

export interface PixiTableProps {
  projection: ProjectedState | null;
  room: RoomSnapshot | null;
  animations?: AnimationItem[];
  reducedMotion?: boolean;
  portraitEffect?: PortraitEffect | null;
  onAnimationConsumed?: (id: number) => void;
}

interface TableScene {
  app: import("pixi.js").Application;
  assets: typeof import("pixi.js").Assets;
  loadedSources: Set<string>;
  disposed: boolean;
  animationStop: () => void;
  render: (
    projection: ProjectedState | null,
    room: RoomSnapshot | null,
    portrait: PortraitEffect | null,
  ) => void;
  animate: (
    items: AnimationItem[],
    reducedMotion: boolean,
    onAnimationConsumed?: (id: number) => void,
  ) => void;
  resize: () => void;
  destroy: () => void;
}

function textStyle(size: number, fill: number, family = "Geist") {
  return {
    fontFamily: family,
    fontSize: size,
    fill,
    fontWeight: "normal" as const,
  };
}

function playerAt(
  players: ProjectedPlayer[] | undefined,
  seat: number,
): ProjectedPlayer | undefined {
  return players?.find((player) => player.seat === seat);
}

function readNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function optionalVisibleTiles(player: ProjectedPlayer): number[] {
  return Array.isArray(player.hand)
    ? player.hand.filter((tile): tile is number => typeof tile === "number")
    : [];
}

function concealedTiles(player: ProjectedPlayer): number {
  return typeof player.concealed_count === "number"
    ? Math.max(0, player.concealed_count)
    : 0;
}

function centerRoundLabel(projection: ProjectedState): string {
  const kyoku = projection.kyoku;
  if (typeof kyoku === "string") return kyoku;
  if (typeof projection.round === "string" && typeof kyoku === "number") {
    return `${projection.round} ${Math.max(1, Math.floor(kyoku))}`;
  }
  return typeof projection.round === "string" ? projection.round : "LIVE KYOKU";
}

function doraTiles(projection: ProjectedState | null): number[] {
  return Array.isArray(projection?.dora_indicators)
    ? projection.dora_indicators.filter(
        (tile): tile is number => typeof tile === "number",
      )
    : [];
}

function safeDestroy(child: unknown): void {
  try {
    (child as { destroy?: () => void }).destroy?.();
  } catch {
    /* Pixi can already have destroyed a subtree. */
  }
}

function usableTexture(
  texture: import("pixi.js").Texture | null | undefined,
): texture is import("pixi.js").Texture {
  if (!texture) return false;
  const width = Number(texture.width);
  const height = Number(texture.height);
  return (
    Number.isFinite(width) &&
    Number.isFinite(height) &&
    width >= 2 &&
    height >= 2
  );
}

async function loadSource(
  assets: typeof import("pixi.js").Assets,
  source: string,
  fromImage?: (image: HTMLImageElement) => import("pixi.js").Texture,
): Promise<import("pixi.js").Texture | null> {
  let timedOut = false;
  let timeout: number | undefined;
  const expiry = new Promise<null>((resolve) => {
    timeout = globalThis.setTimeout(() => {
      timedOut = true;
      resolve(null);
    }, ASSET_LOAD_TIMEOUT_MS);
  });
  try {
    const texture = await Promise.race([
      Promise.resolve(
        assets.load(source) as Promise<import("pixi.js").Texture>,
      ),
      expiry,
    ]);
    if (timeout !== undefined) globalThis.clearTimeout(timeout);
    if (texture || timedOut || !fromImage || typeof Image === "undefined")
      return texture;
  } catch {
    if (timeout !== undefined) globalThis.clearTimeout(timeout);
  }
  if (!fromImage || typeof Image === "undefined") return null;
  return new Promise((resolve) => {
    const image = new Image();
    let settled = false;
    const fallbackTimeout = globalThis.setTimeout(
      () => finish(null),
      ASSET_LOAD_TIMEOUT_MS,
    );
    const finish = (texture: import("pixi.js").Texture | null) => {
      if (settled) return;
      settled = true;
      globalThis.clearTimeout(fallbackTimeout);
      image.onload = null;
      image.onerror = null;
      resolve(texture);
    };
    image.onload = () => {
      if (settled) return;
      try {
        finish(fromImage(image));
      } catch {
        finish(null);
      }
    };
    image.onerror = () => finish(null);
    try {
      image.src = source;
    } catch {
      finish(null);
    }
  });
}

const BACK_TILE_SOURCE = new URL(
  "../assets/tiles/Regular/Back.svg",
  import.meta.url,
).href;

function sourceForBack(): string {
  return BACK_TILE_SOURCE;
}

export function PixiTable({
  projection,
  room,
  animations = [],
  reducedMotion = false,
  portraitEffect = null,
  onAnimationConsumed,
}: PixiTableProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<TableScene | null>(null);
  const latestRef = useRef({ projection, room, portraitEffect });
  const renderFrameRef = useRef<number | null>(null);
  const onAnimationConsumedRef = useRef(onAnimationConsumed);

  useEffect(() => {
    latestRef.current = { projection, room, portraitEffect };
    if (renderFrameRef.current !== null) cancelAnimationFrame(renderFrameRef.current);
    renderFrameRef.current = requestAnimationFrame(() => {
      renderFrameRef.current = null;
      const latest = latestRef.current;
      sceneRef.current?.render(latest.projection, latest.room, latest.portraitEffect);
    });
    return () => {
      if (renderFrameRef.current !== null) {
        cancelAnimationFrame(renderFrameRef.current);
        renderFrameRef.current = null;
      }
    };
  }, [projection, room, portraitEffect]);

  useEffect(() => {
    onAnimationConsumedRef.current = onAnimationConsumed;
  }, [onAnimationConsumed]);

  useEffect(() => {
    sceneRef.current?.animate(
      animations,
      reducedMotion,
      onAnimationConsumedRef.current,
    );
  }, [animations, reducedMotion]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    let observer: ResizeObserver | undefined;
    let resizeListener: (() => void) | undefined;

    const mount = async () => {
      try {
        // Pixi's no-eval renderer is required by the embedded server CSP.
        // @ts-expect-error pixi.js/unsafe-eval does not publish declarations.
        await import("pixi.js/unsafe-eval");
        const {
          Application,
          Assets,
          Container,
          Graphics,
          Sprite,
          Text,
          Texture,
        } = await import("pixi.js");
        if (disposed) return;
        const app = new Application();
        await app.init({
          antialias: true,
          autoDensity: true,
          autoStart: false,
          backgroundAlpha: 0,
          preference: "webgl",
          width: TABLE_WIDTH,
          height: TABLE_HEIGHT,
          resolution: Math.min(window.devicePixelRatio || 1, 2),
        });
        if (disposed) {
          app.destroy({ removeView: true }, { children: true });
          return;
        }
        app.canvas.className = "table-canvas";
        app.canvas.setAttribute("aria-hidden", "true");
        host.replaceChildren(app.canvas);
        host.dataset.renderReady = "false";
        host.dataset.renderedTileCount = "0";
        host.dataset.renderedTablePrimitives = "0";
        host.dataset.renderedVisualPrimitives = "0";
        host.dataset.skinReady = "false";
        host.dataset.skinFallback = "false";
        host.dataset.playerFrameCount = "0";
        host.dataset.wallTileCount = "0";
        host.dataset.portraitReady = "false";
        host.dataset.animationState = "idle";
        app.stage.eventMode = "none";
        app.ticker.stop();

        const loadedSources = new Set<string>();
        let animationStop: () => void = () => undefined;
        let renderVersion = 0;
        let renderedTileCount = 0;
        let renderedTablePrimitives = 0;
        let renderedPlayerFrames = 0;
        let renderedWallTiles = 0;
        let renderScheduled = false;
        const requestRender = () => {
          if (disposed || renderScheduled) return;
          renderScheduled = true;
          requestAnimationFrame(() => {
            renderScheduled = false;
            if (!disposed) app.render();
          });
        };
        const updateRenderInstrumentation = () => {
          host.dataset.renderedTileCount = String(renderedTileCount);
          host.dataset.playerFrameCount = String(renderedPlayerFrames);
          host.dataset.wallTileCount = String(renderedWallTiles);
          host.dataset.renderedTablePrimitives = String(
            renderedTablePrimitives,
          );
          host.dataset.renderedVisualPrimitives = String(
            renderedTileCount + renderedTablePrimitives,
          );
          host.dataset.renderReady =
            renderedTileCount > 0 && renderedTablePrimitives > 0
              ? "true"
              : "false";
        };

        const resize = () => {
          if (disposed) return;
          const width = Math.max(1, host.clientWidth || TABLE_WIDTH);
          const height = Math.max(
            1,
            host.clientHeight || Math.round(width / TABLE_RATIO),
          );
          app.renderer.resize(width, height);
          const scale = Math.min(width / TABLE_WIDTH, height / TABLE_HEIGHT);
          app.stage.scale.set(scale);
          app.stage.position.set(
            (width - TABLE_WIDTH * scale) / 2,
            (height - TABLE_HEIGHT * scale) / 2,
          );
          requestRender();
        };

        const clearStage = () => {
          const children = app.stage.removeChildren();
          for (const child of children) safeDestroy(child);
        };

        const drawText = (
          container: import("pixi.js").Container,
          value: string,
          x: number,
          y: number,
          size: number,
          fill = 0xe8e5da,
          family = "Geist",
        ) => {
          const text = new Text({
            text: value,
            style: textStyle(size, fill, family),
          });
          text.x = x;
          text.y = y;
          container.addChild(text);
          return text;
        };

        const loadTexture: TextureLoader = async (source) => {
          loadedSources.add(source);
          return loadSource(Assets, source, (image) => Texture.from(image));
        };
        const tableArtAssets: TableArtAssets = await loadTableArtAssets(loadTexture);
        if (disposed) {
          await Promise.all(
            [...loadedSources].map((source) =>
              Assets.unload(source).catch(() => undefined),
            ),
          );
          app.destroy({ removeView: true }, { children: true });
          return;
        }
        const tableArtUsesFallback =
          !usableTexture(tableArtAssets.felt) ||
          !usableTexture(tableArtAssets.rail) ||
          !usableTexture(tableArtAssets.center) ||
          !usableTexture(tableArtAssets.tileBack);

        const drawTile = async (
          container: import("pixi.js").Container,
          tile: number,
          x: number,
          y: number,
          frame: { width: number; height: number },
          rotation = 0,
          back = false,
        ) => {
          const version = renderVersion;
          const source = back ? sourceForBack() : tileAssetUrl(tile);
          const texture =
            back && usableTexture(tableArtAssets.tileBack)
              ? tableArtAssets.tileBack
              : await loadTexture(source);
          if (
            disposed ||
            version !== renderVersion ||
            !texture ||
            !usableTexture(texture)
          )
            return;
          const sprite = new Sprite(texture);
          sprite.anchor.set(0.5);
          sprite.x = x;
          sprite.y = y;
          sprite.rotation = rotation;
          sprite.width = frame.width;
          sprite.height = frame.height;
          sprite.eventMode = "none";
          container.addChild(sprite);
          if (version === renderVersion) {
            renderedTileCount += 1;
            updateRenderInstrumentation();
          }
          requestRender();
        };

        const render = (
          nextProjection: ProjectedState | null,
          nextRoom: RoomSnapshot | null,
          nextPortrait: PortraitEffect | null,
        ) => {
          const version = ++renderVersion;
          renderedTileCount = 0;
          renderedTablePrimitives = 0;
          renderedPlayerFrames = 0;
          renderedWallTiles = 0;
          host.dataset.skinReady = "false";
          host.dataset.skinFallback = tableArtUsesFallback ? "true" : "false";
          host.dataset.playerFrameCount = "0";
          host.dataset.wallTileCount = "0";
          host.dataset.portraitReady = "false";
          delete host.dataset.portraitEffect;
          delete host.dataset.portraitName;
          delete host.dataset.portraitResult;
          delete host.dataset.centerData;
          delete host.dataset.portraitLoad;
          clearStage();
          const root = new Container();
          app.stage.addChild(root);
          renderedTablePrimitives = drawTableSkin({
            root,
            assets: tableArtAssets,
            Graphics,
            Sprite,
            drawText,
          });
          if (version === renderVersion && !disposed) {
            host.dataset.skinReady = "true";
            host.dataset.skinFallback = tableArtUsesFallback ? "true" : "false";
          }
          updateRenderInstrumentation();
          if (!nextProjection) {
            requestRender();
            return;
          }

          const players = nextProjection.players ?? [];
          const mode = nextProjection.mode ?? nextRoom?.game_mode;
          const viewerSeat =
            nextProjection.audience === "player"
              ? nextProjection.viewer_seat
              : undefined;
          for (const geometry of tableSeatGeometry(mode, viewerSeat)) {
            const { seat, position } = geometry;
            const player = playerAt(players, seat);
            if (!player) continue;
            const coordinates = {
              x: geometry.hand.x,
              y: geometry.hand.y,
              handRotation: geometry.hand.rotation,
            };
            const playerLayer = new Container();
            root.addChild(playerLayer);
            void drawPlayerFrame({
              root,
              player,
              room: nextRoom,
              projection: nextProjection,
              geometry,
              Graphics,
              Sprite,
              drawText,
              textureLoader: loadTexture,
              isCurrent: () => !disposed && version === renderVersion,
            }).then(() => {
              if (disposed || version !== renderVersion) return;
              renderedPlayerFrames += 1;
              updateRenderInstrumentation();
              requestRender();
            });

            const hand = optionalVisibleTiles(player);
            const count =
              hand.length > 0 ? hand.length : concealedTiles(player);
            const gap = 45;
            const start = -((Math.max(count, 1) - 1) * gap) / 2;
            for (let index = 0; index < count; index += 1) {
              const tile = hand[index] ?? 0;
              const localX = start + index * gap;
              const x =
                position === "bottom" || position === "top"
                  ? coordinates.x + localX
                  : coordinates.x;
              const y =
                position === "bottom" || position === "top"
                  ? coordinates.y + (position === "bottom" ? 28 : -28)
                  : coordinates.y + localX;
              void drawTile(
                playerLayer,
                tile,
                x,
                y,
                TILE_FRAMES.hand,
                coordinates.handRotation,
                hand.length === 0,
              );
            }

            const discards = Array.isArray(player.discards)
              ? player.discards.filter(
                  (tile): tile is number => typeof tile === "number",
                )
              : [];
            const discardLayer = new Container();
            root.addChild(discardLayer);
            discards.forEach((tile, index) => {
              const placement = discardPlacement(position, index);
              void drawTile(
                discardLayer,
                tile,
                placement.x,
                placement.y,
                TILE_FRAMES.discard,
                placement.rotation,
              );
            });

            const melds = Array.isArray(player.melds) ? player.melds : [];
            melds.forEach((meld, meldIndex) => {
              const tiles = Array.isArray(meld.tiles)
                ? meld.tiles.filter(
                    (tile): tile is number => typeof tile === "number",
                  )
                : [];
              if (tiles.length === 0) return;
              const meldLayer = new Container();
              root.addChild(meldLayer);
              tiles.forEach((tile, tileIndex) => {
                const tileOffset =
                  (meldIndex * 4 + tileIndex - (tiles.length - 1) / 2) * 42;
                const horizontal = position === "bottom" || position === "top";
                const x =
                  coordinates.x +
                  (horizontal ? tileOffset : position === "right" ? -72 : 72);
                const y =
                  coordinates.y +
                  (horizontal
                    ? position === "bottom"
                      ? -72
                      : 72
                    : tileOffset);
                void drawTile(
                  meldLayer,
                  tile,
                  x,
                  y,
                  TILE_FRAMES.meld,
                  coordinates.handRotation,
                  false,
                );
              });
            });
          }

          const round = centerRoundLabel(nextProjection);
          const honba = readNumber(nextProjection.honba);
          const kyotaku = readNumber(nextProjection.kyotaku);
          host.dataset.centerData = [
            round,
            honba === undefined ? null : `HONBA ${honba}`,
            kyotaku === undefined ? null : `KYOTAKU ${kyotaku}`,
          ]
            .filter(Boolean)
            .join(" / ");
          void drawWall({
            root,
            assets: tableArtAssets,
            projection: nextProjection,
            Sprite,
            textureLoader: loadTexture,
            isCurrent: () => !disposed && version === renderVersion,
          }).then((count) => {
            if (disposed || version !== renderVersion) return;
            renderedWallTiles = count;
            updateRenderInstrumentation();
            requestRender();
          });
          drawCenterDevice({
            root,
            assets: tableArtAssets,
            projection: nextProjection,
            Graphics,
            Sprite,
            drawText,
            drawTile,
          });

          if (nextPortrait) {
            const portraitLayer = new Container();
            root.addChild(portraitLayer);
            void drawResultPortrait({
              root: portraitLayer,
              Graphics,
              Sprite,
              drawText,
              textureLoader: loadTexture,
              portrait: nextPortrait,
              isCurrent: () => !disposed && version === renderVersion,
            }).then((ready) => {
              if (disposed || version !== renderVersion) return;
              if (!ready) {
                host.dataset.portraitLoad = "failed";
                requestRender();
                return;
              }
              host.dataset.portraitName = nextPortrait.displayName;
              host.dataset.portraitResult = nextPortrait.result;
              host.dataset.portraitEffect = nextPortrait.limit ?? "Mangan";
              host.dataset.portraitReady = "true";
              requestRender();
            });
          }
          requestRender();
        };

        const animate = (
          items: AnimationItem[],
          reduce: boolean,
          onConsumed?: (id: number) => void,
        ) => {
          animationStop();
          host.dataset.animationState = "idle";
          if (items.length === 0 || disposed) return;
          host.dataset.animationState = reduce ? "idle" : "active";
          animationStop = runTableAnimation({
            stage: app.stage,
            Graphics,
            ticker: app.ticker,
            item: items[0],
            reducedMotion: reduce,
            onConsumed: (id) => {
              host.dataset.animationState = "idle";
              onConsumed?.(id);
            },
            requestRender,
          });
        };

        const scene: TableScene = {
          app,
          assets: Assets,
          loadedSources,
          disposed: false,
          animationStop,
          render,
          animate,
          resize,
          destroy: () => {
            scene.disposed = true;
            animationStop();
            clearStage();
            app.destroy({ removeView: true }, { children: true });
            void Promise.all(
              [...loadedSources].map((source) =>
                Assets.unload(source).catch(() => undefined),
              ),
            );
          },
        };
        sceneRef.current = scene;
        observer =
          typeof ResizeObserver === "undefined"
            ? undefined
            : new ResizeObserver(resize);
        observer?.observe(host);
        if (!observer) {
          resizeListener = resize;
          window.addEventListener("resize", resize);
        }
        resize();
        render(
          latestRef.current.projection,
          latestRef.current.room,
          latestRef.current.portraitEffect,
        );
        animate(animations, reducedMotion, onAnimationConsumedRef.current);
      } catch {
        if (disposed) return;
        host.dataset.fallback = "true";
        host.replaceChildren();
      }
    };

    void mount();
    return () => {
      disposed = true;
      observer?.disconnect();
      if (resizeListener) window.removeEventListener("resize", resizeListener);
      sceneRef.current?.destroy();
      sceneRef.current = null;
      host.replaceChildren();
    };
  }, []);

  return (
    <div
      ref={hostRef}
      className="pixi-table"
      data-testid="pixi-table"
      data-table-ratio={TABLE_RATIO.toFixed(4)}
      data-render-ready="false"
      data-rendered-tile-count="0"
      data-rendered-table-primitives="0"
      data-rendered-visual-primitives="0"
      data-skin-ready="false"
      data-skin-fallback="false"
      data-player-frame-count="0"
      data-wall-tile-count="0"
      data-portrait-ready="false"
      data-animation-state="idle"
      role="img"
      aria-label="Authoritative mahjong table"
    />
  );
}

export function tableRatio(): number {
  return TABLE_RATIO;
}

export function projectedTileSources(
  projection: ProjectedState | null,
): string[] {
  const sources = new Set<string>();
  for (const player of projection?.players ?? []) {
    for (const tile of optionalVisibleTiles(player))
      sources.add(tileAssetUrl(tile));
    if (optionalVisibleTiles(player).length === 0 && concealedTiles(player) > 0)
      sources.add(sourceForBack());
    for (const tile of player.discards ?? [])
      if (typeof tile === "number") sources.add(tileAssetUrl(tile));
    for (const meld of player.melds ?? [])
      for (const tile of meld.tiles ?? [])
        if (typeof tile === "number") sources.add(tileAssetUrl(tile));
  }
  for (const tile of doraTiles(projection)) sources.add(tileAssetUrl(tile));
  return [...sources];
}

export function discardTileForAction(action: unknown): number | undefined {
  return actionTile(action);
}
