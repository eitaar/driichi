import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import tableFixtures from "./fixtures/task12-projection.json" with { type: "json" };

const roster = Array.from({ length: 4 }, (_, initial_seat) => ({ participant_id: `p${initial_seat}`, display_name: `Bot ${initial_seat}`, initial_seat, participant_kind: "built_in_bot", character_id: null }));
const run = { run_id: "run-1", room_code: "123456", game_mode: "4p-red-east", target: 3, completed: 0, status: "running", reason: null, failed_match_id: null, roster, matches: [] };
const projection = tableFixtures.projections["4p-red-east"];
function snapshot(revision = 7, rotation = 0) {
  return { revision, benchmark: { run_id: "run-1", target: 3, completed: 0, stop_requested: false, status: "running" }, projection: {
    ...projection, audience: "benchmark_admin", players: projection.players.map((player: object, seat: number) => ({ ...player,
      seat: (seat + rotation) % 4, participant_id: `p${seat}`, display_name: `Bot ${seat}`, score: 25000 + seat * 1000, hand: [seat * 4] })),
  } };
}

async function trackWorkers(page: Page) {
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    const counts = { starts: 0, active: 0 };
    Object.assign(window, { observerWorkers: counts });
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) { super(url, options); counts.starts++; counts.active++; }
      terminate() { counts.active--; super.terminate(); }
    };
  });
}
const workerCounts = (page: Page) => page.evaluate(() => (window as unknown as { observerWorkers: { starts: number; active: number } }).observerWorkers);

test("authorized GET focus, rotation, fullscreen and revocation use one current private table", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    const counts = { starts: 0, active: 0 };
    Object.assign(window, { observerWorkers: counts });
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) { super(url, options); counts.starts++; counts.active++; }
      terminate() { counts.active--; super.terminate(); }
    };
  });
  let next: object = snapshot();
  let status = 200;
  const commands: string[] = [];
  page.on("request", request => { if (request.method() !== "GET" && !request.url().endsWith("/logout")) commands.push(request.url()); });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", route => route.fulfill({ status, json: next }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  const table = page.getByRole("group", { name: "3D mahjong table" });
  await expect(table).toHaveAttribute("data-render-ready", "true");
  const initialStarts = await page.evaluate(() => (window as unknown as { observerWorkers: { starts: number } }).observerWorkers.starts);
  await table.evaluate(node => node.setAttribute("data-original-host", "yes"));
  await page.getByLabel("Focus Participant").click();
  await page.getByRole("option", { name: "Bot 1 (p1) · Seat 2", exact: true }).click();
  await expect(table.locator('[data-position="bottom"]')).toContainText("Bot 1");
  await expect(table.getByRole("group", { name: "Your concealed hand" }).getByRole("img")).toHaveAttribute("alt", "2m");
  await expect(table).toHaveAttribute("data-render-ready", "true");
  const fullscreen = page.getByRole("button", { name: "Fullscreen table" });
  await fullscreen.click();
  await expect(page.getByRole("button", { name: "Exit fullscreen" })).toBeVisible();
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  expect(await page.evaluate(() => document.fullscreenElement?.classList.contains("benchmark-observer"))).toBe(true);
  await page.getByLabel("Focus Participant").click();
  await page.getByRole("option", { name: "Bot 2 (p2) · Seat 3", exact: true }).click();
  await expect(table.locator('[data-position="bottom"]')).toContainText("Bot 2");
  await page.getByLabel("Focus Participant").click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Exit fullscreen" })).toBeVisible();
  expect(await page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(fullscreen).toBeVisible();
  await expect(fullscreen).toBeFocused();
  await page.getByLabel("Focus Participant").click();
  await page.getByRole("option", { name: "Bot 1 (p1) · Seat 2", exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { observerWorkers: { starts: number; active: number } }).observerWorkers)).toEqual({ starts: initialStarts, active: 1 });
  next = { ...snapshot(8), projection: null };
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect(page.getByText("Waiting for next Match.")).toBeVisible();
  await expect(table.getByRole("group", { name: "Your concealed hand" })).toHaveCount(0);
  next = snapshot(9, 1);
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect(page.getByText("Focused: Bot 1 · Seat 3")).toBeVisible();
  await expect(table.locator('[data-position="bottom"]')).toContainText("Bot 1");
  await expect(table).toHaveAttribute("data-original-host", "yes");
  await expect(table).toHaveAttribute("data-render-ready", "true");
  expect((await new AxeBuilder({ page }).include(".benchmark-observer").analyze()).violations).toEqual([]);
  status = 503; next = { detail: "Observation offline" };
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect(page.getByText(/Polling · Stale/)).toBeVisible();
  status = 200; next = snapshot(9, 1);
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  status = 401; next = { detail: "Admin session expired" };
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect(page.getByRole("heading", { name: "Sign in to host." })).toBeVisible();
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => (window as unknown as { observerWorkers: { active: number } }).observerWorkers.active)).toBe(0);
  await expect(page.getByText(/Read-only Admin view/)).toHaveCount(0);
  expect(commands).toEqual([]);
});

