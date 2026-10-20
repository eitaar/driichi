import { expect, test } from "./support/task7b-evidence";
import AxeBuilder from "@axe-core/playwright";
import { attachUiDiagnosticsIfInstalled, installUiDiagnostics } from "./support/ui-diagnostics";

test.use({ hasTouch: true });

// Attaches only when the case installed the opt-in diagnostic; never fails the test.
test.afterEach(async ({ page }, testInfo) => {
  await attachUiDiagnosticsIfInstalled(page, testInfo);
});

const roster = ["z]{}", "a", "b"].map((participant_id, initial_seat) => ({ participant_id, initial_seat, display_name: `Bot ${initial_seat}`, participant_kind: "built_in_bot", character_id: null }));
const run = { run_id: "run-1", room_code: "123456", game_mode: "3p-red-east", target: 60, completed: 60, status: "completed", reason: null, failed_match_id: null, roster,
  matches: Array.from({ length: 60 }, (_, i) => ({ sequence: i + 1, match_id: `match-${i + 1}`, results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat: (seat + i) % 3, rank: seat + 1, final_score: [45000, 35000, 25000][seat] })) })), statistics: [],
};
for (const mobile of [false, true]) test(`${mobile ? "touch Drawer" : "desktop Sheet"}: real chart rendering, numeric parity and keyboard details`, async ({ page }) => {
  test.setTimeout(60000);
  await page.setViewportSize(mobile ? { width: 390, height: 844 } : { width: 1280, height: 900 });
  const errors: string[] = [], privateReads: string[] = [], rendererReads: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  page.on("request", request => { if (request.url().endsWith("/live")) privateReads.push(request.url()); if (/table-scene|three-renderer|renderer\.worker/.test(request.url())) rendererReads.push(request.url()); });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.route("**/api/v1/admin/replays", route => route.fulfill({ json: { replays: [], offset: 0, limit: 50, total: 0, has_more: false } }));
  if (mobile) await installUiDiagnostics(page);
  await page.goto("/admin/benchmark/runs/run-1");
  const summary = page.getByRole("region", { name: "Run summary", exact: true });
  await expect(summary).toHaveAttribute("data-slot", "card");
  await expect(summary.getByRole("progressbar", { name: "Completed Matches" })).toHaveAttribute("value", "60");
  await expect(summary.getByRole("progressbar")).toHaveAttribute("max", "60");
  await expect(summary.getByRole("status")).toHaveText("completed");
  await expect(summary.getByRole("link", { name: "Room controls" })).toHaveAttribute("href", "/admin/rooms/123456");
  await expect(page.getByRole("heading", { name: "Match history", exact: true })).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "Observation", exact: true })).toHaveCount(0);
  await expect(page.locator(".recharts-surface")).toHaveCount(4, { timeout: 20000 });
  for (const chart of await page.locator('[data-slot="chart"]').all()) {
    await expect(chart).toBeVisible();
    expect((await chart.boundingBox())!.height).toBeGreaterThanOrEqual(240);
    expect((await chart.boundingBox())!.width).toBeGreaterThan(100);
  }
  await page.screenshot({ path: test.info().outputPath('four-charts.png'), fullPage: true });
  await expect(page.locator(".recharts-line-curve")).toHaveCount(6);
  await expect(page.getByText("Rank 4", { exact: true })).toHaveCount(0);
  for (const name of ["Cumulative net score", "Moving-average rank", "Rank distribution", "Per-Match net score"]) {
    await expect(page.getByRole("button", { name: `${name} options`, exact: true })).toHaveAttribute("aria-expanded", "false");
  }
  await expect(page.getByRole("region", { name: "Benchmark analytics", exact: true }).getByRole("combobox")).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Benchmark analytics", exact: true }).getByRole("checkbox")).toHaveCount(0);
  for (const name of ["Cumulative net score", "Moving-average rank", "Per-Match net score"]) {
    const options = page.getByRole("button", { name: `${name} options`, exact: true });
    await options.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByLabel(`${name} window`)).toContainText("Recent 50");
  }
  await expect(page.getByRole("button", { name: "Rank distribution options", exact: true })).toHaveAttribute("aria-expanded", "false");
  await expect(page.getByText("n = 50 · Range: Recent 50.")).toHaveCount(3);
  await page.getByLabel("Cumulative net score window").focus();
  await page.keyboard.press("Enter");
  await page.getByRole("option", { name: "Full Run", exact: true }).click();
  await expect(page.getByText("n = 60 · Range: Full Run.")).toBeVisible();
  const options = page.getByRole("button", { name: "Cumulative net score options", exact: true });
  await options.click();
  await expect(page.getByRole("combobox", { name: "Cumulative net score window" })).toHaveCount(0);
  await options.click();
  await expect(page.getByLabel("Cumulative net score window")).toContainText("Full Run");
  await expect(page.getByLabel("Moving-average rank window")).toContainText("Recent 50");
  await expect(page.getByLabel("Per-Match net score window")).toContainText("Recent 50");
  await page.getByLabel("Cumulative net score window").click();
  await page.getByRole("option", { name: "Recent 50", exact: true }).click();
  await expect(page.getByText("n = 60", { exact: true })).toBeVisible();
  await expect(page.locator('.benchmark-chart-hero [data-chart-family="cumulative"]')).toHaveCount(1);
  await expect(page.locator('.benchmark-chart-supporting [data-chart-family]')).toHaveCount(3);
  await expect(page.locator('.recharts-line-curve[stroke-dasharray]')).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Benchmark analytics", exact: true })).not.toContainText(/z\]\{\}|\(a\)|\(b\)/);
  await expect(page.getByRole("table", { name: "Participant statistics" })).toContainText("600,000");
  const surface = page.getByRole("region", { name: "Cumulative net score", exact: true }).locator(".recharts-surface");
  await page.mouse.move(0, 0);
  await surface.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".recharts-tooltip-wrapper").filter({ visible: true }).first()).toContainText("Match 12");
  expect(await surface.evaluate(el => getComputedStyle(el).outlineStyle)).not.toBe("none");
  await page.getByLabel("Selected Match sequence").fill("12");
  const trigger = page.getByRole("button", { name: "View Match details", exact: true });
  await trigger.focus();
  const scroll = await page.evaluate(() => scrollY);
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toHaveAccessibleName("Match 12 details");
  await expect(dialog).toContainText("match-12");
  await expect(dialog).toContainText("45,000");
  await expect(dialog.getByRole("link", { name: "Open Replay" })).toHaveCount(0);
  await expect(dialog.getByRole("status")).toContainText("not verified");
  await expect(dialog.locator(mobile ? '[data-slot="drawer-title"]' : '[data-slot="sheet-title"]')).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(Math.abs(await page.evaluate(() => scrollY) - scroll)).toBeLessThan(5);
  const table = page.getByRole("table", { name: "Per-Match numeric results" });
  await expect(table.getByRole("row")).toHaveCount(26);
  const row = table.getByRole("row").filter({ has: page.getByRole("button", { name: "View Match 60 details" }) });
  await expect(row).toContainText("10,000 / 600,000 / 1.00 (n=10)");
  if (mobile) await page.getByRole("button", { name: "View Match 60 details" }).tap();
  else await page.getByRole("button", { name: "View Match 60 details" }).click();
  await expect(page.getByRole("dialog")).toHaveAccessibleName("Match 60 details");
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const netChart = page.getByRole("region", { name: "Per-Match net score", exact: true });
  await netChart.getByRole("button", { name: "Pause cycling" }).click();
  await netChart.getByLabel("Net score Participant").click();
  await page.getByRole("option", { name: "Bot 0", exact: true }).click();
  await expect(netChart.locator(".net-value-label")).toHaveCount(0);
  await netChart.screenshot({ path: test.info().outputPath('dense-net-no-label-overlap.png') });
  await expect(netChart.locator(".recharts-surface").getByText("0", { exact: true })).toHaveCount(1);
  await netChart.locator(".recharts-bar-rectangle path").last().click();
  await expect(page.getByRole("dialog")).toHaveAccessibleName("Match 60 details");
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(netChart.locator(".recharts-surface")).toBeFocused();
  await netChart.getByLabel("Net score Participant").click();
  await page.getByRole("option", { name: "Bot 2", exact: true }).click();
  await expect(netChart.locator(".recharts-surface").getByText("0", { exact: true })).toHaveCount(1);
  const distribution = page.getByRole("region", { name: "Rank distribution", exact: true });
  await expect(distribution.locator(".recharts-radar-polygon")).toHaveCount(3);
  await distribution.locator(".recharts-radar-polygon").first().click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await page.locator('.recharts-surface[tabindex="0"]').count()).toBe(4);
  const axe = await new AxeBuilder({ page }).include(".benchmark-dashboard").analyze();
  expect(axe.violations).toEqual([]);
  expect(errors).toEqual([]);
  expect(privateReads).toEqual([]);
  expect(rendererReads).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

