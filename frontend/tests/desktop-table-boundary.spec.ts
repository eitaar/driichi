import { expect, test } from "@playwright/test";

const summary = { match_id: "saved-match", source: "room", room_name: "Saved room", game_mode: "4p-red-east", started_at: "2026-09-16T00:00:00Z", completed_at: "2026-09-16T00:10:00Z", file_size: 512, availability: "available", replay_available: true };

test("phone Replay deep link retains metadata and a safe Back without renderer resources", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    Object.assign(window, { tableWorkerStarts: 0 });
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        Object.assign(window, { tableWorkerStarts: (window as unknown as { tableWorkerStarts: number }).tableWorkerStarts + 1 });
        super(url, options);
      }
    };
  });
  await page.route("**/api/v1/admin/replays**", route => route.fulfill({ json: route.request().url().endsWith("/saved-match") ? { ...summary, players: [], frames: [] } : { replays: [summary], total: 1, offset: 0, limit: 50, has_more: false } }));
  await page.goto("/admin/replays/saved-match");
  await expect(page.getByRole("heading", { name: "Saved room Replay" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Desktop required for the 3D table" })).toBeVisible();
  await expect(page.locator("canvas")).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { tableWorkerStarts: number }).tableWorkerStarts)).toBe(0);
  await page.getByRole("link", { name: "Back", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/replays$/);
  await expect(page.getByRole("heading", { name: "Replay library" })).toBeVisible();
  await page.getByRole("button", { name: "Toggle navigation" }).click();
  const menu = page.getByRole("dialog");
  await expect(menu).toBeVisible();
  await menu.getByRole("link", { name: "Benchmarks", exact: true }).focus();
  await page.keyboard.press("Escape");
  await expect(menu).not.toBeVisible();
  await expect(page.getByRole("button", { name: "Toggle navigation" })).toBeFocused();
});

test("phone Benchmark deep link omits private observer but preserves status and Room navigation", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const privateReads: string[] = [];
  page.on("request", request => { if (request.url().endsWith("/live")) privateReads.push(request.url()); });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: { run_id: "run-1", room_code: "123456", game_mode: "4p-red-east", target: 3, completed: 0, status: "running", reason: null, failed_match_id: null, roster: [], matches: [], statistics: [] } }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByRole("heading", { name: "Benchmark Run", exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Room controls" })).toBeVisible();
  await expect(page.getByText("No completed Matches yet.")).toBeVisible();
  await expect(page.getByText("Live table — all current hands")).toHaveCount(0);
  await expect(page.locator("canvas")).toHaveCount(0);
  expect(privateReads).toEqual([]);
});
