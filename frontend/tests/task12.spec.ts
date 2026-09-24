/// <reference types="../node_modules/@types/node" />

import AxeBuilder from "@axe-core/playwright";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Locator, type Page } from "@playwright/test";

test.describe.configure({ mode: "serial" });

const characterFixtureRoot = resolve(
  process.cwd(),
  "tests/fixtures/task12-characters",
);
const projectionFixture = JSON.parse(
  readFileSync(
    resolve(process.cwd(), "tests/fixtures/task12-projection.json"),
    "utf8",
  ),
) as {
  acceptance: {
    wall_tile_counts: Record<"3p-red-east" | "4p-red-east", number>;
  };
  room_envelopes: Record<
    "3p-red-east" | "4p-red-east",
    Record<string, unknown>
  >;
  projections: Record<"3p-red-east" | "4p-red-east", Record<string, unknown>>;
};

async function installCharacterFixtures(
  page: Page,
  missingPortraitCharacterId?: string,
) {
  await page.route("**/assets/characters/**", async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    const match = /^\/assets\/characters\/([^/]+)\/(portrait|icon)\.webp$/.exec(
      pathname,
    );
    if (!match || (match[2] === "portrait" && match[1] === missingPortraitCharacterId)) {
      await route.fulfill({ status: 404, body: "missing test asset" });
      return;
    }
    const file = resolve(characterFixtureRoot, match[1], `${match[2]}.webp`);
    try {
      await route.fulfill({
        status: 200,
        contentType: "image/webp",
        body: readFileSync(file),
      });
    } catch {
      await route.fulfill({ status: 404, body: "missing test asset" });
    }
  });
}