test("runningDesktopStartsWithTableAndRunLeaderboard", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const persistedRun = { ...run, completed: 1, matches: [{ sequence: 1, match_id: "persisted-1", results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: 40000 - seat * 10000 })) }] };
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: persistedRun }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", route => route.fulfill({ json: snapshot() }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await expect(page.getByRole("group", { name: "3D mahjong table" })).toHaveAttribute("data-render-ready", "true");
  const leaderboard = page.getByRole("region", { name: "Run leaderboard" });
  const leader = leaderboard.getByRole("row").nth(1);
  await expect(leader).toContainText("Bot 0 (p0)");
  await expect(leader.getByRole("cell")).toHaveText(["Bot 0 (p0)", "1.00", "100.0%", "15,000"]);
  const currentScores = page.getByRole("list", { name: "Table scores" });
  await expect(currentScores).toContainText("Bot 3: 28,000");
  await expect(currentScores).toContainText("Bot 0: 25,000");
  const tableBox = await page.locator(".benchmark-live").boundingBox();
  const leaderboardBox = await leaderboard.boundingBox();
  expect(leaderboardBox!.x).toBeGreaterThan(tableBox!.x + tableBox!.width);
  expect(tableBox!.width / leaderboardBox!.width).toBeGreaterThan(1.8);
  expect(tableBox!.width / leaderboardBox!.width).toBeLessThan(2.2);
  expect((await new AxeBuilder({ page }).include(".benchmark-observer").analyze()).violations).toEqual([]);
});

test("disclosure and viewport lifetimes dispose resources and reopen without cached hands", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await trackWorkers(page);
  let reads = 0;
  let release: (() => Promise<void>) | undefined;
  let hold = false;
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", async route => {
    reads++;
    if (hold) await new Promise<void>(resolve => { release = async () => { await route.fulfill({ json: snapshot() }); resolve(); }; });
    else await route.fulfill({ json: snapshot() });
  });
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  const table = page.getByRole("group", { name: "3D mahjong table" });
  await expect(table).toHaveAttribute("data-render-ready", "true");
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 1 });
  await page.getByText("Live table — all current hands", { exact: true }).click();
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 0 });
  const closedReads = reads;
  await page.waitForTimeout(5500);
  expect(reads).toBe(closedReads);
  hold = true;
  await page.getByText("Live table — all current hands", { exact: true }).click();
  await expect.poll(() => reads).toBe(closedReads + 1);
  await expect(page.getByText("Connecting to live table…")).toBeVisible();
  await expect(page.getByRole("group", { name: "Your concealed hand" })).toHaveCount(0);
  await release!();
  await expect(table).toHaveAttribute("data-render-ready", "true");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByText("Live table — all current hands", { exact: true })).toHaveCount(0);
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 0 });
  const phoneReads = reads;
  await page.waitForTimeout(5500);
  expect(reads).toBe(phoneReads);
  release = undefined;
  await page.setViewportSize({ width: 1024, height: 600 });
  // StrictMode can start and abort a first mount read before the current one.
  await expect.poll(() => reads).toBeGreaterThan(phoneReads);
  await expect(page.getByRole("group", { name: "Your concealed hand" })).toHaveCount(0);
  await release!();
  hold = false;
  await expect(table).toHaveAttribute("data-render-ready", "true");
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 1 });
});

for (const status of ["completed", "stopped", "interrupted", "failed"]) test(`ordinary Run ${status} tears down private resources before a held read returns`, async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await trackWorkers(page);
  let terminal = false, hold = false;
  let reads = 0;
  let release: (() => Promise<void>) | undefined;
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: { ...run, status: terminal ? status : "running" } }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", async route => {
    reads++;
    if (!hold) await route.fulfill({ json: snapshot() });
    else await new Promise<void>(resolve => { release = async () => { await route.fulfill({ json: snapshot(8) }); resolve(); }; });
  });
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await expect(page.getByRole("group", { name: "3D mahjong table" })).toHaveAttribute("data-render-ready", "true");
  hold = true;
  const initialReads = reads;
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect.poll(() => reads).toBe(initialReads + 1);
  terminal = true;
  await page.getByRole("button", { name: "Refresh Run", exact: true }).click();
  await expect(page.getByText("Live table — all current hands", { exact: true })).toHaveCount(0);
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 0 });
  await release!();
  await expect(page.getByRole("group", { name: "Your concealed hand" })).toHaveCount(0);
  await page.waitForTimeout(5500);
  expect(reads).toBe(initialReads + 1);
});

test("private terminal evidence arrives before ordinary Run detail without retaining hands or renewing", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await trackWorkers(page);
  let ended = false, reads = 0;
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", route => {
    reads++;
    return route.fulfill({ json: ended ? { ...snapshot(8), benchmark: { ...snapshot().benchmark, status: "completed" } } : snapshot() });
  });
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await expect(page.getByRole("group", { name: "3D mahjong table" })).toHaveAttribute("data-render-ready", "true");
  ended = true;
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect(page.getByText("Run ended. No active Match.")).toBeVisible();
  await expect(page.getByRole("group", { name: "Your concealed hand" })).toHaveCount(0);
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 0 });
  const terminalReads = reads;
  await page.waitForTimeout(5500);
  expect(reads).toBe(terminalReads);
});

