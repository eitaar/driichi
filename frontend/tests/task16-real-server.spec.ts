/// <reference types="../node_modules/@types/node" />

import AxeBuilder from "@axe-core/playwright";
import { expect, test as base, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
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
    { scope: "worker", timeout: 240_000 },
  ],
});

test.describe.configure({ mode: "serial", timeout: 240_000 });

type MatchMode = "3p-red-east" | "4p-red-east";
type Viewport = {
  width: number;
  height: number;
  label: "1024x600" | "1280x720" | "1600x900" | "1920x1080";
};

const viewports: Viewport[] = [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1280, height: 720, label: "1280x720" },
  { width: 1600, height: 900, label: "1600x900" },
  { width: 1920, height: 1080, label: "1920x1080" },
];
const screenshotDirectory = resolve(ensureScreenshotDirectory());
const projectionFixture = JSON.parse(
  readFileSync(resolve(process.cwd(), "tests/fixtures/task12-projection.json"), "utf8"),
) as {
  room_envelopes: Record<string, Record<string, unknown>>;
  projections: Record<string, Record<string, unknown>>;
};
const replayByteLimit = 8 * 1024 * 1024;
const replayFrameLimit = 100_000;
let observedMultiCandidateAction = false;

async function expectAccessible(page: Page, surface: string) {
  const axe = new AxeBuilder({ page });
  if (surface === "actual decision" || surface === "results") {
    axe.include(".gameplay-main");
  }
  const results = await axe.analyze();
  expect(
    results.violations,
    `${surface}: ${results.violations.map((violation) => violation.id).join(", ")}`,
  ).toEqual([]);
}

async function captureReviewScreenshot(page: Page, name: string) {
  const image = await page.screenshot({
    path: screenshotPath(screenshotDirectory, name),
    fullPage: false,
  });
  expect(image.byteLength, `${name} screenshot should be nonblank evidence`).toBeGreaterThan(2_000);
}

async function captureAtBothViewports(page: Page, name: string) {
  for (const viewport of viewports) {
    await page.setViewportSize(viewport);
    await expect(page.locator("body")).toBeVisible();
    await captureReviewScreenshot(page, `${name}-${viewport.label}`);
  }
}

async function openEntry(page: Page, harness: RealServerHarness) {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto(harness.frontendOrigin);
  await expect(page.getByTestId("entry-shell")).toHaveAttribute("data-motion", "static");
  await expect(page.getByRole("heading", { name: /your table is live/i })).toBeVisible();
  const roomCode = page.getByRole("textbox", { name: /room code/i });
  await expect(roomCode).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: /open room/i })).toBeFocused();
  await expectAccessible(page, "entry");
  await captureAtBothViewports(page, "entry");
  await page.emulateMedia({ reducedMotion: "no-preference" });
}

async function loginAdmin(page: Page, harness: RealServerHarness) {
  await page.goto(`${harness.frontendOrigin}/admin/login`);
  await page.getByLabel("Username").fill(harness.adminUsername);
  await page.getByLabel("Password").fill(harness.adminPassword);
  await page.getByRole("button", { name: /^sign in$/i }).click();
  await expect(page.getByRole("heading", { name: /admin session active/i })).toBeVisible({ timeout: 20_000 });
  await page.getByRole("link", { name: /open admin/i }).click();
  await expect(page.getByRole("heading", { name: /admin rooms/i })).toBeVisible();
}

async function createRoom(page: Page, mode: MatchMode): Promise<string> {
  const createTrigger = page.getByRole("button", { name: "Create Room" }).first();
  await createTrigger.focus();
  await createTrigger.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel("Room name")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(createTrigger).toBeFocused();

  await createTrigger.click();
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Room name").fill(`Task 16 ${mode}`);
  await dialog.getByLabel("Game mode").selectOption(mode);
  await dialog.getByRole("button", { name: /^create room$/i }).click();
  await expect(page).toHaveURL(/\/admin\/rooms\/\d{6}$/);
  const code = /\/admin\/rooms\/(\d{6})$/.exec(page.url())?.[1];
  if (!code) throw new Error(`could not read created Room code from ${page.url()}`);
  await expect(page.getByRole("heading", { name: `Task 16 ${mode}` })).toBeVisible();
  await expectAccessible(page, "admin");
  await captureAtBothViewports(page, `${mode}-admin`);
  await page.setViewportSize({ width: 1440, height: 900 });
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
  await page.setViewportSize({ width: 1440, height: 900 });
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
  await expectAccessible(humanPage, "lobby");
  await captureAtBothViewports(humanPage, `${mode}-lobby`);
  await humanPage.emulateMedia({ reducedMotion: "reduce" });
  await humanPage.setViewportSize({ width: 1440, height: 900 });
  await humanPage.getByRole("button", { name: /set ready/i }).click();
  await expect(humanPage.getByText(/ready for the next match/i)).toBeVisible({ timeout: 10_000 });

  await expect(adminPage.getByRole("button", { name: /start match/i })).toBeEnabled({ timeout: 30_000 });
  await adminPage.getByRole("button", { name: /start match/i }).click();
}

