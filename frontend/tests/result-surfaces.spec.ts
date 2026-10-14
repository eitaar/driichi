import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Page } from "@playwright/test";

const fixtureRoot = resolve(process.cwd(), "tests/fixtures/task12-characters");
const fixtures = JSON.parse(readFileSync(resolve(process.cwd(), "tests/fixtures/task12-projection.json"), "utf8")) as {
  room_envelopes: Record<string, Record<string, unknown>>;
  projections: Record<string, Record<string, unknown>>;
};
const win = {
  event_index: 1, winner: "P1", seat: 0, target: 1, tsumo: false,
  concealed: [0, 1, 32, 33, 34, 36, 40, 44, 60, 64, 68],
  melds: [{ kind: "Ankan", tiles: [52, 53, 54, 55], opened: false, from_who: null, called_tile: null }], winning_tile: 32, winning_tile_index: 2,
  yaku: [{ id: 1, name: "riichi", han: 1, yakuman_multiplier: null }],
  evaluation: { Ordinary: { han: 1, fu: 30, limit: null } }, win_points: 1000,
};
const step = {
  id: { match_id: "MATCH", kyoku_sequence: 1, step_index: 0 }, content: { Win: win },
  published_at_ms: 100, deadline_at_ms: 5100, remaining_ms: 5000, required: ["P1"], confirmed: [] as string[],
};
const standings = {
  mode: "FourPlayerRedEast", final_scores: [45000, 30000, 15000, 10000],
  players: ["Mika", "Nori", "Ren", "Aya"].map((display_name, seat) => ({
    participant_id: `P${seat + 1}`, display_name, kind: seat === 0 ? "Human" : "BuiltInBot",
    seat, rank: seat + 1, final_score: [45000, 30000, 15000, 10000][seat],
  })),
};

async function installAssets(page: Page, refuse = false) {
  await page.route("**/assets/characters/**", async (route) => {
    const match = /^\/assets\/characters\/([^/]+)\/(portrait|icon)\.webp$/.exec(new URL(route.request().url()).pathname);
    if (!refuse && match) {
      try {
        await route.fulfill({ status: 200, contentType: "image/webp", body: readFileSync(resolve(fixtureRoot, match[1], `${match[2]}.webp`)) });
        return;
      } catch { /* Missing packs and voices are deliberate refusal controls. */ }
    }
    await route.fulfill({ status: 404, body: "missing test asset" });
  });
}

async function installSocket(page: Page, mode = "4p-red-east") {
  await page.addInitScript(({ room, state }) => {
    const browser = window as unknown as {
      __room: Record<string, unknown>; __state: Record<string, unknown>; __sent: unknown[];
      __socket: { emit: (value: unknown) => void };
    };
    browser.__room = room; browser.__state = state; browser.__sent = [];
    const NativeSocket = window.WebSocket;
    class GameplaySocket {
      static OPEN = 1;
      readyState = 1;
      onopen: (() => void) | null = null;
      onmessage: ((event: { data: string }) => void) | null = null;
      onclose: (() => void) | null = null;
      onerror: (() => void) | null = null;
      constructor() {
        browser.__socket = this;
        setTimeout(() => { this.onopen?.(); this.emit({ type: "snapshot", room: browser.__room, state: browser.__state }); }, 0);
      }
      send(value: string) { browser.__sent.push(JSON.parse(value)); }
      emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
      close() { this.readyState = 3; this.onclose?.(); }
    }
    Object.defineProperty(window, "WebSocket", { configurable: true, value: Object.assign(function (url: string, protocols?: string | string[]) {
      return url.includes("/ws/v1/rooms/") ? new GameplaySocket() : new NativeSocket(url, protocols);
    }, { OPEN: 1 }) });
    sessionStorage.setItem("driichi:participant:123456", "P1");
  }, { room: fixtures.room_envelopes[mode], state: fixtures.projections[mode] });
}

