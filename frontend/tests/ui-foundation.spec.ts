import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/ui-foundation.html");
});

test("semantic consumers follow theme without inheriting legacy input styles", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await expect(page.getByLabel("Theme preference")).toHaveText("system");
  await expect(page.locator("html")).toHaveCSS("color-scheme", "dark");
  const card = page.locator('[data-slot="card"]');
  const dark = await card.evaluate(element => getComputedStyle(element).backgroundColor);
  await page.getByRole("button", { name: "Light", exact: true }).click();
  await expect(page.locator("html")).toHaveCSS("color-scheme", "light");
  await expect(card).not.toHaveCSS("background-color", dark);
  await expect(page.getByLabel("Name", { exact: true })).toHaveCSS("height", "32px");
  await expect(page.getByLabel("Name", { exact: true })).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await page.getByRole("button", { name: "Dark", exact: true }).click();
  await expect(card).toHaveCSS("background-color", dark);
  await page.reload();
  await expect(page.getByLabel("Theme preference")).toHaveText("dark");
  await page.getByRole("button", { name: "System", exact: true }).click();
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveCSS("color-scheme", "light");
  await expect(card).not.toHaveCSS("background-color", dark);
});

test("Base render dialog has a title, traps focus and restores its trigger", async ({ page }) => {
  const trigger = page.getByRole("button", { name: "Open dialog" });
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: "Accessible overlay" });
  await expect(dialog).toBeVisible();
  await expect(page.getByLabel("Dialog name")).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(dialog.getByRole("button", { name: "Close" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Dialog name")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await expect(trigger).toBeFocused();
});
