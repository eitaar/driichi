import { useEffect, useRef } from "react";
import type { AnimationItem } from "./animation";
import { seatPositionFor, type TableSeatPosition } from "./orientation";
import type { ProjectedPlayer, ProjectedState, RoomSnapshot } from "./types";
import { tileAssetUrl, tileLabel } from "./tiles";
import { actionTile } from "./actions";

const TABLE_WIDTH = 1600;
const TABLE_HEIGHT = 900;
const TABLE_RATIO = TABLE_WIDTH / TABLE_HEIGHT;
const TILE_FRAMES = {
  hand: { width: 42, height: 56 },
  discard: { width: 31, height: 42 },
  meld: { width: 38, height: 50 },
  dora: { width: 36, height: 48 },
} as const;
const ICON_FRAME = { width: 44, height: 44 } as const;
const PORTRAIT_FRAME = { width: 190, height: 220 } as const;

export interface PortraitEffect {
  characterId: string;
  displayName: string;
  result: "Ron" | "Tsumo";
  han?: number;
  fu?: number;
  limit?: string;
  points?: number;
}

export interface PixiTableProps {
  projection: ProjectedState | null;
  room: RoomSnapshot | null;
  animations?: AnimationItem[];
  reducedMotion?: boolean;
  portraitEffect?: PortraitEffect | null;
}

interface TableScene {
  app: import("pixi.js").Application;
  assets: typeof import("pixi.js").Assets;
  loadedSources: Set<string>;
  disposed: boolean;
  animationStop: () => void;
  render: (projection: ProjectedState | null, room: RoomSnapshot | null, portrait: PortraitEffect | null) => void;
  animate: (items: AnimationItem[], reducedMotion: boolean) => void;
  resize: () => void;
  destroy: () => void;
}

function textStyle(size: number, fill: number, family = "Geist") {
  return { fontFamily: family, fontSize: size, fill, fontWeight: "normal" as const };
}

function playerAt(players: ProjectedPlayer[] | undefined, seat: number): ProjectedPlayer | undefined {
  return players?.find((player) => player.seat === seat);
}

function playerCoordinates(position: TableSeatPosition): { x: number; y: number; rotation: number; handRotation: number } {
  switch (position) {
    case "bottom": return { x: TABLE_WIDTH / 2, y: TABLE_HEIGHT - 102, rotation: 0, handRotation: 0 };
    case "right": return { x: TABLE_WIDTH - 120, y: TABLE_HEIGHT / 2, rotation: Math.PI / 2, handRotation: Math.PI / 2 };
    case "top": return { x: TABLE_WIDTH / 2, y: 90, rotation: Math.PI, handRotation: Math.PI };
    case "left": return { x: 120, y: TABLE_HEIGHT / 2, rotation: -Math.PI / 2, handRotation: -Math.PI / 2 };
  }
}

function readNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function discardPlacement(position: TableSeatPosition, index: number): { x: number; y: number; rotation: number } {
  const column = index % 6;
  const row = Math.floor(index / 6);
  const offset = (column - 2.5) * 48;
  if (position === "bottom") return { x: TABLE_WIDTH / 2 + offset, y: 548 + row * 58, rotation: 0 };
  if (position === "top") return { x: TABLE_WIDTH / 2 + offset, y: 244 - row * 58, rotation: Math.PI };
  if (position === "right") return { x: 1260 - row * 58, y: TABLE_HEIGHT / 2 + offset, rotation: Math.PI / 2 };
  return { x: 340 + row * 58, y: TABLE_HEIGHT / 2 + offset, rotation: -Math.PI / 2 };
}

function optionalVisibleTiles(player: ProjectedPlayer): number[] {
  return Array.isArray(player.hand) ? player.hand.filter((tile): tile is number => typeof tile === "number") : [];
}

function concealedTiles(player: ProjectedPlayer): number {
  return typeof player.concealed_count === "number" ? Math.max(0, player.concealed_count) : 0;
}

function doraTiles(projection: ProjectedState | null): number[] {
  return Array.isArray(projection?.dora_indicators)
    ? projection.dora_indicators.filter((tile): tile is number => typeof tile === "number")
    : [];
}

