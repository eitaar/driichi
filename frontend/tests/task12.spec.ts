/// <reference types="../node_modules/@types/node" />

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Page } from "@playwright/test";

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
  room_envelopes: Record<
    "3p-red-east" | "4p-red-east",
    Record<string, unknown>
  >;
  projections: Record<"3p-red-east" | "4p-red-east", Record<string, unknown>>;
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
    const file = resolve(characterFixtureRoot, match[1], `${match[2]}.webp`);
    try {
      await route.fulfill({
        status: 200,
        contentType: "image/webp",
        body: readFileSync(file),
      });
    } catch {
      await route.fulfill({ status: 404, body: "missing test asset" });
    }
  });
}

async function installSocket(
  page: Page,
  mode: "3p-red-east" | "4p-red-east",
  stateOverride?: Record<string, unknown>,
) {
  const room = projectionFixture.room_envelopes[mode];
  const state = stateOverride ?? projectionFixture.projections[mode];
  await page.addInitScript(
    ({ room, state }) => {
      (window as unknown as { __room: unknown; __state: unknown }).__room =
        room;
      (window as unknown as { __room: unknown; __state: unknown }).__state =
        state;
      class GameplaySocket {
        static OPEN = 1;
        readyState = 1;
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: ((event: { code: number; reason: string }) => void) | null =
          null;
        onerror: (() => void) | null = null;
        sent: string[] = [];
        constructor() {
          const browser = window as unknown as {
            __room: unknown;
            __state: unknown;
            __socket: GameplaySocket;
          };
          browser.__socket = this;
          setTimeout(() => {
            this.onopen?.();
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
          this.onclose?.({ code: 1000, reason: "client_closed" });
        }
      }
      Object.defineProperty(window, "WebSocket", {
        configurable: true,
        value: GameplaySocket,
      });
      sessionStorage.setItem("driichi:participant:123456", "P1");
    },
    { room, state },
  );
}

async function expectRenderedTable(page: Page) {
  const table = page.getByTestId("pixi-table");
  await expect(table).toBeVisible({ timeout: 20000 });
  await expect(table).toHaveAttribute("data-render-ready", "true", {
    timeout: 20000,
  });
  await expect(table).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
  await expect(table).toHaveAttribute(
    "data-rendered-table-primitives",
    /^[1-9]\d*$/,
  );
  await expect(table).toHaveAttribute(
    "data-rendered-visual-primitives",
    /^[1-9]\d*$/,
  );
  const canvasBounds = await table.evaluate((node) => {
    const canvas = node.querySelector("canvas");
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return {
      clientWidth: rect.width,
      clientHeight: rect.height,
      pixelWidth: canvas.width,
      pixelHeight: canvas.height,
    };
  });
  expect(canvasBounds).not.toBeNull();
  expect(canvasBounds?.clientWidth).toBeGreaterThan(0);
  expect(canvasBounds?.clientHeight).toBeGreaterThan(0);
  expect(
    (canvasBounds?.clientWidth ?? 0) / (canvasBounds?.clientHeight ?? 1),
  ).toBeCloseTo(1600 / 900, 1);
  expect(canvasBounds?.pixelWidth).toBeLessThanOrEqual(3200);
  expect(canvasBounds?.pixelHeight).toBeLessThanOrEqual(1800);
  return table;
}

for (const viewport of [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1440, height: 900, label: "1440x900" },
]) {
  for (const mode of ["3p-red-east", "4p-red-east"] as const) {
    test(`captures ${mode} authoritative gameplay at ${viewport.label}`, async ({
      page,
    }) => {
      await page.setViewportSize({
        width: viewport.width,
        height: viewport.height,
      });
      await installCharacterFixtures(page);
      await installSocket(page, mode);
      await page.goto("/room/123456/lobby");
      const table = await expectRenderedTable(page);
      await expect(page.getByText("Mika")).toBeVisible();
      await expect(table).toHaveAttribute("data-center-data", /East 1/);
      await expect(page.getByTestId("decision-timer")).toHaveAttribute(
        "aria-label",
        /Decision timer: \d+ seconds/,
      );
      const legalTiles = page.locator(".table-tile-hit.is-legal");
      await expect(legalTiles).toHaveCount(14);
      await expect(legalTiles.first()).toBeVisible();
      await legalTiles.first().click();
      await expect(page.getByTestId("action-deck")).toHaveAttribute(
        "aria-busy",
        "true",
      );
      await expect(page.locator(".table-tile-hit")).toHaveCount(14);
      await expect(legalTiles).toHaveCount(14);
      const sent = await page.evaluate(() =>
        (
          window as unknown as { __socket: { sent: string[] } }
        ).__socket.sent.map((value) => JSON.parse(value)),
      );
      expect(sent).toEqual([
        { type: "submit_action", decision_id: "d1", action_id: "a1" },
      ]);
      await page.screenshot({
        path: `test-results/task-12/${mode}-${viewport.label}-decision.png`,
        fullPage: false,
      });
    });
  }
}

test("candidate popup transfers focus and closes on Escape", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  const popupState = JSON.parse(JSON.stringify(projectionFixture.projections["4p-red-east"])) as Record<string, unknown>;
  (popupState.decision as Record<string, unknown>).actions = [
    { action_id: "chi-1", action: { Chi: { target: 1, called: 1, consumed: [0, 4] } } },
    { action_id: "chi-2", action: { Chi: { target: 1, called: 2, consumed: [1, 5] } } },
  ];
  await installSocket(page, "4p-red-east", popupState);
  await page.goto("/room/123456/lobby");
  const trigger = page.getByRole("button", { name: "Chi (2)" });
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: "Choose a legal candidate" });
  await expect(dialog).toHaveAttribute("aria-modal", "true");
  await expect(dialog.locator(".candidate-list button").first()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

test("keeps the Riichi legal highlight while its authoritative action is pending", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  const riichiState = JSON.parse(JSON.stringify(projectionFixture.projections["4p-red-east"])) as Record<string, unknown>;
  const hand = (riichiState.players as Array<Record<string, unknown>>)[0].hand as number[];
  (riichiState.decision as Record<string, unknown>).actions = [
    { action_id: "riichi-1", action: { riichi_discard: { tile: hand[0] } } },
  ];
  await installSocket(page, "4p-red-east", riichiState);
  await page.goto("/room/123456/lobby");
  await expect(page.getByTestId("pixi-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20000 });
  const legal = page.locator(".table-tile-hit.is-legal");
  await expect(legal).toHaveCount(1);
  const initialStyle = await legal.first().evaluate((element) => {
    const style = getComputedStyle(element);
    return { border: style.borderTopColor, background: style.backgroundColor };
  });
  expect(initialStyle.border).not.toBe("rgba(0, 0, 0, 0)");
  expect(initialStyle.background).not.toBe("rgba(0, 0, 0, 0)");
  await legal.first().click();
  await expect(page.getByTestId("action-deck")).toHaveAttribute("aria-busy", "true");
  await expect(legal).toHaveCount(1);
  const pendingStyle = await legal.first().evaluate((element) => getComputedStyle(element).backgroundColor);
  expect(pendingStyle).not.toBe("rgba(0, 0, 0, 0)");
});

test("shows the authoritative Mangan post-match results surface", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __state: unknown;
    };
    browser.__socket.emit({
      type: "game_update",
      event: {
        type: "action_resolved",
        events: [
          {
            hora: {
              actor: 0,
              target: 1,
              han: 5,
              fu: 30,
              delta: [8000, -8000, 0, 0],
            },
          },
        ],
      },
      state: browser.__state,
    });
  });
  await expect(table).toHaveAttribute("data-portrait-ready", "true", {
    timeout: 20000,
  });
  await expect(table).toHaveAttribute("data-portrait-effect", "Mangan");
  await expect(table).toHaveAttribute("data-portrait-name", "Mika");
  await expect(table).toHaveAttribute("data-portrait-result", "Ron");
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __room: Record<string, unknown>;
      __state: unknown;
    };
    browser.__socket.emit({
      type: "room_update",
      room: {
        ...browser.__room,
        phase: "post_match",
        result: {
          mode: "4p-red-east",
          players: [
            {
              participant_id: "P1",
              display_name: "Mika",
              rank: 1,
              final_score: 45000,
            },
            {
              participant_id: "P2",
              display_name: "Nori",
              rank: 2,
              final_score: 30000,
            },
            {
              participant_id: "P3",
              display_name: "Ren",
              rank: 3,
              final_score: 15000,
            },
            {
              participant_id: "P4",
              display_name: "Aya",
              rank: 4,
              final_score: 10000,
            },
          ],
        },
      },
      state: browser.__state,
    });
  });
  await expect(table).toHaveAttribute("data-portrait-ready", "true");
  await expect(page.getByTestId("results-panel")).toBeVisible();
  await expect(page.getByText("Permanent Auto")).toHaveCount(3);
  await expect(page.getByAltText("Mika portrait")).toBeVisible();
  await page.screenshot({
    path: "test-results/task-12/results-portrait-state.png",
    fullPage: false,
  });
  await page.waitForTimeout(12100);
  await expect(table).toHaveAttribute("data-portrait-ready", "false");
});

test("shows guidance below the supported gameplay viewport", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1023, height: 599 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expect(page.getByText("Widen this window to play.")).toBeVisible();
  await expect(page.getByTestId("pixi-table")).toHaveCount(0);
});
