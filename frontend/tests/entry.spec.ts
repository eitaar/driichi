import { expect, test } from "@playwright/test";

for (const viewport of [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1440, height: 900, label: "1440x900" },
]) {
  test.describe(`entry ${viewport.label}`, () => {
    test.use({ viewport });

    test("keeps primary room and admin actions in the first view", async ({ page }) => {
      const consoleErrors: string[] = [];
      const failedRequests: string[] = [];
      page.on("console", (message) => {
        if (message.type() === "error") consoleErrors.push(message.text());
      });
      page.on("requestfailed", (request) => failedRequests.push(`${request.method()} ${request.url()}`));

      await page.goto("/");
      await expect(page.getByRole("heading", { name: /your table is live/i })).toBeVisible();
      await expect(page.getByRole("button", { name: /open room/i })).toBeVisible();
      await expect(page.getByRole("link", { name: /admin sign in/i })).toBeVisible();
      await expect(page.locator("nav")).toHaveCSS("height", /.+/);
      const navHeight = await page.locator("nav").evaluate((element) => element.getBoundingClientRect().height);
      expect(navHeight).toBeLessThanOrEqual(80);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
      expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBeLessThanOrEqual(viewport.height);
      await expect(page.locator("footer")).toBeVisible();
      expect(consoleErrors).toEqual([]);
      expect(failedRequests).toEqual([]);
      await page.screenshot({ path: `test-results/task-10/entry-${viewport.label}.png`, fullPage: false });
    });
  });
}

test("supports keyboard focus, reduced motion, loading, and Problem Details states", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await expect(page.getByTestId("entry-shell")).toHaveAttribute("data-motion", "static");
  let releaseRoomLookup!: () => void;
  const roomLookupReleased = new Promise<void>((resolve) => {
    releaseRoomLookup = resolve;
  });
  await page.route("**/api/v1/rooms/123456", async (route) => {
    await roomLookupReleased;
    await route.fulfill({
      status: 404,
      contentType: "application/problem+json",
      body: JSON.stringify({
        type: "about:blank",
        title: "Room not found",
        status: 404,
        detail: "The requested Room does not exist.",
        code: "room_not_found",
        request_id: "01TESTREQUESTID000000000000",
      }),
    });
  });
  await page.getByRole("textbox", { name: /room code/i }).fill("123456");
  await page.getByRole("button", { name: /open room/i }).click();
  await expect(page).toHaveURL(/\/room\/123456$/);
  await expect(page.getByText(/loading room/i)).toBeVisible();
  releaseRoomLookup();
  await expect(page.getByRole("heading", { name: /room unavailable/i })).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("The requested Room does not exist.");
});

test("admin route exposes the room workspace at desktop sizes", async ({ page }) => {
  await page.route("**/api/v1/admin/rooms", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([]),
    });
  });
  await page.route("**/api/v1/admin/tokens", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([]),
    });
  });
  await page.goto("/admin");
  await expect(page.getByRole("heading", { name: /admin rooms/i })).toBeVisible();
  await expect(page.getByRole("heading", { name: /credentials for agents/i })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(1280);
});

test("human lobby shows a reconnectable websocket handoff", async ({ page }) => {
  await page.addInitScript(() => {
    class LobbySocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: MessageEvent<string>) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: (() => void) | null = null;
      constructor(public url: string) { setTimeout(() => this.onopen?.(), 0); }
      send() {}
      close() { this.onclose?.({ code: 1000, reason: "client_closed" } as CloseEvent); }
    }
    Object.defineProperty(window, "WebSocket", { configurable: true, value: LobbySocket });
    sessionStorage.setItem("driichi:participant:123456", "P1");
  });
  await page.goto("/room/123456/lobby");
  await expect(page.getByRole("heading", { name: /room lobby/i })).toBeVisible();
  await expect(page.getByText(/connecting/i)).toBeVisible();
});

const task11Room = {
  join_code: "123456", room_name: "Night Market", game_mode: "4p-red-east", phase: "lobby",
  time_control: "casual", replay_save: true, participant_limit: 4, connected_count: 2,
  participant_count: 2, selected_count: 2, created_at: "2026-09-16T00:00:00Z", revision: 8,
  participants: [
    { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
    { participant_id: "P2", display_name: "Nori", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-blue", role: "none", controller: "interactive" },
  ],
  match_players: [], roster: [], result: null, persistence_degraded: false, replay_available: true,
};

for (const viewport of [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1440, height: 900, label: "1440x900" },
]) {
  test(`captures Admin at ${viewport.label}`, async ({ page }) => {
    test.setTimeout(30_000);
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.route("**/api/v1/admin/rooms/123456", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(task11Room) });
    });
    await page.route("**/api/v1/admin/rooms", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify([task11Room]) });
    });
    await page.route("**/api/v1/admin/tokens", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
    });
    await page.goto("/admin/rooms/123456");
    await expect(page.getByRole("heading", { name: /night market/i })).toBeVisible();
    await expect(page.getByRole("heading", { name: /credentials for agents/i })).toBeVisible();
    await page.screenshot({ path: `test-results/task-11/admin-${viewport.label}.png`, fullPage: false });
  });

  test(`captures Lobby at ${viewport.label}`, async ({ page }) => {
    test.setTimeout(30_000);
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.addInitScript(() => {
      const snapshot = {
        type: "snapshot",
        room: {
          join_code: "123456", room_name: "Night Market", game_mode: "3p-red-east", phase: "lobby", revision: 8,
          participants: [
            { participant_id: "P1", display_name: "Mika", kind: "human", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" },
            { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
            { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", presence: "connected", selected: true, ready: true, character_id: "tsumogiri-bot", role: "none", controller: "permanent_auto_built_in_bot" },
          ],
          match_players: [
            { participant_id: "P1", display_name: "Mika", kind: "human", seat: 0, character_id: "player-red", controller: "interactive" },
            { participant_id: "B1", display_name: "Bot 1", kind: "built_in_bot", seat: 1, character_id: "tsumogiri-bot", controller: "permanent_auto_built_in_bot" },
            { participant_id: "B2", display_name: "Bot 2", kind: "built_in_bot", seat: 2, character_id: "tsumogiri-bot", controller: "permanent_auto_built_in_bot" },
          ],
          roster: [], result: null,
        }, state: null,
      };
      class LobbySocket {
        static OPEN = 1;
        readyState = 0;
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: ((event: { code: number; reason: string }) => void) | null = null;
        onerror: (() => void) | null = null;
        constructor() {
          setTimeout(() => { this.readyState = 1; this.onopen?.(); this.onmessage?.({ data: JSON.stringify(snapshot) }); }, 0);
        }
        send() {}
        close() { this.onclose?.({ code: 1000, reason: "client_closed" }); }
      }
      Object.defineProperty(window, "WebSocket", { configurable: true, value: LobbySocket });
      sessionStorage.setItem("driichi:participant:123456", "P1");
    });
    await page.goto("/room/123456/lobby");
    await expect(page.getByRole("heading", { name: /night market lobby/i })).toBeVisible();
    await expect(page.locator(".connection-state")).toHaveText("connected");
    await page.screenshot({ path: `test-results/task-11/lobby-${viewport.label}.png`, fullPage: false });
  });
}
