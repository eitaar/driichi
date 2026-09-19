import { animationVisualForKind, type AnimationItem, type AnimationKind } from "./animation";
import {
  TABLE_HEIGHT,
  TABLE_WIDTH,
} from "./table-geometry";
import type { DrawText, TextureLoader } from "./table-art";

const RESULT_FRAME = { width: 430, height: 390 } as const;
const RESULT_PORTRAIT_FRAME = { width: 190, height: 220 } as const;
const RESULT_CENTER = { x: TABLE_WIDTH / 2, y: TABLE_HEIGHT / 2 } as const;
const RESULT_INK = 0x0a1024;
const RESULT_INDIGO = 0x111a32;
const RESULT_GOLD = 0xb99a5d;
const RESULT_IVORY = 0xf0e6d9;
const RESULT_MUTED = 0xb6bac8;
const RESULT_VERMILION = 0xd26c63;

export interface PortraitEffect {
  characterId: string;
  displayName: string;
  result: "Ron" | "Tsumo";
  han?: number;
  fu?: number;
  limit?: string;
  points?: number;
}

export interface TableAnimationTicker {
  add(callback: (ticker: import("pixi.js").Ticker) => void): unknown;
  remove(callback: (ticker: import("pixi.js").Ticker) => void): unknown;
  start(): unknown;
  stop(): unknown;
}

export interface TableAnimationContext {
  stage: import("pixi.js").Container;
  Graphics: typeof import("pixi.js").Graphics;
  ticker: TableAnimationTicker;
  item: AnimationItem;
  reducedMotion: boolean;
  onConsumed?: (id: number) => void;
  requestRender?: () => void;
  center?: { x: number; y: number };
}

export interface ResultPortraitContext {
  root: import("pixi.js").Container;
  Graphics: typeof import("pixi.js").Graphics;
  Sprite: typeof import("pixi.js").Sprite;
  drawText: DrawText;
  textureLoader: TextureLoader;
  portrait: PortraitEffect;
  isCurrent?: () => boolean;
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

function safeDestroy(child: unknown): void {
  try {
    (child as { destroy?: () => void }).destroy?.();
  } catch {
    /* Pixi can already have destroyed a subtree. */
  }
}

function drawMarker(
  marker: import("pixi.js").Graphics,
  visual: ReturnType<typeof animationVisualForKind>,
  centerX: number,
  centerY: number,
): void {
  switch (visual.shape) {
    case "circle":
      marker
        .circle(centerX, centerY, visual.radius)
        .fill({ color: visual.color, alpha: visual.alpha });
      break;
    case "square":
      marker
        .roundRect(
          centerX - visual.radius,
          centerY - visual.radius,
          visual.radius * 2,
          visual.radius * 2,
          5,
        )
        .fill({ color: visual.color, alpha: visual.alpha });
      break;
    case "diamond":
      marker
        .moveTo(centerX, centerY - visual.radius)
        .lineTo(centerX + visual.radius, centerY)
        .lineTo(centerX, centerY + visual.radius)
        .lineTo(centerX - visual.radius, centerY)
        .lineTo(centerX, centerY - visual.radius)
        .fill({ color: visual.color, alpha: visual.alpha });
      break;
    case "ring":
      marker
        .circle(centerX, centerY, visual.radius)
        .stroke({ color: visual.color, width: 5, alpha: visual.alpha });
      break;
    case "line":
      marker
        .moveTo(centerX - visual.radius, centerY)
        .lineTo(centerX + visual.radius, centerY)
        .stroke({ color: visual.color, width: 6, alpha: visual.alpha });
      break;
  }
}

export function effectDuration(
  kind: AnimationKind,
  reducedMotion: boolean,
): number {
  return reducedMotion ? 0 : animationVisualForKind(kind).duration;
}

export function runTableAnimation(
  context: TableAnimationContext,
): () => void {
  const {
    stage,
    Graphics,
    ticker,
    item,
    reducedMotion,
    onConsumed,
    requestRender,
  } = context;
  if (reducedMotion) {
    onConsumed?.(item.id);
    return () => undefined;
  }

  const visual = animationVisualForKind(item.kind);
  const centerX = context.center?.x ?? RESULT_CENTER.x;
  const centerY = context.center?.y ?? RESULT_CENTER.y;
  const marker = new Graphics();
  drawMarker(marker, visual, centerX, centerY);
  marker.scale.set(visual.scale);
  marker.eventMode = "none";
  stage.addChild(marker);
  requestRender?.();

  let elapsed = 0;
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    ticker.remove(tick);
    ticker.stop();
    safeDestroy(marker);
    requestRender?.();
  };
  const finish = () => {
    if (stopped) return;
    stop();
    onConsumed?.(item.id);
  };
  const tick = (clock: import("pixi.js").Ticker) => {
    elapsed += clock.deltaMS ?? clock.deltaTime * 16.67;
    const duration = Math.max(1, effectDuration(item.kind, false));
    const progress = Math.min(1, elapsed / duration);
    marker.alpha = visual.alpha * (1 - progress);
    marker.scale.set(visual.scale * (1 + progress * 0.85));
    requestRender?.();
    if (progress >= 1) finish();
  };

