import { useEffect, useMemo } from "react";
import { useThree } from "@react-three/fiber";
import { CanvasTexture, SRGBColorSpace } from "three";
import { tableSeatGeometry } from "./table-geometry";
import type { ProjectedState } from "./types";

export function TableCenter({ projection, wallCount }: { projection: ProjectedState; wallCount: number }) {
  const invalidate = useThree((state) => state.invalidate);
  const texture = useMemo(() => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1024;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.fillStyle = "#283342";
    ctx.fillRect(0, 0, 1024, 1024);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const seats = tableSeatGeometry(projection.mode, projection.viewer_seat);
    const dealer = typeof projection.dealer === "number" ? projection.dealer : null;
    for (const { seat, position } of seats) {
      const player = projection.players?.find((candidate) => candidate.seat === seat);
      if (!player) continue;
      const angle = { bottom: 0, right: Math.PI / 2, top: Math.PI, left: -Math.PI / 2 }[position];
      ctx.save();
      ctx.translate(512, 512);
      ctx.rotate(angle);
      ctx.fillStyle = "#354a66";
      ctx.fillRect(-232, 267, 464, 193);
      ctx.fillStyle = "#dce4ed";
      ctx.font = '500 38px "Segoe UI", sans-serif';
      const wind = dealer === null ? `Seat ${seat + 1}` : ["East", "South", "West", "North"][(seat - dealer + seats.length) % seats.length];
      ctx.fillText(wind, 0, 307);
      ctx.fillStyle = "#fafbfc";
      ctx.font = '600 70px "Segoe UI", sans-serif';
      ctx.fillText(typeof player.score === "number" ? player.score.toLocaleString() : "—", 0, 385);
      if (player.riichi) {
        ctx.fillStyle = "#fafbfc";
        ctx.fillRect(-94, 453, 188, 12);
        ctx.fillStyle = "#a72e3f";
        ctx.beginPath();
        ctx.arc(0, 459, 6, 0, 2 * Math.PI);
        ctx.fill();
      }
      ctx.restore();
    }
    const round = typeof projection.round === "string" ? projection.round : "Round";
    const kyoku = typeof projection.kyoku === "number" ? ` ${projection.kyoku}` : typeof projection.kyoku === "string" ? ` ${projection.kyoku}` : "";
    ctx.fillStyle = "#fafbfc";
    ctx.font = '600 56px "Segoe UI", sans-serif';
    ctx.fillText(`${round}${kyoku}`, 512, 447);
    ctx.fillStyle = "#dce4ed";
    ctx.font = '38px "Segoe UI", sans-serif';
    ctx.fillText(`${wallCount} left`, 512, 518);
    ctx.font = '32px "Segoe UI", sans-serif';
    ctx.fillText(`Honba ${Number(projection.honba) || 0} / Sticks ${Number(projection.kyotaku) || 0}`, 512, 576);
    const result = new CanvasTexture(canvas);
    result.colorSpace = SRGBColorSpace;
    result.anisotropy = 8;
    return result;
  }, [projection, wallCount]);
  useEffect(() => {
    invalidate();
    return () => texture?.dispose();
  }, [texture, invalidate]);
  return texture ? (
    <mesh name="webgl-center-display" position={[0, 0.47, 0]} rotation={[-Math.PI / 2, 0, 0]}>
      <planeGeometry args={[3.05, 2.52]} />
      <meshBasicMaterial map={texture} toneMapped={false} />
    </mesh>
  ) : null;
}
