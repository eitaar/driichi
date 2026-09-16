import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Page } from "@playwright/test";

const characterFixtureRoot = resolve(process.cwd(), "tests/fixtures/task12-characters");

async function installCharacterFixtures(page: Page) {
  await page.route("**/assets/characters/**", async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    const match = /^\/assets\/characters\/([^/]+)\/(portrait|icon)\.webp$/.exec(pathname);
    if (!match) {
      await route.fulfill({ status: 404, body: "missing test asset" });
      return;
    }
    const file = resolve(characterFixtureRoot, match[1], `${match[2]}.webp`);
    try {
      await route.fulfill({ status: 200, contentType: "image/webp", body: readFileSync(file) });
    } catch {
      await route.fulfill({ status: 404, body: "missing test asset" });
    }
  });
}

async function installSocket(page: Page, mode: "3p-red-east" | "4p-red-east") {
  await page.addInitScript(({ mode }) => {
    const threePlayer = mode.startsWith("3p");
    const actionTile = threePlayer ? 52 : 16;
    const ownHand = threePlayer
      ? [0, 1, 2, 3, 32, 33, 34, 35, 36, 40, 44, 48, 52, 88]
      : [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52];
    const discardSets = threePlayer
      ? [[32, 36, 40, 44, 48], [72, 76, 80, 84, 88], [108, 112, 116, 120, 124]]
      : [[4, 8, 12, 20, 24], [36, 40, 44, 48, 52], [72, 76, 80, 84, 88], [108, 112, 116, 120, 124]];
    const participants = [
      { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: true, character_id: "player-red", role: "player_0", controller: "interactive" },
      { participant_id: "P2", display_name: "Nori", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "player_1", controller: "permanent_auto_built_in_bot" },
      { participant_id: "P3", display_name: "Ren", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "player_2", controller: "permanent_auto_built_in_bot" },
      ...(threePlayer ? [] : [{ participant_id: "P4", display_name: "Aya", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "player_3", controller: "permanent_auto_built_in_bot" }]),
    ];
    const matchPlayers = participants.map(({ participant_id, display_name, kind, seat, character_id, controller }, index) => ({
      participant_id, display_name, kind, seat: index, character_id, controller,
    }));
    const players = matchPlayers.map((player, index) => ({
      seat: index,
      participant_id: player.participant_id,
      display_name: player.display_name,
      kind: player.kind,
      score: 25000,
      ...(index === 0 ? { hand: ownHand } : { concealed_count: 13 }),
      discards: discardSets[index],
      melds: index === 1
        ? [{ tiles: threePlayer ? [40, 44, 48] : [24, 28, 32], opened: true, from_who: 0, called_tile: threePlayer ? 40 : 24 }]
        : index === 0
          ? [{ tiles: threePlayer ? [72, 76, 80] : [72, 76, 80], opened: false, from_who: null, called_tile: null }]
          : [],
      riichi: index === 0,
    }));
    const playing = {
      join_code: "123456", room_name: "Night Market", game_mode: mode, phase: "playing", revision: 12,
      participants, match_players: matchPlayers, roster: matchPlayers, result: null,
    };
    const state = {
      audience: "player", viewer_seat: 0, mode, dora_indicators: [actionTile], remaining_wall: 70,
      kyoku: "East 1", honba: 1, kyotaku: 2, players,
      decision: { decision_id: "d12", kind: "turn", duration_ms: 30000, remaining_ms: 30000, actions: [
        { action_id: "a-red-five", action: { discard: { tile: actionTile, tsumogiri: false } } },
        { action_id: "a-pass", action: "pass" },
      ] },
    };
    (window as unknown as { __room: unknown; __state: unknown }).__room = playing;
    (window as unknown as { __room: unknown; __state: unknown }).__state = state;
    class GameplaySocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: { data: string }) => void) | null = null;
      onclose: ((event: { code: number; reason: string }) => void) | null = null;
      onerror: (() => void) | null = null;
      sent: string[] = [];
      constructor() {
        (window as unknown as { __socket: GameplaySocket }).__socket = this;
        setTimeout(() => {
          this.onopen?.();
          this.onmessage?.({ data: JSON.stringify({ type: "snapshot", room: playing, state }) });
        }, 0);
      }
      send(value: string) { this.sent.push(value); }
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
      close() { this.onclose?.({ code: 1000, reason: "client_closed" }); }
    }
    Object.defineProperty(window, "WebSocket", { configurable: true, value: GameplaySocket });
    sessionStorage.setItem("driichi:participant:123456", "P1");
  }, { mode });
}

