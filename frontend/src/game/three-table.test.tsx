import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectedState, RoomSnapshot } from "./types";
import { CAMERA } from "./three-table-layout";

const canvasCalls = vi.hoisted(() => vi.fn());
const canvasState = vi.hoisted(() => ({ fallback: false }));
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
      canvasCalls(props);
      if (canvasState.fallback) return props.fallback;
      return React.createElement("div", {
        "data-testid": "mock-r3f-canvas",
        "aria-hidden": props["aria-hidden"],
      });
    },
    useThree: (selector: (state: Record<string, unknown>) => unknown) =>
      selector({ invalidate: vi.fn(), camera: { position: { set: vi.fn() }, lookAt: vi.fn() } }),
  };
});

vi.mock("./tile-atlas", () => ({ createTileAtlas }));

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
  createTileAtlas.mockClear();
  atlasDispose.mockClear();
  canvasState.fallback = false;
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
  });

  it("exposes deterministic render instrumentation and concise host semantics", async () => {
    render(<ThreeTable projection={projection()} room={room} />);

    const host = screen.getByTestId("three-table");
    expect(host).toHaveAccessibleName("3D mahjong table");
    await waitFor(() => expect(host).toHaveAttribute("data-render-ready", "true"));
    expect(Number(host.getAttribute("data-rendered-tile-count"))).toBeGreaterThan(0);
    expect(Number(host.getAttribute("data-rendered-scene-primitives"))).toBeGreaterThan(0);
    expect(host).toHaveAttribute("data-wall-tile-count", "42");
    expect(host).toHaveAttribute("data-webgl-fallback", "false");
    expect(host).toHaveAttribute("data-animation-state", "idle");
    expect(host).toHaveAttribute("data-player-frame-count", "4");
  });

  it("keeps semantic match facts when WebGL is unavailable", async () => {
    canvasState.fallback = true;
    render(<ThreeTable projection={projection()} room={room} />);

    const fallback = await screen.findByRole("status", { name: "3D table unavailable" });
    expect(fallback).toHaveTextContent("East 1");
    expect(fallback).toHaveTextContent("42 tiles left");
    expect(fallback).toHaveTextContent("Dora 1m");
    await waitFor(() =>
      expect(screen.getByTestId("three-table")).toHaveAttribute("data-webgl-fallback", "true"),
    );
  });

  it("keeps a quiet synchronization state without a projection", () => {
    render(<ThreeTable projection={null} room={null} />);

    expect(screen.getByRole("status", { name: "Table synchronization" })).toHaveTextContent(
      "Synchronizing table",
    );
    expect(screen.getByTestId("three-table")).toHaveAttribute("data-render-ready", "false");
  });

  it("disposes the shared atlas when the adapter unmounts", async () => {
    const view = render(<ThreeTable projection={projection()} room={room} />);
    await waitFor(() => expect(createTileAtlas).toHaveBeenCalledTimes(1));

    view.unmount();
    expect(atlasDispose).toHaveBeenCalledTimes(1);
  });
});