async function installEmbeddedProjection(page: Page) {
  const room = projectionFixture.room_envelopes["4p-red-east"];
  const state = JSON.parse(
    JSON.stringify(projectionFixture.projections["4p-red-east"]),
  ) as Record<string, unknown>;
  state.decision = {
    decision_id: "response-1",
    kind: "Response",
    actions: [
      {
        action_id: "chi-1",
        action: { Chi: { target: 1, called: 1, consumed: [0, 4] } },
      },
      {
        action_id: "chi-2",
        action: { Chi: { target: 1, called: 2, consumed: [1, 5] } },
      },
      { action_id: "pass-1", action: "Pass" },
    ],
  };
  await page.addInitScript(
    ({ room: initialRoom, state: initialState }) => {
      const browser = window as unknown as {
        __room: unknown;
        __state: unknown;
      };
      browser.__room = initialRoom;
      browser.__state = initialState;
      class GameplaySocket {
        static OPEN = 1;
        readyState = 1;
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: ((event: { code: number; reason: string }) => void) | null = null;
        onerror: (() => void) | null = null;
        constructor() {
          setTimeout(() => {
            this.onopen?.();
            this.onmessage?.({
              data: JSON.stringify({
                type: "snapshot",
                room: browser.__room,
                state: browser.__state,
              }),
            });
          }, 0);
        }
        send(_value: string) {}
        close() {
          this.onclose?.({ code: 1000, reason: "client_closed" });
        }
      }
      Object.defineProperty(window, "WebSocket", {
        configurable: true,
        value: GameplaySocket,
      });
      sessionStorage.setItem("driichi:participant:123456", "P1");
    },
    { room, state },
  );
}

async function waitForDecision(page: Page, seats: number) {
  await expect(page.getByTestId("gameplay-shell")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("pixi-table")).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
  await expect.poll(
    async () => (await page.getByTestId("action-deck").locator("button:enabled").count())
      + (await page.locator(".table-tile-hit.is-legal").count()),
    { timeout: 30_000, message: "Human should receive an actual open Decision" },
  ).toBeGreaterThan(0);
  await expect(page.getByTestId("pixi-table")).toHaveAttribute(
    "data-player-frame-count",
    String(seats),
    { timeout: 30_000 },
  );
}

async function tabTo(page: Page, target: Locator, limit = 60) {
  for (let index = 0; index < limit; index += 1) {
    if (await target.evaluate((element) => element === document.activeElement)) return;
    await page.keyboard.press("Tab");
  }
  throw new Error("keyboard focus did not reach the requested control");
}

type ActionOutcome = "accepted" | "rejected";

async function waitForActionResult(
  page: Page,
  actionId: string,
  decisionId: string,
): Promise<ActionOutcome> {
  const shell = page.getByTestId("gameplay-shell");
  let outcome: ActionOutcome | null = null;
  await expect.poll(
    async () => {
      const serializedHistory = await shell.getAttribute("data-action-result-history");
      try {
        const history = JSON.parse(serializedHistory ?? "[]") as Array<{
          action_id?: string;
          decision_id?: string;
          status?: ActionOutcome;
        }>;
        const matching = history.find(
          (result) =>
            result.action_id === actionId && result.decision_id === decisionId,
        );
        if (matching?.status === "accepted" || matching?.status === "rejected") {
          outcome = matching.status;
          return outcome;
        }
      } catch {
        // Keep polling until the authoritative DOM history is valid.
      }
      const status = await shell.getAttribute("data-last-action-result-status");
      const observedActionId = await shell.getAttribute("data-last-action-result-action-id");
      if (
        observedActionId === actionId &&
        (status === "accepted" || status === "rejected")
      ) {
        outcome = status;
        return outcome;
      }
      return null;
    },
    { timeout: 10_000, message: "the submitted action id should receive an authoritative result" },
  ).toBeTruthy();
  return outcome!;
}

