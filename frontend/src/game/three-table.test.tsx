import { act, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectedState, RoomSnapshot } from "./types";
import { CAMERA } from "./three-table-layout";

const canvasCalls = vi.hoisted(() => vi.fn());
const sceneCalls = vi.hoisted(() => vi.fn());
const canvasState = vi.hoisted(() => ({ fallback: false, throwOnRender: false, autoCreate: true }));
const sceneState = vi.hoisted(() => ({ throwOnRender: false, autoReady: true }));
const atlasDispose = vi.hoisted(() => vi.fn());
const createTileAtlas = vi.hoisted(() =>
  vi.fn(async () => ({
    texture: {},
    columns: 8,
    rows: 5,
    cellFor: () => [0, 0] as const,
    dispose: atlasDispose,
  })),
);

vi.mock("@react-three/fiber", async () => {
  const React = await import("react");
  return {
    Canvas: (props: Record<string, unknown>) => {
      const canvasRef = React.useRef<HTMLCanvasElement>(null);
      canvasCalls(props);
      React.useLayoutEffect(() => {
        if (!canvasState.autoCreate || !canvasRef.current) return;
        (props.onCreated as ((state: Record<string, unknown>) => void) | undefined)?.({
          gl: {
            domElement: canvasRef.current,
            outputColorSpace: "",
            shadowMap: { enabled: false, type: 0 },
          },
        });
      }, []);
      if (canvasState.throwOnRender) throw new Error("renderer creation failed");
      if (canvasState.fallback) return props.fallback;
      return React.createElement(
        "div",
        { "data-testid": "mock-r3f-root" },
        React.createElement("canvas", {
          ref: canvasRef,
          "data-testid": "mock-r3f-canvas",
          "aria-hidden": props["aria-hidden"],
        }),
        props.children as ReactNode,
      );
    },
    useThree: (selector: (state: Record<string, unknown>) => unknown) =>
      selector({ invalidate: vi.fn(), camera: { position: { set: vi.fn() }, lookAt: vi.fn() } }),
  };
});

vi.mock("./tile-atlas", () => ({ createTileAtlas }));
vi.mock("./three-table-scene", async () => {
  const React = await import("react");
  return {
    MatchTableScene: (props: Record<string, unknown>) => {
      sceneCalls(props);
      React.useLayoutEffect(() => {
        if (sceneState.autoReady) {
          (props.onRenderReady as ((stats: Record<string, number>) => void) | undefined)?.({
            tileCount: 54,
            primitiveCount: 9,
            pixelRatio: 1,
            triangleCount: 100,
          });
        }
      }, [props.layout]);
      if (sceneState.throwOnRender) throw new Error("scene creation failed");
      return React.createElement("div", { "data-testid": "mock-table-scene" });
    },
  };
});

import { ThreeTable } from "./three-table";

function player(seat: number, participantId: string, extra: Record<string, unknown> = {}) {
  return {
    seat,
    participant_id: participantId,
    display_name: participantId,
    ...extra,
  };
}

function projection(overrides: Partial<ProjectedState> = {}): ProjectedState {
  return {
    mode: "4p-red-east",
    audience: "player",
    viewer_seat: 0,
    round: "East",
    kyoku: 1,
    remaining_wall: 42,
    dora_indicators: [0],
    players: [
      player(0, "local", { hand: [0, 1, 2] }),
      player(1, "right", { concealed_count: 3 }),
      player(2, "top", { concealed_count: 3 }),
      player(3, "left", { concealed_count: 3 }),
    ],
    ...overrides,
  };
}

const room = {
  participants: [],
  match_players: [],
  roster: [],
} as unknown as RoomSnapshot;

