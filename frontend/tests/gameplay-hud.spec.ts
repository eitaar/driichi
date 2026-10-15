/// <reference types="../node_modules/@types/node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// Authoritative-shape socket fixture: DOM acceptance, not native integration evidence.
const fixtures = JSON.parse(readFileSync(resolve(process.cwd(), "tests/fixtures/task12-projection.json"), "utf8"));
async function install(page: Page, theme: string) {
  await page.route("**/assets/characters/**", route => route.fulfill({ status: 404, body: "missing fixture asset" }));
  await page.addInitScript(({ room, state, theme }) => {
    localStorage.setItem("driichi-theme", theme);
    sessionStorage.setItem("driichi:participant:123456", "P1");
    const browser = window as any;
    browser.__hudRoom = room; browser.__hudState = state; browser.__hudSent = [];
    const NativeSocket = window.WebSocket;
    class HudSocket {
      static OPEN = 1; readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: { data: string }) => void) | null = null;
      onclose: (() => void) | null = null;
      constructor() {
        browser.__hudSocket = this;
        setTimeout(() => { this.onopen?.(); this.emit({ type: "snapshot", room, state }); }, 0);
      }
      send(value: string) { browser.__hudSent.push(JSON.parse(value)); }
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
      close() { this.readyState = 3; this.onclose?.(); }
    }
    Object.defineProperty(window, "WebSocket", { configurable: true, value: Object.assign(function (url: string, protocols?: string | string[]) {
      return url.includes("/ws/v1/rooms/") ? new HudSocket() : new NativeSocket(url, protocols);
    }, { OPEN: 1 }) });
  }, { room: fixtures.room_envelopes["4p-red-east"], state: fixtures.projections["4p-red-east"], theme });
}
async function decision(page: Page, actions: unknown[], id = "hud-choice", remaining = 30000) {
  await page.evaluate(({ actions, id, remaining }) => {
    const w = window as any;
    w.__hudState = { ...w.__hudState, decision: { decision_id: id, kind: "Reaction", actions, remaining_ms: remaining } };
    w.__hudSocket.emit({ type: "snapshot", room: w.__hudRoom, state: w.__hudState });
  }, { actions, id, remaining });
}
const actions = [
  { action_id: "ron", action: "Ron" }, { action_id: "pass", action: "Pass" },
  { action_id: "chi-a", action: { Chi: { target: 1, called: 8, consumed: [0, 4] } } },
  { action_id: "chi-b", action: { Chi: { target: 1, called: 8, consumed: [4, 12] } } },
  { action_id: "pon", action: { Pon: { target: 1, called: 10, consumed: [8, 9] } } },
  { action_id: "kan", action: { Ankan: { consumed: [16, 17, 18, 19] } } },
  { action_id: "riichi", action: { riichi_discard: { tile: 1 } } },
];