async function waitForAcceptedRevision(page: Page, beforeRevision: number) {
  const shell = page.getByTestId("gameplay-shell");
  await expect.poll(
    async () => Number(await shell.getAttribute("data-room-revision")),
    { timeout: 15_000, message: "accepted Human action should advance the authoritative Room revision" },
  ).toBeGreaterThan(beforeRevision);
  await expect(page.locator(".gameplay-error")).toHaveCount(0);
  await expect(shell).toHaveAttribute("data-human-controller", /interactive/i);
  await expect(shell).not.toHaveAttribute("data-human-controller", /temporary_auto/i);
}

async function clickCapturedAction(page: Page, actionId: string): Promise<boolean> {
  const action = page.locator(`[data-action-id="${actionId}"]`).first();
  try {
    await action.click({ timeout: 5_000 });
    return true;
  } catch {
    // The realtime decision may have advanced while Playwright was waiting for
    // actionability. Do not let a generic locator click a newer action.
    return false;
  }
}

async function submitConcreteAction(page: Page): Promise<{ submitted: boolean; multiCandidate: boolean; actionId?: string }> {
  const shell = page.getByTestId("gameplay-shell");
  const deck = page.getByTestId("action-deck");
  if (await page.getByTestId("results-panel").isVisible().catch(() => false)) return { submitted: false, multiCandidate: false };
  if ((await deck.getAttribute("aria-busy")) === "true") return { submitted: false, multiCandidate: false };
  const enabled = deck.locator("button:enabled");
  if (await enabled.count() === 0 && await page.locator(".table-tile-hit.is-legal").count() === 0) return { submitted: false, multiCandidate: false };

  const beforeRevision = Number(await shell.getAttribute("data-room-revision"));
  const decisionId = await shell.getAttribute("data-current-decision-id");
  expect(Number.isFinite(beforeRevision)).toBe(true);
  expect(decisionId, "open Decision must expose its decision_id").toBeTruthy();
  const multiTrigger = deck.getByRole("button", { name: /^(chi|pon|kan|kita) \(\d+\)$/i }).first();
  if (await multiTrigger.isVisible().catch(() => false)) {
    try {
      await multiTrigger.focus({ timeout: 5_000 });
      await multiTrigger.click({ timeout: 5_000 });
    } catch {
      return { submitted: false, multiCandidate: false };
    }
    const dialog = page.getByRole("dialog", { name: /choose a legal candidate/i });
    await expect(dialog).toBeVisible();
    await expect.poll(
      async () => dialog.locator(".candidate-list button").count(),
      { timeout: 5_000, message: "multi-candidate Decision should expose every concrete line" },
    ).toBeGreaterThan(1);
    await expect(dialog.locator(".candidate-list button").first()).toBeFocused();
    const concreteLabel = (await dialog.locator(".candidate-list button").first().innerText()).trim();
    expect(concreteLabel).not.toMatch(/^(chi|pon|kan|kita)$/i);
    const actionId = await dialog.locator(".candidate-list button").first().getAttribute("data-action-id");
    expect(actionId, "candidate action must expose its concrete action_id").toBeTruthy();
    if (!(await clickCapturedAction(page, actionId!))) {
      return { submitted: false, multiCandidate: false };
    }
    const outcome = await waitForActionResult(page, actionId!, decisionId!);
    if (outcome !== "accepted") return { submitted: false, multiCandidate: false };
    observedMultiCandidateAction = true;
    await waitForAcceptedRevision(page, beforeRevision);
    return { submitted: true, multiCandidate: true, actionId: actionId! };
  }

  const winningAction = deck.getByRole("button", { name: /^(ron|tsumo)$/i }).first();
  const responseAction = deck.getByRole("button", { name: /^pass$/i }).first();
  const legalTile = page.locator(".table-tile-hit.is-legal").first();
  const candidateAction = deck.locator("button:enabled").first();
  const action = (await winningAction.isVisible().catch(() => false))
    ? winningAction
    : (await responseAction.isVisible().catch(() => false))
      ? responseAction
      : (await legalTile.isVisible().catch(() => false))
        ? legalTile
        : candidateAction;
  if (!(await action.isVisible().catch(() => false))) return { submitted: false, multiCandidate: false };
  const actionId = await action.getAttribute("data-action-id");
  expect(actionId, "submitted action must expose its concrete action_id").toBeTruthy();
  if (!(await clickCapturedAction(page, actionId!))) {
    return { submitted: false, multiCandidate: false };
  }
  const outcome = await waitForActionResult(page, actionId!, decisionId!);
  if (outcome !== "accepted") return { submitted: false, multiCandidate: false };
  await waitForAcceptedRevision(page, beforeRevision);
  return { submitted: true, multiCandidate: false, actionId: actionId! };
}

