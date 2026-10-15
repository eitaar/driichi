/// <reference types="../node_modules/@types/node" />

import { expect, test, type Page } from "@playwright/test";
import { writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import fixture from "./fixtures/task12-projection.json" with { type: "json" };
import seatOracle from "./fixtures/seat-score.json" with { type: "json" };
import { armRendererStall, captureRenderer, prepareRendererStall } from "./support/renderer-stall";

test.use({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 2 });

// Literal crops recorded at 1280×720/DPR2; no production geometry generates the oracle.
const scoreCrops = {
  bottom: { x: 600, y: 362, width: 80, height: 15 },
  right: { x: 715, y: 305, width: 17, height: 43 },
  top: { x: 600, y: 278, width: 80, height: 15 },
  left: { x: 548, y: 305, width: 17, height: 43 },
};
const rasterCrops = {
  bottom: { x: 140, y: 424, width: 232, height: 50 },
  right: { x: 424, y: 140, width: 50, height: 232 },
  top: { x: 140, y: 38, width: 232, height: 50 },
  left: { x: 38, y: 140, width: 50, height: 232 },
};

async function cropError(page: Page, actual: Buffer, reference: Buffer, crop: { x: number; y: number; width: number; height: number }, scale: number) {
  return page.evaluate(async ({ actual, reference, crop, scale }) => {
    const decode = async (encoded: string) => {
      const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(encoded), c => c.charCodeAt(0))], { type: "image/png" }));
      const canvas = document.createElement("canvas"); canvas.width = bitmap.width; canvas.height = bitmap.height;
      const ctx = canvas.getContext("2d")!; ctx.drawImage(bitmap, 0, 0); bitmap.close();
      return ctx.getImageData(crop.x * scale, crop.y * scale, crop.width * scale, crop.height * scale).data;
    };
    const a = await decode(actual), b = await decode(reference);
    return a.reduce((sum, value, i) => sum + Math.abs(value - b[i]), 0) / a.length;
  }, { actual: actual.toString("base64"), reference: reference.toString("base64"), crop, scale });
}