for (const viewport of [{ width: 1024, height: 600 }, { width: 1440, height: 900 }]) {
  for (const theme of ["light", "dark", "system"]) {
    for (const reduced of [false, true]) {
      test(`${viewport.width} ${theme} ${reduced ? "reduced" : "normal"}: themed HUD and keyboard settings/candidates`, async ({ page }) => {
        await page.setViewportSize(viewport);
        await page.emulateMedia({ colorScheme: "dark", reducedMotion: reduced ? "reduce" : "no-preference" });
        const cdp = await page.context().newCDPSession(page);
        await cdp.send("Emulation.setEmulatedMedia", { features: [
          { name: "prefers-color-scheme", value: "dark" },
          { name: "prefers-reduced-motion", value: reduced ? "reduce" : "no-preference" },
          { name: "prefers-reduced-transparency", value: reduced ? "reduce" : "no-preference" },
        ] });
        await install(page, theme);
        await page.goto("/room/123456/lobby");
        const deck = page.getByTestId("action-deck");
        await expect(deck).toBeVisible();
        expect(await page.locator("html").evaluate(el => el.classList.contains("dark"))).toBe(theme !== "light");
        await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20000 });
        await expect(deck.getByRole("button")).toHaveCount(0); // discard-only uses tile targets
        const tiles = page.getByRole("group", { name: "Your concealed hand" });
        await expect(tiles.getByRole("button", { disabled: false })).toHaveCount(14);
        const tileStyle = await tiles.getByRole("button").first().evaluate(el => ({ background: getComputedStyle(el).backgroundColor, image: el.querySelector("img")!.src }));
        const sceneBackground = await page.locator(".three-table").evaluate(el => getComputedStyle(el).backgroundColor);
        await decision(page, actions);
        for (const name of ["Ron", "Pass", "Chi (2)", "Pon", "Kan", "Riichi"]) await expect(deck.getByRole("button", { name, exact: true })).toBeVisible();
        expect((await deck.boundingBox())!.y + (await deck.boundingBox())!.height).toBeLessThan((await tiles.boundingBox())!.y);
        const audit = await new AxeBuilder({ page }).include(".action-deck").include(".gameplay-controls").analyze();
        expect(audit.violations.filter(violation => violation.impact === "serious" || violation.impact === "critical")).toEqual([]);
        const hud = page.locator(".connection-state");
        await expect(hud).toHaveCSS("backdrop-filter", reduced ? "none" : "blur(8px)");
        await page.screenshot({ path: `../.superpowers/sdd/ui-ux-cutover/task-5a-${viewport.width}-${theme}-${reduced}.png` });
        const before = await hud.evaluate(el => getComputedStyle(el).backgroundColor);
        const deckBefore = await deck.evaluate(el => getComputedStyle(el).backgroundColor);
        const actionBefore = await deck.locator('[data-action-id="ron"]').evaluate(el => getComputedStyle(el).backgroundColor);
        const chi = deck.getByRole("button", { name: "Chi (2)", exact: true });
        await chi.focus(); await page.keyboard.press("Enter");
        const candidate = page.getByRole("dialog", { name: "Choose a legal candidate" });
        await expect(candidate).toBeVisible();
        await expect(candidate.locator('[data-action-id="chi-a"]')).toBeFocused();
        const candidateBefore = await candidate.evaluate(el => getComputedStyle(el).backgroundColor);
        const remainingBefore = Number((await page.getByTestId("decision-timer").textContent())!.replace("s", ""));
        const shell = page.getByTestId("gameplay-shell");
        const enqueued = Number(await shell.getAttribute("data-animation-enqueued-count"));
        // Exercise provider changes while a Decision/dialog is mounted; no remount or timer reset.
        await page.evaluate(() => window.dispatchEvent(new StorageEvent("storage", { key: "driichi-theme", storageArea: localStorage, newValue: document.documentElement.classList.contains("dark") ? "light" : "dark" })));
        await expect.poll(() => hud.evaluate(el => getComputedStyle(el).backgroundColor)).not.toBe(before);
        await expect.poll(() => candidate.evaluate(el => getComputedStyle(el).backgroundColor)).not.toBe(candidateBefore);
        await expect.poll(() => deck.evaluate(el => getComputedStyle(el).backgroundColor)).not.toBe(deckBefore);
        await expect.poll(() => deck.locator('[data-action-id="ron"]').evaluate(el => getComputedStyle(el).backgroundColor)).not.toBe(actionBefore);
        await expect(candidate.locator('[data-action-id="chi-a"]')).toBeFocused();
        expect(Number((await page.getByTestId("decision-timer").textContent())!.replace("s", ""))).toBeLessThanOrEqual(remainingBefore);
        await page.evaluate(() => { const w = window as any; w.__hudSocket.emit({ type: "game_update", event: { type: "dahai", actor: 0, tile: 1 }, state: w.__hudState }); });
        await expect(shell).toHaveAttribute("data-animation-enqueued-count", String(enqueued + 1));
        await page.evaluate(() => window.dispatchEvent(new StorageEvent("storage", { key: "driichi-theme", storageArea: localStorage, newValue: document.documentElement.classList.contains("dark") ? "light" : "dark" })));
        await expect(shell).toHaveAttribute("data-animation-enqueued-count", String(enqueued + 1));
        await expect(candidate.locator('[data-action-id="chi-a"]')).toBeFocused();
        await expect(page.locator(".table-tile-hit").first()).toHaveCSS("background-color", tileStyle.background);
        expect(await page.locator(".table-tile-hit img").first().evaluate(el => (el as HTMLImageElement).src)).toBe(tileStyle.image);
        await expect(page.locator(".three-table")).toHaveCSS("background-color", sceneBackground);
        await page.keyboard.press("Escape"); await expect(chi).toBeFocused();
        await page.keyboard.press("Enter");
        await candidate.locator('[data-action-id="chi-b"]').focus(); await page.keyboard.press("Enter");
        await expect(deck).toHaveAttribute("aria-busy", "true");
        await expect(deck.locator('[data-action-id="ron"]')).toBeDisabled();
        await page.evaluate(() => { const w = window as any; w.__hudSocket.emit({ type: "action_result", decision_id: "hud-choice", action_id: "chi-b", status: "rejected", code: "fixture rejection" }); });
        await page.keyboard.press("Escape");
        await expect(page.getByRole("alert").filter({ hasText: "fixture rejection" })).toBeVisible();
        const settings = page.getByRole("button", { name: "Settings", exact: true });
        await settings.focus(); await page.keyboard.press("Enter");
        const panel = page.getByRole("dialog", { name: "Settings", exact: true });
        await expect(panel).toBeVisible();
        const settingsSurface = await panel.evaluate(el => getComputedStyle(el).backgroundColor);
        await page.evaluate(() => window.dispatchEvent(new StorageEvent("storage", { key: "driichi-theme", storageArea: localStorage, newValue: document.documentElement.classList.contains("dark") ? "light" : "dark" })));
        await expect.poll(() => panel.evaluate(el => getComputedStyle(el).backgroundColor)).not.toBe(settingsSurface);
        const master = panel.getByRole("slider", { name: "Master", exact: true });
        await master.focus(); await page.keyboard.press("ArrowLeft");
        await expect(master).toHaveAttribute("aria-valuenow", "0.95");
        await panel.getByRole("switch", { name: "Voice enabled" }).click();
        const image = panel.getByLabel("Table surface image", { exact: true });
        await image.setInputFiles({ name: "invalid.svg", mimeType: "image/svg+xml", buffer: Buffer.from("<svg/>") });
        await expect(image).toHaveAttribute("aria-invalid", "true");
        await expect(panel.getByRole("alert")).toContainText("Choose a PNG");
        await page.keyboard.press("Escape"); await expect(settings).toBeFocused();
        await page.keyboard.press("Enter");
        await expect(panel.getByRole("slider", { name: "Master", exact: true })).toHaveAttribute("aria-valuenow", "0.95");
        await expect(panel.getByRole("switch", { name: "Voice enabled" })).not.toBeChecked();
        await page.keyboard.press("Escape");
        await decision(page, [], "idle"); await expect(deck.getByRole("button")).toHaveCount(0);
      });
    }
  }
}

