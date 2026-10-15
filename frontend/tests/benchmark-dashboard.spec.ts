import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.use({ hasTouch: true });

const roster = ["z]{}", "a", "b"].map((participant_id, initial_seat) => ({ participant_id, initial_seat, display_name: `Bot ${initial_seat}`, participant_kind: "built_in_bot", character_id: null }));
const run = { run_id: "run-1", room_code: "123456", game_mode: "3p-red-east", target: 60, completed: 60, status: "completed", reason: null, failed_match_id: null, roster,
  matches: Array.from({ length: 60 }, (_, i) => ({ sequence: i + 1, match_id: `match-${i + 1}`, results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat: (seat + i) % 3, rank: seat + 1, final_score: [45000, 35000, 25000][seat] })) })), statistics: [],
};
for (const mobile of [false, true]) test(`${mobile ? "touch Drawer" : "desktop Sheet"}: real chart rendering, numeric parity and keyboard details`, async ({ page }) => {
  await page.setViewportSize(mobile ? { width: 390, height: 844 } : { width: 1280, height: 900 });
  const errors: string[] = [], privateReads: string[] = [], rendererReads: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => { if (request.url().endsWith("/live")) privateReads.push(request.url()); if (/table-scene|three-renderer|renderer\.worker/.test(request.url())) rendererReads.push(request.url()); });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: run }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.locator(".recharts-surface")).toHaveCount(4, { timeout: 20000 });
  for (const chart of await page.locator('[data-slot="chart"]').all()) {
    await expect(chart).toBeVisible();
    expect((await chart.boundingBox())!.height).toBeGreaterThanOrEqual(240);
    expect((await chart.boundingBox())!.width).toBeGreaterThan(100);
  }
  await expect(page.locator(".recharts-line-curve")).toHaveCount(6);
  await expect(page.getByText("Rank 4", { exact: true })).toHaveCount(0);
  await page.getByLabel("Cumulative net score window").selectOption("50");
  await expect(page.getByText(/Window net:.*Bot 0.*500,000/)).toBeVisible();
  await expect(page.getByText("All completed Matches: n = 60; denominator per Participant.")).toBeVisible();
  await expect(page.getByRole("table", { name: "Participant statistics" })).toContainText("600,000");
  const surface = page.getByRole("region", { name: "Cumulative net score", exact: true }).locator(".recharts-surface");
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
  await netChart.locator(".recharts-bar-rectangle").last().click();
  await expect(page.getByRole("dialog")).toHaveAccessibleName("Match 60 details");
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(netChart.locator(".recharts-surface")).toBeFocused();
  const distribution = page.getByRole("region", { name: "Rank distribution", exact: true });
  await distribution.locator(".recharts-bar-rectangle").first().click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await page.locator('.recharts-surface[tabindex="0"]').count()).toBe(4);
  const axe = await new AxeBuilder({ page }).include(".benchmark-dashboard").analyze();
  expect(axe.violations).toEqual([]);
  expect(errors).toEqual([]);
  expect(privateReads).toEqual([]);
  expect(rendererReads).toEqual([]);
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
