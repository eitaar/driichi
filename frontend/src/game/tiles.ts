const SUITS = ["Man", "Pin", "Sou"] as const;
const HONORS = ["Ton", "Nan", "Shaa", "Pei", "Haku", "Hatsu", "Chun"] as const;

export function tileTypeId(tile: number): number {
  return Math.floor(tile / 4);
}

export function isRedFive(tile: number): boolean {
  return tile === 16 || tile === 52 || tile === 88;
}

export function tileFileName(tile: number): string {
  if (!Number.isInteger(tile) || tile < 0 || tile >= 136) return "Blank.svg";
  const type = tileTypeId(tile);
  if (type < 27) {
    const suit = SUITS[Math.floor(type / 9)];
    const value = (type % 9) + 1;
    return `${suit}${value}${isRedFive(tile) ? "-Dora" : ""}.svg`;
  }
  return `${HONORS[type - 27] ?? "Blank"}.svg`;
}

export function tileAssetUrl(tile: number, style: "Regular" | "Black" = "Regular"): string {
  return new URL(`../assets/tiles/${style}/${tileFileName(tile)}`, import.meta.url).href;
}

export const tileTextureSource = tileAssetUrl;
export const tileAssetSource = tileAssetUrl;

export function tileLabel(tile: number): string {
  if (!Number.isInteger(tile) || tile < 0 || tile >= 136) return "unknown tile";
  const type = tileTypeId(tile);
  if (type < 27) {
    const suit = ["m", "p", "s"][Math.floor(type / 9)];
    return `${(type % 9) + 1}${suit}${isRedFive(tile) ? " red" : ""}`;
  }
  return ["east", "south", "west", "north", "white", "green", "red"][type - 27] ?? "unknown tile";
}

export function tileIdsFromValue(value: unknown): number[] {
  if (!Array.isArray(value)) return [];
  return value.filter((tile): tile is number => typeof tile === "number" && Number.isInteger(tile));
}