test("keeps expired/rejected/offline actions authoritative and image data local", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 600 });
  await install(page, "dark");
  const uploads: string[] = [];
  page.on("request", request => { if (request.method() !== "GET") uploads.push(request.url()); });
  await page.goto("/room/123456/lobby");
  await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20000 });
  await decision(page, actions.slice(0, 2), "expiring", 1000);
  await expect(page.getByTestId("decision-timer")).toHaveText("0s", { timeout: 4000 });
  await page.getByRole("button", { name: "Pass", exact: true }).click();
  expect(await page.evaluate(() => (window as any).__hudSent)).toEqual([{ type: "submit_action", decision_id: "expiring", action_id: "pass" }]);
  await page.evaluate(() => { const w = window as any; w.__hudSocket.emit({ type: "action_result", decision_id: "expiring", action_id: "pass", status: "rejected", code: "decision_expired" }); });
  await expect(page.getByRole("alert")).toContainText("decision_expired");
  await decision(page, actions, "next");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const panel = page.getByRole("dialog", { name: "Settings", exact: true });
  const png = await page.evaluate(() => {
    const canvas = document.createElement("canvas"); canvas.width = 2; canvas.height = 2;
    return canvas.toDataURL("image/png").split(",")[1];
  });
  await panel.getByLabel("Table surface image", { exact: true }).setInputFiles({ name: "local.png", mimeType: "image/png", buffer: Buffer.from(png, "base64") });
  await expect(panel.getByRole("button", { name: "Clear surface image" })).toBeEnabled();
  await panel.getByRole("button", { name: "Clear surface image" }).click();
  await expect(panel.getByRole("button", { name: "Clear surface image" })).toBeDisabled();
  await panel.getByLabel("Table surround image", { exact: true }).setInputFiles({ name: "broken.png", mimeType: "image/png", buffer: Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0]) });
  await expect(panel.getByRole("alert")).toContainText("could not be decoded");
  await page.keyboard.press("Escape");
  await page.evaluate(() => (window as any).__hudSocket.onerror());
  await expect(page.getByRole("status").filter({ hasText: "Network connection interrupted" })).toBeVisible();
  for (const button of await page.getByTestId("action-deck").getByRole("button").all()) await expect(button).toBeDisabled();
  expect(uploads).toEqual([]);
});