test("Run and Room replacement reject a late private response from the previous lifetime", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await trackWorkers(page);
  const replacement = { ...run, run_id: "run-2", room_code: "654321" };
  let holdOld = false, holdNew = true;
  let releaseOld: (() => Promise<void>) | undefined;
  const releaseNew: Array<() => Promise<void>> = [];
  await page.route("**/api/v1/admin/benchmark/runs", route => route.fulfill({ json: [replacement] }));
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.route("**/api/v1/admin/benchmark/runs/run-2", route => route.fulfill({ json: replacement }));
  await page.route("**/api/v1/admin/rooms/*", route => route.fulfill({ json: { join_code: route.request().url().endsWith("654321") ? "654321" : "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", async route => {
    if (holdOld) await new Promise<void>(resolve => { releaseOld = async () => { await route.fulfill({ json: snapshot(8) }); resolve(); }; });
    else await route.fulfill({ json: snapshot() });
  });
  const next = { ...snapshot(), benchmark: { ...snapshot().benchmark, run_id: "run-2" }, projection: { ...snapshot().projection, players: snapshot().projection.players.map(p => ({ ...p, display_name: `New ${p.display_name}` })) } };
  await page.route("**/api/v1/admin/benchmark/rooms/654321/live", async route => {
    if (holdNew) await new Promise<void>(resolve => { releaseNew.push(async () => { await route.fulfill({ json: next }); resolve(); }); });
    else await route.fulfill({ json: next });
  });
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await expect(page.getByRole("group", { name: "3D mahjong table" })).toHaveAttribute("data-render-ready", "true");
  holdOld = true;
  await page.getByRole("button", { name: "Refresh table" }).click();
  await expect.poll(() => Boolean(releaseOld)).toBe(true);
  await page.getByRole("navigation", { name: "Admin", exact: true }).getByRole("link", { name: "Benchmarks", exact: true }).click();
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect.poll(() => workerCounts(page)).toMatchObject({ active: 0 });
  await page.getByRole("link", { name: "654321 · run-2", exact: true }).click();
  await expect(page.getByText("Connecting to live table…")).toBeVisible();
  await expect.poll(() => releaseNew.length).toBeGreaterThan(0);
  await releaseOld!();
  await expect(page.getByRole("group", { name: "Your concealed hand" })).toHaveCount(0);
  holdNew = false;
  for (const release of releaseNew) await release();
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await expect(page.getByRole("group", { name: "3D mahjong table" })).toHaveAttribute("data-render-ready", "true");
  await expect(page.getByLabel("Focus Participant")).toBeVisible();
  await expect(page.getByRole("list", { name: "Table scores" })).toContainText("New Bot 0");
});

for (const height of [600, 800, 900]) {
  test(`denied fullscreen keeps Exit and status visible while scrolling at ${height}px`, async ({ page }) => {
    await page.setViewportSize({ width: 1100, height });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.addInitScript(() => { HTMLElement.prototype.requestFullscreen = () => Promise.reject(new Error("Denied")); });
    await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
    await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
    await page.route("**/api/v1/admin/benchmark/rooms/123456/live", route => route.fulfill({ json: snapshot() }));
    await page.goto("/admin/benchmark/runs/run-1");
    await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
    const leaderboard = page.getByRole("region", { name: "Run leaderboard" });
    await expect(leaderboard).toContainText("No completed Matches yet.");
    await expect(leaderboard.getByRole("list")).toHaveCount(0);
    const tableBox = await page.locator(".benchmark-live").boundingBox();
    const leaderboardBox = await leaderboard.boundingBox();
    expect(leaderboardBox!.y).toBeGreaterThan(tableBox!.y + tableBox!.height);
    await page.getByLabel("Focus Participant").click();
    await page.getByRole("option", { name: "Bot 1 (p1) · Seat 2", exact: true }).click();
    expect(await page.locator(".benchmark-live").evaluate(node => node.getAnimations().length)).toBe(0);
    await page.getByRole("button", { name: "Fullscreen table" }).click();
    await expect(page.getByText(/Browser fullscreen unavailable/)).toBeVisible();
    await page.locator(".benchmark-live").evaluate(node => {
      const table = node.getBoundingClientRect();
      window.scrollBy(0, table.top + table.height / 2);
    });
    await expect.poll(() => page.locator(".benchmark-live").evaluate(node => node.getBoundingClientRect().top)).toBeLessThan(0);
    for (const control of [page.getByRole("button", { name: "Exit fullscreen" }), page.getByText(/Polling · Fresh/)]) {
      await expect(control).toBeInViewport({ ratio: 1 });
      expect(await control.evaluate(node => {
        const rect = node.getBoundingClientRect();
        return node.contains(document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2));
      })).toBe(true);
    }
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Fullscreen table" })).toBeFocused();
    await expect(page.getByRole("group", { name: "3D mahjong table" })).toHaveCount(1);
  });
}
