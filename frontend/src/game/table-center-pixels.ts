import { tableSeatGeometry } from "./table-geometry";
import type { ProjectedState } from "./types";

// Paint on Window so the Worker keeps the same browser fonts and score formatting.
export function paintTableCenter(projection: ProjectedState, wallCount: number): HTMLCanvasElement | null {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 2048;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.scale(2, 2);
  ctx.fillStyle = "#283342";
  ctx.fillRect(0, 0, 1024, 1024);
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const seats = tableSeatGeometry(projection.mode, projection.viewer_seat);
  const dealer = typeof projection.dealer === "number" ? projection.dealer : null;
  for (const { seat, position } of seats) {
    const player = projection.players?.find((candidate) => candidate.seat === seat);
    if (!player) continue;
    const angle = { bottom: 0, right: -Math.PI / 2, top: Math.PI, left: Math.PI / 2 }[position];
    ctx.save();
    ctx.translate(512, 512);
    ctx.rotate(angle);
    ctx.fillStyle = "#354a66";
    ctx.fillRect(-232, 267, 464, 193);
    ctx.fillStyle = "#dce4ed";
    ctx.font = '500 48px "Segoe UI", sans-serif';
    const wind = dealer === null ? `Seat ${seat + 1}` : ["East", "South", "West", "North"][(seat - dealer + seats.length) % seats.length];
    ctx.fillText(wind, 0, 307);
    ctx.fillStyle = "#fafbfc";
    ctx.font = '600 82px "Segoe UI", sans-serif';
    ctx.fillText(typeof player.score === "number" ? player.score.toLocaleString() : "—", 0, 385);
    ctx.restore();
  }
  const round = typeof projection.round === "string" ? projection.round : "Round";
  const kyoku = typeof projection.kyoku === "number" ? ` ${projection.kyoku}` : typeof projection.kyoku === "string" ? ` ${projection.kyoku}` : "";
  ctx.fillStyle = "#fafbfc";
  ctx.font = '600 64px "Segoe UI", sans-serif';
  ctx.fillText(`${round}${kyoku}`, 512, 447);
  ctx.fillStyle = "#dce4ed";
  ctx.font = '500 48px "Segoe UI", sans-serif';
  ctx.fillText(`${wallCount} left`, 512, 520);
  ctx.font = '500 44px "Segoe UI", sans-serif';
  ctx.fillText(`Honba ${Number(projection.honba) || 0}`, 512, 598);
  ctx.fillText(`Sticks ${Number(projection.kyotaku) || 0}`, 512, 674);
  return canvas;
}
