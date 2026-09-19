import type { Container, Graphics, Sprite, Text, Texture } from "pixi.js";
import {
  TABLE_HEIGHT,
  TABLE_WIDTH,
  type TableSeatGeometry,
  wallPlacements,
} from "./table-geometry";
import type { ProjectedPlayer, ProjectedState, RoomSnapshot } from "./types";

const TABLE_FELT_SOURCE = new URL(
  "../assets/table/table-felt.webp",
  import.meta.url,
).href;
const TABLE_RAIL_SOURCE = new URL(
  "../assets/table/table-rail.webp",
  import.meta.url,
).href;
const CENTER_DEVICE_SOURCE = new URL(
  "../assets/table/center-device.webp",
  import.meta.url,
).href;
const TILE_BACK_MATERIAL_SOURCE = new URL(
  "../assets/table/tile-back-material.webp",
  import.meta.url,
).href;
const BACK_TILE_SOURCE = new URL(
  "../assets/tiles/Regular/Back.svg",
  import.meta.url,
).href;

const TABLE_BACKGROUND = 0x060814;
const TABLE_FIELD = 0x101932;
const TABLE_RAIL = 0x14131a;
const TABLE_GOLD = 0xb99a5d;
const TABLE_INK = 0x0a0e1c;
const IVORY = 0xe8e5da;
const MUTED_IVORY = 0xb6bac8;
const VERMILION = 0xd26c63;

const PORTRAIT_FRAME = { width: 96, height: 112 } as const;
const WALL_TILE_FRAME = { width: 24, height: 15 } as const;

export interface TableArtAssets {
  felt: Texture | null;
  rail: Texture | null;
  center: Texture | null;
  tileBack: Texture | null;
}

export type TextureLoader = (source: string) => Promise<Texture | null>;

export type DrawText = (
  container: Container,
  value: string,
  x: number,
  y: number,
  size: number,
  fill?: number,
  family?: string,
) => Text;

export interface TableArtContext {
  root: Container;
  assets: TableArtAssets;
  Graphics: typeof import("pixi.js").Graphics;
  Sprite: typeof import("pixi.js").Sprite;
  drawText: DrawText;
}

export type DrawTile = (
  container: Container,
  tile: number,
  x: number,
  y: number,
  frame: { width: number; height: number },
  rotation?: number,
  back?: boolean,
) => void;

export interface CenterDeviceContext {
  root: Container;
  assets: TableArtAssets;
  projection: ProjectedState;
  Graphics: typeof import("pixi.js").Graphics;
  Sprite: typeof import("pixi.js").Sprite;
  drawText: DrawText;
  drawTile?: DrawTile;
}

export interface PlayerFrameContext {
  root: Container;
  player: ProjectedPlayer;
  room: RoomSnapshot | null;
  projection: ProjectedState | null;
  geometry: TableSeatGeometry;
  Graphics: typeof import("pixi.js").Graphics;
  Sprite: typeof import("pixi.js").Sprite;
  drawText: DrawText;
  textureLoader: TextureLoader;
  isCurrent?: () => boolean;
}

export interface WallContext {
  root: Container;
  assets: TableArtAssets;
  projection: ProjectedState;
  Sprite: typeof import("pixi.js").Sprite;
  textureLoader: TextureLoader;
  isCurrent?: () => boolean;
}

export async function loadTableArtAssets(
  loadTexture: TextureLoader,
): Promise<TableArtAssets> {
  const [felt, rail, center, tileBack] = await Promise.all([
    Promise.resolve(loadTexture(TABLE_FELT_SOURCE)).catch(() => null),
    Promise.resolve(loadTexture(TABLE_RAIL_SOURCE)).catch(() => null),
    Promise.resolve(loadTexture(CENTER_DEVICE_SOURCE)).catch(() => null),
    Promise.resolve(loadTexture(TILE_BACK_MATERIAL_SOURCE)).catch(() => null),
  ]);
  return { felt, rail, center, tileBack };
}

function usableTexture(value: Texture | null | undefined): value is Texture {
  if (!value) return false;
  const width = Number(value.width);
  const height = Number(value.height);
  return Number.isFinite(width) && Number.isFinite(height) && width >= 2 && height >= 2;
}

