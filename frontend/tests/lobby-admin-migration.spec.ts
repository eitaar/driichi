import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { resolve } from 'node:path';

for (const count of [3, 4]) for (const theme of ['light', 'dark']) {
 test(`${count}p Lobby has stable portrait cards and keyboard/touch states in ${theme}`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(theme => { localStorage.setItem('driichi-theme', theme); sessionStorage.setItem('driichi:participant:123456', 'P1'); }, theme);
  await page.route('**/assets/characters/player-red/*.webp', route => route.fulfill({ path: resolve('tests/fixtures/task12-characters/player-red', new URL(route.request().url()).pathname.split('/').at(-1)!) }));
  const participants = Array.from({ length: count }, (_, index) => ({ participant_id: `P${count-index}`, display_name: index === count-1 ? 'Mika' : `Participant ${count-index}`, kind: index === count-1 ? 'human' : 'built_in_bot', presence: 'connected', selected: true, ready: index !== count-1, character_id: 'player-red', role: 'none', controller: 'interactive' }));
  const room = { join_code: '123456', room_name: 'Night Market', game_mode: `${count}p-red-east`, phase: 'lobby', revision: 1, participants, roster: [], match_players: [], result: null };
  let update!: () => void;
  await page.routeWebSocket('**/ws/v1/rooms/123456/human', socket => {
   socket.send(JSON.stringify({ type: 'snapshot', room, state: null }));
   update = () => socket.send(JSON.stringify({ type: 'room_update', room: { ...room, revision: 2, participants: [...participants].reverse().map(p => ({ ...p, ready: true, presence: 'disconnected' })) } }));
  });
  await page.goto('/room/123456/lobby');
  const cards = page.getByRole('list', { name: 'Participants' }).getByRole('button');
  const names = ['Mika', ...Array.from({ length: count-1 }, (_, index) => `Participant ${index+2}`)];
  await expect(cards).toHaveText(names);
  const frames = cards.locator('.lobby-portrait-frame');
  await expect(frames).toHaveCount(count);
  for (const width of [1440, 1024, 390]) {
   await page.setViewportSize({ width, height: width === 1024 ? 600 : 960 });
   const boxes = await Promise.all((await cards.all()).map(card => card.boundingBox()));
   expect(boxes[0]!.y).toBe(boxes[1]!.y);
   if (width >= 1024) expect(boxes.at(-1)!.y).toBe(boxes[0]!.y);
   else expect(boxes[2]!.y).toBeGreaterThan(boxes[0]!.y);
   expect(await cards.first().evaluate(element => getComputedStyle(element).transform)).toBe('none');
   const crop = await frames.first().evaluate(element => {
    const image = element.parentElement!.querySelector('img')!;
    return { frame: image.parentElement!.clientHeight, image: image.getBoundingClientRect().height, transform: getComputedStyle(element).transform, imageTransform: getComputedStyle(image).transform };
   });
   expect(crop.image).toBeGreaterThan(crop.frame);
   expect(crop.transform).not.toBe('none');
   expect(crop.imageTransform).toBe('none');
   expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByText(`${count-1} / ${count} ready`, { exact: true })).toBeVisible();
  await expect(page.getByText(`${count} connected`, { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Set Ready' })).toBeEnabled();
  const trigger = page.getByRole('button', { name: 'Mika', exact: true });
  await trigger.focus(); await page.keyboard.press('Enter');
  const detail = page.getByRole('dialog', { name: 'Mika' });
  await expect(detail).toContainText('not ready');
  await expect(detail).toContainText('Your Participant');
  await expect(detail).not.toContainText('Seat');
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.keyboard.press('Escape'); await expect(trigger).toBeFocused();
  await page.getByText('Match readiness blockers', { exact: true }).click();
  await expect(page.getByText('Mika: not ready', { exact: true })).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath(`lobby-${count}p-${theme}-390.png`), fullPage: true });
  const stableNames = await cards.allTextContents();
  update();
  await expect(cards).toHaveText(stableNames);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await expect(page.getByText('0 connected', { exact: true })).toBeVisible();
  await trigger.click(); await expect(detail).toContainText('disconnected'); await page.keyboard.press('Escape');
  await page.setViewportSize({ width: 320, height: 740 });
  await page.evaluate(() => document.documentElement.style.fontSize = '200%');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath(`lobby-${count}p-${theme}-320-text200.png`), fullPage: true });
 });
}