async function installSocket(
  page: Page,
  mode: "3p-red-east" | "4p-red-east",
  stateOverride?: Record<string, unknown>,
  snapshotDelayMs = 0,
) {
  const room = projectionFixture.room_envelopes[mode];
  const state = stateOverride ?? projectionFixture.projections[mode];
  await page.addInitScript(
    ({ room, state, snapshotDelayMs }) => {
      (window as unknown as { __room: unknown; __state: unknown }).__room =
        room;
      (window as unknown as { __room: unknown; __state: unknown }).__state =
        state;
      class GameplaySocket {
        static OPEN = 1;
        readyState = 1;
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: ((event: { code: number; reason: string }) => void) | null =
          null;
        onerror: (() => void) | null = null;
        sent: string[] = [];
        constructor() {
          const browser = window as unknown as {
            __room: unknown;
            __state: unknown;
            __socket: GameplaySocket;
          };
          browser.__socket = this;
          setTimeout(() => {
            this.onopen?.();
            this.onmessage?.({
              data: JSON.stringify({ type: "room_update", room: browser.__room }),
            });
          }, 0);
          setTimeout(() => {
            this.onmessage?.({
              data: JSON.stringify({
                type: "snapshot",
                room: browser.__room,
                state: browser.__state,
              }),
            });
          }, snapshotDelayMs);
        }
        send(value: string) {
          this.sent.push(value);
        }
        emit(value: unknown) {
          this.onmessage?.({ data: JSON.stringify(value) });
        }
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
    { room, state, snapshotDelayMs },
  );
}

async function expectRenderedTable(page: Page) {
  const table = page.getByTestId("three-table");
  await expect(table).toBeVisible({ timeout: 20000 });
  await expect(table).toHaveAttribute("data-render-ready", "true", {
    timeout: 20000,
  });
  await expect(table).toHaveAttribute("data-player-frame-count", /^[34]$/);
  await expect(table).toHaveAttribute("data-wall-tile-count", /^\d+$/);
  await expect(table).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
  await expect(table).toHaveAttribute(
    "data-rendered-scene-primitives",
    /^[1-9]\d*$/,
  );
  const tableHeightRatio = Number(await table.getAttribute("data-table-height-ratio"));
  const tableWidthRatio = Number(await table.getAttribute("data-table-width-ratio"));
  const rendererPixelRatio = Number(await table.getAttribute("data-renderer-pixel-ratio"));
  expect(rendererPixelRatio).toBeGreaterThanOrEqual(1);
  expect(rendererPixelRatio).toBeLessThanOrEqual(2);
  expect(tableWidthRatio).toBeGreaterThanOrEqual(0.82);
  expect(tableWidthRatio).toBeLessThanOrEqual(0.9);
  expect(tableHeightRatio).toBeGreaterThanOrEqual(0.78);
  expect(tableHeightRatio).toBeLessThanOrEqual(0.88);
  const canvasQuality = await table.evaluate((node) => {
    const canvas = node.querySelector("canvas");
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const context = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
    return {
      clientWidth: rect.width,
      clientHeight: rect.height,
      pixelWidth: canvas.width,
      pixelHeight: canvas.height,
      antialias: context?.getContextAttributes()?.antialias ?? false,
    };
  });
  expect(canvasQuality).not.toBeNull();
  expect(canvasQuality?.clientWidth).toBeGreaterThan(0);
  expect(canvasQuality?.clientHeight).toBeGreaterThan(0);
  expect(
    (canvasQuality?.clientWidth ?? 0) / (canvasQuality?.clientHeight ?? 1),
  ).toBeCloseTo(1600 / 900, 1);
  expect(canvasQuality?.antialias).toBe(true);
  expect(Math.abs(
    (canvasQuality?.pixelWidth ?? 0)
      - (canvasQuality?.clientWidth ?? 0) * rendererPixelRatio,
  )).toBeLessThanOrEqual(1);
  expect(Math.abs(
    (canvasQuality?.pixelHeight ?? 0)
      - (canvasQuality?.clientHeight ?? 0) * rendererPixelRatio,
  )).toBeLessThanOrEqual(1);
  expect(canvasQuality?.pixelWidth).toBeLessThanOrEqual(3840);
  expect(canvasQuality?.pixelHeight).toBeLessThanOrEqual(2160);
  return table;
}

async function expectInViewport(page: Page, locator: Locator) {
  const boxes = await locator.evaluateAll((elements) =>
    elements.map((element) => {
      const rect = element.getBoundingClientRect();
      return {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        width: rect.width,
        height: rect.height,
      };
    }),
  );
  expect(boxes.length).toBeGreaterThan(0);
  const viewport = await page.evaluate(() => ({
    width: window.innerWidth,
    height: window.innerHeight,
  }));
  for (const box of boxes) {
    expect(box.width).toBeGreaterThan(0);
    expect(box.height).toBeGreaterThan(0);
    expect(box.left).toBeGreaterThanOrEqual(0);
    expect(box.top).toBeGreaterThanOrEqual(0);
    expect(box.right).toBeLessThanOrEqual(viewport.width);
    expect(box.bottom).toBeLessThanOrEqual(viewport.height);
  }
}

async function expectInsideStageAndClearOfHand(
  stage: Locator,
  targets: Locator,
  hand: Locator,
) {
  const stageBox = await stage.boundingBox();
  const handBox = await hand.boundingBox();
  const targetBoxes = await targets.evaluateAll((elements) =>
    elements.map((element) => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    }),
  );
  expect(stageBox).not.toBeNull();
  expect(handBox).not.toBeNull();
  expect(targetBoxes.length).toBeGreaterThan(0);
  for (const box of targetBoxes) {
    expect(box.x).toBeGreaterThanOrEqual(stageBox!.x);
    expect(box.y).toBeGreaterThanOrEqual(stageBox!.y);
    expect(box.x + box.width).toBeLessThanOrEqual(stageBox!.x + stageBox!.width);
    expect(box.y + box.height).toBeLessThanOrEqual(stageBox!.y + stageBox!.height);
    const intersectsHand =
      box.x < handBox!.x + handBox!.width
      && box.x + box.width > handBox!.x
      && box.y < handBox!.y + handBox!.height
      && box.y + box.height > handBox!.y;
    expect(intersectsHand).toBe(false);
  }
}

async function expectApprovedLiveOverlayGeometry(page: Page) {
  const geometry = await page.locator(".table-letterbox").evaluate((stage) => {
    const stageRect = stage.getBoundingClientRect();
    const hand = stage.querySelector<HTMLElement>(".table-hit-layer")!.getBoundingClientRect();
    const actions = stage.querySelector<HTMLElement>(".action-deck")!.getBoundingClientRect();
    const frames = Array.from(stage.querySelectorAll<HTMLElement>(".table-player-frame"))
      .map((frame) => frame.getBoundingClientRect());
    return {
      stage: { width: stageRect.width, height: stageRect.height },
      hand: { top: hand.top, bottom: hand.bottom, height: hand.height },
      actions: { top: actions.top, bottom: actions.bottom },
      frameWidths: frames.map(({ width }) => width),
      factCount: stage.querySelectorAll(".table-center-facts > div").length,
    };
  });
  expect(geometry.actions.bottom).toBeLessThanOrEqual(geometry.hand.top - 12);
  // Hit targets should hug the projected local hand, not create a broad action band.
  expect(geometry.hand.height).toBeLessThanOrEqual(geometry.stage.height * 0.12 + 4);
  expect(Math.max(...geometry.frameWidths)).toBeLessThanOrEqual(geometry.stage.width * 0.1 + 1);
  expect(geometry.factCount).toBe(5);
}

async function expectNoSeriousOrCriticalViolations(page: Page, include?: string) {
  const axe = new AxeBuilder({ page });
  if (include) axe.include(include);
  const results = await axe.analyze();
  const seriousOrCritical = results.violations.filter((violation) =>
    violation.impact === "serious" || violation.impact === "critical",
  );
  expect(
    seriousOrCritical,
    seriousOrCritical.map((violation) => violation.id).join(", "),
  ).toEqual([]);
}

async function expectNoAxeViolations(page: Page, include?: string) {
  const axe = new AxeBuilder({ page });
  if (include) axe.include(include);
  const results = await axe.analyze();
  expect(
    results.violations,
    results.violations.map((violation) => `${violation.id}:${violation.impact}`).join(", "),
  ).toEqual([]);
}

const requiredViewports = [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1280, height: 720, label: "1280x720" },
  { width: 1600, height: 900, label: "1600x900" },
  { width: 1920, height: 1080, label: "1920x1080" },
] as const;