function drawPolygon(
  graphics: Graphics,
  points: ReadonlyArray<readonly [number, number]>,
  fill: { color?: number; texture?: Texture | null; alpha?: number },
  stroke: { color: number; width: number; alpha?: number },
): void {
  const [first, ...rest] = points;
  graphics.moveTo(first[0], first[1]);
  for (const [x, y] of rest) graphics.lineTo(x, y);
  graphics.closePath();
  graphics.fill(fill);
  graphics.stroke(stroke);
}

export function drawTableSkin(context: TableArtContext): number {
  const { root, assets, Graphics } = context;
  let primitives = 0;
  const background = new Graphics();
  background.rect(0, 0, TABLE_WIDTH, TABLE_HEIGHT).fill({
    color: TABLE_BACKGROUND,
    alpha: 1,
  });
  root.addChild(background);
  primitives += 1;

  const rail = new Graphics();
  const outerRail: ReadonlyArray<readonly [number, number]> = [
    [40, 26],
    [1560, 26],
    [1510, 874],
    [90, 874],
  ];
  const field: ReadonlyArray<readonly [number, number]> = [
    [180, 108],
    [1420, 108],
    [1320, 794],
    [280, 794],
  ];
  const generatedSkin = usableTexture(assets.felt) && usableTexture(assets.rail);
  drawPolygon(
    rail,
    outerRail,
    generatedSkin
      ? { texture: assets.rail }
      : { color: TABLE_RAIL, alpha: 1 },
    { color: TABLE_GOLD, width: 3, alpha: 0.92 },
  );
  root.addChild(rail);
  primitives += 1;

  const felt = new Graphics();
  drawPolygon(
    felt,
    field,
    generatedSkin
      ? { texture: assets.felt }
      : { color: TABLE_FIELD, alpha: 1 },
    { color: TABLE_GOLD, width: 2, alpha: 0.72 },
  );
  root.addChild(felt);
  primitives += 1;

  return primitives;
}

function readNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function centerRoundLabel(projection: ProjectedState): string {
  const kyoku = projection.kyoku;
  if (typeof kyoku === "string") return kyoku;
  if (typeof projection.round === "string" && typeof kyoku === "number") {
    return `${projection.round} ${Math.max(1, Math.floor(kyoku))}`;
  }
  return typeof projection.round === "string" ? projection.round : "LIVE KYOKU";
}

function remainingWallValue(projection: ProjectedState): string | null {
  const value = projection.remaining_wall;
  if (typeof value === "number") {
    return `${Math.max(0, Math.floor(value))} TILES LEFT`;
  }
  if (Array.isArray(value)) return `${value.length} TILES LEFT`;
  return null;
}

function doraTiles(projection: ProjectedState): number[] {
  return Array.isArray(projection.dora_indicators)
    ? projection.dora_indicators.filter(
        (tile): tile is number => typeof tile === "number",
      )
    : [];
}

function octagonPoints(
  centerX: number,
  centerY: number,
  radiusX: number,
  radiusY: number,
): ReadonlyArray<readonly [number, number]> {
  return Array.from({ length: 8 }, (_, index) => {
    const angle = -Math.PI / 8 + (index * Math.PI) / 4;
    return [centerX + Math.cos(angle) * radiusX, centerY + Math.sin(angle) * radiusY] as const;
  });
}

