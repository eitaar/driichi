import { expect, test, type Locator } from "@playwright/test";

async function colors(locator: Locator, pseudo?: string) {
  return locator.evaluate((element, pseudo) => {
    const luminance = (rgb: number[]) => rgb.map(value => value / 255).map(c => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4).reduce((sum, c, i) => sum + c * [.2126, .7152, .0722][i], 0);
    let child: HTMLCanvasElement | null = null;
    let opacity = 1;
    // Composite whole layers: opacity applies to text AND background together, not text over an already faded background.
    for (let node: Element | null = element; node; node = node.parentElement) {
      const layer = document.createElement("canvas"); layer.width = 2; layer.height = 1;
      const context = layer.getContext("2d")!;
      const style = getComputedStyle(node);
      context.fillStyle = style.backgroundColor; context.fillRect(0, 0, 2, 1);
      if (child) { context.globalAlpha = opacity; context.drawImage(child, 0, 0); }
      else { context.fillStyle = getComputedStyle(element, pseudo).color; context.fillRect(1, 0, 1, 1); }
      child = layer; opacity = Number(style.opacity);
    }
    const context = document.createElement("canvas").getContext("2d")!;
    context.fillStyle = "white"; context.fillRect(0, 0, 2, 1);
    context.globalAlpha = opacity; context.drawImage(child!, 0, 0);
    const background = Array.from(context.getImageData(0, 0, 1, 1).data).slice(0, 3);
    const foreground = Array.from(context.getImageData(1, 0, 1, 1).data).slice(0, 3);
    const a = luminance(background), b = luminance(foreground);
    context.globalAlpha = 1; context.clearRect(0, 0, 1, 1);
    context.fillStyle = getComputedStyle(element).borderColor; context.fillRect(0, 0, 1, 1);
    const border = Array.from(context.getImageData(0, 0, 1, 1).data).slice(0, 3);
    return { contrast: (Math.max(a, b) + .05) / (Math.min(a, b) + .05), foreground, background, border };
  }, pseudo);
}

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/ui-foundation.html");
});

test("semantic consumers follow theme without inheriting legacy input styles", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await expect(page.getByLabel("Theme preference")).toHaveText("system");
  await expect(page.locator("html")).toHaveCSS("color-scheme", "dark");
  const card = page.locator('[data-slot="card"]').first();
  const dark = await card.evaluate(element => getComputedStyle(element).backgroundColor);
  await page.getByRole("button", { name: "Light", exact: true }).click();
  await expect(page.locator("html")).toHaveCSS("color-scheme", "light");
  await expect(card).not.toHaveCSS("background-color", dark);
  await expect(page.getByLabel("Name", { exact: true })).toHaveCSS("height", "44px");
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
    await expect(input).toHaveCSS("color", await page.locator('[data-slot="card"]').first().evaluate(element => getComputedStyle(element).color));
    expect((await colors(input)).contrast, `${theme} filled Input contrast`).toBeGreaterThanOrEqual(4.5);
  }
});

test("primitive sizes stay readable on desktop and phone without enlarging compact variants", async ({ page }) => {
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 960 });
    const input = page.locator('[data-slot="input"]').first();
    await input.fill("Readable input");
    await expect(input).toHaveCSS("font-size", "16px");
    await expect(input).toHaveCSS("height", "44px");
    await expect(page.locator('[data-slot="select-trigger"]').first()).toHaveCSS("height", "44px");
    await expect(page.getByLabel("Compact Participant")).toHaveCSS("height", "28px");
    for (const [name, height] of [["Normal action", "40px"], ["Large action", "48px"], ["Small action", "28px"], ["Compact action", "28px"]]) {
      await expect(page.getByRole("button", { name, exact: true })).toHaveCSS("height", height);
    }
  }
});