for (const viewport of requiredViewports) {
  for (const mode of ["3p-red-east", "4p-red-east"] as const) {
    test(`captures ${mode} authoritative gameplay at ${viewport.label}`, async ({
      page,
    }) => {
      await page.setViewportSize({
        width: viewport.width,
        height: viewport.height,
      });
      await installCharacterFixtures(page);
      await installSocket(page, mode);
      await page.goto("/room/123456/lobby");
      const table = await expectRenderedTable(page);
      await expect(page.locator(".gameplay-topbar")).toHaveCount(0);
      await expect(page.locator(".gameplay-rail")).toHaveCount(0);
      await expect(page.locator(".gameplay-controls")).toBeVisible();
      await expect(page.getByTestId("action-deck")).toBeVisible();
      await expect(page.locator(".table-hit-layer")).toBeVisible();
      await expect(page.locator(".table-player-overlays[data-surface=\"live\"]")).toHaveAttribute("aria-hidden", "true");
      await expect(page.locator(".gameplay-toast-stack")).not.toHaveAttribute("aria-live");
      await expect(table).toHaveAttribute(
        "data-wall-tile-count",
        String(projectionFixture.acceptance.wall_tile_counts[mode]),
      );
      await expect(table).toHaveAttribute(
        "data-player-frame-count",
        mode === "3p-red-east" ? "3" : "4",
      );
      await expect(page.locator(".table-player-frame")).toHaveCount(
        mode === "3p-red-east" ? 3 : 4,
      );
      await expect(page.locator('.table-player-frame[data-position="top"]')).toHaveCount(
        mode === "3p-red-east" ? 0 : 1,
      );
      await expectInViewport(page, page.locator(".gameplay-controls"));
      await expectInViewport(page, page.getByTestId("action-deck"));
      await expectInViewport(page, page.locator(".table-tile-hit.is-legal"));
      await expectInsideStageAndClearOfHand(
        page.locator(".table-letterbox"),
        page.locator(".table-player-frame"),
        page.locator(".table-hit-layer"),
      );
      await expectInsideStageAndClearOfHand(
        page.locator(".table-letterbox"),
        page.getByTestId("action-deck"),
        page.locator(".table-hit-layer"),
      );
      await expectApprovedLiveOverlayGeometry(page);
      await expect(table.getByText("Mika")).toBeVisible();
      await expect(page.getByTestId("decision-timer")).toHaveAttribute(
        "aria-label",
        /Decision timer: \d+ seconds/,
      );
      const legalTiles = page.locator(".table-tile-hit.is-legal");
      await expect(legalTiles).toHaveCount(14);
      await expect(legalTiles.first()).toBeVisible();
      await page.screenshot({
        path: `test-results/task-12/${mode}-${viewport.label}-live.png`,
        fullPage: false,
      });
      await legalTiles.first().click();
      await expect(page.getByTestId("action-deck")).toHaveAttribute(
        "aria-busy",
        "true",
      );
      await expect(page.locator(".table-tile-hit")).toHaveCount(14);
      await expect(legalTiles).toHaveCount(14);
      await page.evaluate(() => {
        (
          window as unknown as { __socket: { emit: (value: unknown) => void } }
        ).__socket.emit({
          type: "action_result",
          decision_id: "d1",
          action_id: "a1",
          status: "rejected",
          code: "illegal_action",
        });
      });
      await expect(page.getByRole("alert")).toContainText("illegal_action");
      await expect(page.getByTestId("action-deck")).toHaveAttribute(
        "aria-busy",
        "false",
      );
      await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(14);
      await legalTiles.first().click();
      await legalTiles.first().dispatchEvent("click");
      const sent = await page.evaluate(() =>
        (
          window as unknown as { __socket: { sent: string[] } }
        ).__socket.sent.map((value) => JSON.parse(value)),
      );
      expect(sent).toHaveLength(2);
      expect(sent).toEqual([
        { type: "submit_action", decision_id: "d1", action_id: "a1" },
        { type: "submit_action", decision_id: "d1", action_id: "a1" },
      ]);
    });
  }
}

test("keeps called-hand canvas tiles and semantic hit extents in the same projection", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  const local = (state.players as Array<Record<string, unknown>>)[0];
  const hand = [16, 17, 52, 53, 88];
  local.hand = hand;
  local.concealed_count = hand.length;
  local.melds = [
    { tiles: [0, 1, 2] },
    { tiles: [4, 5, 6] },
    { tiles: [8, 9, 10] },
  ];
  state.decision = {
    decision_id: "called-hand-1",
    kind: "Turn",
    actions: hand.map((tile, index) => ({
      action_id: `called-discard-${index}`,
      action: { Discard: { tile, tsumogiri: index === hand.length - 1 } },
    })),
  };
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east", state);
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);

  const stage = page.locator(".table-letterbox");
  const layer = page.locator(".table-hit-layer");
  const targets = layer.locator(".table-tile-hit.is-legal");
  await expect(targets).toHaveCount(hand.length);
  const tileKeys = await targets.evaluateAll((elements) =>
    elements.map((element) => element.getAttribute("data-tile-key")),
  );
  expect(tileKeys).toEqual(hand.map((_, index) => `hand-bottom-seat-0-${index}`));
  const geometry = await stage.evaluate((element) => {
    const stageRect = element.getBoundingClientRect();
    const layer = element.querySelector<HTMLElement>(".table-hit-layer")!;
    const layerRect = layer.getBoundingClientRect();
    const targetRects = Array.from(layer.querySelectorAll<HTMLElement>(".table-tile-hit.is-legal"))
      .map((target) => {
        const rect = target.getBoundingClientRect();
        return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
      });
    return {
      stage: { left: stageRect.left, top: stageRect.top, right: stageRect.right, bottom: stageRect.bottom, width: stageRect.width },
      layer: { left: layerRect.left, top: layerRect.top, right: layerRect.right, bottom: layerRect.bottom, width: layerRect.width, height: layerRect.height },
      targetRects,
    };
  });
  expect(geometry.layer.width).toBeGreaterThan(0);
  expect(geometry.layer.width).toBeLessThan(geometry.stage.width * 0.6);
  expect(geometry.layer.height).toBeGreaterThan(0);
  expect(geometry.targetRects).toHaveLength(hand.length);
  for (const target of geometry.targetRects) {
    expect(target.left).toBeGreaterThanOrEqual(geometry.stage.left);
    expect(target.top).toBeGreaterThanOrEqual(geometry.stage.top);
    expect(target.right).toBeLessThanOrEqual(geometry.stage.right);
    expect(target.bottom).toBeLessThanOrEqual(geometry.stage.bottom);
  }
});

