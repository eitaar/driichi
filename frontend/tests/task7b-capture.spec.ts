import { resolve } from 'node:path';
import type { ElementHandle } from '@playwright/test';
import { expect, test } from './support/task7b-evidence';
import fixtures from './fixtures/task12-projection.json' with { type: 'json' };

for (const theme of ['light', 'dark']) for (const width of [1440, 1024, 390, 320]) {
  test(`Task7B four-chart Run native observer ${theme} ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 1024 ? 600 : 900 });
    await page.addInitScript(theme => localStorage.setItem('driichi-theme', theme), theme);
    if (theme === 'light' && width === 1440) test.setTimeout(60000);
    await page.addInitScript(() => {
      const NativeWorker = window.Worker;
      const counts = { starts: 0, active: 0 };
      Object.assign(window, { task7bWorkers: counts });
      window.Worker = class extends NativeWorker {
        private live = true;
        constructor(url: string | URL, options?: WorkerOptions) { super(url, options); counts.starts++; counts.active++; }
        terminate() { if (this.live) { this.live = false; counts.active--; } super.terminate(); }
      };
    });
    const roster = Array.from({ length: 4 }, (_, seat) => ({ participant_id: `p${seat}`, display_name: `Bot ${seat}`, initial_seat: seat, participant_kind: 'built_in_bot', character_id: null }));
    const benchmark = { run_id: 'run-1', target: 3, completed: 2, stop_requested: false, status: 'running' };
    const run = { ...benchmark, room_code: '123456', game_mode: '4p-red-east', reason: null, failed_match_id: null, roster, matches: [1, 2].map(sequence => ({ sequence, match_id: `saved-${sequence}`, results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: [45000, 30000, 15000, 10000][seat] })) })) };
    let privateReads = 0;
    await page.route('**/api/v1/admin/benchmark/runs/run-1', route => route.fulfill({ json: run }));
    await page.route('**/api/v1/admin/rooms/123456', route => route.fulfill({ json: { join_code: '123456', benchmark } }));
    await page.route('**/api/v1/admin/benchmark/rooms/123456/live', route => {
      privateReads++;
      return route.fulfill({ json: { revision: 7, benchmark, projection: { ...fixtures.projections['4p-red-east'], audience: 'benchmark_admin', players: fixtures.projections['4p-red-east'].players.map((p, seat) => ({ ...p, participant_id: `p${seat}`, display_name: `Bot ${seat}` })) } } });
    });
    await page.goto('/admin/benchmark/runs/run-1');
    if (width === 320) await page.evaluate(() => document.documentElement.style.fontSize = '200%');
    await expect(page.locator('.recharts-surface')).toHaveCount(4, { timeout: 20000 });
    const table = page.getByRole('group', { name: '3D mahjong table' });
    const workers = () => page.evaluate(() => (window as unknown as { task7bWorkers: { starts: number; active: number } }).task7bWorkers);
    let canvas: ElementHandle<SVGElement | HTMLElement> | null = null;
    if (width >= 1024) {
      await expect(table).toHaveAttribute('data-render-ready', 'true', { timeout: 20000 });
      await expect.poll(async () => (await workers()).active).toBe(1);
      canvas = await page.locator('canvas').elementHandle();
      await page.screenshot({ path: test.info().outputPath('actual-native-viewport.png'), fullPage: false });
      await table.screenshot({ path: test.info().outputPath('actual-native-table.png') });
    } else {
      await expect(page.locator('canvas')).toHaveCount(0); expect(privateReads).toBe(0);
      expect(await workers()).toEqual({ starts: 0, active: 0 });
    }
    if (theme === 'light' && width === 1440) {
      const net = page.getByRole('region', { name: 'Per-Match net score', exact: true });
      await expect(net.locator('.net-value-label')).toHaveCount(2);
      await net.getByRole('button', { name: 'Pause cycling' }).click();
      await net.getByRole('button', { name: 'Per-Match net score options' }).click();
      const participant = net.getByLabel('Net score Participant');
      const value = participant.locator('[data-slot="select-value"]');
      const paused = await value.innerText();
      await page.mouse.move(0, 0); await page.getByRole('button', { name: 'Refresh Run', exact: true }).focus();
      await page.waitForTimeout(8200);
      await expect(value).toHaveText(paused);
      await net.getByRole('button', { name: 'Resume cycling' }).click();
      await page.mouse.move(0, 0); await page.getByRole('button', { name: 'Refresh Run', exact: true }).focus();
      const resumedAt = await page.evaluate(() => performance.now());
      await expect.poll(() => value.innerText(), { timeout: 10000 }).not.toBe(paused);
      expect(await page.evaluate(() => performance.now()) - resumedAt).toBeGreaterThanOrEqual(7800);
      await participant.focus(); await page.keyboard.press('Tab'); await page.keyboard.press('Shift+Tab');
      await expect(participant).toBeFocused();
      const reading = await value.innerText();
      await page.waitForTimeout(8200);
      await expect(value).toHaveText(reading);
    }
    if (width === 320) {
      const grid = page.getByRole('region', { name: 'Rank distribution', exact: true }).locator('.recharts-polar-grid-concentric-polygon');
      expect(await grid.evaluateAll(nodes => Math.max(...nodes.map(node => (node as SVGGraphicsElement).getBBox().width)))).toBeGreaterThanOrEqual(80);
    }
    await page.screenshot({ path: test.info().outputPath('actual-product-run-four-charts-observer.png'), fullPage: true });
    if (width >= 1024) {
      await expect(table).toHaveAttribute('data-render-ready', 'true', { timeout: 20000 });
      await expect.poll(async () => (await workers()).active).toBe(1);
      expect(await canvas!.evaluate(node => node.isConnected && node === document.querySelector('canvas'))).toBe(true);
      await test.info().attach('native-final-worker-counts', { body: JSON.stringify(await workers()), contentType: 'application/json' });
    }
  });
}

for (const theme of ['light', 'dark']) for (const width of [1440, 1024, 390, 320]) {
  test(`Task7B original Join and login ${theme} ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 1024 ? 600 : 900 });
    await page.addInitScript(theme => localStorage.setItem('driichi-theme', theme), theme);
    const characters = [{ id: 'reimu', name: 'Reimu' }, { id: 'marisa', name: 'Marisa' }];
    await page.route('**/api/v1/characters/human', route => route.fulfill({ json: characters }));
    await page.route('**/api/v1/rooms/123456', route => route.fulfill({ json: { room_name: 'Night Market', game_mode: '4p-red-east', phase: 'Lobby', join_allowed: true, participant_count: 1, participant_limit: 4 } }));
    await page.route('**/assets/characters/*/*.webp', route => {
      const parts = new URL(route.request().url()).pathname.split('/');
      return route.fulfill({ path: resolve('../../../../driichi-runtime/character-packs', parts.at(-2)!, parts.at(-1)!) });
    });
    await page.goto('/room/123456');
    if (width === 320) await page.evaluate(() => document.documentElement.style.fontSize = '200%');
    await page.getByLabel('Display name').fill('Mika');
    for (const character of characters) {
      await page.getByText(character.name, { exact: true }).click();
      await expect(page.getByRole('radio', { name: character.name, exact: true })).toBeChecked();
      await page.locator('.character-stage img').evaluate(async image => { await (image as HTMLImageElement).decode(); });
      await page.screenshot({ path: test.info().outputPath(`join-${character.id}.png`), fullPage: true });
    }
    await page.goto('/admin/login');
    if (width === 320) await page.evaluate(() => document.documentElement.style.fontSize = '200%');
    await expect(page.getByRole('heading', { name: 'Sign in to host.' })).toBeVisible();
    await page.screenshot({ path: test.info().outputPath('login.png'), fullPage: true });
    // Deliberately non-live form values, intercepted only in this browser test.
    await page.route('**/api/v1/admin/login', route => route.fulfill({ status: 401, json: { title: 'Fixture refusal', detail: 'Offline fixture rejected sign in', status: 401 } }));
    await page.getByLabel('Username', { exact: true }).fill('offline-fixture');
    await page.getByLabel('Password', { exact: true }).fill('not-a-real-credential');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByText('Offline fixture rejected sign in')).toBeVisible();
    await expect(page).toHaveURL(/\/admin\/login$/);
    await page.getByLabel('Username', { exact: true }).fill('');
    await page.getByLabel('Password', { exact: true }).fill('');
    await page.screenshot({ path: test.info().outputPath('login-failure-fields-cleared-by-test.png'), fullPage: true });
    await page.route('**/api/v1/admin/login', route => route.fulfill({ json: {} }));
    await page.route('**/api/v1/admin/rooms', route => route.fulfill({ json: [] }));
    await page.route('**/api/v1/admin/tokens', route => route.fulfill({ json: [] }));
    await page.getByLabel('Username', { exact: true }).fill('offline-fixture');
    await page.getByLabel('Password', { exact: true }).fill('not-a-real-credential');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page).toHaveURL(/\/admin$/);
    await expect(page.getByRole('heading', { name: 'Rooms', exact: true })).toBeVisible();
    await expect(page.getByLabel('Password', { exact: true })).toHaveCount(0);
    await page.screenshot({ path: test.info().outputPath('admin-accepted-offline.png'), fullPage: true });
  });
}
