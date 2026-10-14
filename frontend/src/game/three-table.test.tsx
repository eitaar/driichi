import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { StrictMode, useLayoutEffect } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CanvasTexture } from "three";
import type { ProjectedState, RoomSnapshot } from "./types";
import type { TableRenderInput, TableRenderReply } from "./three-table-render-messages";
import { ThreeTable } from "./three-table";

// Standard platform adapters only; the real ThreeTable, atlas and center painter run.
const workers: PlatformWorker[] = [];
const platform = { create: true, processed: true, ready: true, dispose: true, fail: false };
const stats = { tileCount: 54, primitiveCount: 9, pixelRatio: 1, triangleCount: 100 };
const quality = { antialias: true, clearAlpha: 1, pixelRatio: 1, width: 1280, height: 720,
  precision: "highp", colorSpace: "srgb", toneMapping: 0, shadowType: 1, shaderChecks: true };
class PlatformWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessageerror: (() => void) | null = null;
  messages: TableRenderInput[] = [];
  terminate = vi.fn();
  constructor(readonly url: URL, readonly options: WorkerOptions) {
    if (platform.fail) throw new Error("Worker module unavailable");
    workers.push(this);
  }
  emit(data: TableRenderReply) { this.onmessage?.({ data } as MessageEvent); }
  frame(revision = this.messages.at(-1)!.revision, rendered = stats) {
    this.emit({ type: "ready", revision, stats: rendered, quality });
  }
  postMessage(message: TableRenderInput | { type: "dispose" }) {
    if (message.type === "dispose") {
      if (platform.dispose) queueMicrotask(() => this.emit({ type: "disposed" }));
      return;
    }
    this.messages.push(message);
    queueMicrotask(() => {
      if (message.type === "init" && platform.create) this.emit({ type: "created" });
      if (platform.processed) this.emit({ type: "processed", revision: message.revision });
      if (platform.ready) this.frame(message.revision);
    });
  }
}
const painter = { scale: vi.fn(), fillRect: vi.fn(), drawImage: vi.fn(), save: vi.fn(), restore: vi.fn(), translate: vi.fn(), rotate: vi.fn(), fillText: vi.fn() };
const bitmaps: Array<{ width: number; height: number; close: ReturnType<typeof vi.fn> }> = [];
function player(seat: number, participantId: string, extra: Record<string, unknown> = {}) {
  return { seat, participant_id: participantId, display_name: participantId, ...extra };
}
function projection(overrides: Partial<ProjectedState> = {}): ProjectedState {
  return { mode: "4p-red-east", audience: "player", viewer_seat: 0, round: "East", kyoku: 1,
    remaining_wall: 42, dora_indicators: [0], players: [player(0, "local", { hand: [0, 1, 2] }),
      player(1, "right", { concealed_count: 3 }), player(2, "top", { concealed_count: 3 }), player(3, "left", { concealed_count: 3 })], ...overrides };
}
const room = { participants: [], match_players: [], roster: [] } as unknown as RoomSnapshot;
const currentWorker = () => workers.at(-1)!;
const ready = async () => { await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true")); };

beforeEach(() => {
  workers.length = bitmaps.length = 0;
  Object.assign(platform, { create: true, processed: true, ready: true, dispose: true, fail: false });
  for (const method of Object.values(painter)) if (vi.isMockFunction(method)) method.mockClear();
  vi.stubGlobal("Worker", PlatformWorker);
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  vi.stubGlobal("Image", class {
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(value: string) { if (value) queueMicrotask(() => this.onload?.()); }
  });
  vi.stubGlobal("createImageBitmap", vi.fn(async (source: { width: number; height: number }) => {
    const bitmap = { width: source.width, height: source.height, close: vi.fn() };
    bitmaps.push(bitmap);
    return bitmap;
  }));
  Object.defineProperty(HTMLCanvasElement.prototype, "transferControlToOffscreen", { configurable: true,
    value: vi.fn(function (this: HTMLCanvasElement) { return { width: this.width, height: this.height }; }) });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(() => painter as never);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1280, height: 720, x: 0, y: 0, top: 0, left: 0, right: 1280, bottom: 720, toJSON: () => ({}) });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers(); delete (HTMLCanvasElement.prototype as Partial<HTMLCanvasElement>).transferControlToOffscreen; });

