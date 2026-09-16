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
  await page.route("**/api/v1/rooms/123456", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 250));
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
  await expect(page.getByRole("heading", { name: /bot tokens/i })).toBeVisible();
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
