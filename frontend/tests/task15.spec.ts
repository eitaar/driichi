import AxeBuilder from "@axe-core/playwright";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "@playwright/test";

test.describe.configure({ mode: "serial" });

const characterFixtureRoot = resolve(
  process.cwd(),
  "tests/fixtures/task12-characters/player-red",
);

const summary = {
  match_id: "MATCH15",
  source: "room",
  room_name: "Night Market",
  game_mode: "4p-red-east",
  started_at: "2026-09-16T00:00:00Z",
  completed_at: "2026-09-16T00:10:00Z",
  file_size: 512,
  availability: "available",
  replay_available: true,
};

function replayFrame(index: number, event: string, auxiliary_events: unknown[] = []) {
  return {
    event_index: index,
    visible_event: { type: event },
    visible_state: {
      audience: "replay_admin",
      mode: "FourPlayerRedEast",
      round: "East",
      kyoku: index < 2 ? 1 : 2,
      remaining_wall: Math.max(0, 66 - index * 7),
      players: [0, 1, 2, 3].map((seat) => ({
        seat,
        participant_id: `P${seat}`,
        display_name: `Seat ${seat}`,
        kind: "BuiltInBot",
        score: 25000 + (seat === 0 ? 1500 : -500),
        hand: [0, 1, 2, 10, 11, 12, 20, 21, 22, 30, 31, 32, 40, 41],
        concealed_count: 14,
        discards: [3, 13, 23, 33, 43, 53],
        melds: [{ tiles: [4, 5, 6] }],
        riichi: seat === 2 && index > 0,
      })),
      dora_indicators: [0, 16],
      decision: null,
    },
    auxiliary_events,
  };
}

const replay = {
  ...summary,
  players: [0, 1, 2, 3].map((seat) => ({
    participant_id: `P${seat}`,
    display_name: `Seat ${seat}`,
    participant_kind: "built_in_bot",
    seat,
    character_id: "ordinary-pack",
    final_points: 25000,
  })),
  frames: [
    replayFrame(0, "start_game", [{ event: { Disconnected: { seat: 1 } }, line_index: 0, phase: "before", sequence: 1 }]),
    replayFrame(1, "start_kyoku", [{ event: { Reconnected: { seat: 1 } }, line_index: 1, phase: "after", sequence: 2 }]),
    replayFrame(2, "end_game"),
  ],
};

async function expectNoAxeViolations(page: import("@playwright/test").Page, include?: string) {
  const axe = new AxeBuilder({ page });
  if (include) axe.include(include);
  const results = await axe.analyze();
  expect(
    results.violations,
    results.violations.map((violation) => `${violation.id}:${violation.impact}`).join(", "),
  ).toEqual([]);
}