test('Lobby keeps long names, duplicate identities and a missing portrait usable at 200% text', async ({ page }, testInfo) => {
 const longName = 'Participant-with-a-long-unbroken-display-name'.repeat(3);
 const participants = ['P3', 'P1', 'P2'].map((id, index) => ({ participant_id: id, display_name: index === 0 ? longName : 'Same name', kind: 'human', presence: 'connected', selected: true, ready: false, character_id: index === 0 ? 'missing' : 'player-red', role: 'none', controller: 'interactive' }));
 await page.addInitScript(() => sessionStorage.setItem('driichi:participant:123456', 'P1'));
 await page.route('**/assets/characters/player-red/*.webp', route => route.fulfill({ path: resolve('tests/fixtures/task12-characters/player-red', new URL(route.request().url()).pathname.split('/').at(-1)!) }));
 await page.route('**/assets/characters/missing/*.webp', route => route.fulfill({ status: 404 }));
 await page.routeWebSocket('**/ws/v1/rooms/123456/human', socket => socket.send(JSON.stringify({ type: 'snapshot', room: { join_code: '123456', room_name: 'Room-with-a-long-unbroken-display-name'.repeat(4), game_mode: '3p-red-east', phase: 'lobby', revision: 1, participants, roster: [], match_players: [], result: null }, state: null })));
 await page.goto('/room/123456/lobby');
 const cards = page.getByRole('list', { name: 'Participants' }).getByRole('button');
 await expect(cards).toHaveCount(3);
 const missing = cards.filter({ hasText: longName });
 await expect(missing).toContainText('Portrait unavailable');
 for (const width of [1440, 1024, 390, 320]) {
  await page.setViewportSize({ width, height: width === 1024 ? 600 : 844 });
  await page.evaluate(() => document.documentElement.style.fontSize = '200%');
  await testInfo.attach(`overflow-${width}`, { body: JSON.stringify(await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth, offenders: Array.from(document.querySelectorAll('body *')).map(element => ({ tag: element.tagName, class: element.className, x: element.getBoundingClientRect().x, right: element.getBoundingClientRect().right, height: element.getBoundingClientRect().height, scroll: element.scrollWidth, client: element.clientWidth, whiteSpace: getComputedStyle(element).whiteSpace, wrap: getComputedStyle(element).overflowWrap })).filter(box => box.x < 0 || box.right > innerWidth || box.scroll > box.client) }))), contentType: 'application/json' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await missing.click();
  const detail = page.getByRole('dialog', { name: longName });
  await expect(detail).toContainText('P3');
  await page.keyboard.press('Escape');
  await expect(missing).toBeFocused();
 }
 for (const [index, id] of ['P1', 'P2'].entries()) {
  await cards.filter({ hasText: 'Same name' }).nth(index).click();
  await expect(page.getByRole('dialog', { name: 'Same name' })).toContainText(id);
  await page.keyboard.press('Escape');
 }
 await expect(page.getByRole('button', { name: 'Set Ready' })).toBeDisabled();
 await page.screenshot({ path: testInfo.outputPath('lobby-long-names-missing-320-text200.png'), fullPage: true });
});

