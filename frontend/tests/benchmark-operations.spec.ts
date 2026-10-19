import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const roster = Array.from({ length: 4 }, (_, initial_seat) => ({ participant_id: `p${initial_seat}`, display_name: `Bot ${initial_seat}`, initial_seat, participant_kind: "built_in_bot", character_id: null }));
const run = { run_id: "run-1", room_code: "123456", game_mode: "4p-red-east", target: 3, completed: 0, status: "running", reason: null, failed_match_id: null, roster, matches: [] };
const room = { join_code: "123456", room_name: "Bot room", game_mode: "4p-red-east", phase: "lobby", benchmark_mode: true, selected_count: 4, connected_count: 4, participant_count: 4, created_at: "2026-09-29T00:00:00Z", time_control: "casual", replay_save: true, participant_limit: 32, participants: roster.map(p => ({ ...p, kind: "built_in_bot", presence: "connected", selected: true, ready: false, character_id: "tsumogiri-bot", role: "none", controller: "interactive" })), match_players: [], roster: [], result: null, revision: 1, persistence_degraded: false, replay_available: true, benchmark: null };

test("local 25-row history and keyboard New Run create only a Room before explicit roster Start", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const posts: string[] = [];
  let connected = false;
  await page.route("**/api/v1/admin/**", async route => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (request.method() === "POST") posts.push(path);
    if (path === "/api/v1/admin/benchmark/runs") return route.fulfill({ json: Array.from({ length: 27 }, (_, i) => ({ ...run, run_id: `run-${i}` })) });
    if (path === "/api/v1/admin/benchmark/rooms") {
      expect(request.postDataJSON()).toEqual({ room_name: "Bot room", game_mode: "4p-red-half", time_control: "unlimited" });
      return route.fulfill({ json: room });
    }
    if (path === "/api/v1/admin/rooms") return route.fulfill({ json: [room] });
    if (path === "/api/v1/admin/rooms/123456") return route.fulfill({ json: { ...room, participants: room.participants.map(p => ({ ...p, presence: connected ? "connected" : "disconnected" })) } });
    return route.fulfill({ json: [] });
  });
  await page.goto("/admin/benchmark");
  const table = page.getByRole("table", { name: "Loaded Run history" });
  await expect(table.getByRole("row")).toHaveCount(26);
  await page.getByRole("button", { name: "Next page" }).click();
  await expect(table.getByRole("row")).toHaveCount(3);
  await page.getByLabel("Search loaded Runs").fill("run-26");
  await expect(table.getByRole("row")).toHaveCount(2);
  const trigger = page.getByRole("button", { name: "New Run", exact: true });
  await trigger.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "New Run", exact: true });
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await page.keyboard.press("Enter");
  await page.getByLabel("Room name").fill("Bot room");
  await page.getByLabel("Game mode", { exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("option", { name: "4p red East", exact: true })).toBeFocused();
  await page.keyboard.press("Home");
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("option", { name: "4p red half", exact: true })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Game mode", { exact: true }).locator('[data-slot="select-value"]')).toHaveText("4p red half");
  await page.getByLabel("Time control", { exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("option", { name: "Casual", exact: true })).toBeFocused();
  await page.keyboard.press("End");
  await expect(page.getByRole("option", { name: "Unlimited", exact: true })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Time control", { exact: true }).locator('[data-slot="select-value"]')).toHaveText("Unlimited");
  await expect(page.getByRole("button", { name: "Create benchmark room" })).toHaveCSS("opacity", "1");
  expect((await new AxeBuilder({ page }).include('[data-slot="dialog-content"]').analyze()).violations).toEqual([]);
  await page.getByRole("button", { name: "Create benchmark room" }).click();
  await expect(page.getByRole("heading", { name: "Bot room", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start Run" })).toBeDisabled();
  connected = true;
  await expect(page.getByText("Ready: exact connected eligible roster selected.")).toBeVisible({ timeout: 5000 });
  await expect(page.getByRole("button", { name: "Start Run" })).toBeEnabled();
  expect(posts).toEqual(["/api/v1/admin/benchmark/rooms"]);
  const ready = await page.getByRole("region", { name: "Benchmark control" }).boundingBox();
  const participants = await page.getByRole("heading", { name: /^Participants/ }).boundingBox();
  expect(ready!.y).toBeLessThan(participants!.y);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("mobile Run reload and five-second other-tab Stop recovery need no private observer", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  let pending = true;
  let status = "running";
  let detailReads = 0;
  const privateReads: string[] = [];
  page.on("request", request => { if (request.url().endsWith("/live")) privateReads.push(request.url()); });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => { detailReads++; return route.fulfill({ json: { ...run, status } }); });
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { ...room, benchmark: { run_id: "run-1", target: 3, completed: 0, status: "running", stop_requested: pending } } }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText("Stopping after current Match", { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText("Stopping after current Match", { exact: true })).toBeVisible();
  pending = false;
  await page.getByRole("button", { name: "Refresh Run" }).click();
  const stop = page.getByRole("button", { name: "Stop after current Match" });
  await expect(stop).toBeEnabled();
  pending = true; // Another tab's accepted Stop becomes visible through the ordinary detail poll.
  const before = detailReads;
  await expect(page.getByText("Stopping after current Match", { exact: true })).toBeVisible({ timeout: 7000 });
  expect(detailReads).toBe(before + 1);
  await expect(stop).toBeDisabled();
  expect(privateReads).toEqual([]);
  expect((await new AxeBuilder({ page }).include(".benchmark-main").analyze()).violations).toEqual([]);
  status = "stopped";
  await page.getByRole("button", { name: "Refresh Run" }).click();
  await expect(page.getByText("Stopping after current Match", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Resume/ })).toHaveCount(0);
});

test("keyboard Stop confirmation recovers an uncertain response and leaves navigation available", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  let accepted = false;
  let posts = 0;
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { ...room, benchmark: { run_id: "run-1", target: 3, completed: 0, status: "running", stop_requested: accepted } } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/stop", route => { accepted = true; posts++; return route.fulfill({ status: 503, json: { detail: "Response lost" } }); });
  await page.goto("/admin/benchmark/runs/run-1");
  const stop = page.getByRole("button", { name: "Stop after current Match" });
  await expect(stop).toBeEnabled();
  await stop.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("alertdialog", { name: "Finish current Match, then stop" });
  await expect(dialog).toBeVisible();
  expect(posts).toBe(0);
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  expect((await new AxeBuilder({ page }).include('[data-slot="alert-dialog-content"]').analyze()).violations).toEqual([]);
  await dialog.getByRole("button", { name: "Confirm Stop" }).click();
  await expect(page.getByText("Stopping after current Match", { exact: true })).toBeVisible();
  await expect(stop).toBeDisabled();
  await expect(page.getByRole("link", { name: "Room controls" })).toBeVisible();
  expect(posts).toBe(1);
});

for (const theme of ["light", "dark"]) test(`running Run reflows at 320px and 200% text in ${theme}`, async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  await page.addInitScript(theme => {
    localStorage.setItem("driichi-theme", theme);
    const NativeWorker = window.Worker;
    Object.assign(window, { runWorkerStarts: 0 });
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) { super(url, options); (window as unknown as { runWorkerStarts: number }).runWorkerStarts++; }
    };
  }, theme);
  const privateReads: string[] = [], rendererReads: string[] = [], posts: string[] = [];
  page.on("request", request => {
    if (request.url().endsWith("/live")) privateReads.push(request.url());
    if (/three-table|three-renderer|renderer\.worker|tile-atlas/.test(request.url())) rendererReads.push(request.url());
    if (request.method() === "POST") posts.push(request.url());
  });
  const completedRun = { ...run, completed: 1, matches: [{ sequence: 1, match_id: "persisted-1", results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: 40000 - seat * 10000 })) }] };
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: completedRun }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { ...room, benchmark: { run_id: "run-1", target: 3, completed: 1, status: "running", stop_requested: false } } }));
  await page.goto("/admin/benchmark/runs/run-1");
  await page.evaluate(() => { document.documentElement.style.fontSize = "200%"; });
  const stop = page.getByRole("button", { name: "Stop after current Match", exact: true });
  await expect(stop).toBeEnabled();
  await expect(page.locator(".recharts-surface")).toHaveCount(4, { timeout: 20000 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
  const heading = page.getByRole("heading", { name: "Benchmark Run", exact: true });
  await expect(heading).toBeVisible();
  await expect(heading).toHaveText("Benchmark Run");
  // The approved 320px/text200 presentation allows word wrapping, not clipping.
  expect(await heading.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  await stop.scrollIntoViewIfNeeded();
  await expect(stop).toBeInViewport();
  expect(await stop.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  await stop.focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("alertdialog", { name: "Finish current Match, then stop" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
  const table = page.getByRole("table", { name: "Participant statistics" });
  await table.focus();
  const scroll = table.locator("..");
  expect(await scroll.evaluate(element => element.scrollWidth > element.clientWidth)).toBe(true);
  await page.keyboard.press("ArrowRight");
  await expect.poll(() => scroll.evaluate(element => element.scrollLeft)).toBeGreaterThan(0);
  await scroll.evaluate(element => { element.scrollLeft = element.scrollWidth; });
  await expect(table.getByRole("columnheader", { name: "Rank 4 count / rate" })).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
  expect(privateReads).toEqual([]); expect(rendererReads).toEqual([]); expect(posts).toEqual([]);
  expect(await page.evaluate(() => (window as unknown as { runWorkerStarts: number }).runWorkerStarts)).toBe(0);
  await expect(page.locator("canvas")).toHaveCount(0);
});
