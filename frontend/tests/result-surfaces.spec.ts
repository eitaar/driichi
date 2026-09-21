import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Page } from "@playwright/test";

const fixtureRoot = resolve(process.cwd(), "tests/fixtures/task12-characters");
const projectionFixture = JSON.parse(
  readFileSync(resolve(process.cwd(), "tests/fixtures/task12-projection.json"), "utf8"),
) as {
  room_envelopes: Record<string, Record<string, unknown>>;
  projections: Record<string, Record<string, unknown>>;
};

async function installAssets(page: Page) {
  await page.route("**/assets/characters/**", async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    const match = /^\/assets\/characters\/([^/]+)\/(portrait|icon)\.webp$/.exec(pathname);
    if (!match) {
      await route.fulfill({ status: 404, body: "missing test asset" });
      return;
    }
    try {
      await route.fulfill({
        status: 200,
        contentType: "image/webp",
        body: readFileSync(resolve(fixtureRoot, match[1], `${match[2]}.webp`)),
      });
    } catch {
      await route.fulfill({ status: 404, body: "missing test asset" });
    }
  });
}

async function installSocket(page: Page) {
  const room = projectionFixture.room_envelopes["4p-red-east"];
  const state = projectionFixture.projections["4p-red-east"];
  await page.addInitScript(
    ({ room, state }) => {
      const browser = window as unknown as {
        __room: unknown;
        __state: unknown;
        __socket: {
          emit: (value: unknown) => void;
        };
      };
      browser.__room = room;
      browser.__state = state;
      class GameplaySocket {
        static OPEN = 1;
        readyState = 1;
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: (() => void) | null = null;
        onerror: (() => void) | null = null;
        constructor() {
          browser.__socket = this;
          setTimeout(() => {
            this.onopen?.();
            this.onmessage?.({ data: JSON.stringify({ type: "room_update", room: browser.__room }) });
            this.onmessage?.({ data: JSON.stringify({ type: "snapshot", room: browser.__room, state: browser.__state }) });
          }, 0);
        }
        send(_value: string) {}
        emit(value: unknown) {
          this.onmessage?.({ data: JSON.stringify(value) });
        }
        close() {
          this.onclose?.();
        }
      }
      Object.defineProperty(window, "WebSocket", { configurable: true, value: GameplaySocket });
      sessionStorage.setItem("driichi:participant:123456", "P1");
    },
    { room, state },
  );
}

async function openTable(page: Page) {
  await page.goto("/room/123456/lobby");
  await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
}

async function showRoundWin(page: Page) {
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __state: unknown;
    };
    browser.__socket.emit({
      type: "game_update",
      event: {
        type: "action_resolved",
        events: [{ hora: { actor: 0, target: 1, pai: 16, han: 3, fu: 40, yaku: [["riichi", 1]], delta: [8_000, -8_000, 0, 0] } }],
      },
      state: browser.__state,
    });
  });
  await expect(page.getByTestId("round-win-surface")).toBeVisible();
}

async function showStandings(page: Page) {
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
            { participant_id: "P1", display_name: "Mika", rank: 1, final_score: 45_000, delta: 8_000 },
            { participant_id: "P2", display_name: "Nori", rank: 2, final_score: 30_000, delta: -8_000 },
            { participant_id: "P3", display_name: "Ren", rank: 3, final_score: 15_000 },
            { participant_id: "P4", display_name: "Aya", rank: 4, final_score: 10_000 },
          ],
        },
      },
      state: browser.__state,
    });
  });
  await expect(page.getByTestId("results-panel")).toBeVisible();
}

test.describe("result surface screenshots", () => {
  for (const viewport of [
    { width: 1024, height: 600 },
    { width: 1600, height: 900 },
  ]) {
    test(`captures round win and final standings at ${viewport.width}px`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await installAssets(page);
      await installSocket(page);
      await openTable(page);
      await showRoundWin(page);
      await page.screenshot({ path: `test-results/result-surfaces/round-win-${viewport.width}.png`, fullPage: false });
      await showStandings(page);
      await page.screenshot({ path: `test-results/result-surfaces/final-standings-${viewport.width}.png`, fullPage: false });
    });
  }
});
