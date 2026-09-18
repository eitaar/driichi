/// <reference types="../node_modules/@types/node" />

import AxeBuilder from "@axe-core/playwright";
import { expect, test as base, type BrowserContext, type Page } from "@playwright/test";
import { resolve } from "node:path";
import {
  ensureScreenshotDirectory,
  screenshotPath,
  startRealServer,
  type RealServerHarness,
} from "./support/real-server";

const test = base.extend<{}, { harness: RealServerHarness }>({
  harness: [
    async ({}, use) => {
      const harness = await startRealServer();
      try {
        await use(harness);
      } finally {
        await harness.stop();
      }
    },
    { scope: "worker", timeout: 180_000 },
  ],
});

test.describe.configure({ mode: "serial", timeout: 180_000 });

type MatchMode = "3p-red-east" | "4p-red-east";

const screenshotDirectory = resolve(ensureScreenshotDirectory());

async function expectAccessible(page: Page, surface: string) {
  const axe = new AxeBuilder({ page });
  if (surface.includes("playing") || surface.includes("post-match")) {
    axe.include([".gameplay-topbar", ".gameplay-rail"]);
  }
  const results = await axe.analyze();
  expect(
    results.violations,
    `${surface}: ${results.violations.map((violation) => violation.id).join(", ")}`,
  ).toEqual([]);
}

async function loginAdmin(page: Page, harness: RealServerHarness) {
  await page.goto(`${harness.frontendOrigin}/admin/login`);
  await page.getByLabel("Username").fill(harness.adminUsername);
  await page.getByLabel("Password").fill(harness.adminPassword);
  await page.getByRole("button", { name: /^sign in$/i }).click();
  await expect(page.getByRole("heading", { name: /admin session active/i })).toBeVisible({ timeout: 20_000 });
  await page.getByRole("link", { name: /open admin/i }).click();
  await expect(page.getByRole("heading", { name: /admin rooms/i })).toBeVisible();
  await expectAccessible(page, "admin rooms");
}

async function createRoom(
  page: Page,
  mode: MatchMode,
): Promise<string> {
  await page.getByRole("button", { name: "Create Room" }).first().click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Room name").fill(`Task 16 ${mode}`);
  await dialog.getByLabel("Game mode").selectOption(mode);
  await dialog.getByRole("button", { name: /^create room$/i }).click();
  await expect(page).toHaveURL(/\/admin\/rooms\/\d{6}$/);
  const code = /\/admin\/rooms\/(\d{6})$/.exec(page.url())?.[1];
  if (!code) throw new Error(`could not read created Room code from ${page.url()}`);
  await expect(page.getByRole("heading", { name: `Task 16 ${mode}` })).toBeVisible();
  await expectAccessible(page, `${mode} admin room`);
  return code;
}

async function chooseCharacter(page: Page) {
  const character = page.locator("label.character-option").filter({ hasText: "Player Red" });
  await expect(character).toHaveCount(1);
  await character.locator("input[type=radio]").check();
}

