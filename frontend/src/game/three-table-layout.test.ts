/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { Box3, Vector3 } from "three";
import {
  BACK_FACE_SIZE,
  FACE_SIZE,
  createBackFaceGeometry,
  createTileSideGeometry,
  instanceMatrix,
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
  it("keeps the authored table and fixed camera inside the viewing envelope", () => {
    expect(TABLE_SIZE.width).toBeGreaterThan(TABLE_SIZE.depth);
    expect(CAMERA.fov).toBeGreaterThanOrEqual(28);
    expect(CAMERA.fov).toBeLessThanOrEqual(34);
    expect(CAMERA.position[1]).toBeGreaterThanOrEqual(10.8);
    expect(CAMERA.position[1]).toBeLessThanOrEqual(12.8);
    expect(CAMERA.position[2]).toBeGreaterThanOrEqual(11.6);
    expect(CAMERA.position[2]).toBeLessThanOrEqual(13.8);
    expect(CAMERA.near).toBeLessThan(CAMERA.far);
  });

  it("rejects global scene distortion in the authored render path", () => {
    const sceneSource = readFileSync(resolve(process.cwd(), "src/game/three-table-scene.tsx"), "utf8");
    expect(sceneSource).not.toContain("TABLE_RENDER_SCALE");
    expect(sceneSource).not.toMatch(/<group\s+scale=/);
    expect(sceneSource).not.toContain("setPixelRatio");
  });

  it("joins the rounded ceramic body and ochre back within the approved tile footprint", () => {
    const body = createTileSideGeometry();
    const back = createBackFaceGeometry();
    body.computeBoundingBox();
    back.computeBoundingBox();
    expect(BACK_FACE_SIZE).toEqual([TILE_BODY_SIZE.width, TILE_BODY_SIZE.depth]);
    for (const geometry of [body, back]) {
      expect(geometry.boundingBox!.max.x - geometry.boundingBox!.min.x).toBeCloseTo(TILE_BODY_SIZE.width);
      expect(geometry.boundingBox!.max.z - geometry.boundingBox!.min.z).toBeCloseTo(TILE_BODY_SIZE.depth);
      const normals = geometry.getAttribute("normal").array;
      expect(Array.from(normals).some((value) => Math.abs(value) > 0.01 && Math.abs(value) < 0.99)).toBe(true);
    }
    expect(body.boundingBox!.max.y).toBeCloseTo(TILE_BODY_SIZE.height / 2);
    expect(body.boundingBox!.min.y).toBeLessThan(back.boundingBox!.max.y);
    expect(back.boundingBox!.min.y).toBeCloseTo(-TILE_BODY_SIZE.height / 2);
    expect(back.boundingBox!.max.y).toBeCloseTo(-TILE_BODY_SIZE.height / 4);
    body.dispose();
    back.dispose();
  });

  it("keeps the face aligned with the authored tile body", () => {
    expect(FACE_SIZE).toEqual([TILE_BODY_SIZE.width, TILE_BODY_SIZE.depth]);
    expect(TILE_BODY_HEIGHTS.local).toBeGreaterThanOrEqual(0.18);
    expect(TILE_BODY_HEIGHTS.remote).toBeGreaterThanOrEqual(0.16);
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
        (tile.group === "discard" || tile.group === "meld"),
      );
      expect(tiles).toHaveLength(2);
      for (const tile of tiles) {
        const orientation = tileFaceQuaternion(tile.rotation);
        expect(exactDirection(glyphTop.clone().applyQuaternion(orientation))).toEqual(vector);
        expect(exactDirection(normal.clone().applyQuaternion(orientation))).toEqual([0, 1, 0]);
      }
    }
  });

  it("keeps the public dead-wall indicator face upward", () => {
    const layout = buildMatchSceneLayout(projection({ dora_indicators: [16] }), null, "replay");
    const dora = layout.tiles.find(({ group }) => group === "dora");
    expect(dora).toBeDefined();
    const orientation = tileFaceQuaternion(dora!.rotation);
    expect(new Vector3(0, 0, 1).applyQuaternion(orientation).y).toBeCloseTo(1);
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
      expect(new Set(sideTiles.filter(({ group }) => group !== "hand").map(({ rotation: tileRotation }) => tileRotation[1]))).toEqual(new Set([rotation]));
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
      if (seat === 0) {
        // The local hand is a DOM control on both live and replay surfaces.
        expect(liveHand).toHaveLength(0);
        expect(replayHand).toHaveLength(0);
        continue;
      }
      expect(liveHand).toHaveLength(hands.get(seat)?.length ?? 0);
      expect(replayHand).toHaveLength(hands.get(seat)?.length ?? 0);
      expect(liveHand.every(({ face, tile }) => face === "back" && tile === null)).toBe(true);
      expect(replayHand.every(({ face, tile }, index) => face === "front" && tile === hands.get(seat)?.[index])).toBe(true);
      expect(new Set(replayHand.map(({ scale }) => scale))).toEqual(new Set([REMOTE_TILE_SIZE]));
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

  it("renders opponent backs in 3D while leaving the local hand to the DOM", () => {
    const layout = buildMatchSceneLayout(projection(), null);
    expect(layout.tiles.filter((tile) => tile.group === "hand" && tile.face === "front")).toHaveLength(0);
    const opponentHands = layout.tiles.filter((tile) => tile.group === "hand" && tile.face === "back");
    expect(opponentHands).toHaveLength(9);
    expect(new Set(opponentHands.map(({ scale }) => scale))).toEqual(new Set([REMOTE_TILE_SIZE]));
  });

  it("reveals only public Dora in the seven dead-wall stacks without adding physical tiles", () => {
    const source = projection({ remaining_wall: 63, dora_indicators: [4, 8, 12, 16, 20] });
    const layout = buildMatchSceneLayout(source, null);
    const dora = layout.tiles.filter(({ group }) => group === "dora").sort((a, b) => a.key.localeCompare(b.key));
    expect(dora.map(({ tile }) => tile)).toEqual(source.dora_indicators);
    expect(dora[0].position[0]).toBe(-4.25);
    expect(dora[0].position[1]).toBe(0.45);
    expect(dora[0].position[2]).toBeCloseTo(2.686);
    const dead = layout.tiles.filter(({ key, group }) => key.startsWith("dead-wall-") || group === "dora");
    expect(dead).toHaveLength(14);
    expect(new Set(dead.map(({ position }) => `${position[0]}:${position[2]}`)).size).toBe(7);
    expect(layout.tiles.filter(({ key }) => key.startsWith("wall-")).some(({ key }) => key.includes("-bottom-"))).toBe(false);
  });

  it.each(["3p-red-east", "4p-red-east"] as const)("shows one partial live stack at most and seven separate dead-wall stacks in %s", (mode) => {
    for (const remaining of [0, 1, 7, 35, 69]) {
      const layout = buildMatchSceneLayout(projection({ mode, remaining_wall: remaining }), null);
      const live = layout.tiles.filter(({ key }) => key.startsWith("wall-"));
      const dead = layout.tiles.filter(({ key }) => key.startsWith("dead-wall-"));
      expect(live).toHaveLength(remaining);
      expect(dead).toHaveLength(14);
      const stacks = new Map<string, number[]>();
      for (const tile of live) {
        const key = `${tile.position[0]}:${tile.position[2]}`;
        stacks.set(key, [...(stacks.get(key) ?? []), tile.position[1]]);
      }
      expect([...stacks.values()].filter((heights) => heights.length === 1)).toHaveLength(remaining % 2);
      for (const heights of stacks.values()) {
        expect([...heights].sort()).toEqual(heights.length === 1 ? [0.24] : [0.24, 0.45]);
      }
      expect(new Set(dead.map(({ position }) => `${position[0]}:${position[2]}`)).size).toBe(7);
      if (mode.startsWith("3p")) expect([...live, ...dead].some(({ key }) => key.includes("-top-"))).toBe(false);
      expect([...live, ...dead].every(({ tile, face }) => tile === null && face === "back")).toBe(true);
      for (const tile of [...live, ...dead]) {
        const matrix = instanceMatrix(tile, true);
        expect(new Vector3(0, 0, 1).transformDirection(matrix).y).toBeCloseTo(1);
        expect(new Vector3().applyMatrix4(matrix).y).toBeGreaterThan(tile.position[1]);
      }
    }
  });

  it.each(["ThreePlayerRedEast", "ThreePlayerRedHalf"])("uses the three-seat wall capacity for canonical mode %s", (mode) => {
    const layout = buildMatchSceneLayout(projection({ mode, remaining_wall: 999 }), null);
    expect(layout.wallCount).toBe(94);
    expect(layout.tiles.filter(({ group }) => group === "wall")).toHaveLength(108);
    expect(layout.tiles.filter(({ group }) => group === "wall").some(({ key }) => key.includes("-top-"))).toBe(false);
  });

  it("normalizes malformed remaining wall values and creates matching instances", () => {
    for (const [remainingWall, expected] of [
      [-1, 0],
      [Number.NaN, 0],
      [999, 122],
    ] as const) {
      const layout = buildMatchSceneLayout(projection({ remaining_wall: remainingWall }), null);
      expect(layout.wallCount).toBe(expected);
      expect(layout.tiles.filter(({ key }) => key.startsWith("wall-"))).toHaveLength(expected);
    }
  });

  it("creates one wall instance for each array entry", () => {
    const layout = buildMatchSceneLayout(
      projection({ remaining_wall: [1, 2, 3] }),
      null,
    );
    expect(layout.wallCount).toBe(3);
    expect(layout.tiles.filter(({ key }) => key.startsWith("wall-"))).toHaveLength(3);
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

  it("does not duplicate the local hand on the 3D table", () => {
    const layout = buildMatchSceneLayout(projection({ players: [player(0, "local", { hand: Array.from({ length: 14 }, (_, index) => index) })] }), null);
    expect(layout.tiles.filter(({ group }) => group === "hand")).toHaveLength(0);
  });

  it.each([
    ["3p-red-east", "live"],
    ["3p-red-east", "replay"],
    ["4p-red-east", "live"],
    ["4p-red-east", "replay"],
  ] as const)("advances each river from its owner's left and wraps outward on %s %s", (mode, surface) => {
    const count = mode.startsWith("3p") ? 3 : 4;
    const rowDirections = {
      bottom: [1, 0, 0], right: [0, 0, -1], top: [-1, 0, 0], left: [0, 0, 1],
    } as const;
    const outwardDirections = {
      bottom: [0, 0, 1], right: [1, 0, 0], top: [0, 0, -1], left: [-1, 0, 0],
    } as const;
    for (let viewerSeat = 0; viewerSeat < count; viewerSeat += 1) {
      const source = projection({
        mode, viewer_seat: viewerSeat,
        players: Array.from({ length: count }, (_, seat) => player(seat, `seat-${seat}`, {
          discards: Array.from({ length: 7 }, (_, index) => seat * 8 + index),
        })),
      });
      const layout = buildMatchSceneLayout(source, null, surface);
      for (const { seat, position } of layout.players) {
        const river = layout.tiles.filter((tile) => tile.group === "discard" && tile.key.includes(`seat-${seat}-`));
        expect(river.map(({ tile }) => tile)).toEqual(source.players?.[seat].discards);
        const rowDirection = new Vector3(...rowDirections[position]);
        for (let index = 1; index < 6; index += 1) {
          const step = new Vector3(...river[index].position).sub(new Vector3(...river[index - 1].position));
          expect(step.dot(rowDirection), `${position} progresses toward its owner's right`).toBeCloseTo(0.55);
        }
        const wrap = new Vector3(...river[6].position).sub(new Vector3(...river[0].position));
        expect(wrap.dot(rowDirection)).toBeCloseTo(0);
        expect(wrap.dot(new Vector3(...outwardDirections[position]))).toBeGreaterThan(0);
      }
    }
  });

  it.each(["live", "replay"] as const)("rotates only an explicit declaration and widens its row on %s", (surface) => {
    for (const mode of ["3p-red-east", "4p-red-east"]) {
      const count = mode.startsWith("3p") ? 3 : 4;
      for (let viewer = 0; viewer < count; viewer += 1) {
        for (const declaration of [0, 5, 6, 11, 12, 23]) {
          const players = Array.from({ length: count }, (_, seat) => player(seat, `seat-${seat}`, {
            discards: Array.from({ length: 24 }, (_, index) => index),
            riichi: true, riichi_discard_index: declaration,
          }));
          const normal = buildMatchSceneLayout(projection({ mode, viewer_seat: viewer, players: players.map((p) => ({ ...p, riichi_discard_index: undefined })) }), null, surface);
          const actual = buildMatchSceneLayout(projection({ mode, viewer_seat: viewer, players }), null, surface);
          for (const { seat } of actual.players) {
            const river = actual.tiles.filter((t) => t.group === "discard" && t.key.includes(`seat-${seat}-`));
            const baseline = normal.tiles.filter((t) => t.group === "discard" && t.key.includes(`seat-${seat}-`));
            expect(river.map((t) => t.tile)).toEqual(baseline.map((t) => t.tile));
            river.forEach((t, index) => {
              expect(t.rotation[1] - baseline[index].rotation[1]).toBeCloseTo(index === declaration ? Math.PI / 2 : 0);
              expect(aabbIntersects(sceneTileAabb(t), CENTER_DEVICE_AABB)).toBe(false);
              if (index < declaration || Math.floor(index / 6) !== Math.floor(declaration / 6)) expect(t.position).toEqual(baseline[index].position);
              if (index % 6 !== 0) expect(aabbIntersects(sceneTileAabb(t), sceneTileAabb(river[index - 1]))).toBe(false);
            });
          }
        }
      }
    }
  });

  it.each([undefined, null, -1, 24, 1.5, NaN, "3"])("does not guess a declaration for invalid index %s", (index) => {
    const input = { discards: [0, 1, 2, 3], riichi: true };
    const baseline = buildMatchSceneLayout(projection({ players: [player(0, "local", input)] }), null);
    const actual = buildMatchSceneLayout(projection({ players: [player(0, "local", { ...input, riichi_discard_index: index })] }), null);
    expect(actual).toEqual(baseline);
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

  it("does not project 3D hit targets for the DOM-owned local hand", () => {
    const layout = buildMatchSceneLayout(projection(), null);
    expect(projectLocalHandHitTargets(layout).targets).toHaveLength(0);
  });

  it("keeps four-pon melds clear of short upright hands on Live and Replay", () => {
    const body = createTileSideGeometry();
    const back = createBackFaceGeometry();
    body.computeBoundingBox();
    back.computeBoundingBox();
    const bounds = new Box3().copy(body.boundingBox!).union(back.boundingBox!);
    for (const mode of ["ThreePlayerRedEast", "FourPlayerRedEast"]) {
      const seats = mode === "ThreePlayerRedEast" ? 3 : 4;
      for (let viewer = 0; viewer < seats; viewer++) {
        for (const concealed of [1, 2]) {
          const players = Array.from({ length: seats }, (_, seat) => player(seat, `seat-${seat}`, {
            hand: Array.from({ length: concealed }, (_, i) => 108 + seat * 4 + i),
            concealed_count: concealed,
            melds: Array.from({ length: 4 }, (_, meld) => ({
              tiles: [0, 1, 2].map((i) => 36 + (seat * 4 + meld) * 4 + i), opened: true,
            })),
          }));
          for (const surface of ["live", "replay"] as const) {
            const layout = buildMatchSceneLayout(projection({ mode, viewer_seat: viewer, players }), null, surface);
            for (const { seat, position } of layout.players) {
              const hand = layout.tiles.filter((tile) => tile.group === "hand" && tile.key.includes(`-${position}-seat-${seat}-`));
              const melds = layout.tiles.filter((tile) => tile.group === "meld" && tile.key.includes(`-${position}-seat-${seat}-`));
              for (const tile of hand) for (const meld of melds) {
                expect(bounds.clone().applyMatrix4(instanceMatrix(tile)).intersectsBox(bounds.clone().applyMatrix4(instanceMatrix(meld))), `${mode} ${viewer} ${surface}: ${tile.key} overlaps ${meld.key}`).toBe(false);
              }
            }
          }
        }
      }
    }
    body.dispose();
    back.dispose();
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
            expect(aabbIntersects(sceneTileAabb(discard), CENTER_DEVICE_AABB), `${mode} ${viewerSeat} ${discard.key} overlaps center`).toBe(false);
            for (const meld of melds) {
              expect(aabbIntersects(sceneTileAabb(discard), sceneTileAabb(meld)), `${mode} ${viewerSeat} ${discard.key} overlaps ${meld.key}`).toBe(false);
            }
          }
          for (const meld of melds) {
            expect(aabbIntersects(sceneTileAabb(meld), CENTER_DEVICE_AABB), `${mode} ${viewerSeat} ${meld.key} overlaps center`).toBe(false);
          }
        }
      }
    }
  });
});