  ticker.add(tick);
  ticker.start();
  return stop;
}

function fitPortraitToFrame(
  sprite: import("pixi.js").Sprite,
): boolean {
  if (!usableTexture(sprite.texture)) return false;
  const width = Number(sprite.texture.width);
  const height = Number(sprite.texture.height);
  const scale = Math.min(
    RESULT_PORTRAIT_FRAME.width / width,
    RESULT_PORTRAIT_FRAME.height / height,
  );
  sprite.width = width * scale;
  sprite.height = height * scale;
  return (
    Number.isFinite(sprite.width) &&
    Number.isFinite(sprite.height) &&
    sprite.width > 0 &&
    sprite.height > 0
  );
}

function portraitSource(characterId: string): string {
  return `/assets/characters/${encodeURIComponent(characterId)}/portrait.webp`;
}

export async function drawResultPortrait(
  context: ResultPortraitContext,
): Promise<boolean> {
  const {
    root,
    Graphics,
    Sprite,
    drawText,
    textureLoader,
    portrait,
    isCurrent,
  } = context;
  let texture: import("pixi.js").Texture | null = null;
  try {
    texture = await textureLoader(portraitSource(portrait.characterId));
  } catch {
    texture = null;
  }
  if (!usableTexture(texture) || (isCurrent && !isCurrent())) return false;

  const centerX = RESULT_CENTER.x;
  const centerY = RESULT_CENTER.y;
  const portraitSprite = new Sprite(texture);
  if (!fitPortraitToFrame(portraitSprite)) return false;
  portraitSprite.anchor.set(0.5);

  const frame = new Graphics();
  frame.roundRect(
    centerX - RESULT_FRAME.width / 2,
    centerY - RESULT_FRAME.height / 2,
    RESULT_FRAME.width,
    RESULT_FRAME.height,
    16,
  );
  frame.fill({ color: RESULT_INK, alpha: 0.97 });
  frame.stroke({ color: RESULT_GOLD, width: 2.5, alpha: 0.98 });
  root.addChild(frame);

  const innerFrame = new Graphics();
  innerFrame.roundRect(
    centerX - RESULT_FRAME.width / 2 + 11,
    centerY - RESULT_FRAME.height / 2 + 11,
    RESULT_FRAME.width - 22,
    RESULT_FRAME.height - 22,
    11,
  );
  innerFrame.stroke({ color: RESULT_INDIGO, width: 1, alpha: 0.95 });
  root.addChild(innerFrame);
  portraitSprite.x = centerX;
  portraitSprite.y = centerY - 45;
  portraitSprite.eventMode = "none";
  root.addChild(portraitSprite);

  drawText(
    root,
    portrait.displayName,
    centerX,
    centerY + 91,
    19,
    RESULT_IVORY,
    "Geist Mono",
  ).anchor.set(0.5, 0.5);
  drawText(
    root,
    `${portrait.result}  ${portrait.han ?? "—"} HAN / ${portrait.fu ?? "—"} FU`,
    centerX,
    centerY + 120,
    13,
    RESULT_VERMILION,
    "Geist Mono",
  ).anchor.set(0.5, 0.5);
  if (portrait.limit) {
    drawText(
      root,
      portrait.limit,
      centerX,
      centerY + 146,
      12,
      RESULT_GOLD,
      "Geist Mono",
    ).anchor.set(0.5, 0.5);
  }
  if (portrait.points !== undefined) {
    drawText(
      root,
      `+${portrait.points.toLocaleString()}`,
      centerX,
      centerY + 171,
      15,
      RESULT_MUTED,
      "Geist Mono",
    ).anchor.set(0.5, 0.5);
  }
  return true;
}
