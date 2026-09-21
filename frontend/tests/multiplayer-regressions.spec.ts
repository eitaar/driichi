/// <reference types="../node_modules/@types/node" />

import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

test.describe.configure({ mode: "serial" });

const characterFixtureRoot = resolve(
  process.cwd(),
  "tests/fixtures/task12-characters",
);
const projectionFixture = JSON.parse(
  readFileSync(
    resolve(process.cwd(), "tests/fixtures/task12-projection.json"),
    "utf8",
  ),
) as {
  room_envelopes: Record<string, Record<string, any>>;
  projections: Record<string, Record<string, any>>;
};

async function installCharacterFixtures(page: Page) {
  await page.route("**/assets/characters/**", async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    const match = /^\/assets\/characters\/([^/]+)\/(portrait|icon)\.webp$/.exec(
      pathname,
    );
    if (!match) {
      await route.fulfill({ status: 404, body: "missing test asset" });
      return;
    }
    try {
      await route.fulfill({
        status: 200,
        contentType: "image/webp",
        body: readFileSync(
          resolve(characterFixtureRoot, match[1], `${match[2]}.webp`),
        ),
      });
    } catch {
      await route.fulfill({ status: 404, body: "missing test asset" });
    }
  });
}

function simultaneousDiscardState(): Record<string, any> {
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  state.players[0].hand = [
    16, // red 5m
    17, // regular 5m: same value, different physical tile
    29,
    35,
    49,
    53,
    55,
    68,
    75,
    81,
    83,
    100,
    114,
    117,
  ];
  state.decision = {
    decision_id: "tenpai-with-choice",
    kind: "Turn",
    actions: [
      { action_id: "ordinary-red-five", action: { Discard: { tile: 16 } } },
      { action_id: "ordinary-five-copy", action: { Discard: { tile: 17 } } },
      { action_id: "ordinary-eight", action: { Discard: { tile: 29 } } },
      { action_id: "riichi-red-five", action: { riichi_discard: { tile: 16 } } },
    ],
  };
  return state;
}

function finalResult() {
  return {
    mode: "4p-red-east",
    players: [
      { participant_id: "P1", display_name: "Mika", rank: 1, final_score: 45_000, delta: 8_000 },
      { participant_id: "P2", display_name: "Nori", rank: 2, final_score: 30_000, delta: -8_000 },
      { participant_id: "P3", display_name: "Ren", rank: 3, final_score: 15_000 },
      { participant_id: "P4", display_name: "Aya", rank: 4, final_score: 10_000 },
    ],
  };
}

async function installSocket(
  page: Page,
  state: Record<string, any>,
  roomOverride?: Record<string, any>,
) {
  const room = structuredClone(
    roomOverride ?? projectionFixture.room_envelopes["4p-red-east"],
  );
  await page.addInitScript(
    ({ room: initialRoom, state: initialState }) => {
      const browser = window as unknown as {
        __room: Record<string, any>;
        __state: Record<string, any>;
        __socket: {
          sent: string[];
          emit: (value: unknown) => void;
        };
      };
      browser.__room = initialRoom;
      browser.__state = initialState;
      class RegressionSocket {
        static OPEN = 1;
        readyState = 1;
        sent: string[] = [];
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: (() => void) | null = null;
        onerror: (() => void) | null = null;
        constructor() {
          browser.__socket = this;
          setTimeout(() => {
            this.onopen?.();
            this.onmessage?.({
              data: JSON.stringify({ type: "room_update", room: browser.__room }),
            });
            this.onmessage?.({
              data: JSON.stringify({
                type: "snapshot",
                room: browser.__room,
                state: browser.__state,
              }),
            });
          }, 0);
        }
        send(value: string) {
          this.sent.push(value);
        }
        emit(value: unknown) {
          this.onmessage?.({ data: JSON.stringify(value) });
        }
        close() {
          this.onclose?.();
        }
      }
      Object.defineProperty(window, "WebSocket", {
        configurable: true,
        value: RegressionSocket,
      });
      sessionStorage.setItem("driichi:participant:123456", "P1");
    },
    { room, state },
  );
}

async function emitRoomUpdate(page: Page, room: Record<string, any>, state: Record<string, any>) {
  await page.evaluate(
    ({ room, state }) => {
      const browser = window as unknown as {
        __room: Record<string, any>;
        __state: Record<string, any>;
        __socket: { emit: (value: unknown) => void };
      };
      browser.__room = room;
      browser.__state = state;
      browser.__socket.emit({ type: "room_update", room, state });
    },
    { room, state },
  );
}

