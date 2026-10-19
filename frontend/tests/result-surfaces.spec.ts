/// <reference types="../node_modules/@types/node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Page } from "./support/task7b-evidence";

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
  const projection = { ...fixtures.projections[mode], audience: "replay_admin", decision: null, round: "East", kyoku: 1, honba: 0 };
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

for (const width of [390, 1280]) test(`Replay theme focus uses semantic ring on ${width === 390 ? "phone" : "desktop"}`, async ({ page }) => {
  test.setTimeout(60000);
  await page.setViewportSize({ width, height: 900 });
  await page.addInitScript(() => {
    localStorage.setItem("driichi-theme", "light");
    const NativeWorker = window.Worker;
    const counts = { starts: 0, active: 0 };
    Object.assign(window, { focusWorkers: counts });
    window.Worker = class extends NativeWorker {
      private live = true;
      constructor(url: string | URL, options?: WorkerOptions) { super(url, options); counts.starts++; counts.active++; }
      terminate() { if (this.live) { counts.active--; this.live = false; } super.terminate(); }
    };
  });
  const resources: string[] = [];
  page.on("request", request => { if (/three-table|table-scene|three-renderer|renderer[.-]worker|\/assets\/characters\/|\/live$/.test(request.url())) resources.push(request.url()); });
  await installAssets(page);
  const saved = savedReplay("4p-red-east");
  await page.route("**/api/v1/admin/**", route => route.fulfill({ json: route.request().url().endsWith("/replays/MATCH") ? saved : [] }));
  await page.goto("/admin/replays/MATCH");
  const theme = page.locator(width === 390 ? ".replay-metadata-actions .theme-toggle" : ".gameplay-controls .theme-toggle");
  await expect(theme).toBeVisible();
  const workers = () => page.evaluate(() => (window as unknown as { focusWorkers: { starts: number; active: number } }).focusWorkers);
  if (width >= 1024) {
    await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 20000 });
    await expect.poll(async () => (await workers()).active).toBe(1);
  }
  const canvas = width >= 1024 ? await page.locator("canvas").elementHandle() : null;
  const initialWorkers = await workers();
  for (const mode of ["Light", "Dark"]) {
    if (mode === "Dark") await theme.click();
    await expect(theme).toHaveAccessibleName(`Theme: ${mode}. Switch to ${mode === "Light" ? "Dark" : "System"}`);
    await theme.focus(); await page.keyboard.press("Tab"); await page.keyboard.press("Shift+Tab");
    await expect(theme).toBeFocused();
    expect(await theme.evaluate(element => element.matches(":focus-visible"))).toBe(true);
    const ring = await theme.evaluate(element => {
      const sample = document.createElement("span"); sample.style.color = "var(--ring)"; element.append(sample);
      const color = getComputedStyle(sample).color; sample.remove(); return color;
    });
    await expect(theme).toHaveCSS("outline-style", "solid");
    await expect(theme).toHaveCSS("outline-color", ring);
    await expect(theme).toHaveCSS("width", "44px"); await expect(theme).toHaveCSS("height", "44px");
    await page.screenshot({ path: test.info().outputPath(`replay-focus-${width}-${mode}.png`), fullPage: false });
  }
  expect(await workers()).toEqual(initialWorkers);
  if (canvas) expect(await canvas.evaluate(node => node.isConnected && node === document.querySelector("canvas"))).toBe(true);
  else { await expect(page.locator("canvas")).toHaveCount(0); expect(initialWorkers).toEqual({ starts: 0, active: 0 }); expect(resources).toEqual([]); }
});

