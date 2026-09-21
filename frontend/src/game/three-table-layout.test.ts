/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { Vector3 } from "three";
import {
  BACK_FACE_SIZE,
  createBackFaceGeometry,
  tileFaceQuaternion,
} from "./three-table-scene";
import type { ProjectedState } from "./types";
import {
  CAMERA,
  CENTER_DEVICE_AABB,
  LOCAL_TILE_SIZE,
  REMOTE_TILE_SIZE,
  TABLE_SIZE,
  aabbIntersects,
  buildMatchSceneLayout,
  sceneTileAabb,
} from "./three-table-layout";
import { projectLocalHandHitTargets, projectSceneTileRect } from "./table-hit-targets";

function player(
  seat: number,
  participantId: string,
  overrides: Record<string, unknown> = {},
) {
  return {
    seat,
    participant_id: participantId,
    display_name: participantId,
    ...overrides,
  };
}

function projection(overrides: Partial<ProjectedState> = {}): ProjectedState {
  return {
    mode: "4p-red-east",
    viewer_seat: 0,
    players: [
      player(0, "local", { hand: [1, 2, 3], concealed_count: 3 }),
      player(1, "right", { concealed_count: 3 }),
      player(2, "top", { concealed_count: 3 }),
      player(3, "left", { concealed_count: 3 }),
    ],
    remaining_wall: 0,
    ...overrides,
  };
}