export function drawCenterDevice(context: CenterDeviceContext): void {
  const { root, assets, projection, Graphics, Sprite, drawText } = context;
  const centerX = TABLE_WIDTH / 2;
  const centerY = TABLE_HEIGHT / 2;
  if (usableTexture(assets.center)) {
    const device = new Sprite(assets.center);
    device.anchor.set(0.5);
    device.x = centerX;
    device.y = centerY;
    device.width = 360;
    device.height = 250;
    device.eventMode = "none";
    root.addChild(device);
  } else {
    const device = new Graphics();
    drawPolygon(
      device,
      octagonPoints(centerX, centerY, 180, 125),
      { color: 0x111a32, alpha: 0.98 },
      { color: TABLE_GOLD, width: 3, alpha: 0.96 },
    );
    root.addChild(device);
    const inner = new Graphics();
    drawPolygon(
      inner,
      octagonPoints(centerX, centerY, 160, 106),
      { color: 0x0d1429, alpha: 0.9 },
      { color: 0x6e5b39, width: 1, alpha: 0.7 },
    );
    root.addChild(inner);
  }

  const round = centerRoundLabel(projection);
  const honba = readNumber(projection.honba);
  const kyotaku = readNumber(projection.kyotaku);
  drawText(root, round, centerX, centerY - 30, 19, IVORY, "Geist Mono").anchor.set(0.5, 0.5);
  drawText(
    root,
    `HONBA ${honba ?? "—"}  /  KYOTAKU ${kyotaku ?? "—"}`,
    centerX,
    centerY + 2,
    11,
    MUTED_IVORY,
    "Geist Mono",
  ).anchor.set(0.5, 0.5);
  const wall = remainingWallValue(projection);
  if (wall) {
    drawText(root, wall, centerX, centerY + 29, 11, 0xc7ad74, "Geist Mono").anchor.set(
      0.5,
      0.5,
    );
  }

  const dora = doraTiles(projection);
  if (dora.length === 0) return;
  drawText(root, "DORA", centerX - 140, centerY - 149, 11, VERMILION, "Geist Mono");
  if (!context.drawTile) return;
  dora.forEach((tile, index) => {
    context.drawTile!(
      root,
      tile,
      centerX - 73 + index * 42,
      centerY - 145,
      { width: 32, height: 42 },
    );
  });
}

function characterIdFor(
  room: RoomSnapshot | null,
  participantId: string,
): string | null {
  const roster = room?.roster?.length ? room.roster : room?.match_players ?? [];
  return (
    roster.find((player) => player.participant_id === participantId)?.character_id ??
    null
  );
}

function initialFor(player: ProjectedPlayer): string {
  const name = Array.from(player.display_name.trim())[0];
  if (name) return name.toUpperCase();
  const kind = Array.from((player.kind ?? "player").trim())[0];
  return kind ? kind.toUpperCase() : "?";
}

export function displayPlayerName(value: string): string {
  const codePoints = Array.from(value);
  return codePoints.length > 18
    ? `${codePoints.slice(0, 18).join("")}…`
    : value;
}

function playerFrameLayout(geometry: TableSeatGeometry): {
  cardX: number;
  cardY: number;
  portraitX: number;
  portraitY: number;
  textX: number;
  textAlign: number;
} {
  const { x, y } = geometry.frame;
  if (geometry.position === "right") {
    return {
      cardX: x - 270,
      cardY: y + 30,
      portraitX: x - 58,
      portraitY: y + 58,
      textX: x - 166,
      textAlign: 1,
    };
  }
  if (geometry.position === "top") {
    // The top hand occupies the middle of the north edge. Keep this frame in
    // the right-hand rail pocket so its 270px panel cannot veil the hand.
    return {
      cardX: x + 120,
      cardY: y - 78,
      portraitX: x + 170,
      portraitY: y - 20,
      textX: x + 232,
      textAlign: 0,
    };
  }
  return {
    cardX: x,
    cardY: y,
    portraitX: x + 50,
    portraitY: y + 58,
    textX: x + 112,
    textAlign: 0,
  };
}