test("keeps a long participant name and missing portrait actionable", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  (state.players as Array<Record<string, unknown>>)[0].display_name =
    "A very long participant display name";
  await installCharacterFixtures(page, "player-red");
  await installSocket(page, "4p-red-east", state);
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await expect(table).toHaveAttribute("data-player-frame-count", "4");
  await expect(table).toHaveAttribute("data-render-ready", "true");
  await expect(table.locator('.table-player-frame[data-position="bottom"]')).toContainText(
    "A very long participant display name",
  );
  await expect(table.locator('.table-player-frame[data-position="bottom"] .asset-fallback')).toBeVisible();
  await expect(page.getByTestId("action-deck")).toBeVisible();
  await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(14);
  await page.screenshot({
    path: "test-results/immersive-table/4p-long-name-fallback.png",
    fullPage: false,
  });
});

test("keeps legal actions usable when WebGL creation fails", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function getContext(
      this: HTMLCanvasElement,
      contextId: string,
      ...args: unknown[]
    ) {
      if (contextId === "webgl" || contextId === "webgl2") return null;
      return Reflect.apply(original, this, [contextId, ...args]);
    } as typeof original;
  });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");

  const table = page.getByTestId("three-table");
  await expect(table).toHaveAttribute("data-webgl-fallback", "true", { timeout: 20_000 });
  await expect(page.getByRole("status", { name: "3D table unavailable" })).toContainText("East 1");
  const legalTiles = page.locator(".table-tile-hit.is-legal");
  await expect(legalTiles).toHaveCount(14);
  await legalTiles.first().click();
  const sent = await page.evaluate(() =>
    (window as unknown as { __socket: { sent: string[] } }).__socket.sent.map((value) => JSON.parse(value)),
  );
  expect(sent).toEqual([
    { type: "submit_action", decision_id: "d1", action_id: "a1" },
  ]);
  await expectNoAxeViolations(page, ".gameplay-main");
});

test("falls back when renderer creation fails after WebGL preflight", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    let webglRequests = 0;
    HTMLCanvasElement.prototype.getContext = function getContext(
      this: HTMLCanvasElement,
      contextId: string,
      ...args: unknown[]
    ) {
      if (contextId === "webgl" || contextId === "webgl2" || contextId === "experimental-webgl") {
        webglRequests += 1;
        if (webglRequests > 1) return null;
      }
      return Reflect.apply(original, this, [contextId, ...args]);
    } as typeof original;
  });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");

  const table = page.getByTestId("three-table");
  await expect(table).toHaveAttribute("data-webgl-fallback", "true", { timeout: 20_000 });
  await expect(table).toHaveAttribute("data-render-ready", "false");
  await expect(table).toHaveAttribute("data-rendered-tile-count", "0");
  await expect(table).toHaveAttribute("data-rendered-scene-primitives", "0");
  await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(14);
});

test("falls back accessibly after WebGL context loss", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);

  await table.locator("canvas").dispatchEvent("webglcontextlost");

  await expect(table).toHaveAttribute("data-webgl-fallback", "true");
  await expect(table).toHaveAttribute("data-render-ready", "false");
  await expect(table).toHaveAttribute("data-rendered-tile-count", "0");
  await expect(table).toHaveAttribute("data-rendered-scene-primitives", "0");
  await expect(page.getByRole("status", { name: "3D table unavailable" })).toBeVisible();
  await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(14);
  await expectNoAxeViolations(page, ".gameplay-main");
});

test("runs one bounded discard motion and stops invalidating after idle", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  const shell = page.getByTestId("gameplay-shell");
  const consumedBefore = Number(await shell.getAttribute("data-animation-consumed-count"));

  await page.evaluate(() => {
    performance.clearMeasures("three-table-motion-event");
    performance.clearMeasures("three-table-motion-frame");
  });
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __state: unknown;
      __animationStates: string[];
    };
    const table = document.querySelector<HTMLElement>('[data-testid="three-table"]')!;
    browser.__animationStates = [];
    new MutationObserver(() => {
      browser.__animationStates.push(
        `${table.dataset.animationState}:${table.dataset.animationItemId}`,
      );
    }).observe(table, {
      attributes: true,
      attributeFilter: ["data-animation-state", "data-animation-item-id"],
    });
    browser.__socket.emit({
      type: "game_update",
      event: { type: "dahai", actor: 0, tile: 1 },
      state: browser.__state,
    });
  });

  await expect(table).toHaveAttribute("data-animation-state", "idle", { timeout: 5_000 });
  await expect.poll(() => page.evaluate(() =>
    (window as unknown as { __animationStates: string[] }).__animationStates,
  )).toContain("active:0");
  await expect(table).toHaveAttribute("data-last-consumed-animation-id", "0");
  await expect.poll(
    async () => Number(await shell.getAttribute("data-animation-consumed-count")),
  ).toBe(consumedBefore + 1);
  const idleFrameCount = Number(await table.getAttribute("data-animation-frame-count"));
  expect(idleFrameCount).toBeGreaterThan(0);
  await page.waitForTimeout(300);
  expect(Number(await table.getAttribute("data-animation-frame-count"))).toBe(idleFrameCount);
  expect(Number(await shell.getAttribute("data-animation-consumed-count"))).toBe(consumedBefore + 1);
  const performanceEntries = await page.evaluate(() => ({
    events: performance.getEntriesByName("three-table-motion-event", "measure").map(({ duration }) => duration),
    frames: performance.getEntriesByName("three-table-motion-frame", "measure").map(({ duration }) => duration),
  }));
  expect(performanceEntries.events).toHaveLength(1);
  expect(performanceEntries.frames.length).toBeGreaterThan(0);
  const focusedEventBaselineMs = 250;
  expect(performanceEntries.events[0]).toBeLessThanOrEqual(focusedEventBaselineMs * 1.25);
  const sortedFrameDurations = performanceEntries.frames.toSorted((left, right) => left - right);
  const medianFrameDuration = sortedFrameDurations[Math.floor(sortedFrameDurations.length / 2)];
  const softwareWebglFrameBaselineMs = 75;
  expect(medianFrameDuration).toBeLessThanOrEqual(softwareWebglFrameBaselineMs * 1.25);
});

