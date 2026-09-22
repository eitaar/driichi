/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { Vector3 } from "three";
import {
  BACK_FACE_SIZE,
  FACE_SIZE,
  createBackFaceGeometry,
  createTileSideGeometry,
  tileFaceQuaternion,
} from "./three-table-scene";
import type { ProjectedState } from "./types";
import {
  CAMERA,
  CENTER_DEVICE_AABB,
  LOCAL_TILE_SIZE,
  REMOTE_TILE_SIZE,
  TABLE_SIZE,
  TILE_BODY_HEIGHTS,
  TILE_BODY_SIZE,
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
      position: [0, 12.8, 12.9],
      target: [0, 0.15, 0.38],
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

    expect(BACK_FACE_SIZE).toEqual([0.54, 0.78]);
    expect(backGeometry.parameters.width).toBe(BACK_FACE_SIZE[0]);
    expect(backGeometry.parameters.height).toBe(BACK_FACE_SIZE[1]);
    expect(BACK_FACE_SIZE[0]).toBeLessThan(0.58);
    expect(BACK_FACE_SIZE[1]).toBeLessThan(0.82);

    backGeometry.dispose();
  });

  it("keeps a shared top-rim surface beneath the inset face", () => {
    const bodyGeometry = createTileSideGeometry();
    expect(bodyGeometry.getAttribute("normal")).toBeDefined();
    expect(bodyGeometry.getIndex()?.count).toBe(30);
    bodyGeometry.dispose();
  });

  it("preserves the source aspect ratio and a visible ivory face rim", () => {
    expect(FACE_SIZE[0] / FACE_SIZE[1]).toBeCloseTo(300 / 400, 6);
    expect((TILE_BODY_SIZE.width - FACE_SIZE[0]) / 2).toBeGreaterThan(0.03);
    expect((TILE_BODY_SIZE.depth - FACE_SIZE[1]) / 2).toBeGreaterThan(0.06);
    expect(FACE_SIZE[0]).toBeLessThan(0.58);
    expect(FACE_SIZE[1]).toBeLessThan(0.82);
    expect(TILE_BODY_HEIGHTS).toEqual({ local: 0.18, remote: 0.16 });
    expect(TILE_BODY_SIZE.height).toBe(TILE_BODY_HEIGHTS.local);
  });

  it("orients every visible seat group with glyph tops toward the table center", () => {
    const expected = {
      bottom: [0, 0, -1],
      right: [-1, 0, 0],
      top: [0, 0, 1],
      left: [1, 0, 0],
    } as const;
    const source = projection({
      audience: "replay_admin",
      players: [
        player(0, "bottom", { hand: [0], discards: [1], melds: [{ tiles: [2] }] }),
        player(1, "right", { hand: [3], discards: [4], melds: [{ tiles: [5] }] }),
        player(2, "top", { hand: [6], discards: [7], melds: [{ tiles: [8] }] }),
        player(3, "left", { hand: [9], discards: [10], melds: [{ tiles: [11] }] }),
      ],
    });
    const layout = buildMatchSceneLayout(source, null, "replay");
    const glyphTop = new Vector3(0, 1, 0);
    const normal = new Vector3(0, 0, 1);
    const exactDirection = (value: Vector3): number[] => value.toArray().map((component) => {
      const rounded = Math.round(component * 1e6) / 1e6;
      return Object.is(rounded, -0) ? 0 : rounded;
    });
    for (const [position, vector] of Object.entries(expected) as Array<[
      keyof typeof expected,
      readonly [number, number, number],
    ]>) {
      const tiles = layout.tiles.filter((tile) =>
        tile.key.includes(`-${position}-`) &&
        (tile.group === "hand" || tile.group === "discard" || tile.group === "meld"),
      );
      expect(tiles).toHaveLength(3);
      for (const tile of tiles) {
        const orientation = tileFaceQuaternion(tile.rotation);
        expect(exactDirection(glyphTop.clone().applyQuaternion(orientation))).toEqual(vector);
        expect(exactDirection(normal.clone().applyQuaternion(orientation))).toEqual([0, 1, 0]);
      }
    }
  });

  it("keeps ownerless Dora bottom-readable while preserving its upward normal", () => {
    const layout = buildMatchSceneLayout(projection({ dora_indicators: [16] }), null, "replay");
    const dora = layout.tiles.find(({ group }) => group === "dora");
    expect(dora).toBeDefined();
    const orientation = tileFaceQuaternion(dora!.rotation);
    expect(new Vector3(0, 1, 0).applyQuaternion(orientation).toArray().map((component) => Math.round(component * 1e6) / 1e6)).toEqual([0, 0, -1]);
    expect(new Vector3(0, 0, 1).applyQuaternion(orientation).toArray().map((component) => Math.round(component * 1e6) / 1e6)).toEqual([0, 1, 0]);
  });

  it("lays seat faces flat without twisting their UV axes", () => {
    for (const rotation of [
      [0, 0, 0],
      [0, Math.PI / 2, 0],
      [0, Math.PI, 0],
      [0, -Math.PI / 2, 0],
    ] as const) {
      const orientation = tileFaceQuaternion(rotation);
      expect(new Vector3(0, 0, 1).applyQuaternion(orientation).y).toBeCloseTo(1);
    }
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
    for (const [position, rotation] of [["right", Math.PI / 2], ["left", -Math.PI / 2]] as const) {
      const sideTiles = layout.tiles.filter((tile) => tile.key.includes(`-${position}-`));
      expect(sideTiles.filter(({ group }) => group === "hand").every(({ face }) => face === "back")).toBe(true);
      expect(sideTiles.filter(({ group }) => group === "discard" || group === "meld").every(({ face }) => face === "front")).toBe(true);
      expect(new Set(sideTiles.map(({ rotation: tileRotation }) => tileRotation[1]))).toEqual(new Set([rotation]));
    }
  });

  it.each([
    ["4p-red-east", [0, 1, 2, 3]],
    ["3p-red-east", [0, 1, 2, 3]],
  ] as const)("reveals only authoritative opponent hands on Replay for %s", (mode, seats) => {
    const hands = new Map([
      [0, [0, 16, 52]],
      [1, [3, 17, 53]],
      [2, [6, 18, 88]],
      [3, [9, 19, 89]],
    ]);
    const replayProjection = projection({
      mode,
      audience: "replay_admin",
      players: seats.map((seat) => player(seat, `seat-${seat}`, {
        hand: hands.get(seat),
        concealed_count: hands.get(seat)?.length,
      })).concat(
        mode.startsWith("3p") ? [player(3, "stale-fourth-seat", { hand: hands.get(3), concealed_count: 3 })] : [],
      ),
    });
    const live = buildMatchSceneLayout(replayProjection, null, "live");
    const replay = buildMatchSceneLayout(replayProjection, null, "replay");

    expect(live.players.map(({ seat }) => seat)).toEqual(mode.startsWith("3p") ? [0, 1, 2] : [0, 1, 2, 3]);
    expect(replay.players.map(({ seat }) => seat)).toEqual(live.players.map(({ seat }) => seat));
    expect(live.players.some(({ participantId }) => participantId === "stale-fourth-seat")).toBe(false);
    expect(replay.players.some(({ participantId }) => participantId === "stale-fourth-seat")).toBe(false);

    for (const seat of live.players.map(({ seat: playerSeat }) => playerSeat)) {
      const liveHand = live.tiles.filter((tile) => tile.group === "hand" && tile.key.includes(`seat-${seat}-`));
      const replayHand = replay.tiles.filter((tile) => tile.group === "hand" && tile.key.includes(`seat-${seat}-`));
      expect(liveHand).toHaveLength(hands.get(seat)?.length ?? 0);
      expect(replayHand).toHaveLength(hands.get(seat)?.length ?? 0);
      if (seat === 0) {
        expect(liveHand.every(({ face }) => face === "front")).toBe(true);
      } else {
        expect(liveHand.every(({ face, tile }) => face === "back" && tile === null)).toBe(true);
      }
      expect(replayHand.every(({ face, tile }, index) => face === "front" && tile === hands.get(seat)?.[index])).toBe(true);
      expect(new Set(replayHand.map(({ scale }) => scale))).toEqual(new Set([seat === 0 ? LOCAL_TILE_SIZE : REMOTE_TILE_SIZE]));
    }
  });

  it("keeps absent replay hands concealed instead of reconstructing tile faces", () => {
    const layout = buildMatchSceneLayout(
      projection({
        audience: "replay_admin",
        players: [
          player(0, "local", { hand: [0] }),
          player(1, "missing", { concealed_count: 3 }),
          player(2, "right", { hand: [16] }),
          player(3, "left", { hand: [52] }),
        ],
      }),
      null,
      "replay",
    );
    const missingHand = layout.tiles.filter((tile) => tile.group === "hand" && tile.key.includes("hand-right-seat-1-"));
    expect(missingHand).toHaveLength(3);
    expect(missingHand.every(({ face, tile }) => face === "back" && tile === null)).toBe(true);
  });

  it("keeps replay hand tiles frame-local while stepping events", () => {
    const firstFrame = projection({
      mode: "3p-red-east",
      audience: "replay_admin",
      players: [
        player(0, "local", { hand: [0, 16] }),
        player(1, "right", { hand: [20, 52] }),
        player(2, "left", { hand: [40, 88] }),
      ],
    });
    const secondFrame = {
      ...firstFrame,
      players: firstFrame.players?.map((entry) => entry.seat === 1
        ? { ...entry, hand: [21, 53] }
        : entry),
    };
    const firstLayout = buildMatchSceneLayout(firstFrame, null, "replay");
    const secondLayout = buildMatchSceneLayout(secondFrame, null, "replay");
    expect(firstLayout.tiles.filter((tile) => tile.key.includes("hand-right-seat-1-")).map(({ tile }) => tile)).toEqual([20, 52]);
    expect(secondLayout.tiles.filter((tile) => tile.key.includes("hand-right-seat-1-")).map(({ tile }) => tile)).toEqual([21, 53]);
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
    expect(LOCAL_TILE_SIZE / REMOTE_TILE_SIZE).toBeCloseTo(1.282, 3);
  });

  it("places Dora indicators on the raised center console with local readability", () => {
    const layout = buildMatchSceneLayout(
      projection({ dora_indicators: [4, 8, 12, 16, 20] }),
      null,
    );
    const dora = layout.tiles.filter(({ group }) => group === "dora");
    expect(dora).toHaveLength(5);
    expect(dora.every(({ scale }) => scale === LOCAL_TILE_SIZE)).toBe(true);
    expect(dora.every(({ position }) => position[1] === 0.61 && position[2] === 0.86)).toBe(true);
    expect(dora.map(({ position }) => position[0])).toEqual([-1.28, -0.64, 0, 0.64, 1.28]);
    expect(dora.every(({ position }) => Number.isFinite(position[0]))).toBe(true);
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