for (const theme of ['light', 'dark']) test(`mobile Admin list/detail/Back preserves management and one-time token in ${theme}`, async ({ page }, testInfo) => {
 await page.setViewportSize({ width: 390, height: 844 });
 await page.addInitScript(theme => localStorage.setItem('driichi-theme', theme), theme);
 const room = { join_code: '123456', room_name: 'Night Market', game_mode: '3p-red-east', phase: 'lobby', time_control: 'casual', replay_save: true, participant_limit: 4, connected_count: 1, participant_count: 1, selected_count: 1, created_at: '2026-09-16T00:00:00Z', participants: [{ participant_id: 'P1', display_name: 'Mika', kind: 'human', presence: 'connected', selected: true, ready: false, character_id: 'player-red', role: 'none', controller: 'interactive' }], roster: [], match_players: [], result: null, revision: 1, benchmark_mode: false, benchmark: null, persistence_degraded: false, replay_available: true };
 const token = { token_id: 'tok_1', name: 'runner', state: 'active', created_at: '2026-09-16T00:00:00Z', revoked_at: null };
 const mutations: string[] = [];
 await page.route('**/api/v1/admin/**', route => {
  const request = route.request(), path = new URL(request.url()).pathname;
  if (request.method() !== 'GET') mutations.push(`${request.method()} ${path}`);
  if (path.endsWith('/tokens') && request.method() === 'POST') return route.fulfill({ status: 201, json: { ...token, token: 'secret-once-only' } });
  if (path.endsWith('/tokens')) return route.fulfill({ json: [token] });
  if (path.endsWith('/rooms')) return route.fulfill({ json: [room] });
  return route.fulfill({ json: room });
 });
 await page.route('**/assets/characters/player-red/icon.webp', route => route.fulfill({ path: resolve('tests/fixtures/task12-characters/player-red/icon.webp') }));
 await page.goto('/admin');
 await expect(page.getByRole('heading', { name: 'Rooms', exact: true })).toBeVisible();
 await expect(page.getByRole('region', { name: 'Room detail' })).not.toBeVisible();
 await page.getByRole('link', { name: /Night Market/ }).click();
 await expect(page).toHaveURL(/\/admin\/rooms\/123456$/);
 await expect(page.getByRole('heading', { name: 'Night Market', exact: true })).toBeVisible();
 await expect(page.getByRole('complementary', { name: 'Room management' })).not.toBeVisible();
 await expect(page.getByRole('button', { name: 'Deselect', exact: true })).toBeVisible();
 await expect(page.getByRole('button', { name: 'Start Match', exact: true })).toBeDisabled();
 await expect(page.getByRole('region', { name: 'Selected roster' })).not.toContainText('Seat');
 await page.getByText('Room settings', { exact: false }).click();
 await page.getByLabel('Room name', { exact: true }).fill('Edited room');
 await page.getByRole('button', { name: 'Save settings', exact: true }).click();
 await expect.poll(() => mutations).toContain('PATCH /api/v1/admin/rooms/123456');
 expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
 await page.screenshot({ path: testInfo.outputPath(`admin-detail-${theme}-390.png`), fullPage: true });
 await page.getByRole('link', { name: 'Back to Rooms' }).click();
 await expect(page).toHaveURL(/\/admin$/);
 await page.getByText('Bot tokens', { exact: true }).click();
 await page.getByLabel('Token name').fill('runner');
 await page.getByRole('button', { name: 'Create token', exact: true }).click();
 const secretDialog = page.getByRole('dialog', { name: 'Copy this Bot Token now.' });
 await expect(secretDialog).toContainText('secret-once-only');
 await expect(page).toHaveURL(/\/admin$/);
 expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
 await page.keyboard.press('Escape');
 await expect(page.getByText('secret-once-only', { exact: true })).toHaveCount(0);
 await expect(page.getByLabel('Token name')).toBeFocused();
 await page.getByRole('button', { name: 'Revoke runner' }).click();
 await page.getByRole('button', { name: 'Confirm revoke' }).click();
 await expect.poll(() => mutations).toContain('POST /api/v1/admin/tokens/tok_1/revoke');
 await page.screenshot({ path: testInfo.outputPath(`admin-list-${theme}-390.png`), fullPage: true });
 await page.goto('/admin/rooms/123456');
 await expect(page.getByRole('heading', { name: 'Night Market', exact: true })).toBeVisible();
 await page.setViewportSize({ width: 320, height: 740 });
 await page.evaluate(() => document.documentElement.style.fontSize = '200%');
 expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