for (const entry of seatOracle.cases) {
  test(`independent final pixels ${entry.mode} viewer ${entry.viewer}`, async ({ page }) => {
    test.setTimeout(120_000);
    await page.goto("/tests/support/renderer-worker-product.html");
    const table = page.getByTestId("three-table");
    await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
    expect(page.workers()).toHaveLength(1);
    expect(await page.workers()[0].evaluate(() => typeof window)).toBe("undefined");
    const participants = seatOracle.participants.slice(0, entry.mode === "3p-red-east" ? 3 : 4);
    const mode = entry.mode as keyof typeof fixture.projections;
    const metrics: unknown[] = [];
    for (const updated of [false, true]) {
      const projection = { ...fixture.projections[mode], decision: undefined,
        players: participants.map(p => ({ ...fixture.projections[mode].players[p.seat], ...p,
          seat: updated ? (entry.mode === "3p-red-east" ? p.rotatedSeat3! : p.rotatedSeat4) : p.seat,
          score: updated ? p.updatedScore : p.score,
        })),
      };
      const apply = async (score?: number) => {
        await page.evaluate(detail => window.dispatchEvent(new CustomEvent("seat-score-input", { detail })), {
          projection: { ...projection, players: projection.players.map(p => ({ ...p, score: score ?? p.score })) },
          viewSeat: updated ? entry.rotatedViewer : entry.viewer, surface: "replay",
        });
        await expect(table.getByRole("list", { name: "Table scores" })).toContainText(`Amber: ${(score ?? projection.players[0].score).toLocaleString("en-US")}`);
        await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
      };
      // Uniform-score reference frames cannot encode a mistaken Seat→position mapping.
      const references = new Map<number, { screen: Buffer; raster: Buffer }>();
      for (const p of participants) {
        const score = updated ? p.updatedScore : p.score;
        await apply(score);
        references.set(score, { raster: await page.getByTestId("center-raster").locator("canvas").screenshot(), screen: await table.locator("canvas").screenshot() });
      }
      await apply();
      const raster = await page.getByTestId("center-raster").locator("canvas").screenshot();
      const screen = await table.locator("canvas").screenshot();
      await test.info().attach(`final-${updated ? "rotated" : "initial"}`, { body: screen, contentType: "image/png" });
      await test.info().attach(`window-raster-${updated ? "rotated" : "initial"}`, { body: raster, contentType: "image/png" });
      for (const position of ["bottom", "right", "top", "left"] as const) {
        const name = entry[position];
        if (!name) {
          await expect(table.locator(`[data-position="${position}"]`)).toHaveCount(0);
          const whitePixels = await page.evaluate(async ({ image, crop }) => {
            const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(image), c => c.charCodeAt(0))], { type: "image/png" }));
            const canvas = document.createElement("canvas"); canvas.width = bitmap.width; canvas.height = bitmap.height;
            const ctx = canvas.getContext("2d")!; ctx.drawImage(bitmap, 0, 0); bitmap.close();
            const pixels = ctx.getImageData(crop.x * 2, crop.y * 2, crop.width * 2, crop.height * 2).data;
            let count = 0;
            for (let i = 0; i < pixels.length; i += 4) if (pixels[i] > 180 && pixels[i + 1] > 180 && pixels[i + 2] > 180) count++;
            return count;
          }, { image: screen.toString("base64"), crop: scoreCrops.top });
          expect.soft(whitePixels, "3p top has no score glyphs").toBe(0);
          continue;
        }
        await expect(table.locator(`.table-player-frame[data-position="${position}"]`)).toContainText(name);
        const p = participants.find(p => p.display_name === name)!;
        const expectedScore = updated ? p.updatedScore : p.score;
        const errors: Record<string, number> = {};
        for (const [candidate, reference] of references) {
          errors[candidate] = await cropError(page, screen, reference.screen, scoreCrops[position], 2);
        }
        const best = Number(Object.entries(errors).sort((a, b) => a[1] - b[1])[0][0]);
        metrics.push({ updated, position, participantId: p.participant_id, seat: projection.players.find(player => player.participant_id === p.participant_id)!.seat, expectedScore, best, errors });
        await test.info().attach(`${updated}-${position}-crop`, { body: await page.screenshot({ clip: scoreCrops[position] }), contentType: "image/png" });
        // Exact candidate identity, not a permissive screenshot tolerance.
        expect.soft(best, `${name} ${position} final Worker pixels: ${JSON.stringify(errors)}`).toBe(expectedScore);
        expect.soft(errors[expectedScore], "expected glyphs match exactly").toBe(0);
        for (const [candidate, error] of Object.entries(errors)) {
          if (Number(candidate) !== expectedScore) expect.soft(error, "wrong score must be distinguishable").toBeGreaterThan(1);
        }
        const rasterErrors: Record<string, number> = {};
        for (const [candidate, reference] of references) {
          rasterErrors[candidate] = await cropError(page, raster, reference.raster, rasterCrops[position], 1);
        }
        expect.soft(Number(Object.entries(rasterErrors).sort((a, b) => a[1] - b[1])[0][0]), `${name} ${position} Window raster`).toBe(expectedScore);
      }
    }
    await test.info().attach("spatial-score-metrics", { body: JSON.stringify({ browser: page.context().browser()!.version(), viewport: page.viewportSize(), dpr: 2, metrics }, null, 2), contentType: "application/json" });
  });
}

