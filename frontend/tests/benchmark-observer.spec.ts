import { expect, test } from "@playwright/test";
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
  await page.getByText("Live table — all current hands").click();
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  const table = page.getByRole("group", { name: "3D mahjong table" });
  await expect(table).toHaveAttribute("data-render-ready", "true");
  const initialStarts = await page.evaluate(() => (window as unknown as { observerWorkers: { starts: number } }).observerWorkers.starts);
  await table.evaluate(node => node.setAttribute("data-original-host", "yes"));
  await page.getByLabel("Focus Participant").selectOption("p1");
  await expect(table.locator('[data-position="bottom"]')).toContainText("Bot 1");
  await expect(table.getByRole("group", { name: "Your concealed hand" }).getByRole("img")).toHaveAttribute("alt", "2m");
  await expect(table).toHaveAttribute("data-render-ready", "true");
  const fullscreen = page.getByRole("button", { name: "Fullscreen table" });
  await fullscreen.click();
  await expect(page.getByRole("button", { name: "Exit fullscreen" })).toBeVisible();
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  await page.getByRole("button", { name: "Exit fullscreen" }).click();
  await expect(fullscreen).toBeFocused();
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

test("adjacent Run leaderboard uses persisted results rather than the current Match leader", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const persistedRun = { ...run, completed: 1, matches: [{ sequence: 1, match_id: "persisted-1", results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: 40000 - seat * 10000 })) }] };
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: persistedRun }));
  await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { join_code: "123456", benchmark: snapshot().benchmark } }));
  await page.route("**/api/v1/admin/benchmark/rooms/123456/live", route => route.fulfill({ json: snapshot() }));
  await page.goto("/admin/benchmark/runs/run-1");
  await page.getByText("Live table — all current hands").click();
  await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
  const leaderboard = page.getByRole("region", { name: "Run leaderboard" });
  const leader = leaderboard.getByRole("listitem").first();
  await expect(leader).toContainText("Bot 0 (p0)");
  await expect(leader).toContainText("Mean rank: 1.00");
  await expect(leader).toContainText("First-place rate: 100.0%");
  await expect(leader).toContainText("Net score: 15,000");
  const currentScores = page.getByRole("list", { name: "Table scores" });
  await expect(currentScores).toContainText("Bot 3: 28,000");
  await expect(currentScores).toContainText("Bot 0: 25,000");
  const tableBox = await page.locator(".benchmark-live").boundingBox();
  const leaderboardBox = await leaderboard.boundingBox();
  expect(leaderboardBox!.x).toBeGreaterThan(tableBox!.x + tableBox!.width);
  expect((await new AxeBuilder({ page }).include(".benchmark-observer").analyze()).violations).toEqual([]);
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
    await page.getByText("Live table — all current hands").click();
    const leaderboard = page.getByRole("region", { name: "Run leaderboard" });
    await expect(leaderboard).toContainText("No completed Matches yet.");
    await expect(leaderboard.getByRole("list")).toHaveCount(0);
    const tableBox = await page.locator(".benchmark-live").boundingBox();
    const leaderboardBox = await leaderboard.boundingBox();
    expect(leaderboardBox!.y).toBeGreaterThan(tableBox!.y + tableBox!.height);
    await page.getByLabel("Focus Participant").selectOption("p1");
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