test("captures the complete 4p scene during active motion at 1024x600", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 600 });
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __state: Record<string, unknown>;
    };
    const state = structuredClone(browser.__state) as Record<string, unknown>;
    const players = state.players as Array<Record<string, unknown>>;
    const local = players.find((player) => player.seat === 0)!;
    local.melds = [{ tiles: [1, 2, 3] }];
    browser.__socket.emit({
      type: "game_update",
      event: { type: "pon", actor: 0 },
      state,
    });
  });
  await expect(table).toHaveAttribute("data-animation-state", "active", { timeout: 5_000 });
  await expect(table).toHaveAttribute("data-player-frame-count", "4");
  await expect(table).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
  await expect(table).toHaveAttribute("data-rendered-scene-primitives", /^[1-9]\d*$/);
  await page.screenshot({
    path: "test-results/task-12/4p-1024-active-motion.png",
    fullPage: false,
  });
  await expect(table).toHaveAttribute("data-animation-state", "idle", { timeout: 5_000 });
  await page.screenshot({
    path: "test-results/task-12/4p-1024-idle-motion.png",
    fullPage: false,
  });
});

test("keeps motion within hardware and software renderer budgets", async ({ page }) => {
  async function measureAt(viewport: { width: number; height: number }) {
    await page.setViewportSize(viewport);
    await installCharacterFixtures(page);
    await installSocket(page, "4p-red-east");
    await page.goto("/room/123456/lobby");
    const table = await expectRenderedTable(page);
    await page.waitForTimeout(300);
    const emitMotion = async () => {
      await page.evaluate(() => {
        const browser = window as unknown as {
          __socket: { emit: (value: unknown) => void };
          __state: Record<string, unknown>;
        };
        const state = structuredClone(browser.__state) as Record<string, unknown>;
        const players = state.players as Array<Record<string, unknown>>;
        const local = players.find((player) => player.seat === 0)!;
        local.discards = [...((local.discards as number[] | undefined) ?? []), 1];
        browser.__socket.emit({
          type: "game_update",
          event: { type: "dahai", actor: 0, tile: 1 },
          state,
        });
      });
      await expect(table).toHaveAttribute("data-animation-state", "active", { timeout: 5_000 });
      const activeDpr = Number(await table.getAttribute("data-renderer-pixel-ratio"));
      expect(activeDpr).toBeGreaterThanOrEqual(1);
      expect(activeDpr).toBeLessThanOrEqual(2);
      await expect(table).toHaveAttribute("data-animation-state", "idle", { timeout: 5_000 });
      const idleDpr = Number(await table.getAttribute("data-renderer-pixel-ratio"));
      expect(idleDpr).toBe(activeDpr);
    };
    await page.evaluate(() => {
      performance.clearMeasures("three-table-motion-event");
      performance.clearMeasures("three-table-motion-frame");
    });
    await emitMotion();
    const setupEventMs = await page.evaluate(() =>
      performance.getEntriesByName("three-table-motion-event", "measure")[0]?.duration ?? 0,
    );
    await page.evaluate(() => {
      performance.clearMeasures("three-table-motion-event");
      performance.clearMeasures("three-table-motion-frame");
    });
    await page.waitForTimeout(100);
    for (let index = 0; index < 12; index += 1) await emitMotion();
    const metrics = await page.evaluate(() => {
      const frameEntries = performance
        .getEntriesByName("three-table-motion-frame", "measure")
        .map((entry) => ({
          duration: entry.duration,
          detail: (entry as PerformanceMeasure).detail as { frameIndex?: number } | null,
        }));
      const frameDurations = frameEntries
        .map(({ duration }) => duration)
        .sort((left, right) => left - right);
      const eventDurations = performance
        .getEntriesByName("three-table-motion-event", "measure")
        .map(({ duration }) => duration);
      const steadyDurations = frameEntries
        .filter(({ detail }) => (detail?.frameIndex ?? 0) > 0)
        .map(({ duration }) => duration)
        .sort((left, right) => left - right);
      const percentile = (values: number[], fraction: number) =>
        values[Math.min(values.length - 1, Math.floor(values.length * fraction))] ?? Number.POSITIVE_INFINITY;
      return {
        frameCount: frameDurations.length,
        medianFrameMs: percentile(steadyDurations, 0.5),
        p90FrameMs: percentile(steadyDurations, 0.9),
        eventCount: eventDurations.length,
        eventMs: eventDurations[0] ?? 0,
      };
    });
    expect(metrics.frameCount).toBeGreaterThan(0);
    expect(metrics.eventMs).toBeGreaterThan(0);
    return { ...metrics, setupEventMs };
  }

  const at1600 = await measureAt({ width: 1600, height: 900 });
  const at1920 = await measureAt({ width: 1920, height: 1080 });
  expect(at1600.setupEventMs).toBeGreaterThan(0);
  expect(at1920.setupEventMs).toBeGreaterThan(0);
  expect(at1600.setupEventMs).toBeLessThanOrEqual(750);
  expect(at1920.setupEventMs).toBeLessThanOrEqual(750);
  expect(at1600.eventCount).toBe(12);
  expect(at1920.eventCount).toBe(12);

  if (process.env.DRIICHI_HARDWARE_WEBGL === "1") {
    expect(at1600.frameCount).toBeGreaterThanOrEqual(96);
    expect(at1920.frameCount).toBeGreaterThanOrEqual(96);
    expect(at1600.medianFrameMs).toBeLessThanOrEqual(17.5);
    expect(at1920.medianFrameMs).toBeLessThanOrEqual(32);
    expect(at1920.p90FrameMs).toBeLessThanOrEqual(50);
  } else {
    // SwiftShader is a deterministic correctness proxy, not the desktop GPU
    // named by the 60fps contract. Keep a separate catastrophic-regression
    // bound without pretending software rasterization is hardware evidence.
    expect(at1600.frameCount).toBeGreaterThanOrEqual(48);
    expect(at1920.frameCount).toBeGreaterThanOrEqual(48);
    expect(at1600.medianFrameMs).toBeLessThanOrEqual(60);
    expect(at1600.p90FrameMs).toBeLessThanOrEqual(100);
    expect(at1920.medianFrameMs).toBeLessThanOrEqual(75);
    expect(at1920.p90FrameMs).toBeLessThanOrEqual(120);
  }
});