export async function drawPlayerFrame(context: PlayerFrameContext): Promise<void> {
  const {
    root,
    player,
    room,
    projection,
    geometry,
    Graphics,
    Sprite,
    drawText,
    textureLoader,
    isCurrent,
  } = context;
  const layout = playerFrameLayout(geometry);
  const characterId = characterIdFor(room, player.participant_id);
  const source = characterId
    ? `/assets/characters/${encodeURIComponent(characterId)}/portrait.webp`
    : null;
  let texture: Texture | null = null;
  if (source) {
    try {
      texture = await textureLoader(source);
    } catch {
      texture = null;
    }
  }
  if (isCurrent && !isCurrent()) return;

  const isLocal =
    projection?.audience === "player" && projection.viewer_seat === player.seat;
  const panel = new Graphics();
  panel.roundRect(layout.cardX, layout.cardY, 270, 140, 14);
  panel.fill({ color: TABLE_INK, alpha: 0.93 });
  panel.stroke({
    color: isLocal ? TABLE_GOLD : 0x3b4154,
    width: isLocal ? 2.5 : 1,
    alpha: isLocal ? 1 : 0.88,
  });
  root.addChild(panel);

  const mask = new Graphics();
  mask.rect(
    layout.portraitX - PORTRAIT_FRAME.width / 2,
    layout.portraitY - PORTRAIT_FRAME.height / 2,
    PORTRAIT_FRAME.width,
    PORTRAIT_FRAME.height,
  );
  mask.fill({ color: 0xffffff, alpha: 1 });
  root.addChild(mask);

  if (usableTexture(texture)) {
    const sprite = new Sprite(texture);
    const width = Number(texture.width);
    const height = Number(texture.height);
    const scale = Math.max(PORTRAIT_FRAME.width / width, PORTRAIT_FRAME.height / height);
    sprite.anchor.set(0.5);
    sprite.x = layout.portraitX;
    sprite.y = layout.portraitY;
    sprite.width = width * scale;
    sprite.height = height * scale;
    sprite.mask = mask;
    sprite.eventMode = "none";
    root.addChild(sprite);
  } else {
    const fallback = new Graphics();
    fallback.rect(
      layout.portraitX - PORTRAIT_FRAME.width / 2,
      layout.portraitY - PORTRAIT_FRAME.height / 2,
      PORTRAIT_FRAME.width,
      PORTRAIT_FRAME.height,
    );
    fallback.fill({ color: 0x20283c, alpha: 1 });
    fallback.stroke({ color: 0x697083, width: 1, alpha: 0.9 });
    root.addChild(fallback);
    drawText(
      root,
      initialFor(player),
      layout.portraitX,
      layout.portraitY,
      38,
      0xd5d9e5,
      "Geist Mono",
    ).anchor.set(0.5, 0.5);
  }

  const portraitEdge = new Graphics();
  portraitEdge.rect(
    layout.portraitX - PORTRAIT_FRAME.width / 2,
    layout.portraitY - PORTRAIT_FRAME.height / 2,
    PORTRAIT_FRAME.width,
    PORTRAIT_FRAME.height,
  );
  portraitEdge.stroke({ color: isLocal ? TABLE_GOLD : 0x697083, width: 1.5, alpha: 0.95 });
  root.addChild(portraitEdge);

  const name = drawText(
    root,
    displayPlayerName(player.display_name),
    layout.textX,
    layout.cardY + 32,
    16,
    isLocal ? IVORY : 0xd7d8df,
    "Geist",
  );
  name.anchor.set(layout.textAlign, 0.5);
  const score = readNumber(player.score);
  const scoreText = drawText(
    root,
    score === undefined ? "—" : score.toLocaleString(),
    layout.textX,
    layout.cardY + 61,
    16,
    0xc7ad74,
    "Geist Mono",
  );
  scoreText.anchor.set(layout.textAlign, 0.5);
  const position = drawText(
    root,
    geometry.position.toUpperCase(),
    layout.textX,
    layout.cardY + 91,
    10,
    MUTED_IVORY,
    "Geist Mono",
  );
  position.anchor.set(layout.textAlign, 0.5);
  if (player.riichi) {
    const riichi = drawText(
      root,
      "RIICHI",
      layout.textX,
      layout.cardY + 114,
      10,
      VERMILION,
      "Geist Mono",
    );
    riichi.anchor.set(layout.textAlign, 0.5);
  }
}

export async function drawWall(context: WallContext): Promise<number> {
  const { root, assets, projection, Sprite, textureLoader, isCurrent } = context;
  const placements = wallPlacements(projection.remaining_wall);
  if (placements.length === 0) return 0;

  let texture: Texture | null = usableTexture(assets.tileBack) ? assets.tileBack : null;
  if (!texture) {
    try {
      texture = await textureLoader(BACK_TILE_SOURCE);
    } catch {
      texture = null;
    }
  }
  if (!usableTexture(texture) || (isCurrent && !isCurrent())) return 0;

  for (const placement of placements) {
    if (isCurrent && !isCurrent()) return 0;
    const sprite = new Sprite(texture);
    sprite.anchor.set(0.5);
    sprite.x = placement.x;
    sprite.y = placement.y;
    sprite.rotation = placement.rotation;
    sprite.width = WALL_TILE_FRAME.width;
    sprite.height = WALL_TILE_FRAME.height;
    sprite.eventMode = "none";
    root.addChild(sprite);
  }
  return placements.length;
}
