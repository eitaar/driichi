import type { ParticipantAnalytics } from "./benchmark-analytics";

// Visibility never changes the full-roster percentage scale.
export function radarUpperBound(participants: readonly Pick<ParticipantAnalytics, "rankCounts">[], completed: number): number {
  const maximum = completed > 0 ? Math.max(0, ...participants.flatMap(p => p.rankCounts.map(count => count / completed * 100))) : 0;
  const target = Math.min(100, maximum * 1.10);
  return [25, 50, 75, 100].find(limit => limit >= target) ?? 100;
}

// Stable text metrics and the measured legend top define the fitting rectangle.
export function fitRadar(width: number, legendTop: number, labelWidth: number, labelHeight: number, rem: number): { cx: number; cy: number; radius: number } {
  const gap = rem / 2, tickGap = 8;
  const horizontal = labelWidth + tickGap + gap, vertical = labelHeight + tickGap + gap;
  const availableWidth = Math.max(0, width - 2 * horizontal), availableHeight = Math.max(0, legendTop - gap - 2 * vertical);
  return { cx: width / 2, cy: (legendTop - gap) / 2, radius: Math.max(1, Math.min(availableWidth, availableHeight) / 2) };
}