test("keeps DPR 2 fixed through active and idle rendering", async ({ browser }) => {
  const context = await browser.newContext({
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();

  try {
    await installCharacterFixtures(page);
    await installSocket(page, "4p-red-east");
    await page.goto("/room/123456/lobby");
    const table = await expectRenderedTable(page);
    await expect(table).toHaveAttribute("data-renderer-pixel-ratio", "2");

    await page.evaluate(() => {
      const browser = window as unknown as {
        __socket: { emit: (value: unknown) => void };
        __state: Record<string, unknown>;
      };
      const state = structuredClone(browser.__state) as Record<string, unknown>;
      const players = state.players as Array<Record<string, unknown>>;
      const local = players.find((player) => player.seat === 0)!;
      local.discards = [...((local.discards as number[] | undefined) ?? []), 1];
      browser.__socket.emit({
        type: "game_update",
        event: { type: "dahai", actor: 0, tile: 1 },
        state,
      });
    });

    await expect(table).toHaveAttribute("data-animation-state", "active", { timeout: 5_000 });
    await expect(table).toHaveAttribute("data-renderer-pixel-ratio", "2");
    await expect(table).toHaveAttribute("data-animation-state", "idle", { timeout: 5_000 });
    await expect(table).toHaveAttribute("data-renderer-pixel-ratio", "2");
  } finally {
    await context.close();
  }
});

test("keeps reduced-motion discard effects static", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await expect(page.getByTestId("gameplay-shell")).toHaveAttribute(
    "data-motion",
    "static",
  );
  const shell = page.getByTestId("gameplay-shell");
  const enqueuedBefore = Number(
    await shell.getAttribute("data-animation-enqueued-count"),
  );
  const consumedBefore = Number(
    await shell.getAttribute("data-animation-consumed-count"),
  );
  expect(Number.isFinite(enqueuedBefore)).toBe(true);
  expect(Number.isFinite(consumedBefore)).toBe(true);
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __state: unknown;
    };
    browser.__socket.emit({
      type: "game_update",
      event: { type: "dahai", actor: 0, tile: 1 },
      state: browser.__state,
    });
  });
  await expect(table).toHaveAttribute("data-render-ready", "true");
  await expect.poll(
    async () => Number(await shell.getAttribute("data-animation-enqueued-count")),
    { timeout: 5_000, message: "discard event should enqueue an animation" },
  ).toBe(enqueuedBefore + 1);
  await expect.poll(
    async () => Number(await shell.getAttribute("data-animation-consumed-count")),
    { timeout: 5_000, message: "reduced-motion animation should be consumed" },
  ).toBe(consumedBefore + 1);
  await expect(table).toHaveAttribute("data-animation-state", "static");
  await expect(table).toHaveAttribute("data-last-consumed-animation-id", "0");
  expect(Number(await table.getAttribute("data-animation-frame-count"))).toBe(0);
  await page.screenshot({
    path: "test-results/immersive-table/4p-reduced-motion.png",
    fullPage: false,
  });
});

test("keeps Settings accessible when opened", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  await page.getByText("Settings", { exact: true }).click();
  await expect(page.locator(".audio-settings[open]")).toBeVisible();
  await expectNoSeriousOrCriticalViolations(page, ".gameplay-shell");
});

test("omits the Action row when no Decision is open", async ({ page }) => {
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  state.decision = null;
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east", state);
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  await expect(page.getByTestId("action-deck")).toHaveCount(0);
});