test("local surface image renders and clears in the actual product Worker under unchanged CSP", async ({ page }) => {
  const uploaded: string[] = [];
  page.on("request", request => { if (request.method() !== "GET") uploaded.push(request.url()); });
  const response = await page.goto("/tests/support/renderer-worker-product.html");
  expect(response!.headers()["content-security-policy"]).toContain("img-src 'self';");
  const table = page.getByTestId("three-table");
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  const baseline = await table.locator("canvas").screenshot();
  const png = await page.evaluate(() => {
    const canvas = document.createElement("canvas"); canvas.width = 8; canvas.height = 4;
    const context = canvas.getContext("2d")!;
    context.fillStyle = "#ff0000"; context.fillRect(0, 0, 4, 4);
    context.fillStyle = "#0000ff"; context.fillRect(4, 0, 4, 4);
    return canvas.toDataURL("image/png").split(",")[1];
  });
  await page.getByLabel("Table surface image", { exact: true }).setInputFiles({ name: "local.png", mimeType: "image/png", buffer: Buffer.from(png, "base64") });
  await expect(page.getByRole("button", { name: "Clear surface image", exact: true })).toBeEnabled();
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  const rendered = await table.locator("canvas").screenshot();
  const colors = await page.evaluate(async encoded => {
    const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(encoded), char => char.charCodeAt(0))], { type: "image/png" }));
    const canvas = document.createElement("canvas"); canvas.width = bitmap.width; canvas.height = bitmap.height;
    const context = canvas.getContext("2d")!; context.drawImage(bitmap, 0, 0); bitmap.close();
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let red = 0, blue = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      const [r, g, b] = [pixels[index], pixels[index + 1], pixels[index + 2]];
      if (r > 140 && r > 2 * g && r > 2 * b) red++;
      if (b > 140 && b > 2 * r && b > 2 * g) blue++;
    }
    return { red, blue, total: canvas.width * canvas.height };
  }, rendered.toString("base64"));
  // A white mapless material also differs from baseline; require both uploaded colors.
  expect(colors.red).toBeGreaterThan(colors.total / 20);
  expect(colors.blue).toBeGreaterThan(colors.total / 20);
  await page.getByRole("button", { name: "Clear surface image", exact: true }).click();
  await expect(page.getByRole("button", { name: "Clear surface image", exact: true })).toBeDisabled();
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  // Avoid an unbounded byte-array diff when an exact PNG comparison fails.
  expect((await table.locator("canvas").screenshot()).equals(baseline)).toBe(true);
  expect(uploaded).toEqual([]);
});

test("actual product Worker preserves quality and viewport revisions across Live, Replay and fresh lifetimes", async ({ page }) => {
  await page.goto("/tests/support/renderer-worker-product.html");
  const table = page.getByTestId("three-table");
  const inspect = async () => {
    await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
    const measured = await table.evaluate(node => {
      const canvas = node.querySelector("canvas")!;
      const rect = canvas.getBoundingClientRect();
      const quality = JSON.parse(node.querySelector<HTMLElement>(".three-table-renderer")!.dataset.rendererQuality!);
      return { rect: { width: rect.width, height: rect.height }, quality,
        tileCount: Number((node as HTMLElement).dataset.renderedTileCount),
        primitiveCount: Number((node as HTMLElement).dataset.renderedScenePrimitives),
        triangleCount: Number((node as HTMLElement).dataset.renderedSceneTriangles) };
    });
    expect(measured.quality).toMatchObject({ antialias: true, clearAlpha: 1, pixelRatio: 2,
      colorSpace: "srgb", toneMapping: 0, shadowType: 1, precision: "highp", shaderChecks: true });
    expect(measured.quality.width).toBe(measured.rect.width * 2);
    expect(measured.quality.height).toBe(measured.rect.height * 2);
    expect(measured.tileCount).toBeGreaterThan(0); expect(measured.primitiveCount).toBeGreaterThan(0); expect(measured.triangleCount).toBeGreaterThan(0);
    const realm = await page.workers()[0].evaluate(() => ({ hasWindow: typeof window !== "undefined", hasDocument: typeof document !== "undefined" }));
    expect(realm).toEqual({ hasWindow: false, hasDocument: false });
    return { ...measured, realm };
  };
  const evidence: unknown[] = [await inspect()];
  for (const size of [{ width: 1024, height: 600 }, { width: 1600, height: 900 }, { width: 1280, height: 720 }]) {
    await page.setViewportSize(size);
    await expect.poll(() => table.evaluate(node => {
      const quality = JSON.parse(node.querySelector<HTMLElement>(".three-table-renderer")!.dataset.rendererQuality!);
      return [quality.width, quality.height];
    })).toEqual([size.width * 2, size.height * 2]);
    evidence.push(await inspect());
  }
  await page.getByRole("button", { name: "Change surface", exact: true }).click();
  await expect(table).toHaveAttribute("data-surface", "replay"); evidence.push(await inspect());
  await page.getByRole("button", { name: "Change mode", exact: true }).click();
  await expect(table).toHaveAttribute("data-player-frame-count", "3"); evidence.push(await inspect());
  await page.getByRole("button", { name: "Toggle table", exact: true }).click();
  await expect(table).toHaveCount(0);
  await expect.poll(() => page.workers().length).toBe(0);
  await page.getByRole("button", { name: "Toggle table", exact: true }).click(); evidence.push(await inspect());
  await expect.poll(() => page.workers().length).toBe(1);
  const path = test.info().outputPath("product-native-quality.json");
  writeFileSync(path, JSON.stringify(evidence, null, 2), { flag: "wx", mode: 0o600 });
  await test.info().attach("product-native-quality", { path, contentType: "application/json" });
});

