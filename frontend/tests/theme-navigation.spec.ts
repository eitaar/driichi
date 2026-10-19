import { expect, test } from "./support/task7b-evidence";

const themeLabel = /^Theme:/;
test("shipping public and Admin theme choices persist, follow System and synchronize tabs", async ({ page, context }) => {
  await context.route("**/api/v1/**", route => route.fulfill({ json: [] }));
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  const theme = page.getByRole("button", { name: themeLabel });
  await expect(theme).toHaveAccessibleName("Theme: System. Switch to Light");
  await expect(page.locator("html")).toHaveClass(/dark/);
  const other = await context.newPage();
  await other.goto("/admin");
  for (const [label, next, value, dark] of [["Light", "Dark", "light", false], ["Dark", "System", "dark", true], ["System", "Light", "system", true]] as const) {
    await theme.focus(); await page.keyboard.press("Enter");
    const accessibleName = `Theme: ${label}. Switch to ${next}`;
    await expect(theme).toHaveAccessibleName(accessibleName);
    expect(await page.evaluate(() => localStorage.getItem("driichi-theme"))).toBe(value);
    await expect.poll(() => page.locator("html").evaluate(el => el.classList.contains("dark"))).toBe(dark);
    await expect(other.getByRole("button", { name: themeLabel })).toHaveAccessibleName(accessibleName);
    await page.reload();
    await expect(theme).toHaveAccessibleName(accessibleName);
  }
  await page.emulateMedia({ colorScheme: "light" });
  await expect.poll(() => page.locator("html").evaluate(el => el.classList.contains("dark"))).toBe(false);
  await other.getByRole("button", { name: themeLabel }).click();
  await other.getByRole("button", { name: themeLabel }).click();
  await expect(theme).toHaveAccessibleName("Theme: Dark. Switch to System");
  await expect(page.locator("html")).toHaveClass(/dark/);
  await other.close();
});

for (const path of ["/", "/admin"]) for (const mode of ["Light", "Dark"]) test(`${path} ${mode} theme navigation reflows at 320px and 200% text`, async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.route("**/api/v1/**", route => route.fulfill({ json: new URL(route.request().url()).pathname === "/api/v1/admin/replays"
    ? { replays: [], offset: 0, limit: 50, total: 0, has_more: false } : [] }));
  await page.goto(path);
  await page.evaluate(() => document.documentElement.style.fontSize = "200%");
  const theme = page.getByRole("button", { name: themeLabel });
  await theme.click();
  if (mode === "Dark") await theme.click();
  await expect(theme).toHaveAccessibleName(`Theme: ${mode}. Switch to ${mode === "Light" ? "Dark" : "System"}`);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await theme.scrollIntoViewIfNeeded();
  await expect(theme).toBeInViewport();
  await expect(theme).toHaveCSS("width", "44px");
  await expect(theme).toHaveCSS("height", "44px");
  await expect(theme).toHaveCSS("border-top-width", "0px");
  expect(await theme.evaluate(element => {
    const header = element.closest("header")!;
    return header.contains(element) && header.getBoundingClientRect().right >= element.getBoundingClientRect().right && getComputedStyle(element).position === "static";
  })).toBe(true);
  await theme.focus(); await page.keyboard.press("Space");
  await expect(theme).toBeFocused();
  if (path === "/admin") {
    const trigger = page.getByRole("button", { name: "Toggle navigation" });
    await trigger.focus(); await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("link", { name: "Replays", exact: true }).focus();
    await page.keyboard.press("Enter");
    await expect(dialog).not.toBeVisible();
    await expect(page.getByRole("heading", { name: "No Replays yet.", exact: true })).toBeVisible();
    await expect(trigger).toHaveCount(1);
    await expect(page).toHaveURL(/\/admin\/replays$/);
  }
  expect(errors).toEqual([]);
});