async function showRoundWin(page: Page, currentStep = step) {
  await page.evaluate((step) => {
    const browser = window as unknown as { __room: Record<string, unknown>; __state: Record<string, unknown>; __socket: { emit: (value: unknown) => void } };
    browser.__room = { ...browser.__room, revision: 13, kyoku_end: step };
    browser.__state = { ...browser.__state, decision: null };
    browser.__socket.emit({ type: "game_update", room: browser.__room, state: browser.__state,
      event: { type: "action_resolved", events: [{ hora: { actor: 0, target: 1, delta: (browser.__state.players as unknown[]).map((_, seat) => seat === 0 ? 99999 : seat === 1 ? -99999 : 0) } }] } });
  }, currentStep);
  await expect(page.getByTestId("round-win-surface")).toBeVisible();
  await expect(page.getByTestId("winning-tile")).toHaveCount(1);
  await expect(page.getByLabel("Ron win details")).toContainText("1,000");
  await expect(page.getByText("99,999", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("group", { name: "Closed Ankan" }).locator('img[src$="/Back.svg"]')).toHaveCount(2);
}

async function showStandings(page: Page, result = standings) {
  await page.evaluate((result) => {
    const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
    browser.__socket.emit({ type: "room_update", room: { ...browser.__room, phase: "post_match", kyoku_end: null, result }, state: browser.__state });
  }, result);
  await expect(page.getByTestId("results-panel")).toBeVisible();
  await expect(page.getByRole("list", { name: "Final standings" }).getByRole("listitem")).toHaveCount(result.players.length);
}

function standingsFor(mode: string) {
  const count = mode.startsWith("3p") ? 3 : 4;
  const scores = count === 3 ? [45000, 35000, 25000] : standings.final_scores;
  return { ...standings, mode: count === 3 ? "ThreePlayerRedEast" : "FourPlayerRedEast", final_scores: scores,
    players: standings.players.slice(0, count).map((player, seat) => ({ ...player, final_score: scores[seat] })) };
}

// Synthetic layout fixtures, not saved EngineAdapter or full-Match evidence.
function savedReplay(mode: string) {
  const projection = { ...fixtures.projections[mode], audience: "replay_admin", decision: null };
  const result = standingsFor(mode);
  const players: Array<Record<string, unknown>> = (fixtures.room_envelopes[mode].roster as Array<Record<string, unknown>>)
    .map((player, seat) => ({ ...player, participant_kind: player.kind, final_points: result.final_scores[seat] }));
  const score = mode.startsWith("3p") ? 35000 : 25000;
  return { match_id: "MATCH", source: "room", room_name: "Night Market", game_mode: mode,
    started_at: "2026-09-16T00:00:00Z", completed_at: "2026-09-16T00:10:00Z", file_size: 512,
    availability: "available", replay_available: true, players, result,
    frames: [0, 1, 2, 3].map((index) => ({ event_index: index,
      visible_event: { type: ["start_kyoku", "hora", "end_kyoku", "end_game"][index] }, visible_state: projection,
      auxiliary_events: [], result: index === 1 ? win : null,
      settlement: index === 2 ? { changes: players.map((player, seat) => ({ participant_id: player.participant_id,
        seat, before: score, delta: 0, after: score })), pot_before: 0, pot_after: 0, draw_reason: "suucha_riichi" } : null })) };
}

test.describe("authoritative result surfaces", () => {
  for (const delivery of ["held", "refused"] as const) {
  test(`keeps result confirmation usable while the 3D module is ${delivery}`, async ({ page }) => {
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    let requested = false;
    await page.route("**/src/game/three-table.tsx*", async route => {
      requested = true;
      if (delivery === "refused") { await route.abort("failed"); return; }
      await held;
      await route.continue();
    });
    await installAssets(page, true); await installSocket(page);
    try {
      await page.goto("/room/123456/lobby", { waitUntil: "commit" });
      await expect.poll(() => requested).toBe(true);
      await showRoundWin(page);
      const confirm = page.getByRole("button", { name: "Continue", exact: true });
      await expect(confirm).toBeEnabled();
      await expect(confirm).toBeFocused();
      await page.keyboard.press("Enter");
      await expect(page.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
      expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([{ type: "confirm_result", step_id: step.id }]);
      await expect(page.locator(".table-letterbox canvas")).toHaveCount(0);
      if (delivery === "refused") await expect(page.getByRole("status").filter({ hasText: "3D table unavailable." })).toBeVisible();
    } finally { release(); }
  });
  }
  for (const viewport of [{ width: 1024, height: 600 }, { width: 1280, height: 720 }, { width: 1600, height: 900 }, { width: 1920, height: 1080 }]) {
    for (const mode of ["3p-red-east", "4p-red-east"]) {
      for (const surface of ["Live", "Replay"]) {
        test(`layout ${surface} ${mode} win and standings at ${viewport.width}x${viewport.height}`, async ({ page }) => {
          await page.setViewportSize(viewport);
          await page.emulateMedia({ reducedMotion: "reduce" });
          await installAssets(page);
          const requests: string[] = [];
          page.on("request", request => { if (request.url().includes("/api/")) requests.push(request.method()); });
          if (surface === "Live") {
            await installSocket(page, mode);
            await page.goto("/room/123456/lobby");
            await showRoundWin(page);
            await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeFocused();
            await page.keyboard.press("Enter");
            await expect(page.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
          } else {
            const saved = savedReplay(mode);
            await page.route("**/api/v1/admin/**", route => route.fulfill({ json: route.request().url().endsWith("/replays/MATCH") ? saved : [] }));
            await page.goto("/admin/replays/MATCH");
            await page.getByRole("button", { name: "Next Event" }).click();
            await expect(page.getByRole("button", { name: "Continue", exact: true })).toHaveCount(0);
          }
          await expect(page.getByTestId("winning-tile")).toHaveCount(1);
          await expect(page.getByLabel("Ron win details")).toContainText("1,000");
          await expect(page.getByRole("group", { name: "Closed Ankan" }).locator('img[src$="/Back.svg"]')).toHaveCount(2);
          const winBox = await page.getByTestId("round-win-surface").boundingBox();
          expect(winBox).not.toBeNull();
          expect([winBox!.x, winBox!.y, winBox!.width, winBox!.height].every(Number.isFinite)).toBe(true);
          expect(winBox!.width).toBeGreaterThan(0); expect(winBox!.height).toBeGreaterThan(0);
          expect(winBox!.x).toBeGreaterThanOrEqual(0); expect(winBox!.y).toBeGreaterThanOrEqual(0);
          expect(winBox!.x + winBox!.width).toBeLessThanOrEqual(viewport.width);
          expect(winBox!.y + winBox!.height).toBeLessThanOrEqual(viewport.height);
          await page.screenshot({ path: test.info().outputPath("round-win.png"), fullPage: false });
          if (surface === "Live") await showStandings(page, standingsFor(mode));
          else { await page.getByRole("button", { name: "Next Event" }).click(); await page.getByRole("button", { name: "Next Event" }).click(); }
          const panel = page.getByTestId("results-panel");
          await expect(panel).toBeVisible();
          await expect(page.getByRole("list", { name: "Final standings" }).getByRole("listitem")).toHaveCount(mode.startsWith("3p") ? 3 : 4);
          const box = await panel.boundingBox();
          expect(box).not.toBeNull();
          expect([box!.x, box!.y, box!.width, box!.height].every(Number.isFinite)).toBe(true);
          expect(box!.width).toBeGreaterThan(0); expect(box!.height).toBeGreaterThan(0);
          expect(box!.x).toBeGreaterThanOrEqual(0); expect(box!.y).toBeGreaterThanOrEqual(0);
          expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width);
          expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height);
          expect(requests.every(method => method === "GET")).toBe(true);
          await page.screenshot({ path: test.info().outputPath("final-standings.png"), fullPage: false });
        });
      }
    }
  }

  test("keyboard confirmation survives reduced motion, refused assets, audio and WebGL", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await installAssets(page, true); await installSocket(page);
    await page.addInitScript(() => {
      HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;
      HTMLMediaElement.prototype.play = () => Promise.reject(new Error("playback refused"));
    });
    await page.goto("/room/123456/lobby");
    await expect(page.getByTestId("gameplay-shell")).toBeVisible();
    await showRoundWin(page);
    await expect(page.getByTestId("round-win-surface")).toHaveAttribute("data-motion", "static");
    await expect(page.getByRole("img", { name: "Mika portrait unavailable" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
    expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([{ type: "confirm_result", step_id: step.id }]);
    await expect(page.getByTestId("round-win-surface")).toBeVisible();
    await showStandings(page);
    await expect(page.getByTestId("dismiss-results")).toBeFocused();
  });

  test("keeps a shrinking cohort visible and distinguishes open kan and draw steps", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await installAssets(page, true); await installSocket(page);
    await page.goto("/room/123456/lobby");
    await showRoundWin(page, { ...step, required: ["P1", "P2", "P3"], confirmed: ["P2"] });
    const before = Number(await page.getByTestId("result-timer").getAttribute("data-remaining-ms"));
    await page.evaluate((step) => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      browser.__room = { ...browser.__room, kyoku_end: { ...step, required: ["P1", "P3"], confirmed: ["P2"] } };
      browser.__socket.emit({ type: "room_update", room: browser.__room, state: browser.__state });
    }, step);
    await expect(page.getByTestId("round-win-surface")).toBeVisible();
    await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeEnabled();
    expect(Number(await page.getByTestId("result-timer").getAttribute("data-remaining-ms"))).toBeLessThanOrEqual(before);
    await page.evaluate((step) => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      browser.__room = { ...browser.__room, kyoku_end: { ...step, id: { ...step.id, step_index: 1 }, content: { Win: { ...step.content.Win, melds: [{ kind: "Daiminkan", tiles: [52, 53, 54, 55], opened: true, from_who: 1, called_tile: 52 }] } } } };
      browser.__socket.emit({ type: "room_update", room: browser.__room, state: browser.__state });
    }, step);
    await expect(page.getByRole("group", { name: "Open Daiminkan" })).toBeVisible();
    await expect(page.getByRole("group", { name: "Open Daiminkan" }).locator('img[src$="/Back.svg"]')).toHaveCount(0);
    await page.evaluate((step) => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      const settlement = { changes: [0, 1, 2, 3].map((seat) => ({ participant_id: `P${seat + 1}`, seat, before: 25000, delta: 0, after: 25000 })), pot_before: 0, pot_after: 0, draw_reason: "suucha_riichi" };
      browser.__room = { ...browser.__room, kyoku_end: { ...step, id: { ...step.id, kyoku_sequence: 2, step_index: 0 }, content: { Draw: settlement } } };
      browser.__socket.emit({ type: "room_update", room: browser.__room, state: browser.__state });
    }, step);
    await expect(page.getByLabel("Score transfer").getByRole("heading", { name: "suucha_riichi" })).toBeVisible();
    await expect(page.getByLabel("Score transfer")).toContainText("+0");
    await expect(page.getByTestId("round-win-surface")).toHaveCount(0);
    await page.screenshot({ path: test.info().outputPath("draw-1280.png") });
  });

  for (const mode of ["3p-red-east", "4p-red-east"]) {
    test(`${mode} Replay seeks and stops at 64x without actions or confirmations`, async ({ page }) => {
      await page.setViewportSize({ width: 1280, height: 720 });
      await installAssets(page, true);
      const count = mode.startsWith("3p") ? 3 : 4;
      const saved = savedReplay(mode);
      const requests: string[] = [];
      page.on("request", (request) => { if (request.url().includes("/api/")) requests.push(request.method()); });
      await page.route("**/api/v1/admin/**", (route) => route.fulfill({ json: route.request().url().endsWith("/replays/MATCH") ? saved : [] }));
      await page.addInitScript(() => {
        const NativeSocket = window.WebSocket;
        (window as unknown as { __socketCount: number; __socketUrls: string[] }).__socketCount = 0;
        (window as unknown as { __socketUrls: string[] }).__socketUrls = [];
        Object.defineProperty(window, "WebSocket", { configurable: true, value: class extends NativeSocket {
          constructor(url: string, protocols?: string | string[]) {
            const browser = window as unknown as { __socketCount: number; __socketUrls: string[] };
            browser.__socketUrls.push(url);
            if (url.includes("/ws/v1/rooms/")) browser.__socketCount += 1;
            super(url, protocols);
          }
        } });
      });
      await page.goto("/admin/replays/MATCH");
      await expect(page.getByTestId("replay-viewer")).toBeVisible();
      await page.getByRole("button", { name: "Next Event" }).click();
      await expect(page.getByTestId("winning-tile")).toHaveCount(1);
      await expect(page.getByRole("group", { name: "Closed Ankan" }).locator('img[src$="/Back.svg"]')).toHaveCount(2);
      await expect(page.getByRole("button", { name: "Continue", exact: true })).toHaveCount(0);
      await page.getByLabel("Playback speed").selectOption("64");
      await page.getByRole("button", { name: "Play", exact: true }).click();
      await expect(page.getByRole("list", { name: "Final standings" }).getByRole("listitem")).toHaveCount(count);
      await expect(page.getByRole("button", { name: "Play", exact: true })).toBeDisabled();
      await page.getByRole("button", { name: "Previous Event" }).click();
      await expect(page.getByLabel("Score transfer").getByRole("heading", { name: "suucha_riichi" })).toBeVisible();
      await page.getByRole("button", { name: "Previous Event" }).click();
      await expect(page.getByTestId("round-win-surface")).toBeVisible();
      await expect(page.getByTestId("results-panel")).toHaveCount(0);
      expect(requests.length).toBeGreaterThan(0); expect(requests.every((method) => method === "GET")).toBe(true);
      const sockets = await page.evaluate(() => {
        const browser = window as unknown as { __socketCount: number; __socketUrls: string[] };
        return { gameConnections: browser.__socketCount, urls: browser.__socketUrls };
      });
      expect(sockets.gameConnections).toBe(0);
      expect(sockets.urls.every((url) => new URL(url).pathname === "/")).toBe(true); // Vite HMR, not Game Actions.
    });
  }
});