for (const viewport of [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1280, height: 720, label: "1280x720" },
  { width: 1600, height: 900, label: "1600x900" },
  { width: 1920, height: 1080, label: "1920x1080" },
]) {
  test(`captures Replay library and viewer at ${viewport.label}`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.route("**/assets/characters/ordinary-pack/**", async (route) => {
      const fileName = new URL(route.request().url()).pathname.endsWith("/icon.webp")
        ? "icon.webp"
        : "portrait.webp";
      await route.fulfill({
        status: 200,
        contentType: "image/webp",
        body: readFileSync(resolve(characterFixtureRoot, fileName)),
      });
    });
    await page.route("**/api/v1/admin/replays**", async (route) => {
      if (route.request().method() === "DELETE") {
        await route.fulfill({ status: 204, body: "" });
        return;
      }
      const url = new URL(route.request().url());
      if (url.pathname.endsWith("/MATCH15")) {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(replay) });
        return;
      }
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ replays: [summary], offset: 0, limit: 50, total: 1, has_more: false }) });
    });
    await page.goto("/admin/replays");
    await expect(page.getByRole("heading", { name: /replay library/i })).toBeVisible();
    await expect(page.getByText("Night Market")).toBeVisible();
    await page.screenshot({ path: `test-results/task-15/library-${viewport.label}.png`, fullPage: false });

    await page.getByRole("link", { name: /view replay match15/i }).click();
    await expect(page.getByRole("heading", { name: /night market replay/i })).toBeVisible();
    const stage = page.locator(".replay-table-stage");
    await expect(stage).toBeVisible();
    const foldComposition = await stage.evaluate((element) => {
      const stage = element.getBoundingClientRect();
      const eventLog = document.querySelector<HTMLElement>(".replay-event-log")?.getBoundingClientRect();
      return {
        viewport: { width: window.innerWidth, height: window.innerHeight },
        stage: { top: stage.top, right: stage.right, bottom: stage.bottom, left: stage.left, width: stage.width, height: stage.height },
        eventLogTop: eventLog?.top ?? 0,
      };
    });
    expect(foldComposition.stage.top).toBeGreaterThanOrEqual(72);
    expect(foldComposition.stage.bottom).toBeLessThanOrEqual(foldComposition.viewport.height);
    expect(foldComposition.stage.width / foldComposition.stage.height).toBeCloseTo(16 / 9, 2);
    expect(foldComposition.stage.width).toBeGreaterThanOrEqual(
      Math.min(
        foldComposition.viewport.width * 0.9,
        (foldComposition.viewport.height - 72) * (16 / 9) * 0.98,
      ),
    );
    expect(foldComposition.eventLogTop).toBeGreaterThanOrEqual(foldComposition.stage.bottom);
    const statusBounds = await page.locator(".replay-status-toast").evaluate((element) => {
      const status = element.getBoundingClientRect();
      const stage = element.closest(".replay-table-stage")?.getBoundingClientRect();
      return {
        status: { left: status.left, top: status.top, right: status.right, bottom: status.bottom },
        stage: stage ? { left: stage.left, top: stage.top, right: stage.right, bottom: stage.bottom, height: stage.height } : null,
      };
    });
    expect(statusBounds.stage).not.toBeNull();
    expect(statusBounds.status.left).toBeGreaterThanOrEqual(statusBounds.stage!.left);
    expect(statusBounds.status.right).toBeLessThanOrEqual(statusBounds.stage!.right);
    expect(statusBounds.status.top).toBeGreaterThanOrEqual(statusBounds.stage!.top);
    expect(statusBounds.status.bottom).toBeLessThanOrEqual(statusBounds.stage!.top + statusBounds.stage!.height * 0.10);
    const replayControls = stage.locator(".replay-controls-overlay");
    await expect(replayControls).toBeVisible();
    await expect(stage.locator(".replay-status-toast")).toBeVisible();
    await expect(page.locator(".replay-event-log")).toBeVisible();
    {
      const transportBounds = await replayControls.evaluate((element) => {
        const container = element.getBoundingClientRect();
        const controls = Array.from(
          element.querySelectorAll<HTMLElement>("button, select"),
        ).map((control) => {
          const rect = control.getBoundingClientRect();
          return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
        });
        const stage = element.parentElement?.getBoundingClientRect();
        return {
          container: {
            left: container.left,
            top: container.top,
            right: container.right,
            bottom: container.bottom,
          },
          stage: stage ? {
            left: stage.left,
            top: stage.top,
            right: stage.right,
            bottom: stage.bottom,
          } : null,
          controls,
          scrollHeight: element.scrollHeight,
          clientHeight: element.clientHeight,
          scrollWidth: element.scrollWidth,
          clientWidth: element.clientWidth,
        };
      });
      expect(transportBounds.stage).not.toBeNull();
      expect(transportBounds.container.left).toBeGreaterThanOrEqual(transportBounds.stage!.left);
      expect(transportBounds.container.top).toBeGreaterThanOrEqual(transportBounds.stage!.top);
      expect(transportBounds.container.right).toBeLessThanOrEqual(transportBounds.stage!.right);
      expect(transportBounds.container.bottom).toBeLessThanOrEqual(transportBounds.stage!.bottom);
      expect(transportBounds.controls).not.toHaveLength(0);
      expect(Math.max(...transportBounds.controls.map(({ top }) => top)) - Math.min(...transportBounds.controls.map(({ top }) => top))).toBeLessThanOrEqual(2);
      expect(Math.max(...transportBounds.controls.map(({ bottom }) => bottom)) - Math.min(...transportBounds.controls.map(({ bottom }) => bottom))).toBeLessThanOrEqual(2);
      for (const control of transportBounds.controls) {
        expect(control.left).toBeGreaterThanOrEqual(transportBounds.container.left);
        expect(control.top).toBeGreaterThanOrEqual(transportBounds.container.top);
        expect(control.right).toBeLessThanOrEqual(transportBounds.container.right);
        expect(control.bottom).toBeLessThanOrEqual(transportBounds.container.bottom);
      }
      expect(transportBounds.scrollHeight).toBeLessThanOrEqual(
        Math.ceil(transportBounds.container.bottom - transportBounds.container.top),
      );
      expect(transportBounds.scrollWidth).toBeLessThanOrEqual(
        Math.ceil(transportBounds.container.right - transportBounds.container.left),
      );
    }
    await expect(page.locator(".replay-table-wrap + .replay-controls")).toHaveCount(0);
    const table = page.getByTestId("three-table");
    await expect(table).toHaveAttribute("data-surface", "replay");
    await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
    await expect(table).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
    await page.screenshot({ path: `test-results/task-15/replay-all-hands-${viewport.label}.png`, fullPage: false });
    await expect(table).toHaveAttribute("data-rendered-scene-primitives", /^[1-9]\d*$/);
    const tableHeightRatio = Number(await table.getAttribute("data-table-height-ratio"));
    const tableWidthRatio = Number(await table.getAttribute("data-table-width-ratio"));
    expect(tableWidthRatio).toBeGreaterThanOrEqual(0.82);
    expect(tableWidthRatio).toBeLessThanOrEqual(0.9);
    expect(tableHeightRatio).toBeGreaterThanOrEqual(0.78);
    expect(tableHeightRatio).toBeLessThanOrEqual(0.88);
    expect(Number(await table.getAttribute("data-rendered-tile-count"))).toBeGreaterThanOrEqual(100);
    await expect(page.locator(".table-player-frame")).toHaveCount(4);
    await expect(page.locator(".table-player-portrait")).toHaveCount(4);
    await expect.poll(() => page.locator(".table-player-portrait").evaluateAll((images) =>
      images.every((image) => (image as HTMLImageElement).naturalWidth > 1),
    )).toBe(true);
    await expect(page.locator('.table-player-frame[data-position="top"]')).toHaveCount(1);
    const composition = await stage.evaluate((element) => {
      const bounds = element.getBoundingClientRect();
      const safeHand = {
        left: bounds.left + bounds.width * .18,
        right: bounds.right - bounds.width * .18,
        top: bounds.top + bounds.height * .64,
        bottom: bounds.top + bounds.height * .8,
      };
      const boxes = Array.from(
        element.querySelectorAll<HTMLElement>(".table-player-frame, .replay-controls-overlay"),
      ).map((target) => {
        const rect = target.getBoundingClientRect();
        return {
          label: target.className,
          left: rect.left,
          right: rect.right,
          top: rect.top,
          bottom: rect.bottom,
        };
      });
      return {
        bounds: { left: bounds.left, right: bounds.right, top: bounds.top, bottom: bounds.bottom },
        safeHand,
        boxes,
      };
    });
    expect(composition.boxes).toHaveLength(5);
    for (const box of composition.boxes) {
      expect(box.left).toBeGreaterThanOrEqual(composition.bounds.left);
      expect(box.right).toBeLessThanOrEqual(composition.bounds.right);
      expect(box.top).toBeGreaterThanOrEqual(composition.bounds.top);
      expect(box.bottom).toBeLessThanOrEqual(composition.bounds.bottom);
      const intersectsHand = box.left < composition.safeHand.right
        && box.right > composition.safeHand.left
        && box.top < composition.safeHand.bottom
        && box.bottom > composition.safeHand.top;
      expect(intersectsHand, box.label).toBe(false);
    }
    await expect(page.getByText(/room assets/i)).toBeVisible();
    await expect(page.getByRole("status")).toContainText(/disconnected/i);
    await expectNoAxeViolations(page, ".replay-table-stage");
    await page.getByRole("button", { name: /next event/i }).click();
    await expect(page.getByRole("button", { name: /^play$/i })).toBeVisible();
    const play = replayControls.getByRole("button", { name: /^play$/i });
    await play.focus();
    await expect(play).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("button", { name: /previous event/i })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("button", { name: /next event/i })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("button", { name: "0.5x" })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("button", { name: "1x" })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("button", { name: "2x" })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("button", { name: "4x" })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(replayControls.getByRole("combobox", { name: /jump to kyoku/i })).toBeFocused();
    await replayControls.getByRole("button", { name: "2x" }).click();
    await expect(page.getByRole("button", { name: "2x" })).toHaveAttribute("aria-pressed", "true");
    await page.screenshot({ path: `test-results/task-15/viewer-${viewport.label}.png`, fullPage: false });

    await page.getByRole("link", { name: /back to replay library/i }).click();
    await page.getByRole("button", { name: /delete replay match15/i }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: /confirm delete/i }).click();
    await expect(dialog).toHaveCount(0);
  });

  test(`covers URL pagination, retry, later-page deletion, and playback destinations at ${viewport.label}`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    let deletedLaterPage = false;
    let failNextList = false;
    await page.route("**/assets/characters/ordinary-pack/**", async (route) => {
      const fileName = new URL(route.request().url()).pathname.endsWith("/icon.webp")
        ? "icon.webp"
        : "portrait.webp";
      await route.fulfill({
        status: 200,
        contentType: "image/webp",
        body: readFileSync(resolve(characterFixtureRoot, fileName)),
      });
    });
    await page.route("**/api/v1/admin/replays**", async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      if (request.method() === "DELETE") {
        if (url.pathname.endsWith("/MATCH51")) deletedLaterPage = true;
        await route.fulfill({ status: 204, body: "" });
        return;
      }
      if (url.pathname.endsWith("/MATCH15")) {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(replay) });
        return;
      }
      if (url.pathname.endsWith("/RANKED15")) {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ ...replay, match_id: "RANKED15", source: "ranked", room_name: null }) });
        return;
      }
      if (failNextList) {
        failNextList = false;
        await route.fulfill({ status: 503, contentType: "application/problem+json", body: JSON.stringify({ code: "replay_storage_unavailable", detail: "Temporary Replay storage failure." }) });
        return;
      }
      const offset = Number(url.searchParams.get("offset") ?? 0);
      const body = offset === 50
        ? { replays: deletedLaterPage ? [] : [{ ...summary, match_id: "MATCH51" }], offset, limit: 50, total: deletedLaterPage ? 50 : 51, has_more: false }
        : { replays: [summary], offset, limit: 50, total: deletedLaterPage ? 50 : 51, has_more: !deletedLaterPage };
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
    });

    await page.goto("/admin/replays");
    await expect(page.getByRole("heading", { name: /replay library/i })).toBeVisible();
    await page.getByRole("button", { name: /^next$/i }).click();
    await expect(page).toHaveURL(/\/admin\/replays\?offset=50&limit=50$/);
    await expect(page.getByText("MATCH51")).toBeVisible();
    await page.goBack();
    await expect(page).toHaveURL(/\/admin\/replays$/);
    await expect(page.getByText("Night Market")).toBeVisible();
    await page.goForward();
    await expect(page).toHaveURL(/\/admin\/replays\?offset=50&limit=50$/);
    await expect(page.getByText("MATCH51")).toBeVisible();
    await page.getByRole("button", { name: /delete replay match51/i }).click();
    await page.getByRole("dialog").getByRole("button", { name: /confirm delete/i }).click();
    await expect(page).toHaveURL(/\/admin\/replays$/);
    await expect(page.getByText("Night Market")).toBeVisible();

    failNextList = true;
    await page.goto("/admin/replays");
    await expect(page.getByRole("heading", { name: /replay library unavailable/i })).toBeVisible();
    await page.getByRole("button", { name: /^retry$/i }).click();
    await expect(page.getByText("Night Market")).toBeVisible();

    await page.getByRole("link", { name: /view replay match15/i }).click();
    await expect(page.getByText(/room assets/i)).toBeVisible();
    await page.clock.install();
    await page.clock.pauseAt(Date.now() + 1_000);
    await page.getByRole("button", { name: /^play$/i }).click();
    await expect(page.getByRole("button", { name: /^pause$/i })).toBeVisible();
    await page.getByRole("button", { name: /next event/i }).click();
    await expect(page.getByText("EVENT 002")).toBeVisible();
    await expect(page.getByRole("button", { name: /^play$/i })).toBeVisible();
    await page.getByRole("button", { name: /next event/i }).click();
    await expect(page.getByText("EVENT 003")).toBeVisible();
    await page.getByRole("button", { name: /previous event/i }).click();
    await expect(page.getByText("EVENT 002")).toBeVisible();
    await page.getByRole("combobox", { name: /jump to kyoku/i }).selectOption("2");
    await expect(page.getByText("EVENT 003")).toBeVisible();
    await expect(page.getByRole("button", { name: /^play$/i })).toBeVisible();

    await page.clock.resume();
    await page.goto("/admin/replays/RANKED15");
    await expect(page.getByText(/generic \/ silent/i)).toBeVisible();
  });
}