describe("three-dimensional table layout", () => {
  it("exposes the exact table and camera constants", () => {
    expect(TABLE_SIZE).toEqual({ width: 13.6, depth: 11 });
    expect(CAMERA).toEqual({
      fov: 34,
      position: [0, 12.8, 12.3],
      target: [0, 0.15, 1],
      near: 0.1,
      far: 60,
    });
    expect(CAMERA.fov).toBeGreaterThanOrEqual(28);
    expect(CAMERA.fov).toBeLessThanOrEqual(34);
    expect(CAMERA.position[1]).toBeGreaterThanOrEqual(10.8);
    expect(CAMERA.position[1]).toBeLessThanOrEqual(12.8);
    expect(CAMERA.position[2]).toBeGreaterThanOrEqual(11.6);
    expect(CAMERA.position[2]).toBeLessThanOrEqual(13.8);
  });

  it("rejects global scene distortion in the authored render path", () => {
    const sceneSource = readFileSync(resolve(process.cwd(), "src/game/three-table-scene.tsx"), "utf8");
    expect(sceneSource).not.toContain("TABLE_RENDER_SCALE");
    expect(sceneSource).not.toMatch(/<group\s+scale=/);
    expect(sceneSource).not.toContain("setPixelRatio");
  });

  it("keeps concealed backs inset within the unchanged front-face footprint", () => {
    const backGeometry = createBackFaceGeometry();

    expect(BACK_FACE_SIZE).toEqual([0.48, 0.7]);
    expect(backGeometry.parameters.width).toBe(BACK_FACE_SIZE[0]);
    expect(backGeometry.parameters.height).toBe(BACK_FACE_SIZE[1]);
    expect(BACK_FACE_SIZE[0]).toBeLessThan(0.56);
    expect(BACK_FACE_SIZE[1]).toBeLessThan(0.78);

    backGeometry.dispose();
  });

  it("lays side-seat face and back planes flat without twisting their UV axes", () => {
    const right = tileFaceQuaternion([0, -Math.PI / 2, 0]);
    const left = tileFaceQuaternion([0, Math.PI / 2, 0]);
    const normal = new Vector3(0, 0, 1);
    expect(normal.clone().applyQuaternion(right).y).toBeCloseTo(1);
    expect(normal.clone().applyQuaternion(left).y).toBeCloseTo(1);
    expect(new Vector3(1, 0, 0).applyQuaternion(right).z).toBeCloseTo(1);
    expect(new Vector3(1, 0, 0).applyQuaternion(left).z).toBeCloseTo(-1);
    expect(new Vector3(0, 1, 0).applyQuaternion(right).x).toBeCloseTo(1);
    expect(new Vector3(0, 1, 0).applyQuaternion(left).x).toBeCloseTo(-1);
    expect(new Vector3(1, 0, 0).applyQuaternion(tileFaceQuaternion([0, 0, 0])).x).toBeCloseTo(1);
    expect(new Vector3(1, 0, 0).applyQuaternion(tileFaceQuaternion([0, Math.PI, 0])).x).toBeCloseTo(-1);
  });

  it("keeps right and left hand backs and front tiles on the same seat frame", () => {
    const layout = buildMatchSceneLayout(
      projection({
        players: [
          player(0, "local", { hand: [1] }),
          player(1, "right", { concealed_count: 2, discards: [3], melds: [{ tiles: [4, 5, 6] }] }),
          player(2, "top", { concealed_count: 2 }),
          player(3, "left", { concealed_count: 2, discards: [7], melds: [{ tiles: [8, 9, 10] }] }),
        ],
      }),
      null,
    );
    for (const [position, rotation] of [["right", -Math.PI / 2], ["left", Math.PI / 2]] as const) {
      const sideTiles = layout.tiles.filter((tile) => tile.key.includes(`-${position}-`));
      expect(sideTiles.filter(({ group }) => group === "hand").every(({ face }) => face === "back")).toBe(true);
      expect(sideTiles.filter(({ group }) => group === "discard" || group === "meld").every(({ face }) => face === "front")).toBe(true);
      expect(new Set(sideTiles.map(({ rotation: tileRotation }) => tileRotation[1]))).toEqual(new Set([rotation]));
    }
  });

  it("keeps only the oriented three-player seats", () => {
    const layout = buildMatchSceneLayout(
      projection({
        mode: "3p-red-east",
        players: [
          player(0, "zero", { hand: [1] }),
          player(1, "one", { concealed_count: 1 }),
          player(2, "two", { concealed_count: 1 }),
          player(3, "stale", { concealed_count: 1 }),
        ],
      }),
      null,
    );

    expect(layout.players.map(({ seat, position }) => [seat, position])).toEqual([
      [0, "bottom"],
      [1, "right"],
      [2, "left"],
    ]);
    expect(layout.players.some(({ position }) => position === "top")).toBe(false);
    expect(layout.players.map(({ participantId }) => participantId)).not.toContain("stale");
  });

  it("uses local scale for the bottom hand and remote scale for opponents", () => {
    expect(LOCAL_TILE_SIZE / REMOTE_TILE_SIZE).toBeGreaterThanOrEqual(1.28);
    const layout = buildMatchSceneLayout(projection(), null);
    const localHand = layout.tiles.filter((tile) => tile.group === "hand" && tile.face === "front");
    const opponentHands = layout.tiles.filter(
      (tile) => tile.group === "hand" && tile.face === "back",
    );
    expect(localHand.length).toBe(3);
    expect(new Set(localHand.map(({ scale }) => scale))).toEqual(new Set([LOCAL_TILE_SIZE]));
    expect(opponentHands.length).toBe(9);
    expect(new Set(opponentHands.map(({ scale }) => scale))).toEqual(new Set([REMOTE_TILE_SIZE]));
  });

  it("normalizes malformed remaining wall values and creates matching instances", () => {
    for (const [remainingWall, expected] of [
      [-1, 0],
      [Number.NaN, 0],
      [999, 136],
    ] as const) {
      const layout = buildMatchSceneLayout(projection({ remaining_wall: remainingWall }), null);
      expect(layout.wallCount).toBe(expected);
      expect(layout.tiles.filter(({ group }) => group === "wall")).toHaveLength(expected);
    }
  });

  it("creates one wall instance for each array entry", () => {
    const layout = buildMatchSceneLayout(
      projection({ remaining_wall: [1, 2, 3] }),
      null,
    );
    expect(layout.wallCount).toBe(3);
    expect(layout.tiles.filter(({ group }) => group === "wall")).toHaveLength(3);
  });

  it("is deterministic with unique keys and finite transforms", () => {
    const input = projection({
      remaining_wall: 17,
      dora_indicators: [4, 8],
      players: [
        player(0, "local", {
          hand: Array.from({ length: 14 }, (_, index) => index),
          discards: [1, 2, 3, 4, 5, 6, 7],
          melds: [{ tiles: [21, 22, 23] }],
        }),
        player(1, "right", {
          concealed_count: 3,
          discards: [8],
          melds: [{ tiles: [24, 25, 26] }],
        }),
        player(2, "top", { concealed_count: 2 }),
        player(3, "left", { concealed_count: 1 }),
      ],
    });
    const first = buildMatchSceneLayout(input, null);
    const second = buildMatchSceneLayout(input, null);
    expect(second).toEqual(first);
    expect(new Set(first.tiles.map(({ key }) => key)).size).toBe(first.tiles.length);
    for (const tile of first.tiles) {
      expect(tile.position.every(Number.isFinite)).toBe(true);
      expect(tile.rotation.every(Number.isFinite)).toBe(true);
      expect(Number.isFinite(tile.scale)).toBe(true);
    }
  });

  it("separates the final draw tile in a fourteen-tile local hand", () => {
    const layout = buildMatchSceneLayout(
      projection({
        players: [
          player(0, "local", { hand: Array.from({ length: 14 }, (_, index) => index) }),
        ],
      }),
      null,
    );
    const hand = layout.tiles.filter(({ group }) => group === "hand");
    const gaps = hand.slice(1).map((tile, index) => tile.position[0] - hand[index].position[0]);
    expect(gaps.slice(0, -1).every((gap) => Math.abs(gap - gaps[0]) < 1e-9)).toBe(true);
    expect(gaps.at(-1)).toBeGreaterThan(gaps[0]);
  });

  it("wraps a seven-discard river after six tiles", () => {
    const layout = buildMatchSceneLayout(
      projection({
        players: [player(0, "local", { hand: [], discards: [1, 2, 3, 4, 5, 6, 7] })],
      }),
      null,
    );
    const discards = layout.tiles.filter(({ group }) => group === "discard");
    expect(discards).toHaveLength(7);
    expect(discards.slice(0, 6).every(({ position }) => position[2] === 1.73)).toBe(true);
    expect(discards[6]?.position[2]).toBe(2.39);
  });

  it("projects every reduced local hand to the exact rendered tile extent", () => {
    for (const mode of ["3p-red-east", "4p-red-east"] as const) {
      const seats = mode.startsWith("3p") ? 3 : 4;
      for (let viewerSeat = 0; viewerSeat < seats; viewerSeat += 1) {
        for (let calls = 0; calls <= 4; calls += 1) {
          const hand = Array.from(
            { length: 14 - calls * 3 },
            (_, index) => [0, 1, 16, 17, 52, 53, 88][index % 7] + Math.floor(index / 7) * 4,
          );
          const players = Array.from({ length: seats }, (_, seat) =>
            player(seat, `seat-${seat}`, {
              hand: seat === viewerSeat ? hand : undefined,
              concealed_count: seat === viewerSeat ? hand.length : 13,
              melds: seat === viewerSeat
                ? Array.from({ length: calls }, (_, meldIndex) => ({
                    tiles: [meldIndex * 4, meldIndex * 4 + 1, meldIndex * 4 + 2],
                  }))
                : [],
            }),
          );
          const layout = buildMatchSceneLayout(
            projection({ mode, viewer_seat: viewerSeat, players }),
            null,
          );
          const rendered = layout.tiles.filter(
            (tile) => tile.group === "hand" && tile.key.includes("hand-bottom-seat-"),
          );
          const projected = projectLocalHandHitTargets(layout);
          expect(rendered).toHaveLength(hand.length);
          expect(projected.targets).toHaveLength(hand.length);
          projected.targets.forEach((target, index) => {
            expect(target.tileKey).toBe(rendered[index]?.key);
            expect(target.rect).toEqual(projectSceneTileRect(rendered[index]!));
          });
        }
      }
    }
  });

  it("keeps maximum rivers and called melds disjoint from one another and the center device", () => {
    for (const mode of ["3p-red-east", "4p-red-east"] as const) {
      const seats = mode.startsWith("3p") ? 3 : 4;
      for (let viewerSeat = 0; viewerSeat < seats; viewerSeat += 1) {
        const players = Array.from({ length: seats }, (_, seat) =>
          player(seat, `seat-${seat}`, {
            hand: seat === viewerSeat ? [0, 1] : Array.from({ length: 13 }, (_, index) => index + 20),
            concealed_count: seat === viewerSeat ? 2 : 13,
            discards: Array.from({ length: 24 }, (_, index) => index + seat * 24),
            melds: Array.from({ length: 4 }, (_, meldIndex) => ({
              tiles: Array.from({ length: 4 }, (_, tileIndex) => 100 + seat * 16 + meldIndex * 4 + tileIndex),
            })),
          }),
        );
        const layout = buildMatchSceneLayout(
          projection({ mode, viewer_seat: viewerSeat, players }),
          null,
        );
        for (const scenePlayer of layout.players) {
          const discards = layout.tiles.filter(
            (tile) => tile.group === "discard" && tile.key.includes(`-${scenePlayer.position}-seat-${scenePlayer.seat}-`),
          );
          const melds = layout.tiles.filter(
            (tile) => tile.group === "meld" && tile.key.includes(`-${scenePlayer.position}-seat-${scenePlayer.seat}-`),
          );
          for (const discard of discards) {
            expect(aabbIntersects(sceneTileAabb(discard), CENTER_DEVICE_AABB)).toBe(false);
            for (const meld of melds) {
              expect(aabbIntersects(sceneTileAabb(discard), sceneTileAabb(meld))).toBe(false);
            }
          }
          for (const meld of melds) {
            expect(aabbIntersects(sceneTileAabb(meld), CENTER_DEVICE_AABB)).toBe(false);
          }
        }
      }
    }
  });
});