async function expectRenderedTable(page: Page) {
  const table = page.getByTestId("pixi-table");
  await expect(table).toBeVisible({ timeout: 20000 });
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20000 });
  await expect(table).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
  await expect(table).toHaveAttribute("data-rendered-table-primitives", /^[1-9]\d*$/);
  await expect(table).toHaveAttribute("data-rendered-visual-primitives", /^[1-9]\d*$/);
  const canvasBounds = await table.evaluate((node) => {
    const canvas = node.querySelector("canvas");
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return { clientWidth: rect.width, clientHeight: rect.height, pixelWidth: canvas.width, pixelHeight: canvas.height };
  });
  expect(canvasBounds).not.toBeNull();
  expect(canvasBounds?.clientWidth).toBeGreaterThan(0);
  expect(canvasBounds?.clientHeight).toBeGreaterThan(0);
  expect((canvasBounds?.clientWidth ?? 0) / (canvasBounds?.clientHeight ?? 1)).toBeCloseTo(1600 / 900, 1);
  expect(canvasBounds?.pixelWidth).toBeLessThanOrEqual(3200);
  expect(canvasBounds?.pixelHeight).toBeLessThanOrEqual(1800);
  return table;
}

for (const viewport of [{ width: 1024, height: 600, label: "1024x600" }, { width: 1440, height: 900, label: "1440x900" }]) {
  for (const mode of ["3p-red-east", "4p-red-east"] as const) {
    test(`captures ${mode} authoritative gameplay at ${viewport.label}`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await installCharacterFixtures(page);
      await installSocket(page, mode);
      await page.goto("/room/123456/lobby");
      const table = await expectRenderedTable(page);
      await expect(page.getByText("Mika")).toBeVisible();
      await expect(table).toHaveAttribute("data-center-data", /East 1/);
      await expect(page.getByTestId("decision-timer")).toHaveAttribute("aria-label", /Decision timer: \d+ seconds/);
      await expect(page.getByRole("button", { name: /discard 5[mp]/i })).toBeVisible();
      await expect(page.locator(".table-tile-hit")).toHaveCount(14);
      await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(1);
      await page.getByRole("button", { name: /discard 5[mp]/i }).click();
      await expect(page.getByTestId("action-deck")).toHaveAttribute("aria-busy", "true");
      await expect(page.locator(".table-tile-hit")).toHaveCount(14);
      await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(1);
      const sent = await page.evaluate(() => (window as unknown as { __socket: { sent: string[] } }).__socket.sent.map(JSON.parse));
      expect(sent).toEqual([{ type: "submit_action", decision_id: "d12", action_id: "a-red-five" }]);
      await page.screenshot({ path: `test-results/task-12/${mode}-${viewport.label}-decision.png`, fullPage: false });
    });
  }
}

test("shows the authoritative Mangan post-match results surface", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await page.evaluate(() => {
    const browser = window as unknown as { __socket: { emit: (value: unknown) => void }; __state: unknown };
    browser.__socket.emit({ type: "game_update", event: { type: "action_resolved", events: [{ hora: { actor: 0, target: 1, han: 5, fu: 30, delta: [8000, -8000, 0, 0] } }] }, state: browser.__state });
  });
  await expect(table).toHaveAttribute("data-portrait-ready", "true", { timeout: 20000 });
  await expect(table).toHaveAttribute("data-portrait-effect", "Mangan");
  await expect(table).toHaveAttribute("data-portrait-name", "Mika");
  await expect(table).toHaveAttribute("data-portrait-result", "Ron");
  await page.evaluate(() => {
    const browser = window as unknown as { __socket: { emit: (value: unknown) => void }; __room: Record<string, unknown>; __state: unknown };
    browser.__socket.emit({ type: "room_update", room: { ...browser.__room, phase: "post_match", result: { mode: "4p-red-east", players: [
      { participant_id: "P1", display_name: "Mika", rank: 1, final_score: 45000 },
      { participant_id: "P2", display_name: "Nori", rank: 2, final_score: 30000 },
      { participant_id: "P3", display_name: "Ren", rank: 3, final_score: 15000 },
      { participant_id: "P4", display_name: "Aya", rank: 4, final_score: 10000 },
    ] } }, state: browser.__state });
  });
  await expect(page.getByTestId("results-panel")).toBeVisible();
  await expect(page.getByText("Permanent Auto")).toHaveCount(3);
  await expect(page.getByAltText("Mika portrait")).toBeVisible();
  await page.screenshot({ path: "test-results/task-12/results-portrait-state.png", fullPage: false });
});

test("shows guidance below the supported gameplay viewport", async ({ page }) => {
  await page.setViewportSize({ width: 1023, height: 599 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expect(page.getByText("Widen this window to play.")).toBeVisible();
  await expect(page.getByTestId("pixi-table")).toHaveCount(0);
});
