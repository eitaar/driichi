import { expect, test } from "./support/task7b-evidence";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const summary = { match_id: "saved-match", source: "room", room_name: "Saved room", game_mode: "4p-red-east", started_at: "2026-09-16T00:00:00Z", completed_at: "2026-09-16T00:10:00Z", file_size: 512, availability: "available", replay_available: true };

test("phone Replay deep link retains metadata and a safe Back without renderer resources", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    Object.assign(window, { tableWorkerStarts: 0 });
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        Object.assign(window, { tableWorkerStarts: (window as unknown as { tableWorkerStarts: number }).tableWorkerStarts + 1 });
        super(url, options);
      }
    };
  });
  await page.route("**/api/v1/admin/replays**", route => route.fulfill({ json: route.request().url().endsWith("/saved-match") ? { ...summary, players: [], frames: [] } : { replays: [summary], total: 1, offset: 0, limit: 50, has_more: false } }));
  await page.goto("/admin/replays/saved-match");
  await expect(page.getByRole("heading", { name: "Saved room Replay" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Desktop required for the 3D table" })).toBeVisible();
  await expect(page.locator("canvas")).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { tableWorkerStarts: number }).tableWorkerStarts)).toBe(0);
  await page.getByRole("link", { name: "Back", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/replays$/);
  await expect(page.getByRole("heading", { name: "Replay library" })).toBeVisible();
  await page.getByRole("button", { name: "Toggle navigation" }).click();
  const menu = page.getByRole("dialog");
  await expect(menu).toBeVisible();
  await menu.getByRole("link", { name: "Benchmarks", exact: true }).focus();
  await page.keyboard.press("Escape");
  await expect(menu).not.toBeVisible();
  await expect(page.getByRole("button", { name: "Toggle navigation" })).toBeFocused();
});

test("phone Benchmark deep link omits private observer but preserves status and Room navigation", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    Object.assign(window, { tableWorkerStarts: 0 });
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        (window as unknown as { tableWorkerStarts: number }).tableWorkerStarts++;
      }
    };
  });
  const privateReads: string[] = [], rendererReads: string[] = [];
  page.on("request", request => {
    if (request.url().endsWith("/live")) privateReads.push(request.url());
    if (/benchmark-observer|table-scene|three-renderer|renderer\.worker/.test(request.url())) rendererReads.push(request.url());
  });
  await page.route("**/api/v1/admin/benchmark/runs/run-1", route => route.fulfill({ json: { run_id: "run-1", room_code: "123456", game_mode: "4p-red-east", target: 3, completed: 0, status: "running", reason: null, failed_match_id: null, roster: Array.from({ length: 4 }, (_, initial_seat) => ({ participant_id: `p${initial_seat}`, display_name: `Bot ${initial_seat}`, initial_seat, participant_kind: "built_in_bot", character_id: null })), matches: [], statistics: [] } }));
  await page.goto("/admin/benchmark/runs/run-1");
  await expect(page.getByRole("heading", { name: "Benchmark Run", exact: true })).toBeVisible();
  const runSummary = page.getByRole("region", { name: "Run summary", exact: true });
  await expect(runSummary).toHaveAttribute("data-slot", "card");
  await expect(runSummary.getByRole("link", { name: "Room controls" })).toBeVisible();
  await expect(runSummary.getByRole("progressbar", { name: "Completed Matches" })).toHaveAttribute("value", "0");
  await expect(runSummary.getByRole("progressbar")).toHaveAttribute("max", "3");
  await expect(runSummary.getByRole("status").filter({ hasText: /^running$/ })).toHaveText("running");
  await expect(page.getByText("No completed Matches yet.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Observation", exact: true })).toHaveCount(0);
  await expect(page.locator("canvas")).toHaveCount(0);
  expect(privateReads).toEqual([]);
  expect(rendererReads).toEqual([]);
  expect(await page.evaluate(() => (window as unknown as { tableWorkerStarts: number }).tableWorkerStarts)).toBe(0);
});