async function completeMatch(page: Page, mode: MatchMode, seats: number) {
  await waitForDecision(page, seats);
  await expect(page.getByTestId("gameplay-shell")).toHaveAttribute("data-motion", "static");
  await expect(page.getByTestId("gameplay-shell")).toHaveAttribute("data-human-controller", /interactive/i);
  await captureAtBothViewports(page, `${mode}-decision`);
  await expectAccessible(page, "actual decision");

  const deadline = Date.now() + 240_000;
  let submitted = 0;
  while (Date.now() < deadline) {
    if (await page.getByTestId("results-panel").isVisible().catch(() => false)) break;
    const action = await submitConcreteAction(page);
    if (action.submitted) submitted += 1;
    await page.waitForTimeout(80);
  }
  expect(submitted, `${mode} should submit accepted legal actions`).toBeGreaterThan(0);
  await expect(page.getByTestId("results-panel")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("heading", { name: /standings/i })).toBeVisible();
  await expect(page.locator(".results-list li")).toHaveCount(seats);
  await expect(page.getByText("Permanent Auto")).toHaveCount(seats - 1);
  await expect(page.getByText("Temporary Auto")).toHaveCount(0);
  await expect(page.getByTestId("gameplay-shell")).toHaveAttribute("data-human-controller", /interactive/i);
  await expectAccessible(page, "results");
  await captureAtBothViewports(page, `${mode}-results`);
}