describe("ThreeTable native Worker adapter", () => {
  it("shares static ordinary-Dora phase with the Replay DOM hand, not red artwork alone", async () => {
    const state = projection({ dora_indicators: [12], players: [player(0, "local", { hand: [16, 17, 0] })] });
    const view = render(<ThreeTable projection={state} room={room} surface="replay" reducedMotion />);
    await ready();
    const host = screen.getByTestId("three-table");
    expect(host.style.getPropertyValue("--dora-sheen-opacity")).toBe("0.28");
    expect([...screen.getByRole("group", { name: "Your concealed hand" }).querySelectorAll(".is-dora img")].map(image => image.getAttribute("alt"))).toEqual(["5m red, Dora", "5m, Dora"]);
    expect(currentWorker().messages.at(-1)!.sheen).toEqual({ enabled: true, reduced: true, visible: true });
    view.rerender(<ThreeTable projection={{ ...state, dora_indicators: [] }} room={room} surface="replay" reducedMotion />);
    expect(host.style.getPropertyValue("--dora-sheen-opacity")).toBe("0");
    expect(screen.getByRole("group", { name: "Your concealed hand" }).querySelectorAll(".is-dora")).toHaveLength(0);
  });
  it("uses a same-origin module Worker and bounded numeric DPR without a Window WebGL context", async () => {
    vi.stubGlobal("WebGLRenderingContext", undefined);
    vi.stubGlobal("devicePixelRatio", 3);
    render(<ThreeTable projection={projection()} room={room} />);
    await ready();
    expect(currentWorker().url.pathname).toMatch(/three-table-worker\.tsx$/);
    expect(currentWorker().options.type).toBe("module");
    expect(currentWorker().messages[0]).toMatchObject({ dpr: 2, width: 1280, height: 720 });
    for (const [kind] of vi.mocked(HTMLCanvasElement.prototype.getContext).mock.calls) expect(kind).toBe("2d");
    expect(screen.getByTestId("three-table").querySelector("canvas")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("three-table").querySelector(".table-player-overlays")).toHaveAttribute("aria-hidden", "true");
  });
  it("keeps Live hands concealed and Replay hands authoritative at the render boundary", async () => {
    const state = projection({ audience: "replay_admin", players: [player(0, "local", { hand: [0, 16] }), player(1, "right", { hand: [20, 52], concealed_count: 2 })] });
    const view = render(<ThreeTable projection={state} room={room} surface="live" />);
    await ready();
    const tiles = () => currentWorker().messages.filter(input => input.scene).at(-1)!.scene!.layout.tiles.filter(tile => tile.group === "hand" && tile.key.includes("seat-1-"));
    expect(tiles().every(({ face, tile }) => face === "back" && tile === null)).toBe(true);
    view.rerender(<ThreeTable projection={state} room={room} surface="replay" />);
    await waitFor(() => expect(tiles().map(({ face, tile }) => [face, tile])).toEqual([["front", 20], ["front", 52]]));
  });
  it("does not invent a Window creation deadline while a renderer reply is held", async () => {
    platform.create = platform.ready = false;
    const timers = vi.spyOn(window, "setTimeout");
    render(<ThreeTable projection={projection()} room={room} />);
    await waitFor(() => expect(currentWorker()?.messages.length).toBeGreaterThan(0));
    expect(timers.mock.calls.filter(([, delay]) => delay === 3000)).toHaveLength(0);
    act(() => { currentWorker().emit({ type: "created" }); currentWorker().frame(); });
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-webgl-fallback", "false");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
  });
  it("ignores old lifetime callbacks after unmount without disabling the new renderer", async () => {
    const first = render(<ThreeTable projection={projection()} room={room} />);
    await ready();
    const old = currentWorker();
    first.unmount();
    render(<ThreeTable projection={projection()} room={room} />);
    await ready();
    act(() => { old.emit({ type: "fatal" }); old.frame(); });
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-webgl-fallback", "false");
  });
  it("reports actual-frame instrumentation only after both construction and a current frame", async () => {
    platform.create = platform.ready = false;
    render(<ThreeTable projection={projection()} room={room} />);
    await waitFor(() => expect(currentWorker()?.messages.length).toBeGreaterThan(0));
    const host = screen.getByTestId("three-table");
    expect(host).toHaveAccessibleName("3D mahjong table");
    act(() => currentWorker().emit({ type: "processed", revision: currentWorker().messages.at(-1)!.revision }));
    expect(host).toHaveAttribute("data-render-ready", "false");
    act(() => currentWorker().emit({ type: "created" }));
    expect(host).toHaveAttribute("data-rendered-tile-count", "0");
    act(() => currentWorker().frame());
    expect(host).toHaveAttribute("data-render-ready", "true");
    expect(host).toHaveAttribute("data-rendered-tile-count", "54");
    expect(host).toHaveAttribute("data-rendered-scene-primitives", "9");
    expect(host).toHaveAttribute("data-wall-tile-count", "42");
    expect(host).toHaveAttribute("data-player-frame-count", "4");
  });
  it.each(["live", "replay"] as const)("preserves %s readiness across same-value resends but rejects stale wall and Dora frames", async surface => {
    platform.ready = false;
    const state = projection();
    const view = render(<ThreeTable projection={state} room={room} surface={surface} />);
    await waitFor(() => expect(currentWorker()?.messages.length).toBeGreaterThan(0));
    const old = currentWorker().messages.at(-1)!.revision;
    view.rerender(<ThreeTable projection={structuredClone(state)} room={{ ...room, revision: 2 } as RoomSnapshot} surface={surface} />);
    act(() => currentWorker().frame(old));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
    view.rerender(<ThreeTable projection={{ ...state, remaining_wall: 41 }} room={room} surface={surface} />);
    act(() => currentWorker().frame(old));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    await waitFor(() => expect(currentWorker().messages.at(-1)!.revision).toBeGreaterThan(old));
    const wall = currentWorker().messages.at(-1)!.revision;
    act(() => currentWorker().frame(wall, { ...stats, tileCount: 53 }));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-rendered-tile-count", "53");
    view.rerender(<ThreeTable projection={{ ...state, remaining_wall: 41, dora_indicators: [16] }} room={room} surface={surface} />);
    act(() => currentWorker().frame(wall));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
  });
  it.each(["live", "replay"] as const)("keeps %s geometry ready when only the Window-owned Decision changes", async surface => {
    const decision = { decision_id: "d1", kind: "turn", remaining_ms: 10000,
      actions: [{ action_id: "discard-1", action: { type: "dahai", tile: 0 } }] };
    const state = projection({ decision });
    const view = render(<ThreeTable projection={state} room={room} surface={surface} />);
    await ready();
    platform.ready = false;
    for (const next of [{ ...decision, remaining_ms: 9990 }, { ...decision, decision_id: "d2", actions: [] }]) {
      view.rerender(<ThreeTable projection={{ ...state, decision: next }} room={room} surface={surface} />);
      expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
      expect(screen.getByTestId("three-table")).toHaveAttribute("data-renderer-pixel-ratio", "1");
    }
    expect(currentWorker().messages.filter(input => input.scene).at(-1)!.scene!.projection.decision).toBeUndefined();
    expect(state.decision).toBe(decision);
    expect(state.decision?.actions[0].action_id).toBe("discard-1");
  });
  it("invalidates image readiness in the clear commit before passive effects can queue the Worker", async () => {
    const state = projection();
    const image = { image: { width: 8, height: 4 } as ImageBitmap, dispose: vi.fn() };
    const selected = { surface: image, surround: null };
    const cleared = { surface: null, surround: null };
    const commits: string[] = [];
    function Surface({ appearance }: { appearance: typeof selected | typeof cleared }) {
      useLayoutEffect(() => {
        if (appearance === cleared) commits.push(screen.getByTestId("three-table").dataset.renderReady!);
      }, [appearance]);
      return <ThreeTable projection={state} room={room} appearance={appearance} />;
    }
    const view = render(<Surface appearance={selected} />);
    await ready();
    const worker = currentWorker(), oldRevision = worker.messages.at(-1)!.revision;
    platform.ready = false;
    view.rerender(<Surface appearance={cleared} />);
    expect(commits).toEqual(["false"]);
    act(() => worker.frame(oldRevision));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    await waitFor(() => expect(worker.messages.at(-1)!.revision).toBeGreaterThan(oldRevision));
    expect(worker.messages.at(-1)!.assets.surface).toBeNull();
    act(() => worker.frame());
    await ready();
  });
  it("bounds rendering to one in-flight input and latest pending, ignoring stale processed acknowledgements", async () => {
    platform.processed = platform.ready = false;
    const state = projection();
    const view = render(<ThreeTable projection={state} room={room} />);
    await waitFor(() => expect(currentWorker()?.messages.length).toBe(1));
    const worker = currentWorker(), first = worker.messages[0].revision;
    view.rerender(<ThreeTable projection={{ ...state, remaining_wall: 41 }} room={room} />);
    view.rerender(<ThreeTable projection={{ ...state, remaining_wall: 40 }} room={room} />);
    act(() => worker.emit({ type: "processed", revision: first + 100 }));
    expect(worker.messages).toHaveLength(1);
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    act(() => worker.emit({ type: "processed", revision: first }));
    await waitFor(() => expect(worker.messages).toHaveLength(2));
    expect(worker.messages[1].scene!.layout.wallCount).toBe(40);
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    act(() => worker.frame(first));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    act(() => worker.frame(worker.messages[1].revision));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
  });
  it("closes a late prepared bitmap after unmount without transferring the stale view", async () => {
    let finish!: (bitmap: ImageBitmap) => void;
    const held = new Promise<ImageBitmap>(resolve => { finish = resolve; });
    vi.mocked(createImageBitmap).mockImplementationOnce(() => held);
    const view = render(<ThreeTable projection={projection()} room={room} />);
    await waitFor(() => expect(createImageBitmap).toHaveBeenCalled());
    const worker = currentWorker();
    view.unmount();
    const late = { width: 2048, height: 1710, close: vi.fn() };
    await act(async () => { finish(late as unknown as ImageBitmap); });
    expect(late.close).toHaveBeenCalledOnce();
    expect(worker.messages).toHaveLength(0);
    for (const bitmap of bitmaps) expect(bitmap.close).toHaveBeenCalledOnce();
  });
  it("closes superseded prepared pixels and initializes only the latest private view", async () => {
    let finish!: (bitmap: ImageBitmap) => void;
    const held = new Promise<ImageBitmap>(resolve => { finish = resolve; });
    vi.mocked(createImageBitmap).mockImplementationOnce(() => held);
    const state = projection();
    const view = render(<ThreeTable projection={state} room={room} />);
    await waitFor(() => expect(createImageBitmap).toHaveBeenCalled());
    view.rerender(<ThreeTable projection={{ ...state, remaining_wall: 41 }} room={room} />);
    const stale = { width: 2048, height: 1710, close: vi.fn() };
    await act(async () => { finish(stale as unknown as ImageBitmap); });
    await ready();
    expect(stale.close).toHaveBeenCalledOnce();
    expect(bitmaps[0].close).toHaveBeenCalledOnce();
    expect(currentWorker().messages).toHaveLength(1);
    expect(currentWorker().messages[0].scene!.layout.wallCount).toBe(41);
  });
  it("closes already-prepared pixels when a later bitmap decode fails", async () => {
    const prepared: Array<{ close: ReturnType<typeof vi.fn> }> = [];
    vi.mocked(createImageBitmap).mockImplementation(async source => {
      if (source instanceof HTMLCanvasElement && source.width === 2048 && source.height === 2048) {
        throw new Error("Center bitmap unavailable");
      }
      const size = source as { width: number; height: number };
      const bitmap = { width: size.width, height: size.height, close: vi.fn() };
      prepared.push(bitmap);
      return bitmap as unknown as ImageBitmap;
    });
    render(<ThreeTable projection={projection()} room={room} />);
    await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-webgl-fallback", "true"));
    expect(prepared.length).toBeGreaterThan(0);
    for (const bitmap of prepared) expect(bitmap.close).toHaveBeenCalledOnce();
    expect(currentWorker().messages).toHaveLength(0);
    expect(screen.getByLabelText("3D table unavailable")).toBeVisible();
  });
  it("retains center pixels across resends and decision/results, and repaints changed scores", async () => {
    const state = projection({ players: [player(0, "local", { hand: [0, 1, 2], score: 25000 })] });
    const view = render(<ThreeTable projection={state} room={room} />);
    await ready();
    expect(painter.fillText).toHaveBeenCalledWith((25000).toLocaleString(), 0, 385);
    painter.fillText.mockClear();
    view.rerender(<ThreeTable projection={structuredClone(state)} room={room} />);
    expect(painter.fillText).not.toHaveBeenCalled();
    view.rerender(<ThreeTable projection={{ ...state, decision: { decision_id: "d1", kind: "turn", actions: [] }, kyoku_results: { wins: [], settlement: null } }} room={room} />);
    expect(painter.fillText).not.toHaveBeenCalled();
    await ready();
    expect(currentWorker().messages.at(-1)!.assets.center).toBeUndefined();
    view.rerender(<ThreeTable projection={{ ...state, players: [player(0, "local", { hand: [0, 1, 2], score: 27600 })] }} room={room} />);
    expect(painter.fillText).toHaveBeenCalledWith((27600).toLocaleString(), 0, 385);
  });
  it.each([
    [{ round: "South", kyoku: 2 }, "South 2", 512, 447], [{ honba: 2 }, "Honba 2", 512, 598],
    [{ kyotaku: 3 }, "Sticks 3", 512, 674], [{ remaining_wall: 41 }, "41 left", 512, 520], [{ dealer: 0 }, "East", 0, 307],
  ] as const)("repaints changed displayed center facts %j", async (changed, text, x, y) => {
    const state = projection();
    const view = render(<ThreeTable projection={state} room={room} />);
    await ready(); painter.fillText.mockClear();
    view.rerender(<ThreeTable projection={{ ...state, ...changed }} room={room} />);
    expect(painter.fillText).toHaveBeenCalledWith(text, x, y);
  });
  it("completes one current motion exactly once and retains frame measurements", async () => {
    const consumed = vi.fn();
    render(<ThreeTable projection={projection()} room={room} animations={[{ id: 12, kind: "discard", event: { type: "dahai" } }]} onAnimationConsumed={consumed} />);
    await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-item-id", "12"));
    act(() => currentWorker().emit({ type: "motion-frame", itemId: 12, now: 100, pixelRatio: 1.25 }));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-renderer-pixel-ratio", "1.25");
    act(() => currentWorker().emit({ type: "motion-complete", itemId: 12 }));
    expect(consumed.mock.calls).toEqual([[12]]);
    act(() => currentWorker().emit({ type: "motion-complete", itemId: 12 }));
    expect(consumed.mock.calls).toEqual([[12]]);
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-state", "idle");
  });
  it("keeps fixed DPR and the unchanged rendered scene when a motion ends before its next frame report", async () => {
    render(<ThreeTable projection={projection()} room={room} animations={[{ id: 18, kind: "discard", event: {} }]} />);
    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-animation-item-id", "18"));
    await ready();
    platform.ready = false;
    act(() => currentWorker().emit({ type: "motion-complete", itemId: 18 }));
    expect(host).toHaveAttribute("data-animation-state", "idle");
    expect(host).toHaveAttribute("data-renderer-pixel-ratio", "1");
    expect(host).toHaveAttribute("data-render-ready", "true");
    expect(host).toHaveAttribute("data-rendered-tile-count", "54");
  });
  it("consumes the active motion once when Reduced Motion switches on", async () => {
    const consumed = vi.fn(); const state = projection();
    const animations = [{ id: 19, kind: "discard" as const, event: { type: "dahai" } }];
    const view = render(<ThreeTable projection={state} room={room} animations={animations} onAnimationConsumed={consumed} />);
    await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-item-id", "19"));
    view.rerender(<ThreeTable projection={state} room={room} animations={animations} reducedMotion onAnimationConsumed={consumed} />);
    act(() => currentWorker().emit({ type: "motion-complete", itemId: 19 }));
    expect(consumed.mock.calls).toEqual([[19]]);
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-state", "static");
  });
  it("consumes each exact ID immediately in Reduced Motion", async () => {
    const consumed = vi.fn();
    render(<ThreeTable projection={projection()} room={room} animations={[{ id: 20, kind: "draw", event: {} }, { id: 21, kind: "win", event: {} }]} reducedMotion onAnimationConsumed={consumed} />);
    await waitFor(() => expect(consumed.mock.calls).toEqual([[20], [21]]));
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-last-consumed-animation-id", "21");
  });
  it("does not cancel current motion on a room-only or identical-projection resend", async () => {
    const cancelled = vi.fn(); const state = projection();
    const animations = [{ id: 29, kind: "call" as const, event: {} }];
    const view = render(<ThreeTable projection={state} room={room} animations={animations} onAnimationCancelled={cancelled} />);
    await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-item-id", "29"));
    view.rerender(<ThreeTable projection={structuredClone(state)} room={structuredClone(room)} animations={animations} onAnimationCancelled={cancelled} />);
    expect(cancelled).not.toHaveBeenCalled();
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-item-id", "29");
  });
  it("cancels changed geometry and rejects an old motion completion without consuming its replacement", async () => {
    const consumed = vi.fn(), cancelled = vi.fn(); const state = projection();
    const view = render(<ThreeTable projection={state} room={room} animations={[{ id: 30, kind: "call", event: {} }]} onAnimationConsumed={consumed} onAnimationCancelled={cancelled} />);
    await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-item-id", "30"));
    view.rerender(<ThreeTable projection={{ ...state, remaining_wall: 41 }} room={room} animations={[{ id: 30, kind: "call", event: {} }, { id: 31, kind: "draw", event: {} }]} onAnimationConsumed={consumed} onAnimationCancelled={cancelled} />);
    await waitFor(() => expect(screen.getByTestId("three-table")).toHaveAttribute("data-animation-item-id", "31"));
    act(() => currentWorker().emit({ type: "motion-complete", itemId: 30 }));
    expect(cancelled.mock.calls).toEqual([[30]]); expect(consumed).not.toHaveBeenCalled();
    act(() => currentWorker().emit({ type: "motion-complete", itemId: 31 }));
    expect(consumed.mock.calls).toEqual([[31]]);
  });
  it.each(["missing", "constructor", "fatal", "module-error"])("preserves semantic facts and zero instrumentation on %s failure", async failure => {
    if (failure === "missing") vi.stubGlobal("Worker", undefined);
    if (failure === "constructor") platform.fail = true;
    render(<ThreeTable projection={projection()} room={room} />);
    if (failure === "fatal" || failure === "module-error") {
      await ready();
      act(() => { if (failure === "fatal") currentWorker().emit({ type: "fatal" }); else currentWorker().onerror?.(); });
    }
    const fallback = await screen.findByRole("status", { name: "3D table unavailable" });
    expect(fallback).toHaveTextContent("East 1"); expect(fallback).toHaveTextContent("42 tiles left"); expect(fallback).toHaveTextContent("Dora 1m");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-rendered-tile-count", "0");
  });
  it("leaves synchronization announcements to the gameplay surface", () => {
    render(<ThreeTable projection={null} room={null} />);
    expect(screen.queryByRole("status", { name: "Table synchronization" })).not.toBeInTheDocument();
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
  });
  it("disposes the Window atlas and terminates only after the renderer acknowledges teardown", async () => {
    platform.dispose = false;
    const dispose = vi.spyOn(CanvasTexture.prototype, "dispose");
    const view = render(<ThreeTable projection={projection()} room={room} />);
    await ready(); const worker = currentWorker();
    view.unmount();
    expect(dispose).toHaveBeenCalledTimes(1);
    expect(worker.terminate).not.toHaveBeenCalled();
    act(() => worker.emit({ type: "disposed" }));
    expect(worker.terminate).toHaveBeenCalledTimes(1);
  });
  it("bounds cleanup of an unresponsive renderer at 3000ms without waiting in the UI", async () => {
    platform.dispose = false;
    const view = render(<ThreeTable projection={projection()} room={room} />);
    await ready(); const worker = currentWorker();
    vi.useFakeTimers(); view.unmount();
    act(() => vi.advanceTimersByTime(2999)); expect(worker.terminate).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1)); expect(worker.terminate).toHaveBeenCalledTimes(1);
  });
  it("gives StrictMode fresh canvases and never transfers one twice", async () => {
    render(<StrictMode><ThreeTable projection={projection()} room={room} /></StrictMode>);
    await ready();
    const transfers = vi.mocked(HTMLCanvasElement.prototype.transferControlToOffscreen);
    expect(new Set(transfers.mock.contexts).size).toBe(transfers.mock.calls.length);
    expect(screen.getByTestId("three-table").querySelectorAll("canvas")).toHaveLength(1);
  });
});
