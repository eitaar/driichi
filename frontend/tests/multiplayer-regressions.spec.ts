/// <reference types="../node_modules/@types/node" />

import { expect, test, type Page, type Route } from "./support/task7b-evidence";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { waitForGameplayReady } from "./support/gameplay-ready";

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
    mode: "FourPlayerRedEast",
    final_scores: [45_000, 30_000, 15_000, 10_000],
    players: [
      { participant_id: "P1", display_name: "Mika", kind: "Human", seat: 0, rank: 1, final_score: 45_000, delta: 8_000 },
      { participant_id: "P2", display_name: "Nori", kind: "BuiltInBot", seat: 1, rank: 2, final_score: 30_000, delta: -8_000 },
      { participant_id: "P3", display_name: "Ren", kind: "BuiltInBot", seat: 2, rank: 3, final_score: 15_000 },
      { participant_id: "P4", display_name: "Aya", kind: "BuiltInBot", seat: 3, rank: 4, final_score: 10_000 },
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
        __constructions: number;
        __closes: number;
        __room: Record<string, any>;
        __state: Record<string, any>;
        __socket: {
          sent: string[];
          emit: (value: unknown) => void;
        };
      };
      browser.__constructions = 0;
      browser.__closes = 0;
      browser.__room = initialRoom;
      browser.__state = initialState;
      const NativeWebSocket = window.WebSocket;
      class RegressionSocket {
        static OPEN = 1;
        readyState = 1;
        sent: string[] = [];
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: (() => void) | null = null;
        onerror: (() => void) | null = null;
        constructor(...args: ConstructorParameters<typeof WebSocket>) {
          if (!String(args[0]).endsWith("/ws/v1/rooms/123456/human")) return new NativeWebSocket(...args) as unknown as RegressionSocket;
          ++browser.__constructions;
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
          ++browser.__closes;
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

test("keeps Human startup and updates independent of a held renderer module", async ({ page }) => {
  test.setTimeout(60_000); // Cold bootstrap plus a separate held-renderer transition; no result clocks here.
  // Public synthetic socket/HTTP boundary, not genuine clock or native OPEN evidence.
  const state = simultaneousDiscardState();
  const room = structuredClone(projectionFixture.room_envelopes["4p-red-east"]);
  const lobby = { ...room, phase: "lobby", participants: room.participants.map((participant: Record<string, any>) => ({ ...participant, ready: participant.participant_id !== "P1" })) };
  await installCharacterFixtures(page);
  await installSocket(page, state, lobby);
  let held: Route | undefined;
  let released = false;
  // Cold bootstrap can exceed 2.5s without any dependency on the held renderer.
  await page.route("**/src/app.tsx*", async route => {
    await new Promise(resolve => setTimeout(resolve, 3500));
    await route.continue().catch(() => undefined);
  });
  await page.route("**/src/game/three-table.tsx*", route => { held = route; });
  try {
    // Commit, not DOMContentLoaded: an eager module can block the latter.
    await page.goto("/room/123456/lobby", { waitUntil: "commit" });
    await expect.poll(() => page.evaluate(() => (window as unknown as { __constructions: number }).__constructions), { timeout: 30_000 }).toBeGreaterThan(0);
    await expect(page.getByLabel("Room summary")).toContainText("4 / 4 selected");
    await expect(page.locator(".connection-state")).toHaveCount(0);
    expect(held, "lobby bootstrap must not demand the renderer").toBeUndefined();
    await expect(page.getByRole("button", { name: /^set ready/i })).toBeEnabled();
    await page.getByRole("button", { name: /^set ready/i }).click();
    expect(await page.evaluate(() => JSON.parse((window as unknown as { __socket: { sent: string[] } }).__socket.sent.at(-1)!))).toMatchObject({ type: "set_ready" });
    await expect(page.getByLabel("Room summary")).toContainText("3 / 4 ready");
    const baseline = await page.evaluate(() => { const w = window as unknown as { __constructions: number; __closes: number }; return { constructions: w.__constructions, closes: w.__closes }; });
    await emitRoomUpdate(page, room, state);
    await expect.poll(() => Boolean(held)).toBe(true);
    await expect(page.getByRole("status", { name: "3D table loading" })).toHaveText("Preparing 3D table");
    await expect(page.getByTestId("gameplay-shell")).toBeVisible();
    await expect(page.getByTestId("action-deck")).toBeVisible();
    const updated = structuredClone(state);
    updated.players[0].display_name = "Updated while loading";
    await emitRoomUpdate(page, room, updated);
    expect(await page.evaluate(() => { const w = window as unknown as { __constructions: number; __closes: number }; return { constructions: w.__constructions, closes: w.__closes }; })).toEqual(baseline);
    released = true;
    await held!.continue();
    await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
    await expect(page.locator(".table-player-copy").getByText("Updated while loading", { exact: true })).toBeVisible();
    expect(await page.evaluate(() => { const w = window as unknown as { __constructions: number; __closes: number }; return { constructions: w.__constructions, closes: w.__closes }; })).toEqual(baseline);
  } finally {
    if (held && !released) await held.continue().catch(() => undefined);
  }
});

test("waits for deferred gameplay readiness before finite result clocks", async ({ page }) => {
  const state = simultaneousDiscardState();
  const room = structuredClone(projectionFixture.room_envelopes["4p-red-east"]);
  const lobby = { ...room, phase: "lobby" };
  await installCharacterFixtures(page);
  await installSocket(page, state, lobby);
  let release: (() => void) | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let finish: (() => void) | undefined;
  const finished = new Promise<void>(resolve => { finish = resolve; });
  await page.route("**/src/game/three-table.tsx*", async route => {
    // Exceeds the ordinary visibility budget, inside the existing renderer-ready budget.
    await new Promise<void>(resolve => { release = resolve; timer = setTimeout(resolve, 6000); });
    try { await route.continue(); } catch { /* preserve the primary assertion failure */ }
    finally { finish?.(); }
  });
  try {
    await page.goto("/room/123456/lobby");
    await expect(page.getByLabel("Room summary")).toContainText("4 / 4 selected");
    await expect(page.locator(".connection-state")).toHaveCount(0);
    await emitRoomUpdate(page, room, state);
    await waitForGameplayReady(page);
    await expect(page.getByTestId("gameplay-shell")).toBeVisible();
  } finally {
    clearTimeout(timer);
    release?.();
    if (release) await finished;
  }
});

test("keeps Human ownership on chunk failure and reloads only by explicit link", async ({ page }) => {
  const state = simultaneousDiscardState();
  const room = structuredClone(projectionFixture.room_envelopes["4p-red-east"]);
  const lobby = { ...room, phase: "lobby", participants: room.participants.map((participant: Record<string, any>) => ({ ...participant, ready: participant.participant_id !== "P1" })) };
  await installCharacterFixtures(page);
  await installSocket(page, state, lobby);
  let allowGameplay = false;
  let moduleRequests = 0;
  let navigations = 0;
  page.on("framenavigated", frame => { if (frame === page.mainFrame()) ++navigations; });
  // The Match boundary now owns gameplay; optional 3D failures have their own DOM-action controls.
  await page.route("**/src/game/gameplay.tsx*", async route => {
    ++moduleRequests;
    if (allowGameplay) await route.continue(); else await route.abort("failed");
  });
  await page.goto("/room/123456/lobby");
  await expect(page.getByRole("button", { name: /^set ready/i })).toBeEnabled();
  const baseline = await page.evaluate(() => { const w = window as unknown as { __constructions: number; __closes: number }; return { constructions: w.__constructions, closes: w.__closes }; });
  const initialNavigations = navigations;
  await emitRoomUpdate(page, room, state);
  await expect(page.getByRole("alert")).toHaveText("Match could not be loaded.Reload page");
  await expect(page.getByRole("link", { name: "Reload page" })).toHaveAttribute("href", page.url());
  expect(navigations).toBe(initialNavigations);
  expect(moduleRequests).toBe(1);
  const updatedLobby = structuredClone(lobby);
  updatedLobby.participants[0].display_name = "Updated after module failure";
  await emitRoomUpdate(page, updatedLobby, state);
  await expect(page.getByRole('button', { name: 'Updated after module failure', exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /^set ready/i })).toBeEnabled();
  expect(await page.evaluate(() => { const w = window as unknown as { __constructions: number; __closes: number }; return { constructions: w.__constructions, closes: w.__closes }; })).toEqual(baseline);
  expect(navigations).toBe(initialNavigations);
  expect(moduleRequests).toBe(1);
  // Re-enter the failed surface; only the user's plain link should restart navigation/transport.
  await emitRoomUpdate(page, room, state);
  await expect(page.getByRole("link", { name: "Reload page" })).toBeVisible();
  allowGameplay = true;
  await page.getByRole("link", { name: "Reload page" }).click();
  await expect.poll(() => navigations).toBe(initialNavigations + 1);
  await expect(page.getByRole("button", { name: /^set ready/i })).toBeEnabled();
  await emitRoomUpdate(page, room, state);
  await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
});

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
        status: "rejected",
        code: "illegal_action",
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
  await page.evaluate(() => {
    (window as unknown as { __socket: { emit: (value: unknown) => void } }).__socket.emit({
      type: "action_result",
      decision_id: "tenpai-with-choice",
      action_id: "riichi-red-five",
      status: "accepted",
    });
  });
  await expect(riichi).toBeDisabled();
  await expect(ordinaryTargets.first()).toBeDisabled();
  await riichi.dispatchEvent("click");
  expect(await page.evaluate(() =>
    (window as unknown as { __socket: { sent: string[] } }).__socket.sent.length,
  )).toBe(4);
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

  const postMatchRoom: Record<string, any> = {
    ...room,
    phase: "post_match",
    revision: room.revision + 1,
    result: finalResult(),
  };
  await emitRoomUpdate(page, postMatchRoom, state);
  await expect(page.getByTestId("results-panel")).toBeVisible();
  const dismiss = page.getByRole("button", { name: "Return to room", exact: true });
  await expect(dismiss).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: /^lobby$/i })).toBeVisible();
  const review = page.getByTestId("review-results");
  await expect(review).toBeFocused();
  await review.click();
  await expect(page.getByTestId("results-panel")).toBeVisible();
  await dismiss.click();

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