function visibleWallValue(projection: ProjectedState | null): string | null {
  const value = projection?.remaining_wall ?? projection?.remaining_tiles ?? projection?.wall_remaining;
  if (typeof value === "number") return `${Math.max(0, Math.floor(value))} TILES LEFT`;
  if (Array.isArray(value)) return `${value.length} TILES LEFT`;
  return null;
}

function safeDestroy(child: unknown): void {
  try { (child as { destroy?: () => void }).destroy?.(); } catch { /* Pixi can already have destroyed a subtree. */ }
}

function usableTexture(texture: import("pixi.js").Texture): boolean {
  const width = Number(texture.width);
  const height = Number(texture.height);
  return Number.isFinite(width) && Number.isFinite(height) && width >= 2 && height >= 2;
}

function fitSpriteToFrame(sprite: import("pixi.js").Sprite, frame: { width: number; height: number }): boolean {
  if (!usableTexture(sprite.texture)) return false;
  const width = Number(sprite.texture.width);
  const height = Number(sprite.texture.height);
  const scale = Math.min(frame.width / width, frame.height / height);
  sprite.width = width * scale;
  sprite.height = height * scale;
  return Number.isFinite(sprite.width) && Number.isFinite(sprite.height) && sprite.width > 0 && sprite.height > 0;
}

async function loadSource(
  assets: typeof import("pixi.js").Assets,
  source: string,
  fromImage?: (image: HTMLImageElement) => import("pixi.js").Texture,
): Promise<import("pixi.js").Texture | null> {
  try { return await assets.load(source) as import("pixi.js").Texture; } catch {
    if (!fromImage || typeof Image === "undefined") return null;
    return new Promise((resolve) => {
      const image = new Image();
      image.onload = () => {
        try { resolve(fromImage(image)); } catch { resolve(null); }
      };
      image.onerror = () => resolve(null);
      image.src = source;
    });
  }
}

const BACK_TILE_SOURCE = new URL("../assets/tiles/Regular/Back.svg", import.meta.url).href;

function sourceForBack(): string {
  return BACK_TILE_SOURCE;
}

function characterIdFor(room: RoomSnapshot | null, participantId: string): string | null {
  const roster = room?.roster?.length ? room.roster : room?.match_players ?? [];
  return roster.find((player) => player.participant_id === participantId)?.character_id ?? null;
}