test("releases a genuinely blocked product Worker and mounts a fresh native lifetime", async ({ page }) => {
  await page.goto("/tests/support/renderer-worker-product.html");
  const table = page.getByTestId("three-table");
  const toggle = page.getByRole("button", { name: "Toggle table", exact: true });
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  const key = `renderer-stall-${randomUUID()}`;
  await captureRenderer(page, key);
  await toggle.click();
  await expect.poll(() => page.workers().length).toBe(0);
  await toggle.click();
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  const blocked = page.workers()[0];
  await page.evaluate(key => {
    const host = window as unknown as Record<string, unknown>;
    host.__disposedReplies = 0;
    host.__terminationRequests = [];
    const terminate = Worker.prototype.terminate;
    Worker.prototype.terminate = function () {
      if (this === host[key]) (host.__terminationRequests as number[]).push(performance.now());
      return terminate.call(this);
    };
    (host[key] as Worker).addEventListener("message", event => {
      if (event.data.type === "disposed") host.__disposedReplies = Number(host.__disposedReplies) + 1;
    });
  }, key);
  const events = await prepareRendererStall(page, key);
  await armRendererStall(page, key);
  await page.setViewportSize({ width: 1281, height: 720 });
  await expect.poll(() => events.filter(event => event.phase === "start").length).toBe(1);
  const cleanupStart = await page.evaluate(() => performance.now());
  await toggle.click();
  await expect(table).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => (window as unknown as { __terminationRequests: number[] }).__terminationRequests.length)).toBe(1);
  const terminationAt = await page.evaluate(() => (window as unknown as { __terminationRequests: number[] }).__terminationRequests[0]);
  expect(terminationAt - cleanupStart).toBeLessThanOrEqual(5_000);
  // The 3000ms owner guard requests termination; browser thread destruction has its own grace.
  await expect.poll(() => page.workers().length, { timeout: 20_000 }).toBe(0);
  expect(events.map(event => [event.phase, event.realm])).toEqual([["start", "worker"]]);
  expect(await page.evaluate(() => (window as unknown as { __disposedReplies: number }).__disposedReplies)).toBe(0);
  await toggle.click();
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  expect(page.workers()).toHaveLength(1);
  expect(page.workers()[0]).not.toBe(blocked);
  await expect(table).toHaveAttribute("data-renderer-pixel-ratio", "2");
  await expect(table).toHaveAttribute("data-rendered-scene-primitives", /^[1-9]\d*$/);
  const path = test.info().outputPath("blocked-worker-recovery.json");
  writeFileSync(path, JSON.stringify({ events, cleanupStart, terminationAt, disposedReplies: 0, freshNativeLifetime: true }, null, 2), { flag: "wx", mode: 0o600 });
});

