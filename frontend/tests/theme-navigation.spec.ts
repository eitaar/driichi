import { expect, test } from "@playwright/test";

test("shipping public and Admin theme choices persist, follow System and synchronize tabs", async ({ page, context }) => {
  await context.route("**/api/v1/**", route => route.fulfill({ json: [] }));
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  const theme = page.getByRole("combobox", { name: "Theme", exact: true });
  await expect(theme).toContainText("System");
  await expect(page.locator("html")).toHaveClass(/dark/);
  const other = await context.newPage();
  await other.goto("/admin");
  for (const [label, value, dark] of [["Light", "light", false], ["Dark", "dark", true], ["System", "system", true]] as const) {
    await theme.focus(); await page.keyboard.press("Enter");
    await page.getByRole("option", { name: label, exact: true }).click();
    await expect(theme).toContainText(label);
    expect(await page.evaluate(() => localStorage.getItem("driichi-theme"))).toBe(value);
    await expect.poll(() => page.locator("html").evaluate(el => el.classList.contains("dark"))).toBe(dark);
    await expect(other.getByRole("combobox", { name: "Theme", exact: true })).toContainText(label);
    await page.reload();
    await expect(theme).toContainText(label);
  }
  await page.emulateMedia({ colorScheme: "light" });
  await expect.poll(() => page.locator("html").evaluate(el => el.classList.contains("dark"))).toBe(false);
  await other.getByRole("combobox", { name: "Theme", exact: true }).click();
  await other.getByRole("option", { name: "Dark", exact: true }).click();
  await expect(theme).toContainText("Dark");
  await expect(page.locator("html")).toHaveClass(/dark/);
  await other.close();
});

for (const path of ["/", "/admin"]) for (const mode of ["Light", "Dark"]) test(`${path} ${mode} theme navigation reflows at 320px and 200% text`, async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  await page.route("**/api/v1/**", route => route.fulfill({ json: [] }));
  await page.goto(path);
  await page.evaluate(() => document.documentElement.style.fontSize = "200%");
  const theme = page.getByRole("combobox", { name: "Theme", exact: true });
  await theme.click();
  await page.getByRole("option", { name: mode, exact: true }).click();
  await expect(theme).toContainText(mode);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await theme.scrollIntoViewIfNeeded();
  await expect(theme).toBeInViewport();
  await theme.focus(); await page.keyboard.press("Enter");
  await expect(page.getByRole("listbox")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(theme).toBeFocused();
});
