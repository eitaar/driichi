import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import type { SceneRenderStats } from "./three-table-scene";
import type { MatchSceneLayout } from "./three-table-layout";
import type { SceneMotion } from "./three-table-motion";
import type { TileAtlas } from "./tile-atlas";
import type { TableAppearance } from "./table-appearance";
import type { ProjectedState } from "./types";
import { paintTableCenter } from "./table-center-pixels";
import type { TableRenderAssets, TableRenderInput, TableRenderReply } from "./three-table-render-messages";

interface Props {
  layout: MatchSceneLayout;
  projection: ProjectedState;
  atlas: TileAtlas;
  appearance?: TableAppearance;
  motion: SceneMotion | null;
  sheenEnabled: boolean;
  reducedMotion: boolean;
  onCreated(): void;
  onFailure(): void;
  onInvalidate(): void;
  onRenderReady(stats: SceneRenderStats): void;
  onMotionComplete(id: number): void;
  onMotionFrame(now: number, pixelRatio: number): void;
}

export function ThreeTableRenderer(props: Props) {
  const host = useRef<HTMLDivElement>(null);
  const latest = useRef(props);
  latest.current = props;
  const displayKey = JSON.stringify([
    props.projection.mode, props.projection.viewer_seat, props.projection.dealer,
    props.projection.round, props.projection.kyoku, props.projection.honba, props.projection.kyotaku,
    props.projection.players?.map(({ seat, score }) => [seat, score]), props.layout.wallCount,
  ]);
  const center = useMemo(() => paintTableCenter(props.projection, props.layout.wallCount), [displayKey]);
  const centerRef = useRef(center);
  centerRef.current = center;
  const enqueue = useRef<(() => void) | null>(null);

  useEffect(() => {
    const element = host.current!;
    const canvas = document.createElement("canvas");
    canvas.setAttribute("aria-hidden", "true");
    element.append(canvas);
    let worker: Worker;
    try {
      if (typeof Worker === "undefined" || typeof canvas.transferControlToOffscreen !== "function") throw new Error("Offscreen renderer unavailable");
      worker = new Worker(new URL("./three-table-worker.tsx", import.meta.url), { type: "module" });
    } catch {
      canvas.remove();
      latest.current.onFailure();
      return;
    }
    let active = true;
    let initialized = false;
    let preparing = false;
    let inFlight: number | null = null;
    let revision = 0;
    let cleanupTimer: number | null = null;
    type Snapshot = { revision: number; props: Props; center: HTMLCanvasElement | null; width: number; height: number; dpr: number; visible: boolean };
    let pending: Snapshot | null = null;
    let sent: Snapshot | null = null;
    let lastQueued: Snapshot | null = null;
    const bitmapOptions: ImageBitmapOptions = { premultiplyAlpha: "none", colorSpaceConversion: "none" };

    const pump = async () => {
      if (!active || preparing || inFlight !== null || !pending) return;
      const snapshot = pending;
      pending = null;
      preparing = true;
      const assets: TableRenderAssets = {};
      const bitmaps: ImageBitmap[] = [];
      try {
        if (!snapshot.center) throw new Error("Center pixels unavailable");
        if (!initialized) {
          const bitmap = await createImageBitmap(snapshot.props.atlas.texture.image, { ...bitmapOptions, imageOrientation: "none" });
          bitmaps.push(bitmap);
          assets.atlas = { bitmap, columns: snapshot.props.atlas.columns, rows: snapshot.props.atlas.rows,
            cells: Array.from({ length: 137 }, (_, index) => snapshot.props.atlas.cellFor(index - 1)) };
        }
        if (sent?.center !== snapshot.center) {
          assets.center = await createImageBitmap(snapshot.center, { ...bitmapOptions, imageOrientation: "flipY" });
          bitmaps.push(assets.center);
        }
        for (const key of ["surface", "surround"] as const) {
          const local = snapshot.props.appearance?.[key] ?? null;
          if (!sent || (sent.props.appearance?.[key] ?? null) !== local) {
            const bitmap = local ? await createImageBitmap(local.image, { ...bitmapOptions, imageOrientation: "none" }) : null;
            assets[key] = bitmap;
            if (bitmap) bitmaps.push(bitmap);
          }
        }
        if (!active || snapshot.revision !== revision) {
          for (const bitmap of bitmaps) bitmap.close();
          return;
        }
        const message: TableRenderInput = {
          type: initialized ? "render" : "init", revision: snapshot.revision, assets,
          width: snapshot.width, height: snapshot.height, dpr: snapshot.dpr,
          windowTimeOrigin: performance.timeOrigin, motion: snapshot.props.motion,
          sheen: { enabled: snapshot.props.sheenEnabled, reduced: snapshot.props.reducedMotion, visible: snapshot.visible },
          ...(sent?.props.layout === snapshot.props.layout ? {} : { scene: { layout: snapshot.props.layout, projection: snapshot.props.projection } }),
        };
        const transfers: Transferable[] = [...bitmaps];
        if (!initialized) {
          message.canvas = canvas.transferControlToOffscreen();
          transfers.push(message.canvas);
        }
        inFlight = snapshot.revision;
        worker.postMessage(message, transfers);
        initialized = true;
        sent = snapshot;
      } catch {
        for (const bitmap of bitmaps) bitmap.close();
        if (active && snapshot.revision === revision) latest.current.onFailure();
      } finally {
        preparing = false;
        if (active && pending && inFlight === null) void pump();
      }
    };
    const queue = () => {
      if (!active) return;
      const rect = element.getBoundingClientRect();
      revision++;
      const next = { revision, props: latest.current, center: centerRef.current,
        width: Math.max(1, rect.width), height: Math.max(1, rect.height),
        dpr: Math.min(2, Math.max(1, window.devicePixelRatio)), visible: !document.hidden };
      // A motion/clock update does not invalidate unchanged geometry or its fixed DPR.
      if (!lastQueued || lastQueued.props.layout !== next.props.layout || lastQueued.center !== next.center
        || lastQueued.width !== next.width || lastQueued.height !== next.height || lastQueued.dpr !== next.dpr
        || lastQueued.props.appearance?.surface !== next.props.appearance?.surface
        || lastQueued.props.appearance?.surround !== next.props.appearance?.surround) latest.current.onInvalidate();
      lastQueued = pending = next;
      void pump();
    };
    enqueue.current = queue;
    worker.onmessage = ({ data }: MessageEvent<unknown>) => {
      if (!data || typeof data !== "object" || !("type" in data)) return;
      const reply = data as TableRenderReply;
      if (reply.type === "disposed" && !active) {
        if (cleanupTimer !== null) window.clearTimeout(cleanupTimer);
        worker.terminate();
        return;
      }
      if (!active) return;
      if (reply.type === "fatal") latest.current.onFailure();
      else if (reply.type === "created") latest.current.onCreated();
      else if (reply.type === "processed" && reply.revision === inFlight) {
        inFlight = null;
        void pump();
      } else if (reply.type === "ready" && reply.revision === revision) {
        const { stats, quality } = reply;
        if (!stats || !quality || ![stats.tileCount, stats.primitiveCount, stats.pixelRatio, stats.triangleCount, quality.width, quality.height, quality.clearAlpha].every(value => typeof value === "number" && Number.isFinite(value))
          || stats.tileCount <= 0 || stats.primitiveCount <= 0 || stats.pixelRatio! < 1 || stats.pixelRatio! > 2) return;
        element.dataset.rendererQuality = JSON.stringify(quality);
        latest.current.onRenderReady(stats);
      } else if (reply.type === "motion-complete" && reply.itemId === latest.current.motion?.itemId) {
        latest.current.onMotionComplete(reply.itemId);
      } else if (reply.type === "motion-frame" && reply.itemId === latest.current.motion?.itemId
        && Number.isFinite(reply.now) && Number.isFinite(reply.pixelRatio) && reply.pixelRatio >= 1 && reply.pixelRatio <= 2) {
        latest.current.onMotionFrame(reply.now, reply.pixelRatio);
      }
    };
    worker.onerror = () => { if (active) latest.current.onFailure(); };
    worker.onmessageerror = () => { if (active) latest.current.onFailure(); };
    const resize = new ResizeObserver(queue);
    resize.observe(element);
    document.addEventListener("visibilitychange", queue);
    window.addEventListener("resize", queue);
    queue();
    return () => {
      active = false;
      enqueue.current = null;
      pending = null;
      resize.disconnect();
      document.removeEventListener("visibilitychange", queue);
      window.removeEventListener("resize", queue);
      canvas.remove();
      // This is a resource-recovery bound only, never a result/creation deadline.
      cleanupTimer = window.setTimeout(() => worker.terminate(), 3000);
      try { worker.postMessage({ type: "dispose" }); }
      catch { window.clearTimeout(cleanupTimer); worker.terminate(); }
    };
  }, [props.atlas]);
  useLayoutEffect(() => { enqueue.current?.(); }, [props.layout, props.projection, props.appearance, props.motion, props.sheenEnabled, props.reducedMotion, center]);
  return <div ref={host} className="three-table-renderer" aria-hidden="true" />;
}
