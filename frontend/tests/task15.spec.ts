import { expect, test } from "@playwright/test";

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
      players: [0, 1, 2, 3].map((seat) => ({
        seat,
        participant_id: `P${seat}`,
        display_name: `Seat ${seat}`,
        kind: "BuiltInBot",
        score: 25000,
        hand: [0, 1, 2],
        concealed_count: 3,
        discards: [],
        melds: [],
        riichi: false,
      })),
      dora_indicators: [0],
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

for (const viewport of [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1440, height: 900, label: "1440x900" },
]) {
  test(`captures Replay library and viewer at ${viewport.label}`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.route("**/assets/characters/ordinary-pack/**", async (route) => {
      await route.fulfill({ status: 200, contentType: "image/svg+xml", body: "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><rect width=\"1\" height=\"1\" fill=\"#fff\"/></svg>" });
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
    await expect(page.getByTestId("pixi-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
    await expect(page.getByText(/room assets/i)).toBeVisible();
    await expect(page.getByRole("status")).toContainText(/disconnected/i);
    await page.getByRole("button", { name: /next event/i }).click();
    await expect(page.getByRole("button", { name: /^play$/i })).toBeVisible();
    await page.getByRole("button", { name: "2x" }).click();
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
      await route.fulfill({ status: 200, contentType: "image/svg+xml", body: "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><rect width=\"1\" height=\"1\" fill=\"#fff\"/></svg>" });
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
    await page.clock.pauseAt(Date.now());
    await page.getByRole("button", { name: /^play$/i }).click();
    await expect(page.getByRole("button", { name: /^pause$/i })).toBeVisible();
    await page.getByRole("button", { name: /next event/i }).click();
    await expect(page.getByText("EVENT 2 / 3")).toBeVisible();
    await expect(page.getByRole("button", { name: /^play$/i })).toBeVisible();
    await page.getByRole("button", { name: /next event/i }).click();
    await expect(page.getByText("EVENT 3 / 3")).toBeVisible();
    await page.getByRole("button", { name: /previous event/i }).click();
    await expect(page.getByText("EVENT 2 / 3")).toBeVisible();
    await page.getByRole("combobox", { name: /jump to kyoku/i }).selectOption("2");
    await expect(page.getByText("EVENT 3 / 3")).toBeVisible();
    await expect(page.getByRole("button", { name: /^play$/i })).toBeVisible();

    await page.clock.resume();
    await page.goto("/admin/replays/RANKED15");
    await expect(page.getByText(/generic \/ silent/i)).toBeVisible();
  });
}