beforeEach(() => {
  canvasCalls.mockClear();
  sceneCalls.mockClear();
  createTileAtlas.mockClear();
  atlasDispose.mockClear();
  canvasState.fallback = false;
  canvasState.throwOnRender = false;
  canvasState.autoCreate = true;
  sceneState.throwOnRender = false;
  sceneState.autoReady = true;
  vi.stubGlobal("WebGLRenderingContext", class WebGLRenderingContext {});
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(
    () => ({ getExtension: () => null }) as never,
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ThreeTable", () => {
  it("uses demand rendering, bounded DPR, and the fixed camera", async () => {
    render(<ThreeTable projection={projection()} room={room} />);

    await waitFor(() => expect(canvasCalls).toHaveBeenCalled());
    const props = canvasCalls.mock.lastCall?.[0] as Record<string, unknown>;
    expect(props.frameloop).toBe("demand");
    expect(props.dpr).toEqual([1, 1.5]);
    expect(props.camera).toEqual({
      fov: CAMERA.fov,
      position: CAMERA.position,
      near: CAMERA.near,
      far: CAMERA.far,
    });
    expect(screen.getByTestId("mock-r3f-canvas")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("three-table").querySelector(".table-player-overlays"))
      .toHaveAttribute("aria-hidden", "true");
  });

  it("reports renderer-backed instrumentation only after Canvas and the scene render", async () => {
    canvasState.autoCreate = false;
    sceneState.autoReady = false;
    render(<ThreeTable projection={projection()} room={room} />);

    const host = screen.getByTestId("three-table");
    expect(host).toHaveAccessibleName("3D mahjong table");
    await waitFor(() => expect(canvasCalls).toHaveBeenCalled());
    expect(host).toHaveAttribute("data-render-ready", "false");
    expect(host).toHaveAttribute("data-rendered-tile-count", "0");
    expect(host).toHaveAttribute("data-rendered-scene-primitives", "0");

    const canvasProps = canvasCalls.mock.lastCall?.[0] as {
      onCreated(state: Record<string, unknown>): void;
    };
    const canvas = screen.getByTestId("mock-r3f-canvas") as HTMLCanvasElement;
    act(() => canvasProps.onCreated({
      gl: { domElement: canvas, outputColorSpace: "", shadowMap: { enabled: false, type: 0 } },
    }));
    const sceneProps = sceneCalls.mock.lastCall?.[0] as {
      onRenderReady(stats: { tileCount: number; primitiveCount: number; pixelRatio: number; triangleCount: number }): void;
    };
    act(() => sceneProps.onRenderReady({ tileCount: 54, primitiveCount: 9, pixelRatio: 1, triangleCount: 100 }));

    await waitFor(() => expect(host).toHaveAttribute("data-render-ready", "true"));
    expect(host).toHaveAttribute("data-rendered-tile-count", "54");
    expect(host).toHaveAttribute("data-rendered-scene-primitives", "9");
    expect(host).toHaveAttribute("data-wall-tile-count", "42");
    expect(host).toHaveAttribute("data-webgl-fallback", "false");
    expect(host).toHaveAttribute("data-animation-state", "idle");
    expect(host).toHaveAttribute("data-renderer-pixel-ratio", "1");
    expect(host).toHaveAttribute("data-player-frame-count", "4");
  });

  it("runs one motion to idle and reports its exact ID once", async () => {
    const onAnimationConsumed = vi.fn();
    render(
      <ThreeTable
        projection={projection()}
        room={room}
        animations={[{ id: 12, kind: "discard", event: { type: "dahai" } }]}
        onAnimationConsumed={onAnimationConsumed}
      />,
    );

    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-animation-state", "active"));
    expect(host).toHaveAttribute("data-animation-item-id", "12");
    const activeScene = sceneCalls.mock.lastCall?.[0] as {
      onMotionComplete(id: number): void;
      onMotionFrame(now: number, pixelRatio: number): void;
    };
    act(() => activeScene.onMotionFrame(100, 1.25));
    expect(host).toHaveAttribute("data-renderer-pixel-ratio", "1.25");
    act(() => activeScene.onMotionComplete(12));

    await waitFor(() => expect(host).toHaveAttribute("data-animation-state", "idle"));
    expect(host).toHaveAttribute("data-renderer-pixel-ratio", "1");
    expect(host).toHaveAttribute("data-last-consumed-animation-id", "12");
    expect(onAnimationConsumed).toHaveBeenCalledTimes(1);
    expect(onAnimationConsumed).toHaveBeenCalledWith(12);
    act(() => activeScene.onMotionComplete(12));
    expect(onAnimationConsumed).toHaveBeenCalledTimes(1);
  });

  it("consumes the active ID exactly once when Reduced Motion switches on", async () => {
    const onAnimationConsumed = vi.fn();
    const animation = { id: 19, kind: "discard" as const, event: { type: "dahai" } };
    const currentProjection = projection();
    const view = render(
      <ThreeTable
        projection={currentProjection}
        room={room}
        animations={[animation]}
        onAnimationConsumed={onAnimationConsumed}
      />,
    );
    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-animation-item-id", "19"));
    const activeScene = sceneCalls.mock.lastCall?.[0] as {
      onMotionComplete(id: number): void;
    };

    view.rerender(
      <ThreeTable
        projection={currentProjection}
        room={room}
        animations={[animation]}
        reducedMotion
        onAnimationConsumed={onAnimationConsumed}
      />,
    );

    await waitFor(() => expect(onAnimationConsumed).toHaveBeenCalledWith(19));
    expect(onAnimationConsumed).toHaveBeenCalledTimes(1);
    expect(host).toHaveAttribute("data-animation-state", "static");
    act(() => activeScene.onMotionComplete(19));
    expect(onAnimationConsumed).toHaveBeenCalledTimes(1);
  });

  it("applies Reduced Motion immediately and consumes each exact ID once", async () => {
    const onAnimationConsumed = vi.fn();
    const animations = [
      { id: 20, kind: "draw", event: { type: "tsumo" } },
      { id: 21, kind: "win", event: { type: "hora" } },
    ] as const;
    render(
      <ThreeTable
        projection={projection()}
        room={room}
        animations={[...animations]}
        reducedMotion
        onAnimationConsumed={onAnimationConsumed}
      />,
    );

    const host = screen.getByTestId("three-table");
    expect(host).toHaveAttribute("data-animation-state", "static");
    await waitFor(() => expect(onAnimationConsumed).toHaveBeenCalledTimes(2));
    expect(onAnimationConsumed.mock.calls).toEqual([[20], [21]]);
    expect(host).toHaveAttribute("data-last-consumed-animation-id", "21");
    expect(sceneCalls.mock.lastCall?.[0]).toMatchObject({ motion: null });
  });

  it("does not cancel active motion for a room-only rerender", async () => {
    const onAnimationCancelled = vi.fn();
    const currentProjection = projection();
    const view = render(
      <ThreeTable
        projection={currentProjection}
        room={room}
        animations={[{ id: 29, kind: "call", event: { type: "pon" } }]}
        onAnimationCancelled={onAnimationCancelled}
      />,
    );
    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-animation-item-id", "29"));

    view.rerender(
      <ThreeTable
        projection={currentProjection}
        room={{ ...room, revision: 2 } as RoomSnapshot}
        animations={[{ id: 29, kind: "call", event: { type: "pon" } }]}
        onAnimationCancelled={onAnimationCancelled}
      />,
    );

    expect(host).toHaveAttribute("data-animation-item-id", "29");
    expect(onAnimationCancelled).not.toHaveBeenCalled();
  });

  it("cancels replaced motion and consumes its replacement exactly once", async () => {
    const onAnimationConsumed = vi.fn();
    const onAnimationCancelled = vi.fn();
    const view = render(
      <ThreeTable
        projection={projection()}
        room={room}
        animations={[{ id: 30, kind: "call", event: { type: "pon" } }]}
        onAnimationConsumed={onAnimationConsumed}
        onAnimationCancelled={onAnimationCancelled}
      />,
    );
    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-animation-item-id", "30"));
    const staleScene = sceneCalls.mock.lastCall?.[0] as {
      onMotionComplete(id: number): void;
    };

    view.rerender(
      <ThreeTable
        projection={projection({ remaining_wall: 41 })}
        room={room}
        animations={[
          { id: 30, kind: "call", event: { type: "pon" } },
          { id: 31, kind: "draw", event: { type: "tsumo" } },
        ]}
        onAnimationConsumed={onAnimationConsumed}
        onAnimationCancelled={onAnimationCancelled}
      />,
    );
    await waitFor(() => expect(host).toHaveAttribute("data-animation-item-id", "31"));
    expect(onAnimationCancelled).toHaveBeenCalledTimes(1);
    expect(onAnimationCancelled).toHaveBeenCalledWith(30);
    act(() => staleScene.onMotionComplete(30));
    expect(onAnimationConsumed).not.toHaveBeenCalled();
    expect(onAnimationCancelled).toHaveBeenCalledTimes(1);

    const replacementScene = sceneCalls.mock.lastCall?.[0] as {
      onMotionComplete(id: number): void;
    };
    act(() => replacementScene.onMotionComplete(31));
    await waitFor(() => expect(onAnimationConsumed).toHaveBeenCalledWith(31));
    expect(onAnimationConsumed).toHaveBeenCalledTimes(1);
    act(() => replacementScene.onMotionComplete(31));
    expect(onAnimationConsumed).toHaveBeenCalledTimes(1);
    view.unmount();
  });

  it("uses the semantic fallback when the WebGL global is missing", async () => {
    vi.unstubAllGlobals();
    render(<ThreeTable projection={projection()} room={room} />);

    await waitFor(() =>
      expect(screen.getByTestId("three-table")).toHaveAttribute("data-webgl-fallback", "true"),
    );
    expect(screen.getByRole("status", { name: "3D table unavailable" })).toBeVisible();
    expect(canvasCalls).not.toHaveBeenCalled();
  });

  it("keeps semantic match facts and zero instrumentation when WebGL is unavailable", async () => {
    canvasState.fallback = true;
    render(<ThreeTable projection={projection()} room={room} />);

    const fallback = await screen.findByRole("status", { name: "3D table unavailable" });
    expect(fallback).toHaveTextContent("East 1");
    expect(fallback).toHaveTextContent("42 tiles left");
    expect(fallback).toHaveTextContent("Dora 1m");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-rendered-tile-count", "0");
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-rendered-scene-primitives", "0");
  });

  it("falls back with zero instrumentation when Canvas creation throws", async () => {
    canvasState.throwOnRender = true;
    render(<ThreeTable projection={projection()} room={room} />);

    const fallback = await screen.findByRole("status", { name: "3D table unavailable" });
    expect(fallback).toBeVisible();
    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-webgl-fallback", "true"));
    expect(host).toHaveAttribute("data-render-ready", "false");
    expect(host).toHaveAttribute("data-rendered-tile-count", "0");
    expect(host).toHaveAttribute("data-rendered-scene-primitives", "0");
  });

  it("falls back with zero instrumentation after WebGL context loss", async () => {
    render(<ThreeTable projection={projection()} room={room} />);
    const host = screen.getByTestId("three-table");
    await waitFor(() => expect(host).toHaveAttribute("data-render-ready", "true"));

    act(() => {
      screen.getByTestId("mock-r3f-canvas").dispatchEvent(
        new Event("webglcontextlost", { bubbles: false, cancelable: true }),
      );
    });

    await waitFor(() => expect(host).toHaveAttribute("data-webgl-fallback", "true"));
    expect(screen.getByRole("status", { name: "3D table unavailable" })).toBeVisible();
    expect(host).toHaveAttribute("data-render-ready", "false");
    expect(host).toHaveAttribute("data-rendered-tile-count", "0");
    expect(host).toHaveAttribute("data-rendered-scene-primitives", "0");
  });

  it("leaves synchronization announcements to the gameplay surface", () => {
    render(<ThreeTable projection={null} room={null} />);

    expect(screen.queryByRole("status", { name: "Table synchronization" })).not.toBeInTheDocument();
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
  });

  it("disposes the shared atlas when the adapter unmounts", async () => {
    const view = render(<ThreeTable projection={projection()} room={room} />);
    await waitFor(() => expect(createTileAtlas).toHaveBeenCalledTimes(1));

    view.unmount();
    expect(atlasDispose).toHaveBeenCalledTimes(1);
  });
});