test("filled controls and action states use readable neutral colors in Light and Dark", async ({ page }) => {
  await page.getByLabel("Name", { exact: true }).fill("Readable input");
  for (const theme of ["Light", "Dark"]) {
    await page.getByRole("button", { name: theme, exact: true }).click();
    const checkbox = page.getByRole("checkbox", { name: "Ready for next Match", exact: true });
    await checkbox.focus();
    await page.keyboard.press("Space");
    await expect(checkbox).toBeChecked();
    const labels = [page.locator("#inherited-label"), page.getByRole("link", { name: "DOUBLE RIICHI", exact: true }), page.getByRole("link", { name: "Admin sign in", exact: true }), page.locator("main"), page.getByLabel("Name", { exact: true }), page.getByLabel("Locked name"), page.getByLabel("Locked Participant", { exact: true }), page.getByLabel("Participant", { exact: true }), page.locator('label[for="ready"]'), page.locator('label[for="disabled-ready"]'), page.locator("#name-help"), page.locator("#name-error"), ...["Normal action", "Pending action", "Disabled action"].map(name => page.getByRole("button", { name, exact: true }))];
    const metrics = [];
    for (const label of labels) {
      const result = await colors(label);
      metrics.push({ label: await label.getAttribute("id") ?? await label.textContent(), ...result });
      expect.soft(result.contrast, `${theme} ${await label.getAttribute("id") ?? await label.textContent()} contrast`).toBeGreaterThanOrEqual(4.5);
      if (await label.getAttribute("id") !== "name-error") {
        for (const rgb of [result.foreground, result.background, result.border]) expect.soft(Math.max(...rgb) - Math.min(...rgb), `${theme} normal control is monochrome`).toBeLessThanOrEqual(1);
      }
    }
    expect((await colors(page.getByLabel("Empty name"), "::placeholder")).contrast, `${theme} placeholder contrast`).toBeGreaterThanOrEqual(4.5);
    await page.getByLabel("Name", { exact: true }).focus();
    const focused = await colors(page.getByLabel("Name", { exact: true }));
    expect(focused.contrast, `${theme} focused Input contrast`).toBeGreaterThanOrEqual(4.5);
    expect(Math.max(...focused.border) - Math.min(...focused.border), `${theme} focus ring is neutral`).toBeLessThanOrEqual(1);
    for (const rgb of Object.values(await colors(checkbox)).filter(Array.isArray)) expect.soft(Math.max(...rgb) - Math.min(...rgb)).toBeLessThanOrEqual(1);
    await page.getByLabel("Participant", { exact: true }).focus();
    await page.keyboard.press("ArrowDown");
    const option = page.getByRole("option", { name: "Birch" });
    await expect(option).toBeVisible();
    expect((await colors(option)).contrast).toBeGreaterThanOrEqual(4.5);
    await page.keyboard.press("Home");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(page.getByLabel("Participant", { exact: true })).toContainText("Birch");
    await test.info().attach(`foundation-${theme}`, { body: await page.locator('[data-slot="card"]').first().screenshot(), contentType: "image/png" });
    await test.info().attach(`contrast-${theme}`, { body: JSON.stringify(metrics, null, 2), contentType: "application/json" });
    await checkbox.uncheck();
  }
});

test("Participant chart hues contrast with both themes while rank patterns stay neutral", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  for (const theme of ["Light", "Dark"]) {
    await page.getByRole("button", { name: theme, exact: true }).click();
    const distribution = page.getByRole("region", { name: "Rank distribution", exact: true });
    await expect(distribution.locator("pattern")).toHaveCount(4);
    const samples = await distribution.evaluate(element => {
      const context = document.createElement("canvas").getContext("2d")!;
      const rgb = (color: string) => { context.fillStyle = color; context.fillRect(0, 0, 1, 1); return Array.from(context.getImageData(0, 0, 1, 1).data).slice(0, 3); };
      const luminance = (rgb: number[]) => rgb.map(v => v / 255).map(c => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4).reduce((s, c, i) => s + c * [.2126, .7152, .0722][i], 0);
      const style = getComputedStyle(element);
      const background = luminance(rgb(style.getPropertyValue("--card")));
      return {
        participants: [1, 2, 3, 4].map(i => { const channels = rgb(style.getPropertyValue(`--chart-${i}`)); const l = luminance(channels); return { channels, contrast: (Math.max(l, background) + .05) / (Math.min(l, background) + .05) }; }),
        ranks: Array.from(element.querySelectorAll("pattern rect")).map(rect => rgb(getComputedStyle(rect).fill)),
      };
    });
    for (const p of samples.participants) {
      expect(p.contrast, `${theme} chart graphic contrast`).toBeGreaterThanOrEqual(3);
      expect(Math.max(...p.channels) - Math.min(...p.channels), "Participant hue").toBeGreaterThan(10);
    }
    for (const rank of samples.ranks) expect(Math.max(...rank) - Math.min(...rank), "Rank fill is neutral").toBeLessThanOrEqual(1);
    await expect(distribution.locator("pattern path")).toHaveCount(4);
    await test.info().attach(`chart-colors-${theme}`, { body: JSON.stringify(samples, null, 2), contentType: "application/json" });
  }
});

test("nested chart options stay closed on desktop and usable at narrow enlarged text", async ({ page }) => {
  for (const theme of ["Light", "Dark"]) {
    await page.getByRole("button", { name: theme, exact: true }).click();
    for (const viewport of [{ width: 1440, height: 960 }, { width: 390, height: 844 }, { width: 320, height: 844 }]) {
      await page.setViewportSize(viewport);
      await page.evaluate(enlarged => { document.documentElement.style.fontSize = enlarged ? "200%" : ""; }, viewport.width === 320);
      const chart = page.getByRole("region", { name: "Cumulative net score", exact: true });
      const options = chart.getByRole("button", { name: "Cumulative net score options", exact: true });
      await expect(options.locator("xpath=..")).toHaveJSProperty("tagName", "H3");
      await expect(options).toHaveAttribute("aria-expanded", "false");
      await expect(chart.getByRole("combobox")).toHaveCount(0);
      await options.focus();
      await page.keyboard.press("Enter");
      await expect(chart.getByLabel("Cumulative net score window")).toBeVisible();
      const checkbox = chart.getByRole("checkbox").first();
      await checkbox.focus();
      await page.keyboard.press("Space");
      await expect(checkbox).not.toBeChecked();
      await options.click();
      await options.click();
      await expect(checkbox).not.toBeChecked();
      await checkbox.check();
      await options.click();
      await expect(chart.locator(".recharts-surface")).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    }
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