// Task7B predeclared supplementary native captures; existing assertions below stay frozen.
for (const width of [1024, 1440]) for (const theme of ['light', 'dark']) {
  test(`Task7B Ron Tsumo native capture ${width} ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 1024 ? 600 : 900 });
    await page.addInitScript(theme => localStorage.setItem('driichi-theme', theme), theme);
    await installAssets(page); await installSocket(page);
    await page.goto('/room/123456/lobby');
    await expect(page.getByTestId('three-table')).toHaveAttribute('data-render-ready', 'true');
    await showRoundWin(page);
    await page.screenshot({ path: test.info().outputPath('ron-native.png') });
    await page.evaluate(step => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit(value: unknown): void } };
      browser.__room = { ...browser.__room, revision: 14, kyoku_end: { ...step, id: { ...step.id, step_index: 1 }, content: { Win: { ...step.content.Win, tsumo: true, target: step.content.Win.seat } } } };
      browser.__socket.emit({ type: 'room_update', room: browser.__room, state: browser.__state });
    }, step);
    await expect(page.getByLabel('Tsumo win details')).toBeVisible();
    await page.screenshot({ path: test.info().outputPath('tsumo-native.png') });
  });
}

test.describe("authoritative result surfaces", () => {
  test("compact phone Win shows the validated Character and awarded points before Expand without renderer resources", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.addInitScript(() => {
      const Native = window.Worker;
      Object.assign(window, { resultWorkers: 0 });
      window.Worker = class extends Native {
        constructor(url: string | URL, options?: WorkerOptions) {
          super(url, options);
          (window as unknown as { resultWorkers: number }).resultWorkers++;
        }
      };
    });
    const reads: string[] = [];
    page.on("request", request => { if (/\/live$|table-scene|three-renderer|renderer\.worker|three-table-worker|\.wasm/.test(request.url())) reads.push(request.url()); });
    await installAssets(page);
    await installSocket(page);
    await page.goto("/room/123456/lobby");
    await expect(page.getByRole("heading", { name: "Desktop required for the 3D table" })).toBeVisible();
    await page.evaluate(step => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit(value: unknown): void } };
      browser.__socket.emit({ type: "game_update", room: { ...browser.__room, revision: 13, kyoku_end: step }, state: browser.__state });
    }, step);
    const summary = page.getByRole("region", { name: "Match result", exact: true });
    await expect(summary.getByRole("heading", { name: "Mika · Ron" })).toBeVisible();
    await expect(summary.getByText("1,000 points", { exact: true })).toBeVisible();
    const portrait = summary.getByRole("img", { name: "Mika portrait", exact: true });
    await expect(portrait).toBeVisible();
    expect(await portrait.evaluate(image => (image as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByRole("button", { name: /^Continue/ })).toHaveCount(0);
    await summary.getByRole("button", { name: "Expand" }).click();
    await expect(page.getByRole("dialog").getByRole("group", { name: "Winning hand", exact: true })).toBeVisible();
    await expect(page.locator("canvas")).toHaveCount(0);
    expect(reads).toEqual([]);
    expect(await page.evaluate(() => (window as unknown as { resultWorkers: number }).resultWorkers)).toBe(0);
  });
  test("phone Replay exposes chapter metadata and saved standings without playback resources", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const resources: string[] = [];
    const methods: string[] = [];
    page.on("request", request => {
      if (/three-table|table-scene|three-renderer|renderer[.-]worker|\.wasm|\/assets\/characters\/|\/live$/.test(request.url())) resources.push(request.url());
      if (request.url().includes("/api/")) methods.push(request.method());
    });
    await page.addInitScript(() => {
      const browser = window as unknown as { __audioContexts: number; __workerStarts: number };
      browser.__audioContexts = 0;
      browser.__workerStarts = 0;
      const NativeWorker = window.Worker;
      window.Worker = class extends NativeWorker {
        constructor(url: string | URL, options?: WorkerOptions) { super(url, options); browser.__workerStarts++; }
      };
      const NativeAudioContext = window.AudioContext;
      window.AudioContext = class extends NativeAudioContext { constructor() { super(); browser.__audioContexts++; } };
    });
    const saved = savedReplay("4p-red-east");
    saved.frames = saved.frames.map((frame, index) => ({ ...frame, visible_state: { ...frame.visible_state, round: "East", kyoku: 1, honba: index < 2 ? 0 : 1 } }));
    await page.route("**/api/v1/admin/**", route => route.fulfill({ json: route.request().url().endsWith("/replays/MATCH") ? saved : [] }));
    await page.goto("/admin/replays/MATCH");
    await expect(page.getByRole("heading", { name: "Night Market Replay" })).toBeVisible();
    const chapters = page.getByRole("combobox", { name: "Jump to Kyoku" });
    await expect(chapters).toHaveValue("0");
    await chapters.selectOption("2");
    await expect(page.getByText("Chapter start: Event 3 · End Kyoku")).toBeVisible();
    await expect(page.getByLabel("Score transfer")).toContainText("suucha_riichi");
    await expect(page.getByRole("list", { name: "Final standings" }).getByRole("listitem")).toHaveCount(4);
    await expect(page.locator("canvas")).toHaveCount(0);
    await expect(page.getByRole("slider")).toHaveCount(0);
    await expect(page.getByRole("button", { name: /^(Play|Pause|Continue|Next Event|Previous Event)$/ })).toHaveCount(0);
    const theme = page.locator(".replay-metadata-actions .theme-toggle");
    await theme.focus();
    await expect(theme).toBeFocused();
    await theme.click();
    expect(await page.evaluate(() => (window as unknown as { __workerStarts: number }).__workerStarts)).toBe(0);
    expect(resources).toEqual([]);
    expect(methods.every(method => method === "GET")).toBe(true);
    expect(await page.evaluate(() => (window as unknown as { __audioContexts: number }).__audioContexts)).toBe(0);
  });
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
      const confirm = page.getByRole("button", { name: /^Continue \(\d+s\)$/ });
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
            await expect(page.getByRole("button", { name: /^Continue \(\d+s\)$/ })).toBeFocused();
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
          if (surface === "Replay") {
            await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
            const canvas = await page.locator("canvas").elementHandle();
            const theme = page.locator(".gameplay-controls .theme-toggle");
            await expect(page.locator(".gameplay-controls")).toHaveCSS("z-index", "7");
            await theme.focus(); await expect(theme).toBeFocused();
            await theme.click(); // Includes the 1024×600 overlap boundary; real hit testing.
            await page.getByRole("button", { name: "Settings", exact: true }).click();
            await expect(page.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
            await page.keyboard.press("Escape");
            await expect(page.getByRole("button", { name: "Settings", exact: true })).toBeFocused();
            expect(await canvas!.evaluate(node => node.isConnected && node === document.querySelector("canvas"))).toBe(true);
            const stage = (await page.locator(".replay-table-stage").boundingBox())!;
            expect(winBox!.x).toBeGreaterThanOrEqual(stage.x);
            expect(winBox!.x + winBox!.width).toBeLessThanOrEqual(stage.x + stage.width);
          }
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
          if (surface === "Replay") {
            const stage = (await page.locator(".replay-table-stage").boundingBox())!;
            expect(box!.x).toBeGreaterThanOrEqual(stage.x);
            expect(box!.x + box!.width).toBeLessThanOrEqual(stage.x + stage.width);
          }
          expect(requests.every(method => method === "GET")).toBe(true);
          await page.screenshot({ path: test.info().outputPath("final-standings.png"), fullPage: false });
        });
      }
    }
  }

  test("local Settings and Theme remain reachable above live Win without table remount", async ({ page }) => {
    await page.setViewportSize({ width: 1024, height: 600 });
    await installAssets(page, true); await installSocket(page);
    await page.goto("/room/123456/lobby");
    await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
    const canvas = await page.locator("canvas").elementHandle();
    await showRoundWin(page);
    const theme = page.locator(".gameplay-controls .theme-toggle");
    await expect(page.locator(".gameplay-controls")).toHaveCSS("z-index", "7");
    await expect(page.locator(".result-win-modal")).toHaveCSS("z-index", "6");
    const before = Number(await page.locator(".round-win-continue").getAttribute("data-remaining-ms"));
    await theme.click(); // Playwright hit testing rejects an occluded control.
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Settings", exact: true })).toBeFocused();
    expect(Number(await page.locator(".round-win-continue").getAttribute("data-remaining-ms"))).toBeLessThanOrEqual(before);
    expect(await canvas!.evaluate(node => node.isConnected && node === document.querySelector("canvas"))).toBe(true);
    expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([]);
  });

  test("no_review_and_one_legal_confirmation survives expiry and revoked access", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await installAssets(page, true); await installSocket(page);
    await page.goto("/room/123456/lobby");
    await showRoundWin(page, { ...step, remaining_ms: 4500 });
    await expect(page.getByRole("button", { name: "Review", exact: true })).toHaveCount(0);
    const confirm = page.getByRole("button", { name: /^Continue \(\d+s\)$/ });
    await expect(confirm).toHaveCount(1);
    await expect(confirm).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
    expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([{ type: "confirm_result", step_id: step.id }]);
    await page.evaluate(() => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      browser.__room = { ...browser.__room, kyoku_end: { ...(browser.__room.kyoku_end as object), required: [] } };
      browser.__socket.emit({ type: "room_update", room: browser.__room, state: browser.__state });
    });
    await expect(confirm).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Dismiss", exact: true })).toBeFocused();
    await page.getByRole("button", { name: "Dismiss", exact: true }).click();
    await showRoundWin(page, { ...step, id: { ...step.id, step_index: 1 }, remaining_ms: 700 });
    await expect(confirm).toBeDisabled({ timeout: 3000 });
    await expect(confirm).toHaveText("Continue (0s)");
    const settings = page.getByRole("button", { name: "Settings", exact: true });
    await expect(settings).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
    expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([{ type: "confirm_result", step_id: step.id }]);
    await page.keyboard.press("Escape");
    await expect(settings).toBeFocused();
  });

  test("phone results expand read-only metadata without a renderer or confirmation", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const resources: string[] = [];
    page.on("request", request => { if (/three-table|renderer-worker/.test(request.url())) resources.push(request.url()); });
    await installAssets(page, true); await installSocket(page);
    await page.goto("/room/123456/lobby");
    await page.evaluate(step => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      browser.__room = { ...browser.__room, kyoku_end: step };
      browser.__socket.emit({ type: "room_update", room: browser.__room, state: browser.__state });
    }, step);
    await expect(page.getByRole("heading", { name: "Mika · Ron" })).toBeVisible();
    await page.getByRole("button", { name: "Expand", exact: true }).click();
    const drawer = page.getByRole("dialog");
    await expect(drawer.getByLabel("Ron win details")).toContainText("1,000");
    await expect(page.getByRole("button", { name: /^Continue/ })).toHaveCount(0);
    await drawer.getByRole("button", { name: "Close", exact: true }).click();
    await expect(page.getByRole("button", { name: "Expand", exact: true })).toBeFocused();
    await page.evaluate(result => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      browser.__socket.emit({ type: "room_update", room: { ...browser.__room, phase: "post_match", kyoku_end: null, result }, state: browser.__state });
    }, standings);
    await expect(page.getByRole("list", { name: "Final standings", exact: true }).getByRole("listitem")).toHaveCount(4);
    await expect(page.getByTestId("match-victory-page")).toBeVisible();
    await expect(page.getByRole("button", { name: "Return to room", exact: true })).toBeVisible();
    await expect(page.locator("canvas")).toHaveCount(0);
    expect(resources).toEqual([]);
    expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([]);
  });

  for (const content of ["win", "scores", "final"] as const) {
    test(`short phone ${content} details scroll to the end with reachable Close`, async ({ page }) => {
      await page.setViewportSize({ width: 390, height: 320 });
      await page.emulateMedia({ reducedMotion: "reduce" });
      await installAssets(page, true); await installSocket(page);
      await page.goto("/room/123456/lobby");
      await page.evaluate(({ step, standings, content }) => {
        const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
        const settlement = { changes: [0, 1, 2, 3].map(seat => ({ participant_id: `P${seat + 1}`, seat, before: 25000, delta: 0, after: 25000 })), pot_before: 0, pot_after: 0, draw_reason: null };
        browser.__socket.emit({ type: "room_update", state: browser.__state, room: { ...browser.__room,
          phase: content === "final" ? "post_match" : "playing", result: content === "final" ? standings : null,
          kyoku_end: content === "final" ? null : { ...step, content: content === "win" ? { Win: { ...step.content.Win,
            yaku: Array.from({ length: 16 }, (_, id) => ({ id, name: `detail_${id}`, han: 1, yakuman_multiplier: null })),
            evaluation: { Ordinary: { han: 16, fu: 30, limit: null } } } } : { Settlement: settlement } },
        } });
      }, { step, standings, content });
      if (content === "final") {
        await expect(page.getByTestId("match-victory-page")).toBeVisible();
        const back = page.getByRole("button", { name: "Return to room", exact: true });
        await expect(back).toBeFocused();
        expect(await page.evaluate(() => document.documentElement.scrollHeight > innerHeight)).toBe(true);
        await page.keyboard.press("Control+End");
        await expect(page.getByRole("list", { name: "Final standings" }).getByRole("listitem").last()).toBeInViewport();
        await expect(back).toBeInViewport();
        await expect(page.locator("canvas")).toHaveCount(0);
        expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([]);
        await page.keyboard.press("Enter");
        await expect(page.getByRole("heading", { name: "Lobby", exact: true })).toBeVisible();
        return;
      }
      await page.getByRole("button", { name: "Expand", exact: true }).click();
      const dialog = page.getByRole("dialog");
      const scroll = dialog.getByRole("region", { name: "Result details", exact: true });
      await expect(scroll).toBeVisible();
      expect(await scroll.evaluate(element => element.scrollHeight > element.clientHeight)).toBe(true);
      const close = dialog.getByRole("button", { name: "Close", exact: true });
      await expect(close).toBeInViewport();
      await scroll.focus();
      await page.keyboard.press("Control+End");
      await expect.poll(() => scroll.evaluate(element => Math.abs(element.scrollHeight - element.clientHeight - element.scrollTop))).toBeLessThan(2);
      const last = content === "win" ? dialog.locator(".round-win-total") : dialog.getByLabel("After 25,000").last();
      await expect(last).toBeInViewport();
      await page.keyboard.press("Tab");
      await expect(close).toBeFocused();
      await page.keyboard.press("Enter");
      await expect(dialog).toHaveCount(0);
      await expect(page.getByRole("button", { name: "Expand", exact: true })).toBeFocused();
      await expect(page.locator("canvas")).toHaveCount(0);
      expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([]);
    });
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
    await expect(page.getByRole("button", { name: /^Continue \(\d+s\)$/ })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Waiting for players" })).toBeDisabled();
    expect(await page.evaluate(() => (window as unknown as { __sent: unknown[] }).__sent)).toEqual([{ type: "confirm_result", step_id: step.id }]);
    await expect(page.getByTestId("round-win-surface")).toBeVisible();
    await showStandings(page);
    await expect(page.getByRole("button", { name: "Return to room", exact: true })).toBeFocused();
  });

  test("keeps a shrinking cohort visible and distinguishes open kan and draw steps", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await installAssets(page, true); await installSocket(page);
    await page.goto("/room/123456/lobby");
    await showRoundWin(page, { ...step, required: ["P1", "P2", "P3"], confirmed: ["P2"] });
    const confirm = page.getByRole("button", { name: /^Continue \(\d+s\)$/ });
    const before = Number(await confirm.getAttribute("data-remaining-ms"));
    await page.evaluate((step) => {
      const browser = window as unknown as { __room: Record<string, unknown>; __state: unknown; __socket: { emit: (value: unknown) => void } };
      browser.__room = { ...browser.__room, kyoku_end: { ...step, required: ["P1", "P3"], confirmed: ["P2"] } };
      browser.__socket.emit({ type: "room_update", room: browser.__room, state: browser.__state });
    }, step);
    await expect(page.getByTestId("round-win-surface")).toBeVisible();
    await expect(confirm).toBeEnabled();
    expect(Number(await confirm.getAttribute("data-remaining-ms"))).toBeLessThanOrEqual(before);
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
    await expect(page.getByRole("heading", { name: "suucha_riichi" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Result", exact: true })).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("button", { name: "Scores", exact: true }).click();
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
        const NativeWorker = window.Worker;
        const workers = { starts: 0, active: 0 };
        Object.assign(window, { replayWorkers: workers });
        window.Worker = class extends NativeWorker {
          private stopped = false;
          constructor(url: string | URL, options?: WorkerOptions) { super(url, options); workers.starts++; workers.active++; }
          terminate() { if (!this.stopped) { workers.active--; this.stopped = true; } super.terminate(); }
        };
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
      await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
      const canvas = await page.locator("canvas").elementHandle();
      const workers = () => page.evaluate(() => (window as unknown as { replayWorkers: { starts: number; active: number } }).replayWorkers);
      await expect.poll(async () => (await workers()).active).toBe(1);
      await page.getByRole("button", { name: "Next Event" }).click();
      const theme = page.locator(".gameplay-controls .theme-toggle");
      await expect(page.locator(".gameplay-controls")).toHaveCSS("z-index", "7");
      await theme.focus(); await expect(theme).toBeFocused(); await theme.click();
      await page.getByRole("button", { name: "Settings", exact: true }).click();
      await expect(page.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(page.getByRole("button", { name: "Settings", exact: true })).toBeFocused();
      expect(await canvas!.evaluate(node => node.isConnected && node === document.querySelector("canvas"))).toBe(true);
      const starts = (await workers()).starts;
      await expect(page.getByTestId("winning-tile")).toHaveCount(1);
      await expect(page.getByRole("group", { name: "Closed Ankan" }).locator('img[src$="/Back.svg"]')).toHaveCount(2);
      await expect(page.getByRole("button", { name: "Continue", exact: true })).toHaveCount(0);
      await page.getByLabel("Playback speed").selectOption("64");
      await page.getByRole("button", { name: "Play", exact: true }).click();
      await expect(page.getByRole("list", { name: "Final standings" }).getByRole("listitem")).toHaveCount(count);
      await expect(page.getByRole("button", { name: "Play", exact: true })).toBeDisabled();
      await expect(page.getByRole("region", { name: "Replay final standings", exact: true })).toBeVisible();
      await expect(page.locator("canvas, .results-overlay, main main")).toHaveCount(0);
      await expect.poll(async () => (await workers()).active).toBe(0);
      await page.getByRole("button", { name: "East 1 · 0 honba", exact: true }).click();
      await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
      await expect.poll(async () => (await workers()).active).toBe(1);
      const remounted = await workers();
      expect(remounted.starts - starts).toBeGreaterThanOrEqual(1);
      expect(remounted.starts - starts).toBeLessThanOrEqual(2);
      // StrictMode can start twice; actual readiness and steady active ownership decide leaks.
      await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
      expect(await workers()).toEqual(remounted);
      await page.screenshot({ path: test.info().outputPath('replay-final-to-chapter-native-ready.png'), fullPage: false });
      await page.getByRole("button", { name: "Next Event" }).click();
      await page.getByRole("button", { name: "Next Event" }).click();
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