for (const viewport of requiredViewports) {
  test(`candidate popup transfers focus and stays in its safe area at ${viewport.label}`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await installCharacterFixtures(page);
    const popupState = JSON.parse(JSON.stringify(projectionFixture.projections["4p-red-east"])) as Record<string, unknown>;
    (popupState.decision as Record<string, unknown>).actions = [
      { action_id: "chi-1", action: { Chi: { target: 1, called: 1, consumed: [0, 4] } } },
      { action_id: "chi-2", action: { Chi: { target: 1, called: 2, consumed: [1, 5] } } },
      { action_id: "pass-1", action: "Pass" },
    ];
    await installSocket(page, "4p-red-east", popupState);
    await page.goto("/room/123456/lobby");
    const trigger = page.getByRole("button", { name: "Chi (2)" });
    await trigger.click();
    const dialog = page.getByRole("dialog", { name: "Choose a legal candidate" });
    await expect(dialog).toHaveAttribute("aria-modal", "true");
    await expect(trigger).toHaveAttribute("aria-haspopup", "dialog");
    await expect(trigger).toHaveAttribute("aria-expanded", "true");
    const dialogId = await dialog.getAttribute("id");
    expect(dialogId).toBeTruthy();
    await expect(trigger).toHaveAttribute("aria-controls", dialogId ?? "");
    await expect(dialog.locator(".candidate-list button").first()).toBeFocused();
    await expectInsideStageAndClearOfHand(
      page.locator(".table-letterbox"),
      dialog,
      page.locator(".table-hit-layer"),
    );
    await expectNoAxeViolations(page, ".gameplay-main");
    const candidateButtons = dialog.locator(".candidate-list button");
    await candidateButtons.last().focus();
    await page.keyboard.press("Tab");
    await expect(dialog.getByRole("button", { name: "Close" })).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(candidateButtons.last()).toBeFocused();

    const backgroundPass = page.getByRole("button", { name: "Pass" });
    await backgroundPass.focus();
    await expect(backgroundPass).not.toBeFocused();
    await expect(candidateButtons.last()).toBeFocused();

    await candidateButtons.first().click();
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute("aria-busy", "true");
    await page.evaluate(() => {
      (window as unknown as { __socket: { emit: (value: unknown) => void } }).__socket.emit({
        type: "action_result",
        decision_id: "d1",
        action_id: "chi-1",
        status: "rejected",
        code: "illegal_action",
      });
    });
    await expect(dialog).toHaveAttribute("aria-busy", "false");
    await expect(candidateButtons.first()).toBeEnabled();
    await expect(candidateButtons.first()).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(trigger).toHaveAttribute("aria-expanded", "false");
    await expect(trigger).toBeFocused();
  });
}

test("disables open candidate choices after transport disconnects", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  const popupState = structuredClone(projectionFixture.projections["4p-red-east"]);
  (popupState.decision as Record<string, unknown>).actions = [
    { action_id: "chi-1", action: { Chi: { target: 1, called: 1, consumed: [0, 4] } } },
    { action_id: "chi-2", action: { Chi: { target: 1, called: 2, consumed: [1, 5] } } },
  ];
  await installSocket(page, "4p-red-east", popupState);
  await page.goto("/room/123456/lobby");
  const trigger = page.getByRole("button", { name: "Chi (2)" });
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: "Choose a legal candidate" });
  await expect(dialog.locator(".candidate-list button")).toHaveCount(2);
  await page.evaluate(() => {
    (window as unknown as { __socket: { onerror: (() => void) | null } }).__socket.onerror?.();
  });
  await expect(dialog.locator(".candidate-list button").first()).toBeDisabled();
  await expect(dialog.locator(".candidate-list button").last()).toBeDisabled();
});

test("keeps the Riichi legal highlight while its authoritative action is pending", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  const riichiState = JSON.parse(JSON.stringify(projectionFixture.projections["4p-red-east"])) as Record<string, unknown>;
  const hand = (riichiState.players as Array<Record<string, unknown>>)[0].hand as number[];
  (riichiState.decision as Record<string, unknown>).actions = [
    { action_id: "riichi-1", action: { riichi_discard: { tile: hand[0] } } },
  ];
  await installSocket(page, "4p-red-east", riichiState);
  await page.goto("/room/123456/lobby");
  await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20000 });
  const legal = page.locator(".table-tile-hit.is-legal");
  await expect(legal).toHaveCount(1);
  const initialStyle = await legal.first().evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      borderStyle: style.borderTopStyle,
      borderWidth: style.borderTopWidth,
      background: style.backgroundColor,
      boxShadow: style.boxShadow,
    };
  });
  expect(initialStyle.borderStyle).toBe("none");
  expect(initialStyle.borderWidth).toBe("0px");
  expect(initialStyle.background).toBe("rgba(0, 0, 0, 0)");
  expect(initialStyle.boxShadow).toBe("none");
  await legal.first().focus();
  await expect.poll(() => legal.first().evaluate((element) => getComputedStyle(element).outlineStyle)).not.toBe("none");
  await legal.first().click();
  await expect(page.getByTestId("action-deck")).toHaveAttribute("aria-busy", "true");
  await expect(legal).toHaveCount(1);
  const pendingStyle = await legal.first().evaluate((element) => getComputedStyle(element).backgroundColor);
  expect(pendingStyle).toBe("rgba(0, 0, 0, 0)");
});