for (const participant of ["Bot 0", "Bot 2"]) test(`cumulative zero remains visible with only ${participant} in Recent 50`, async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.goto("/admin/benchmark/runs/run-1");
  const chart = page.getByRole("region", { name: "Cumulative net score", exact: true });
  await expect(chart.locator(".recharts-line-curve")).toHaveCount(3, { timeout: 20000 });
  await chart.getByRole("button", { name: "Cumulative net score options", exact: true }).click();
  await chart.getByLabel("Cumulative net score window").focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  for (const checkbox of await chart.getByRole("checkbox").all()) {
    if ((await checkbox.getAttribute("aria-label")) !== `Cumulative net score ${participant}`) {
      await checkbox.focus();
      await page.keyboard.press("Space");
      await expect(checkbox).not.toBeChecked();
    }
  }
  await expect(chart.locator(".recharts-line-curve")).toHaveCount(1);
  const zero = chart.locator(".recharts-reference-line-line");
  // Horizontal SVG lines have zero-height bounds, so Playwright calls them hidden.
  await expect(zero).toHaveCount(1);
  const line = await zero.evaluate(el => {
    const bounds = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return { y: bounds.y, width: bounds.width, stroke: style.stroke, visibility: style.visibility };
  });
  const plot = await chart.locator(".recharts-surface").boundingBox();
  expect(line.width).toBeGreaterThan(100);
  expect(line.stroke).not.toBe("none");
  expect(line.visibility).toBe("visible");
  expect(line.y).toBeGreaterThanOrEqual(plot!.y);
  expect(line.y).toBeLessThanOrEqual(plot!.y + plot!.height);
  await expect(chart.locator(".recharts-surface").getByText("0", { exact: true })).toHaveCount(1);
  await expect(page.getByRole("table", { name: "Participant statistics" })).toContainText("600,000");
});