for (const fromDesktop of [false, true]) {
  test(`HumanLobby Post-Match standings survive ${fromDesktop ? "desktop-to-phone resize" : "initial phone load"}`, async ({ page }) => {
    await page.setViewportSize({ width: fromDesktop ? 1280 : 390, height: 844 });
    await page.addInitScript(() => {
      sessionStorage.setItem("driichi:participant:123456", "P1");
      const NativeWorker = window.Worker;
      const resources = { starts: 0, active: 0 };
      Object.assign(window, { tableWorkers: resources });
      window.Worker = class extends NativeWorker {
        constructor(url: string | URL, options?: WorkerOptions) {
          super(url, options);
          resources.starts += 1;
          resources.active += 1;
        }
        terminate() { resources.active -= 1; super.terminate(); }
      };
    });
    const players = [
      { participant_id: "P1", display_name: "Mika", seat: 0, final_score: 30000, rank: 2 },
      { participant_id: "B1", display_name: "Bot One", seat: 1, final_score: 45000, rank: 1 },
      { participant_id: "B2", display_name: "Bot Two", seat: 2, final_score: 0, rank: 3 },
    ];
    const sent: string[] = [];
    await page.route("**/api/v1/characters/human", route => route.fulfill({ json: [] }));
    await page.route("**/assets/characters/player-red/*.webp", route => route.fulfill({ path: resolve("tests/fixtures/task12-characters/player-red", new URL(route.request().url()).pathname.split("/").at(-1)!) }));
    const projection = JSON.parse(readFileSync(resolve("tests/fixtures/task12-projection.json"), "utf8")).projections["3p-red-east"];
    projection.players = projection.players.map((player: object, seat: number) => ({ ...player, participant_id: players[seat].participant_id, display_name: players[seat].display_name }));
    await page.routeWebSocket("**/ws/v1/rooms/123456/human", socket => {
      socket.onMessage(message => sent.push(String(message)));
      socket.send(JSON.stringify({ type: "snapshot", room: {
        join_code: "123456", room_name: "Post-Match Room", game_mode: "3p-red-east", phase: "post_match", revision: 1,
        participants: players.map(player => ({ ...player, kind: player.seat === 0 ? "human" : "built_in_bot", presence: "connected", selected: true, ready: false, character_id: "player-red", role: "none", controller: "interactive" })),
        roster: players.map(player => ({ ...player, kind: "human", character_id: "player-red", controller: "interactive" })), match_players: [],
        result: { mode: "ThreePlayerRedEast", players: players.map(player => ({ ...player, kind: player.seat === 0 ? "Human" : "BuiltInBot" })), final_scores: [30000, 45000, 0] },
      }, state: projection }));
    });
    await page.goto("/room/123456/lobby");
    await expect(page.getByRole("heading", { name: "Final standings" })).toBeVisible();
    await expect(page.getByTestId("match-victory-page")).toBeVisible();
    if (fromDesktop) await page.setViewportSize({ width: 390, height: 844 });
    const standings = page.getByRole("list", { name: "Final standings" });
    await expect(standings).toBeVisible();
    await expect(standings.getByRole("listitem")).toHaveText(["1Bot One45,000", "2Mika30,000", "3Bot Two0"]);
    await expect(standings.getByLabel("Rank 1")).toBeVisible();
    await expect(page.locator("canvas")).toHaveCount(0);
    await expect.poll(() => page.evaluate(() => (window as unknown as { tableWorkers: { starts: number; active: number } }).tableWorkers)).toEqual({ starts: 0, active: 0 });
    await expect(page.getByRole("button", { name: "Set Ready" })).toHaveCount(0);
    await page.getByRole("button", { name: "Return to room", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Lobby", exact: true })).toBeVisible();
    expect(sent).toEqual([]);
    await page.getByRole("button", { name: "Review final standings" }).click();
    await expect(standings).toBeVisible();
    await page.getByRole("button", { name: "Return to room", exact: true }).click();
    await expect(page.getByRole('button', { name: 'Mika', exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Set Ready" }).click();
    await expect.poll(() => sent.map(message => JSON.parse(message))).toEqual([{ type: "set_ready", preloaded_characters: ["player-red", "player-red", "player-red"] }]);
  });
}