export function PixiTable({ projection, room, animations = [], reducedMotion = false, portraitEffect = null }: PixiTableProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<TableScene | null>(null);
  const latestRef = useRef({ projection, room, portraitEffect });

  useEffect(() => {
    latestRef.current = { projection, room, portraitEffect };
    sceneRef.current?.render(projection, room, portraitEffect);
  }, [projection, room, portraitEffect]);

  useEffect(() => {
    sceneRef.current?.animate(animations, reducedMotion);
  }, [animations, reducedMotion]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    let observer: ResizeObserver | undefined;
    let resizeListener: (() => void) | undefined;

    const mount = async () => {
      try {
        const { Application, Assets, Container, Graphics, Sprite, Text, Texture } = await import("pixi.js");
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
        host.dataset.portraitReady = "false";
        app.stage.eventMode = "none";
        app.ticker.stop();

        const loadedSources = new Set<string>();
        let animationStop = () => undefined;
        let renderVersion = 0;
        let renderedTileCount = 0;
        let renderedTablePrimitives = 0;
        const updateRenderInstrumentation = () => {
          host.dataset.renderedTileCount = String(renderedTileCount);
          host.dataset.renderedTablePrimitives = String(renderedTablePrimitives);
          host.dataset.renderedVisualPrimitives = String(renderedTileCount + renderedTablePrimitives);
          host.dataset.renderReady = renderedTileCount > 0 && renderedTablePrimitives > 0 ? "true" : "false";
        };

        const resize = () => {
          if (disposed) return;
          const width = Math.max(1, host.clientWidth || TABLE_WIDTH);
          const height = Math.max(1, host.clientHeight || Math.round(width / TABLE_RATIO));
          app.renderer.resize(width, height);
          const scale = Math.min(width / TABLE_WIDTH, height / TABLE_HEIGHT);
          app.stage.scale.set(scale);
          app.stage.position.set((width - TABLE_WIDTH * scale) / 2, (height - TABLE_HEIGHT * scale) / 2);
          app.render();
        };

        const clearStage = () => {
          const children = app.stage.removeChildren();
          children.forEach((child) => safeDestroy(child));
        };

        const drawText = (container: import("pixi.js").Container, value: string, x: number, y: number, size: number, fill = 0xe8e5da, family = "Geist") => {
          const text = new Text({ text: value, style: textStyle(size, fill, family) });
          text.x = x;
          text.y = y;
          container.addChild(text);
          return text;
        };

        const drawTableShell = (container: import("pixi.js").Container) => {
          const frame = new Graphics();
          frame.roundRect(32, 30, TABLE_WIDTH - 64, TABLE_HEIGHT - 60, 8);
          frame.fill({ color: 0x071b16, alpha: 1 });
          frame.stroke({ color: 0x315249, width: 2, alpha: 0.85 });
          const field = new Graphics();
          field.roundRect(246, 148, TABLE_WIDTH - 492, TABLE_HEIGHT - 296, 5);
          field.fill({ color: 0x0d3027, alpha: 1 });
          field.stroke({ color: 0x315249, width: 1, alpha: 0.62 });
          const cross = new Graphics();
          cross.moveTo(TABLE_WIDTH / 2, 148).lineTo(TABLE_WIDTH / 2, TABLE_HEIGHT - 148);
          cross.moveTo(246, TABLE_HEIGHT / 2).lineTo(TABLE_WIDTH - 246, TABLE_HEIGHT / 2);
          cross.stroke({ color: 0x21473c, width: 1, alpha: 0.5 });
          container.addChild(frame, field, cross);
          drawText(container, "DOUBLE RIICHI / LIVE TABLE", 68, 54, 16, 0xaeb8ae, "Geist Mono");
          drawText(container, room?.room_name ?? "AUTHORITATIVE TABLE", TABLE_WIDTH - 68, 54, 14, 0x9ba8a0, "Geist Mono").anchor.set(1, 0);
        };

        const drawPlayerIcon = async (
          container: import("pixi.js").Container,
          player: ProjectedPlayer,
          nextRoom: RoomSnapshot | null,
          x: number,
          y: number,
          version: number,
        ) => {
          const characterId = characterIdFor(nextRoom, player.participant_id);
          const source = characterId ? `/assets/characters/${encodeURIComponent(characterId)}/icon.webp` : "";
          const texture = source ? await loadSource(Assets, source, (image) => Texture.from(image)) : null;
          if (disposed || version !== renderVersion) return;
          if (texture && usableTexture(texture)) {
            loadedSources.add(source);
            const sprite = new Sprite(texture);
            sprite.anchor.set(0.5);
            sprite.x = x;
            sprite.y = y;
            fitSpriteToFrame(sprite, ICON_FRAME);
            container.addChild(sprite);
          } else {
            const fallback = new Graphics();
            fallback.roundRect(x - 22, y - 22, 44, 44, 5);
            fallback.fill({ color: 0x1a2923, alpha: 1 });
            fallback.stroke({ color: 0x52665b, width: 1, alpha: 1 });
            container.addChild(fallback);
            drawText(container, (player.kind ?? "player").slice(0, 1).toUpperCase(), x, y - 8, 18, 0xb6c3ba, "Geist Mono").anchor.set(0.5, 0.5);
          }
          app.render();
        };

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
          loadedSources.add(source);
          const texture = await loadSource(Assets, source, (image) => Texture.from(image));
          if (disposed || version !== renderVersion || !texture || !usableTexture(texture)) return;
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
          app.render();
        };

        const render = (nextProjection: ProjectedState | null, nextRoom: RoomSnapshot | null, nextPortrait: PortraitEffect | null) => {
          const version = ++renderVersion;
          renderedTileCount = 0;
          renderedTablePrimitives = 0;
          host.dataset.portraitReady = "false";
          delete host.dataset.portraitEffect;
          delete host.dataset.portraitName;
          delete host.dataset.portraitResult;
          delete host.dataset.centerData;
          delete host.dataset.portraitLoad;
          clearStage();
          const root = new Container();
          app.stage.addChild(root);
          drawTableShell(root);
          renderedTablePrimitives = root.children.length;
          updateRenderInstrumentation();
          if (!nextProjection) {
            drawText(root, "Waiting for an authoritative projection", TABLE_WIDTH / 2, TABLE_HEIGHT / 2 - 13, 23, 0xc3cdc2, "Geist Mono").anchor.set(0.5, 0.5);
            drawText(root, "The table will synchronize when the host sends the next snapshot.", TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 22, 15, 0x81938a).anchor.set(0.5, 0.5);
            app.render();
            return;
          }

          const players = nextProjection.players ?? [];
          const mode = nextProjection.mode ?? nextRoom?.game_mode;
          const viewerSeat = nextProjection.audience === "player" ? nextProjection.viewer_seat : undefined;
          const seats = mode?.startsWith("3p") ? 3 : 4;
          const seatOrder = Array.from({ length: seats }, (_, seat) => seat);
          for (const seat of seatOrder) {
            const player = playerAt(players, seat);
            const position = seatPositionFor(mode, seat, viewerSeat) ?? (seats === 3 ? ["bottom", "right", "left"][seat] : ["bottom", "right", "top", "left"][seat]) as TableSeatPosition;
            if (!player) continue;
            const coordinates = playerCoordinates(position);
            const playerLayer = new Container();
            root.addChild(playerLayer);
            const iconX = position === "right" ? coordinates.x - 72 : position === "left" ? coordinates.x + 72 : coordinates.x - 122;
            const iconY = position === "top" ? coordinates.y + 25 : coordinates.y - 25;
            void drawPlayerIcon(root, player, nextRoom, iconX, iconY, version);
            const label = `${player.display_name}  ${readNumber(player.score)?.toLocaleString() ?? "—"}`;
            const labelText = drawText(root, label, coordinates.x, coordinates.y - (position === "bottom" ? 32 : 0), 17, position === "bottom" ? 0xffaa9e : 0xc3cdc2, "Geist Mono");
            labelText.anchor.set(position === "right" ? 1 : position === "left" ? 0 : 0.5, 0.5);
            if (player.riichi) drawText(root, "RIICHI", coordinates.x, coordinates.y - (position === "bottom" ? 55 : 22), 11, 0xd26c63, "Geist Mono").anchor.set(0.5, 0.5);

            const hand = optionalVisibleTiles(player);
            const count = hand.length > 0 ? hand.length : concealedTiles(player);
            const gap = 45;
            const start = -((Math.max(count, 1) - 1) * gap) / 2;
            for (let index = 0; index < count; index += 1) {
              const tile = hand[index] ?? 0;
              const localX = start + index * gap;
              const x = position === "bottom" || position === "top" ? coordinates.x + localX : coordinates.x;
              const y = position === "bottom" || position === "top" ? coordinates.y + (position === "bottom" ? 28 : -28) : coordinates.y + localX;
              void drawTile(playerLayer, tile, x, y, TILE_FRAMES.hand, coordinates.handRotation, hand.length === 0);
            }

            const discards = Array.isArray(player.discards) ? player.discards.filter((tile): tile is number => typeof tile === "number") : [];
            const discardLayer = new Container();
            root.addChild(discardLayer);
            discards.forEach((tile, index) => {
              const placement = discardPlacement(position, index);
              void drawTile(discardLayer, tile, placement.x, placement.y, TILE_FRAMES.discard, placement.rotation);
            });

            const melds = Array.isArray(player.melds) ? player.melds : [];
            melds.forEach((meld, meldIndex) => {
              const tiles = Array.isArray(meld.tiles) ? meld.tiles.filter((tile): tile is number => typeof tile === "number") : [];
              if (tiles.length === 0) return;
              const meldLayer = new Container();
              root.addChild(meldLayer);
              tiles.forEach((tile, tileIndex) => {
                const tileOffset = (meldIndex * 4 + tileIndex - (tiles.length - 1) / 2) * 42;
                const horizontal = position === "bottom" || position === "top";
                const x = coordinates.x + (horizontal ? tileOffset : position === "right" ? -72 : 72);
                const y = coordinates.y + (horizontal ? position === "bottom" ? -72 : 72 : tileOffset);
                void drawTile(meldLayer, tile, x, y, TILE_FRAMES.meld, coordinates.handRotation, false);
              });
            });
          }

          const dora = doraTiles(nextProjection);
          if (dora.length > 0) {
            drawText(root, "DORA", TABLE_WIDTH / 2 - 110, 168, 11, 0xd26c63, "Geist Mono");
            dora.forEach((tile, index) => void drawTile(root, tile, TABLE_WIDTH / 2 - 45 + index * 48, 178, TILE_FRAMES.dora));
          }
          const round = typeof nextProjection.kyoku === "string"
            ? nextProjection.kyoku
            : typeof nextProjection.round === "string" ? nextProjection.round : "LIVE KYOKU";
          const honba = readNumber(nextProjection.honba);
          const kyotaku = readNumber(nextProjection.kyotaku);
          host.dataset.centerData = [round, honba === undefined ? null : `HONBA ${honba}`, kyotaku === undefined ? null : `KYOTAKU ${kyotaku}`].filter(Boolean).join(" / ");
          drawText(root, round, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 - 18, 20, 0xe8e5da, "Geist Mono").anchor.set(0.5, 0.5);
          drawText(root, `HONBA ${honba ?? 0}  /  KYOTAKU ${kyotaku ?? 0}`, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 16, 11, 0x9ba8a0, "Geist Mono").anchor.set(0.5, 0.5);
          const wall = visibleWallValue(nextProjection);
          if (wall) drawText(root, wall, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 120, 13, 0x9ba8a0, "Geist Mono").anchor.set(0.5, 0.5);

          if (nextPortrait) {
            const portraitLayer = new Container();
            root.addChild(portraitLayer);
            void (async () => {
              const source = `/assets/characters/${encodeURIComponent(nextPortrait.characterId)}/portrait.webp`;
              loadedSources.add(source);
              const texture = await loadSource(Assets, source, (image) => Texture.from(image));
              if (disposed || version !== renderVersion) return;
              const veil = new Graphics();
              veil.rect(TABLE_WIDTH / 2 - 215, TABLE_HEIGHT / 2 - 195, 430, 390);
              veil.fill({ color: 0x06130f, alpha: 0.94 });
              veil.stroke({ color: 0x9a3f38, width: 2, alpha: 0.9 });
              portraitLayer.addChild(veil);
              const sprite = texture && usableTexture(texture) ? new Sprite(texture) : null;
              if (sprite && fitSpriteToFrame(sprite, PORTRAIT_FRAME)) {
                sprite.anchor.set(0.5);
                sprite.x = TABLE_WIDTH / 2;
                sprite.y = TABLE_HEIGHT / 2 - 45;
                portraitLayer.addChild(sprite);
              } else {
                host.dataset.portraitLoad = texture ? "unusable" : "failed";
                const fallback = new Graphics();
                fallback.roundRect(TABLE_WIDTH / 2 - PORTRAIT_FRAME.width / 2, TABLE_HEIGHT / 2 - 155, PORTRAIT_FRAME.width, PORTRAIT_FRAME.height, 8);
                fallback.fill({ color: 0x1b2b25, alpha: 1 });
                fallback.stroke({ color: 0x61766a, width: 1, alpha: 1 });
                portraitLayer.addChild(fallback);
                drawText(portraitLayer, nextPortrait.displayName.trim().slice(0, 1).toUpperCase() || "?", TABLE_WIDTH / 2, TABLE_HEIGHT / 2 - 45, 54, 0xd5ded4, "Geist Mono").anchor.set(0.5, 0.5);
              }
              drawText(portraitLayer, nextPortrait.displayName, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 91, 19, 0xf0e6d9, "Geist Mono").anchor.set(0.5, 0.5);
              drawText(portraitLayer, `${nextPortrait.result}  ${nextPortrait.han ?? "—"} HAN / ${nextPortrait.fu ?? "—"} FU`, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 120, 13, 0xffaa9e, "Geist Mono").anchor.set(0.5, 0.5);
              if (nextPortrait.limit) drawText(portraitLayer, nextPortrait.limit, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 146, 12, 0xe9c2ae, "Geist Mono").anchor.set(0.5, 0.5);
              if (nextPortrait.points !== undefined) drawText(portraitLayer, `+${nextPortrait.points.toLocaleString()}`, TABLE_WIDTH / 2, TABLE_HEIGHT / 2 + 171, 15, 0xf0e6d9, "Geist Mono").anchor.set(0.5, 0.5);
              host.dataset.portraitName = nextPortrait.displayName;
              host.dataset.portraitResult = nextPortrait.result;
              host.dataset.portraitEffect = nextPortrait.limit ?? "Mangan";
              host.dataset.portraitReady = "true";
              app.render();
            })();
          }
          app.render();
        };

        const animate = (items: AnimationItem[], reduce: boolean) => {
          animationStop();
          if (items.length === 0 || reduce || disposed) return;
          const marker = new Graphics();
          marker.circle(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 22);
          marker.fill({ color: 0xb93a35, alpha: 0.25 });
          app.stage.addChild(marker);
          let elapsed = 0;
          const duration = 280;
          const tick = (ticker: { deltaMS?: number; deltaTime?: number }) => {
            elapsed += ticker.deltaMS ?? (ticker.deltaTime ?? 1) * 16.67;
            const progress = Math.min(1, elapsed / duration);
            marker.alpha = 1 - progress;
            marker.scale.set(1 + progress * 0.85);
            if (progress >= 1) {
              app.ticker.remove(tick);
              app.ticker.stop();
              safeDestroy(marker);
              animationStop = () => undefined;
            }
          };
          animationStop = () => {
            app.ticker.remove(tick);
            app.ticker.stop();
            safeDestroy(marker);
          };
          app.ticker.add(tick);
          app.ticker.start();
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
            void Promise.all([...loadedSources].map((source) => Assets.unload(source).catch(() => undefined)));
          },
        };
        sceneRef.current = scene;
        observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(resize) : undefined;
        observer?.observe(host);
        if (!observer) {
          resizeListener = resize;
          window.addEventListener("resize", resize);
        }
        resize();
        render(latestRef.current.projection, latestRef.current.room, latestRef.current.portraitEffect);
        animate(animations, reducedMotion);
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

  return <div ref={hostRef} className="pixi-table" data-testid="pixi-table" data-table-ratio={TABLE_RATIO.toFixed(4)} data-render-ready="false" data-rendered-tile-count="0" data-rendered-table-primitives="0" data-rendered-visual-primitives="0" data-portrait-ready="false" aria-label="Authoritative mahjong table" />;
}

export function tableRatio(): number {
  return TABLE_RATIO;
}

export function projectedTileSources(projection: ProjectedState | null): string[] {
  const sources = new Set<string>();
  for (const player of projection?.players ?? []) {
    for (const tile of optionalVisibleTiles(player)) sources.add(tileAssetUrl(tile));
    if (optionalVisibleTiles(player).length === 0 && concealedTiles(player) > 0) sources.add(sourceForBack());
    for (const tile of player.discards ?? []) if (typeof tile === "number") sources.add(tileAssetUrl(tile));
    for (const meld of player.melds ?? []) for (const tile of meld.tiles ?? []) if (typeof tile === "number") sources.add(tileAssetUrl(tile));
  }
  for (const tile of doraTiles(projection)) sources.add(tileAssetUrl(tile));
  return [...sources];
}

export function discardTileForAction(action: unknown): number | undefined {
  return actionTile(action);
}