test("hero/support charts survive responsive round trips and Radar filters retain the shared domain", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.goto("/admin/benchmark/runs/run-1");
  const distribution = page.getByRole("region", { name: "Rank distribution", exact: true });
  const ticks = distribution.locator(".recharts-polar-radius-axis-tick-value");
  await expect(ticks).toHaveText(["0%", "50%", "100%"]);
  await distribution.getByRole("button", { name: "Rank distribution options" }).click();
  await distribution.getByRole("checkbox", { name: "Rank distribution Bot 0", exact: true }).uncheck();
  await expect(ticks).toHaveText(["0%", "50%", "100%"]);
  for (const width of [390, 1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await expect(page.locator(".recharts-surface")).toHaveCount(4);
    await expect(page.getByRole("button", { name: "Performance", exact: true })).toHaveCount(0);
    for (const chart of await page.locator('[data-slot="chart"]').all()) await expect(chart).toBeVisible();
  }
  await distribution.getByRole("checkbox", { name: "Rank distribution Bot 1", exact: true }).uncheck();
  await distribution.getByRole("checkbox", { name: "Rank distribution Bot 2", exact: true }).uncheck();
  await expect(distribution.getByRole("alert")).toContainText("No Participants selected");
});

for (const theme of ["light", "dark"]) test(`drawable charts survive 320px and 200% text in ${theme}`, async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  await page.addInitScript(theme => { localStorage.setItem("driichi-theme", theme); }, theme);
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.goto("/admin/benchmark/runs/run-1");
  await page.evaluate(() => { document.documentElement.style.fontSize = "200%"; });
  await expect(page.locator(".recharts-surface")).toHaveCount(4, { timeout: 20000 });
  for (const name of ["Cumulative net score", "Moving-average rank", "Per-Match net score", "Rank distribution"]) {
    const chart = page.getByRole("region", { name, exact: true });
    await chart.locator(".recharts-surface").scrollIntoViewIfNeeded();
    if (name !== "Rank distribution") {
      const plot = chart.locator("clipPath rect").first();
      await expect.poll(async () => Number(await plot.getAttribute("height"))).toBeGreaterThan(100);
      expect(Number(await plot.getAttribute("width"))).toBeGreaterThan(10);
    } else {
      const host = (await chart.locator('[data-slot="chart"]').boundingBox())!;
      const legend = (await chart.locator(".house-chart-legend").boundingBox())!;
      for (const label of await chart.locator(".recharts-polar-angle-axis-tick-value").all()) {
        const box = (await label.boundingBox())!;
        expect(box.x).toBeGreaterThanOrEqual(host.x - 1);
        expect(box.x + box.width).toBeLessThanOrEqual(host.x + host.width + 1);
        expect(box.y).toBeGreaterThanOrEqual(host.y - 1);
        expect(box.y + box.height).toBeLessThanOrEqual(legend.y + 1);
      }
    }
    const marks = chart.locator(name === "Rank distribution" ? ".recharts-radar-polygon" : name === "Per-Match net score" ? ".recharts-bar-rectangle path" : ".recharts-line-curve");
    expect(await marks.count()).toBeGreaterThan(0);
    expect(await marks.first().evaluate(el => (el as SVGGraphicsElement).getBBox().width)).toBeGreaterThan(0);
    if (name !== "Rank distribution") {
      const ticks = chart.locator(".recharts-yAxis-tick-labels .recharts-cartesian-axis-tick-value");
      expect(await ticks.count()).toBeGreaterThan(0);
      const bounds = await chart.locator(".recharts-surface").boundingBox();
      for (const tick of await ticks.all()) {
        const box = await tick.boundingBox();
        expect(box!.x).toBeGreaterThanOrEqual(bounds!.x - 1);
        expect(box!.x + box!.width).toBeLessThanOrEqual(bounds!.x + bounds!.width + 1);
      }
      if (name === "Cumulative net score") expect(await ticks.allTextContents()).toEqual(expect.arrayContaining([expect.stringMatching(/^-/)]));
    }
    await expect(chart.getByRole("button", { name: `${name} options` })).toHaveAttribute("aria-expanded", "false");
  }
  await page.getByRole("heading", { name: "Match history", exact: true }).scrollIntoViewIfNeeded();
  await expect(page.getByRole("heading", { name: "Benchmark Run", exact: true })).toHaveCount(1);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("invalid persisted scores fail closed with Retry, not summary-only charts", async ({ page }) => {
  let invalid = true;
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: invalid ? { ...run, matches: [] } : run }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByRole("alert")).toContainText("Invalid Match evidence");
  await expect(page.getByRole("button", { name: "Retry", exact: true })).toBeVisible();
  await expect(page.locator(".recharts-surface")).toHaveCount(0);
  invalid = false;
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator(".recharts-surface")).toHaveCount(4, { timeout: 20000 });
  await expect(page.getByRole("alert")).toHaveCount(0);
});