test("cached Replay navigation resets local images and playback and retires the old Worker", async ({ page }) => {
  const nonGet: string[] = [];
  page.on("request", request => { if (request.method() !== "GET") nonGet.push(request.url()); });
  const projection = { ...fixture.projections["4p-red-east"], audience: "replay_admin" };
  const saved = (match_id: string) => ({
    match_id, source: "ranked", room_name: match_id, game_mode: "4p-red-east",
    started_at: "2026-09-16T00:00:00Z", completed_at: "2026-09-16T00:10:00Z",
    file_size: 512, availability: "available", replay_available: true,
    players: projection.players.map(player => ({ participant_id: player.participant_id,
      display_name: player.display_name, participant_kind: "built_in_bot", seat: player.seat,
      final_points: player.score })),
    frames: [0, 1].map(event_index => ({ event_index, visible_event: { type: "start_kyoku" },
      visible_state: projection, auxiliary_events: [] })),
  });
  await page.route("**/api/v1/admin/replays/**", async route => {
    const matchId = new URL(route.request().url()).pathname.split("/").at(-1)!;
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(saved(matchId)) });
  });
  // Visit B first so browser-back from A has real cached data, not a loading-boundary unmount.
  await page.goto("/admin/replays/CACHED-B");
  const table = page.getByTestId("three-table");
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  await page.evaluate(() => {
    history.pushState({}, "", "/admin/replays/CACHED-A");
    window.dispatchEvent(new PopStateEvent("popstate"));
  });
  await expect(page.getByRole("heading", { name: "CACHED-A Replay", exact: true })).toBeVisible();
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  await page.getByText("Settings", { exact: true }).click();
  const png = await page.evaluate(() => {
    const canvas = document.createElement("canvas"); canvas.width = 8; canvas.height = 4;
    const context = canvas.getContext("2d")!; context.fillStyle = "#ff0000"; context.fillRect(0, 0, 8, 4);
    return canvas.toDataURL("image/png").split(",")[1];
  });
  for (const target of ["surface", "surround"]) {
    await page.getByLabel(`Table ${target} image`, { exact: true }).setInputFiles({ name: "local.png", mimeType: "image/png", buffer: Buffer.from(png, "base64") });
    await expect(page.getByRole("button", { name: `Clear ${target} image`, exact: true })).toBeEnabled();
  }
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  await page.keyboard.press("Escape");
  await page.getByRole("combobox", { name: "Playback speed" }).selectOption("2");
  await page.getByRole("button", { name: "Next Event", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Replay position" })).toHaveValue("1");
  const oldWorker = page.workers()[0];
  expect(oldWorker).toBeDefined();
  await page.goBack();
  await expect(page.getByRole("heading", { name: "CACHED-B Replay", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  for (const target of ["surface", "surround"]) {
    await expect(page.getByRole("button", { name: `Clear ${target} image`, exact: true })).toBeDisabled();
  }
  await page.keyboard.press("Escape");
  await expect(page.getByRole("slider", { name: "Replay position" })).toHaveValue("0");
  await expect(page.getByRole("combobox", { name: "Playback speed" })).toHaveValue("1");
  await expect.poll(() => page.workers().includes(oldWorker)).toBe(false);
  await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 20_000 });
  expect(page.workers()).toHaveLength(1);
  expect(page.workers()[0]).not.toBe(oldWorker);
  await expect(table).toHaveAttribute("data-renderer-pixel-ratio", "2");
  await expect(table).toHaveAttribute("data-rendered-scene-primitives", /^[1-9]\d*$/);
  expect(nonGet).toEqual([]);
});
