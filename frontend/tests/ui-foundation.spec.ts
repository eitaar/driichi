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

test("filled Input has readable semantic foreground in both themes", async ({ page }) => {
  const input = page.getByLabel("Name", { exact: true });
  await input.fill("Readable participant name");
  for (const theme of ["Light", "Dark"]) {
    await page.getByRole("button", { name: theme, exact: true }).click();
    await expect(input).toHaveValue("Readable participant name");
    await expect(input).toHaveCSS("color", await page.locator('[data-slot="card"]').evaluate(element => getComputedStyle(element).color));
    const contrast = await input.evaluate(element => {
      const context = document.createElement("canvas").getContext("2d")!;
      const luminance = () => {
        const channels = Array.from(context.getImageData(0, 0, 1, 1).data).slice(0, 3).map(value => {
          const channel = value / 255;
          return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
        });
        return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
      };
      // Canvas resolves CSS color spaces and composites the translucent Input over its Card.
      for (const color of [getComputedStyle(element.closest('[data-slot="card"]')!).backgroundColor, getComputedStyle(element).backgroundColor]) {
        context.fillStyle = color;
        context.fillRect(0, 0, 1, 1);
      }
      const background = luminance();
      context.fillStyle = getComputedStyle(element).color;
      context.fillRect(0, 0, 1, 1);
      const foreground = luminance();
      return (Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05);
    });
    expect(contrast, `${theme} filled Input contrast`).toBeGreaterThanOrEqual(4.5);
  }
});

test("vertical ToggleGroup navigates with ArrowDown and ArrowUp", async ({ page }) => {
  const group = page.getByRole("group", { name: "Text alignment" });
  const left = group.getByRole("button", { name: "Align left" });
  const center = group.getByRole("button", { name: "Align center" });
  await left.click();
  await expect(left).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("ArrowDown");
  await expect(center).toBeFocused();
  await page.keyboard.press("Space");
  await expect(center).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("ArrowUp");
  await expect(left).toBeFocused();
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