test("serves the embedded gameplay with visible tiles under its CSP", async ({ page, harness }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const tileResponses: Array<{ status: number; contentType: string }> = [];
  page.on("response", (response) => {
    if (new URL(response.url()).pathname.endsWith(".svg")) {
      tileResponses.push({
        status: response.status(),
        contentType: response.headers()["content-type"] ?? "",
      });
    }
  });
  await installEmbeddedProjection(page);
  const documentResponse = await page.goto(
    `${harness.rustOrigin}/room/123456/lobby`,
  );
  expect(documentResponse?.headers()["content-security-policy"]).toContain(
    "script-src 'self'",
  );
  const table = page.getByTestId("pixi-table");
  await expect(table).toHaveAttribute("data-render-ready", "true", {
    timeout: 30_000,
  });
  await expect(table).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
  expect(
    tileResponses.some(
      ({ status, contentType }) =>
        status === 200 && contentType.startsWith("image/svg+xml"),
    ),
  ).toBe(true);
  await expect(page.getByRole("button", { name: "Chi (2)" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Pass" })).toBeEnabled();

  const settings = page.locator(".audio-settings summary");
  await tabTo(page, settings);
  await page.keyboard.press("Enter");
  await expect(page.locator(".audio-settings[open]")).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.locator(".audio-settings[open]")).toHaveCount(0);

  const chiTrigger = page.getByRole("button", { name: "Chi (2)" });
  await tabTo(page, chiTrigger);
  await page.keyboard.press("Enter");
  const candidateDialog = page.getByRole("dialog", { name: /choose a legal candidate/i });
  await expect(candidateDialog).toBeVisible();
  await expect(candidateDialog.locator(".candidate-list button").first()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(candidateDialog).toHaveCount(0);
  await expect(chiTrigger).toBeFocused();
});

type JsonResponse = { status: number; body: unknown };

async function getJson(page: Page, path: string): Promise<JsonResponse> {
  return page.evaluate(async (requestPath) => {
    const response = await fetch(requestPath);
    let body: unknown;
    try { body = await response.json(); } catch { body = null; }
    return { status: response.status, body };
  }, path);
}

async function verifyReplayAndOpenRoutes(
  adminPage: Page,
  harness: RealServerHarness,
  mode: MatchMode,
) {
  type ReplayList = { replays: Array<Record<string, unknown>>; total: number };
  let list: ReplayList | undefined;
  await expect.poll(
    async () => {
      const result = await getJson(adminPage, "/api/v1/admin/replays");
      list = result.body as ReplayList;
      return list.replays.filter((replay) => replay.room_name === `Task 16 ${mode}`).length;
    },
    { timeout: 30_000, message: `${mode} should create a saved Replay` },
  ).toBeGreaterThan(0);
  expect(list?.total).toBeGreaterThan(0);
  const summary = list?.replays.find((replay) => replay.room_name === `Task 16 ${mode}`);
  expect(summary).toBeDefined();
  const matchId = String(summary?.match_id ?? "");
  expect(matchId).toMatch(/^[A-Za-z0-9-]{1,128}$/);
  expect(Number(summary?.file_size)).toBeGreaterThan(0);
  expect(Number(summary?.file_size)).toBeLessThanOrEqual(replayByteLimit);

  const viewResponse = await getJson(adminPage, `/api/v1/admin/replays/${encodeURIComponent(matchId)}`);
  expect(viewResponse.status).toBe(200);
  const view = viewResponse.body as { file_size: number; frames: Array<Record<string, unknown>> };
  expect(view.file_size).toBeGreaterThan(0);
  expect(view.file_size).toBeLessThanOrEqual(replayByteLimit);
  expect(view.frames.length).toBeGreaterThan(0);
  expect(view.frames.length).toBeLessThanOrEqual(replayFrameLimit);
  expect(JSON.stringify(view).length).toBeLessThanOrEqual(replayByteLimit);
  expect(view.frames.every((frame, index) => frame.event_index === index)).toBe(true);

  await adminPage.emulateMedia({ reducedMotion: "reduce" });
  await adminPage.goto(`${harness.frontendOrigin}/admin/replays`);
  await expect(adminPage.getByRole("heading", { name: /replay library/i })).toBeVisible();
  await expectAccessible(adminPage, "replay library");
  await captureAtBothViewports(adminPage, `${mode}-replay-library`);
  const row = adminPage.locator(".replay-row").filter({ hasText: `Task 16 ${mode}` });
  await expect(row).toHaveCount(1);
  await row.getByRole("link", { name: /view replay/i }).click();
  await expect(adminPage.getByRole("heading", { name: new RegExp(`Task 16 ${mode} Replay`, "i") })).toBeVisible();
  await expect(adminPage.getByTestId("pixi-table")).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
  await expect(adminPage.getByTestId("replay-viewer")).toHaveAttribute("data-motion", "static");
  await expectAccessible(adminPage, "replay viewer");
  await captureAtBothViewports(adminPage, `${mode}-replay-viewer`);
  await adminPage.getByRole("button", { name: /next event/i }).click();
  const replayControls = adminPage.locator(".replay-controls-overlay");
  const replayPlay = replayControls.getByRole("button", { name: /^play$/i });
  await replayPlay.focus();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("button", { name: /previous event/i })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("button", { name: /next event/i })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("button", { name: "0.5x" })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("button", { name: "1x" })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("button", { name: "2x" })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("button", { name: "4x" })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(replayControls.getByRole("combobox", { name: /jump to kyoku/i })).toBeFocused();
  await adminPage.locator(".skip-link").focus();
  await expect(adminPage.locator(".skip-link")).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(adminPage.getByRole("link", { name: /double riichi home/i })).toBeFocused();
  await adminPage.keyboard.press("Tab");
  await expect(adminPage.getByRole("link", { name: /^rooms$/i })).toBeFocused();
}

for (const [mode, seats] of [
  ["3p-red-east", 3],
  ["4p-red-east", 4],
] as const) {
  test(`completes a real ${mode} Human match through Post-Match and Replay`, async ({ browser, harness }) => {
    test.setTimeout(360_000);
    const adminContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const humanContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const adminPage = await adminContext.newPage();
    try {
      await openEntry(adminPage, harness);
      await loginAdmin(adminPage, harness);
      const joinCode = await createRoom(adminPage, mode);
      const humanPage = await joinHuman(humanContext, harness, joinCode, mode);
      await fillWithBotsAndStart(adminPage, humanPage, mode, seats);
      await completeMatch(humanPage, mode, seats);
      await expect(adminPage.getByText("Post-Match", { exact: true })).toBeVisible({ timeout: 30_000 });
      await expect(adminPage.locator(".seat-row.is-filled")).toHaveCount(seats);
      const humanParticipant = adminPage.locator(".participant-row").filter({ hasText: new RegExp(`Human ${mode}`) });
      await expect(humanParticipant).toContainText(/interactive/i);
      await expect(humanParticipant).not.toContainText(/temporary[_ ]auto/i);
      await expectAccessible(adminPage, "admin");
      await captureAtBothViewports(adminPage, `${mode}-admin-post-match`);
      await verifyReplayAndOpenRoutes(adminPage, harness, mode);
      if (mode === "4p-red-east") expect(observedMultiCandidateAction).toBe(true);
      await humanContext.close();
      await adminContext.close();
    } catch (error) {
      await humanContext.close().catch(() => undefined);
      await adminContext.close().catch(() => undefined);
      throw error;
    }
  });
}