async function joinHuman(
  context: BrowserContext,
  harness: RealServerHarness,
  joinCode: string,
  mode: MatchMode,
): Promise<Page> {
  const page = await context.newPage();
  await page.goto(`${harness.frontendOrigin}/room/${joinCode}`);
  await expect(page.getByRole("heading", { name: /join task 16/i })).toBeVisible();
  await expectAccessible(page, `${mode} room join`);
  await page.getByLabel("Display name").fill(`Human ${mode}`);
  await chooseCharacter(page);
  await page.getByRole("button", { name: /^join room$/i }).click();
  await expect(page.getByRole("heading", { name: /you're in task 16/i })).toBeVisible();
  await expectAccessible(page, `${mode} join accepted`);
  await page.getByRole("link", { name: /enter lobby/i }).click();
  await expect(page.getByRole("heading", { name: new RegExp(`Task 16 ${mode} Lobby`, "i") })).toBeVisible();
  await expect(page.locator(".connection-state")).toHaveText("connected", { timeout: 20_000 });
  return page;
}

async function fillWithBotsAndStart(
  adminPage: Page,
  humanPage: Page,
  mode: MatchMode,
  seats: number,
) {
  const humanParticipant = adminPage.locator(".participant-row").filter({ hasText: new RegExp(`Human ${mode}`) });
  await expect(humanParticipant).toHaveCount(1);
  await humanParticipant.getByRole("button", { name: /^select$/i }).click();
  await expect(adminPage.getByText(new RegExp(`ROSTER \/ 1 OF ${seats}`, "i"))).toBeVisible({ timeout: 20_000 });
  await expect(adminPage.getByRole("button", { name: /fill with bots/i })).toBeEnabled({ timeout: 20_000 });
  await adminPage.getByRole("button", { name: /fill with bots/i }).click();
  await expect(adminPage.getByText(new RegExp(`ROSTER \/ ${seats} OF ${seats}`, "i"))).toBeVisible({ timeout: 20_000 });
  await expect(adminPage.locator(".participant-row")).toHaveCount(seats);

  await expect(humanPage.getByText(new RegExp(`ROSTER \/ ${seats} OF ${seats}`, "i"))).toBeVisible({ timeout: 20_000 });
  await expect(humanPage.getByRole("button", { name: /set ready/i })).toBeEnabled({ timeout: 20_000 });
  await expectAccessible(humanPage, `${mode} lobby`);
  await humanPage.screenshot({ path: screenshotPath(screenshotDirectory, `${mode}-lobby`) });
  await humanPage.getByRole("button", { name: /set ready/i }).click();
  await expect(humanPage.getByText(/ready for the next match/i)).toBeVisible({ timeout: 10_000 });

  await expect(adminPage.getByRole("button", { name: /start match/i })).toBeEnabled({ timeout: 30_000 });
  await adminPage.getByRole("button", { name: /start match/i }).click();
}

async function clickLegalAction(page: Page): Promise<boolean> {
  const actionDeck = page.getByTestId("action-deck");
  if (await actionDeck.getAttribute("aria-busy") === "true") return false;
  const winningAction = actionDeck.getByRole("button", { name: /^(ron|tsumo)$/i }).first();
  if (await winningAction.isVisible().catch(() => false)) {
    await winningAction.click();
    return true;
  }
  const responseAction = actionDeck.getByRole("button", { name: /^pass$/i }).first();
  if (await responseAction.isVisible().catch(() => false)) {
    await responseAction.click();
    return true;
  }
  const legalTile = page.locator(".table-tile-hit.is-legal").first();
  if (await legalTile.isVisible().catch(() => false)) {
    await legalTile.click();
    return true;
  }
  const candidateAction = actionDeck.locator("button:enabled").first();
  if (await candidateAction.isVisible().catch(() => false)) {
    await candidateAction.click();
    return true;
  }
  return false;
}

async function completeMatch(page: Page, mode: MatchMode, seats: number) {
  await expect(page.getByTestId("pixi-table")).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
  await expect(page.locator(".score-row")).toHaveCount(seats, { timeout: 30_000 });
  await page.screenshot({ path: screenshotPath(screenshotDirectory, `${mode}-playing`) });

  const deadline = Date.now() + 90_000;
  let clicks = 0;
  while (Date.now() < deadline) {
    if (await page.getByTestId("results-panel").isVisible().catch(() => false)) break;
    if (await clickLegalAction(page)) clicks += 1;
    await page.waitForTimeout(40);
  }
  expect(clicks, `${mode} should submit at least one legal action`).toBeGreaterThan(0);
  await expect(page.getByTestId("results-panel")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("heading", { name: /standings/i })).toBeVisible();
  await expect(page.locator(".results-list li")).toHaveCount(seats);
  await expect(page.getByText("Permanent Auto")).toHaveCount(seats - 1);
  await expectAccessible(page, `${mode} post-match`);
  await page.screenshot({ path: screenshotPath(screenshotDirectory, `${mode}-post-match`) });
}

for (const [mode, seats] of [
  ["3p-red-east", 3],
  ["4p-red-east", 4],
] as const) {
  test(`completes a real ${mode} Human match through Post-Match`, async ({ browser, harness }) => {
    const adminContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const humanContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const adminPage = await adminContext.newPage();
    try {
      await loginAdmin(adminPage, harness);
      const joinCode = await createRoom(adminPage, mode);
      const humanPage = await joinHuman(humanContext, harness, joinCode, mode);
      await fillWithBotsAndStart(adminPage, humanPage, mode, seats);
      await completeMatch(humanPage, mode, seats);
      await expect(adminPage.getByText("Post-Match", { exact: true })).toBeVisible({ timeout: 30_000 });
      await expect(adminPage.locator(".seat-row.is-filled")).toHaveCount(seats);
    } finally {
      await humanContext.close();
      await adminContext.close();
    }
  });
}