test("shows the authoritative Mangan post-match results surface", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __state: unknown;
    };
    browser.__socket.emit({
      type: "game_update",
      event: {
        type: "action_resolved",
        events: [
          {
            hora: {
              actor: 0,
              target: 1,
              han: 5,
              fu: 30,
              delta: [8000, -8000, 0, 0],
            },
          },
        ],
      },
      state: browser.__state,
    });
  });
  await page.evaluate(() => {
    const browser = window as unknown as {
      __socket: { emit: (value: unknown) => void };
      __room: Record<string, unknown>;
      __state: unknown;
    };
    browser.__socket.emit({
      type: "room_update",
      room: {
        ...browser.__room,
        phase: "post_match",
        result: {
          mode: "4p-red-east",
          players: [
            {
              participant_id: "P1",
              display_name: "Mika",
              rank: 1,
              final_score: 45000,
            },
            {
              participant_id: "P2",
              display_name: "Nori",
              rank: 2,
              final_score: 30000,
            },
            {
              participant_id: "P3",
              display_name: "Ren",
              rank: 3,
              final_score: 15000,
            },
            {
              participant_id: "P4",
              display_name: "Aya",
              rank: 4,
              final_score: 10000,
            },
          ],
        },
      },
      state: browser.__state,
    });
  });
  await expect(table).toHaveAttribute("data-render-ready", "true");
  await expect(page.getByTestId("results-panel")).toBeVisible();
  await expect(page.getByText("Permanent Auto")).toHaveCount(3);
  await expect(page.getByTestId("results-panel").locator(".results-winner-hero-portrait")).toHaveAttribute("alt", "Mika portrait");
  await page.screenshot({
    path: "test-results/task-12/results-portrait-state.png",
    fullPage: false,
  });
});

test("shows guidance below the supported gameplay viewport", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1023, height: 599 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expect(page.getByText("Widen this window to play.")).toBeVisible();
  await expect(page.getByTestId("three-table")).toHaveCount(0);
});

test("reactively gates the table on resize and keeps terminal alerts visible when narrow", async ({ page }) => {
  await page.setViewportSize({ width: 1023, height: 599 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expect(page.getByText("Widen this window to play.")).toBeVisible();
  await page.setViewportSize({ width: 1280, height: 720 });
  await expectRenderedTable(page);
  await page.setViewportSize({ width: 900, height: 599 });
  await expect(page.getByText("Widen this window to play.")).toBeVisible();
  await page.evaluate(() => {
    const socket = (window as unknown as {
      __socket: { onclose: ((event: { code: number; reason: string }) => void) | null };
    }).__socket;
    socket.onclose?.({ code: 4002, reason: "room_deleted" });
  });
  await expect(page.getByRole("alert")).toContainText("deleted this Room");
});

test("shows in-table synchronization before the first projection", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east", undefined, 2_000);
  await page.goto("/room/123456/lobby");
  await expect(page.getByText("Waiting for an authoritative projection")).toBeVisible();
  await expect(page.locator(".gameplay-sync-state[role=\"status\"]")).toHaveCount(1);
  await expect(page.getByTestId("three-table").locator("[role=\"status\"]")).toHaveCount(0);
  await expect(page.getByTestId("action-deck")).toHaveCount(0);
});

test("preserves the last table scene during a transient reconnect", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  await page.evaluate(() => {
    const socket = (window as unknown as {
      __socket: {
        onerror: (() => void) | null;
        onclose: ((event: { code: number; reason: string }) => void) | null;
      };
    }).__socket;
    socket.onerror?.();
  });
  await expect(page.getByTestId("three-table")).toBeVisible();
  await expect(page.locator(".gameplay-blocking-state")).toHaveCount(0);
  await page.evaluate(() => {
    const socket = (window as unknown as {
      __socket: { onclose: ((event: { code: number; reason: string }) => void) | null };
    }).__socket;
    socket.onclose?.({ code: 1006, reason: "" });
  });
  await expect(page.getByTestId("three-table")).toBeVisible();
  await expect(page.getByText(/last authoritative table state remains visible/i)).toBeVisible();
  await expect(page.locator(".gameplay-shell")).toHaveAttribute("data-testid", "gameplay-shell");
});

test("keeps full player status semantics while terminal gameplay is blocked", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  const players = state.players as Array<Record<string, unknown>>;
  players[0].display_name = "A very long participant display name";
  players[0].riichi = true;
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east", state);
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  const playerStatus = page.locator('ul[aria-label="Player status"]');
  await expect(playerStatus).toContainText("A very long participant display name");
  await expect(playerStatus).toContainText("Score 25,000");
  await expect(playerStatus).toContainText("Seat position: bottom");
  await expect(playerStatus).toContainText("Riichi");
  await page.evaluate(() => {
    const socket = (window as unknown as {
      __socket: { onclose: ((event: { code: number; reason: string }) => void) | null };
    }).__socket;
    socket.onclose?.({ code: 4002, reason: "room_deleted" });
  });
  await expect(page.locator(".gameplay-blocking-state")).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("deleted this Room");
  await expect(page.getByTestId("gameplay-shell")).toBeVisible();
  await expect(page.locator(".lobby-shell")).toHaveCount(0);
  await expect(playerStatus).toContainText("A very long participant display name");
  await expect(page.getByTestId("three-table")).toHaveCount(0);
});

test("blocks play when the Room is deleted", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  await page.evaluate(() => {
    const socket = (window as unknown as {
      __socket: { onclose: ((event: { code: number; reason: string }) => void) | null };
    }).__socket;
    socket.onclose?.({ code: 4002, reason: "room_deleted" });
  });
  await expect(page.locator(".gameplay-blocking-state")).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("deleted this Room");
  await expect(page.locator(".table-tile-hit")).toHaveCount(0);
});
