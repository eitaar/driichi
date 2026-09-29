// Physical tile IDs group four copies per kind; winds and dragons wrap separately.
export function isDora(tile: number | null, indicators: readonly number[]): boolean {
  if (tile === null || !Number.isInteger(tile) || tile < 0 || tile >= 136) return false;
  return indicators.some((indicator) => {
    if (!Number.isInteger(indicator) || indicator < 0 || indicator >= 136) return false;
    const kind = Math.floor(indicator / 4);
    const start = kind < 27 ? Math.floor(kind / 9) * 9 : kind < 31 ? 27 : 31;
    const count = kind < 27 ? 9 : kind < 31 ? 4 : 3;
    return Math.floor(tile / 4) === start + (kind - start + 1) % count;
  });
}