test("keeps ordinary tenpai discards clickable beside a separate Riichi action", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  const state = simultaneousDiscardState();
  await installSocket(page, state);
  await page.goto("/room/123456/lobby");
  await expect(page.getByTestId("three-table")).toHaveAttribute(
    "data-render-ready",
    "true",
    { timeout: 20_000 },
  );

  const ordinaryIds = [
    "ordinary-red-five",
    "ordinary-five-copy",
    "ordinary-eight",
  ];
  const ordinaryTargets = page.locator(".table-tile-hit.is-legal");
  await expect(ordinaryTargets).toHaveCount(ordinaryIds.length);
  expect(await ordinaryTargets.evaluateAll((elements) =>
    elements.map((element) => element.getAttribute("data-action-id")),
  )).toEqual(ordinaryIds);
  await expect(page.getByTestId("action-deck").getByRole("button", { name: /^riichi$/i })).toHaveAttribute(
    "data-action-id",
    "riichi-red-five",
  );

  for (const actionId of ordinaryIds) {
    const target = page.locator(`.table-tile-hit[data-action-id="${actionId}"]`);
    await expect(target).toBeEnabled();
    await target.click();
    await expect(page.getByTestId("action-deck")).toHaveAttribute("aria-busy", "true");
    expect(await page.evaluate(() => {
      const browser = window as unknown as { __socket: { sent: string[] } };
      return JSON.parse(browser.__socket.sent.at(-1) ?? "{}");
    })).toEqual({
      type: "submit_action",
      decision_id: "tenpai-with-choice",
      action_id: actionId,
    });
    // No authoritative update was sent, so pending only disables the target;
    // the projection and every physical hand target remain mounted.
    await expect(ordinaryTargets).toHaveCount(ordinaryIds.length);
    await expect(target).toBeDisabled();
    await page.evaluate(({ actionId }) => {
      const browser = window as unknown as {
        __socket: { emit: (value: unknown) => void };
      };
      browser.__socket.emit({
        type: "action_result",
        decision_id: "tenpai-with-choice",
        action_id: actionId,
        status: "accepted",
      });
    }, { actionId });
    await expect(page.getByTestId("action-deck")).toHaveAttribute("aria-busy", "false");
    await expect(target).toBeEnabled();
  }

  const riichi = page.getByTestId("action-deck").getByRole("button", { name: /^riichi$/i });
  await riichi.click();
  expect(await page.evaluate(() => {
    const browser = window as unknown as { __socket: { sent: string[] } };
    return JSON.parse(browser.__socket.sent.at(-1) ?? "{}");
  })).toEqual({
    type: "submit_action",
    decision_id: "tenpai-with-choice",
    action_id: "riichi-red-five",
  });
});

test("leaves result presentation, reaches Ready, and can present the next result without reload", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  state.decision = null;
  const room = structuredClone(projectionFixture.room_envelopes["4p-red-east"]);
  room.participants = room.participants.map((participant: Record<string, any>) => ({
    ...participant,
    ready: participant.participant_id === "P1" ? false : true,
  }));
  await installSocket(page, state, room);
  await page.goto("/room/123456/lobby");
  await expect(page.getByTestId("three-table")).toHaveAttribute(
    "data-render-ready",
    "true",
    { timeout: 20_000 },
  );

  const postMatchRoom = {
    ...room,
    phase: "post_match",
    revision: room.revision + 1,
    result: finalResult(),
  };
  await emitRoomUpdate(page, postMatchRoom, state);
  await expect(page.getByTestId("results-panel")).toBeVisible();
  const dismiss = page.getByTestId("dismiss-results");
  await expect(dismiss).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: /night market lobby/i })).toBeVisible();
  const review = page.getByTestId("review-results");
  await expect(review).toBeFocused();
  await review.click();
  await expect(page.getByTestId("results-panel")).toBeVisible();
  await page.getByTestId("dismiss-results").click();

  const ready = page.getByRole("button", { name: /^set ready$/i });
  await expect(ready).toBeEnabled();
  await ready.click();
  expect(await page.evaluate(() => {
    const browser = window as unknown as { __socket: { sent: string[] } };
    return JSON.parse(browser.__socket.sent.at(-1) ?? "{}");
  })).toEqual({
    type: "set_ready",
    preloaded_characters: ["player-red", "tsumogiri-bot", "tsumogiri-bot", "tsumogiri-bot"],
  });

  const readyRoom = {
    ...postMatchRoom,
    revision: postMatchRoom.revision + 1,
    participants: postMatchRoom.participants.map((participant: Record<string, any>) => ({
      ...participant,
      ready: participant.participant_id === "P1" ? true : participant.ready,
    })),
  };
  await emitRoomUpdate(page, readyRoom, state);
  await expect(page.getByRole("button", { name: /^ready set$/i })).toBeDisabled();

  const playingRoom = { ...readyRoom, phase: "playing", revision: readyRoom.revision + 1, result: null };
  await emitRoomUpdate(page, playingRoom, state);
  await expect(page.getByTestId("gameplay-shell")).toBeVisible();
  await page.waitForTimeout(50);
  const nextPostMatchRoom = {
    ...playingRoom,
    phase: "post_match",
    revision: playingRoom.revision + 1,
    result: finalResult(),
  };
  await emitRoomUpdate(page, nextPostMatchRoom, state);
  await expect(page.getByTestId("results-panel")).toBeVisible();
  expect(page.url()).toMatch(/\/room\/123456\/lobby$/);
});
