import { expect, type Page } from "@playwright/test";

/** Shared by saved-fixture and live-service checks so control callers cannot drift. */
export async function verifyReplayControls(page: Page) {
  await page.getByRole("button", { name: /next event/i }).click();
  const controls = page.locator(".replay-playback");
  await controls.getByRole("button", { name: /previous event/i }).focus();
  await page.keyboard.press("Tab");
  await expect(controls.getByRole("button", { name: /^play$/i })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(controls.getByRole("button", { name: /next event/i })).toBeFocused();
  await page.keyboard.press("Tab");
  const speed = controls.getByRole("combobox", { name: /playback speed/i });
  await expect(speed).toBeFocused();
  expect(await speed.locator("option").evaluateAll(options => options.map(option => (option as HTMLOptionElement).value))).toEqual(["0.5", "1", "2", "4", "8", "16", "32", "64"]);
  await speed.selectOption("2");
  await expect(speed).toHaveValue("2");
  await page.keyboard.press("Tab");
  await expect(controls.getByRole("link", { name: /^replay library$/i })).toBeFocused();
  await page.locator(".skip-link").focus();
  await expect(page.locator(".skip-link")).toBeFocused();
  const chapters = page.getByRole("navigation", { name: "Kyoku chapters" }).getByRole("button");
  expect(await chapters.count()).toBeGreaterThan(0);
  for (const chapter of await chapters.all()) {
    await page.keyboard.press("Tab");
    await expect(chapter).toBeFocused();
  }
  await chapters.first().focus();
  await page.keyboard.press("Enter");
  await expect(chapters.first()).toHaveAttribute("aria-current", "step");
  await expect(controls.getByRole("slider", { name: /replay position/i })).toHaveValue("0");
  await expect(controls.getByRole("button", { name: /^play$/i })).toBeVisible();
  await chapters.last().focus();
  await page.keyboard.press("Tab");
  await expect(page.locator(".gameplay-controls .theme-toggle")).toBeFocused();
  await page.keyboard.press("Tab");
  const settings = page.getByRole("button", { name: "Settings", exact: true });
  await expect(settings).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(settings).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(controls.getByRole("slider", { name: /replay position/i })).toBeFocused();
}
