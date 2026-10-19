import { expect, test } from './support/task7b-evidence';
import AxeBuilder from '@axe-core/playwright';
import { resolve } from 'node:path';
import { attachUiDiagnosticsIfInstalled, installUiDiagnostics } from './support/ui-diagnostics';

// Attaches only when the case installed the opt-in diagnostic; never fails the test.
test.afterEach(async ({ page }, testInfo) => {
  await attachUiDiagnosticsIfInstalled(page, testInfo);
});

for (const count of [3, 4]) for (const theme of ['light', 'dark']) {
 test(`${count}p Lobby has stable portrait cards and keyboard/touch states in ${theme}`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(theme => { localStorage.setItem('driichi-theme', theme); sessionStorage.setItem('driichi:participant:123456', 'P1'); }, theme);
  await page.route('**/assets/characters/player-red/*.webp', route => route.fulfill({ path: `tests/fixtures/task12-characters/player-red/${new URL(route.request().url()).pathname.split('/').at(-1)}` }));
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
  const frames = cards.locator('.lobby-center-art');
  await expect(frames).toHaveCount(count);
  for (const width of [1440, 1024, 390]) {
   await page.setViewportSize({ width, height: width === 1024 ? 600 : 960 });
   // The card height can still reflect the prior viewport during its resize transition.
   await expect.poll(() => frames.first().evaluate(element => {
    const image = element.parentElement!.querySelector('img')!;
    return image.getBoundingClientRect().height - element.clientHeight;
   })).toBeGreaterThan(0);
   const boxes = await Promise.all((await cards.all()).map(card => card.boundingBox()));
   expect(boxes[0]!.y).toBe(boxes[1]!.y);
   if (width >= 1024) expect(boxes.at(-1)!.y).toBe(boxes[0]!.y);
   else expect(boxes[2]!.y).toBeGreaterThan(boxes[0]!.y);
   expect(await cards.first().evaluate(element => getComputedStyle(element).transform)).toBe('none');
   const crop = await frames.first().evaluate(element => {
    const image = element.parentElement!.querySelector('img')!;
    return { frame: element.clientHeight, image: image.getBoundingClientRect().height, transform: getComputedStyle(element).transform, imageTransform: getComputedStyle(image).transform };
   });
   expect(crop.image).toBeGreaterThan(crop.frame);
   expect(crop.transform).not.toBe('none');
   expect(crop.imageTransform).toBe('none');
   expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByText(`${count-1} / ${count} ready`, { exact: true })).toBeVisible();
  await expect(page.getByText(`${count} connected`, { exact: true })).toHaveCount(0);
  await expect(page.getByText('connected', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Set Ready' })).toBeEnabled();
  const trigger = page.getByRole('button', { name: 'Mika', exact: true });
  await trigger.focus(); await page.keyboard.press('Space');
  const detail = page.getByRole('dialog', { name: 'Mika' });
  await expect(detail).toContainText('not ready');
  await expect(detail).toContainText('Your Participant');
  await expect(detail).not.toContainText('Seat');
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.keyboard.press('Escape'); await expect(trigger).toBeFocused();
  await expect(page.getByText('Match readiness blockers', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Room status', { exact: true })).toHaveCount(0);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath(`lobby-${count}p-${theme}-390.png`), fullPage: true });
  const stableNames = await cards.allTextContents();
  update();
  await expect(cards).toHaveText(stableNames);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await expect(page.getByText('0 connected', { exact: true })).toHaveCount(0);
  await trigger.click(); await expect(detail).toContainText('disconnected'); await page.keyboard.press('Escape');
  for (const width of [320, 390]) {
   await page.setViewportSize({ width, height: 844 });
   await page.evaluate(() => document.documentElement.style.fontSize = '200%');
   expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
   const enlargedCards = await Promise.all((await cards.all()).map(card => card.boundingBox()));
   expect(enlargedCards[1]!.y).toBeGreaterThan(enlargedCards[0]!.y);
   for (const card of await cards.all()) {
    if (await card.getAttribute('aria-label') !== 'Mika') continue;
    expect(await card.locator('.lobby-center-name').evaluate(element => {
     const text = element.firstChild!;
     return Array.from(text.textContent!.matchAll(/\S+/g)).every(word => {
      const range = document.createRange(); range.setStart(text, word.index); range.setEnd(text, word.index + word[0].length);
      return range.getClientRects().length === 1;
     });
    })).toBe(true);
   }
   for (const text of ['Lobby', 'Night Market', '123456']) {
    const lineCount = await page.locator('main > header').getByText(text, { exact: true }).evaluate(element => {
     const range = document.createRange();
     range.selectNodeContents(element);
     return range.getClientRects().length;
    });
    expect(lineCount, `${text} must remain readable as one line at ${width}px/200%`).toBe(1);
   }
   await page.screenshot({ path: testInfo.outputPath(`lobby-${count}p-${theme}-${width}-text200.png`), fullPage: true });
  }
 });
}

test('Lobby keeps long names, duplicate identities and a missing portrait usable at 200% text', async ({ page }, testInfo) => {
 const longName = 'Participant-with-a-long-unbroken-display-name'.repeat(3).slice(0, 64);
 const participants = ['P3', 'P1', 'P2'].map((id, index) => ({ participant_id: id, display_name: index === 0 ? longName : 'Same name', kind: 'human', presence: 'connected', selected: true, ready: id === 'P2', character_id: index === 0 ? 'missing' : 'player-red', role: 'none', controller: 'interactive' }));
 await page.addInitScript(() => sessionStorage.setItem('driichi:participant:123456', 'P1'));
 await page.route('**/assets/characters/player-red/*.webp', route => route.fulfill({ path: `tests/fixtures/task12-characters/player-red/${new URL(route.request().url()).pathname.split('/').at(-1)}` }));
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
  expect(await page.getByRole('heading', { name: 'Lobby', exact: true }).evaluate(element => {
   const range = document.createRange(); range.selectNodeContents(element); return range.getClientRects().length;
  })).toBe(1);
  await missing.click();
  const detail = page.getByRole('dialog', { name: longName });
  await expect(detail).toContainText(longName);
  await expect(detail).not.toContainText('P3');
  await page.keyboard.press('Escape');
  await expect(missing).toBeFocused();
 }
 for (const [index, id] of ['P1', 'P2'].entries()) {
  await cards.filter({ hasText: 'Same name' }).nth(index).click();
  const detail = page.getByRole('dialog', { name: 'Same name' });
  await expect(detail).not.toContainText(/P1|P2|Identity|Presence/);
  if (id === 'P1') await expect(detail).toContainText('not ready');
  else await expect(detail).not.toContainText('not ready');
  await page.keyboard.press('Escape');
 }
 await expect(page.getByRole('button', { name: 'Set Ready' })).toBeDisabled();
 await page.screenshot({ path: testInfo.outputPath('lobby-long-names-missing-320-text200.png'), fullPage: true });
});

// Task 7 executes these physical geometry/face checks against original licensed art.
for (const count of [3, 4]) for (const theme of ['light', 'dark']) for (const width of [1440, 390, 320]) {
 test(`Lobby original faces and footer ${count}p ${theme} ${width}${width === 320 ? ' text200' : ''}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 900 });
  await page.addInitScript(theme => { localStorage.setItem('driichi-theme', theme); sessionStorage.setItem('driichi:participant:123456', 'P1'); }, theme);
  for (const character of ['reimu', 'marisa']) {
   await page.route(`**/assets/characters/${character}/*.webp`, route => route.fulfill({ path: resolve('../../../../driichi-runtime/character-packs', character, new URL(route.request().url()).pathname.split('/').at(-1)!) }));
  }
  const participants = Array.from({ length: count }, (_, index) => ({ participant_id: `P${index + 1}`, display_name: index === 0 ? 'N'.repeat(64) : 'Same name', kind: 'human', presence: 'connected', selected: true, ready: false, character_id: index % 2 ? 'marisa' : 'reimu', role: 'none', controller: 'interactive' }));
  await page.routeWebSocket('**/ws/v1/rooms/123456/human', socket => socket.send(JSON.stringify({ type: 'snapshot', room: { join_code: '123456', room_name: 'Night Market', game_mode: `${count}p-red-east`, phase: 'lobby', revision: 1, participants, roster: [], match_players: [], result: null }, state: null })));
  await page.goto('/room/123456/lobby');
  if (width === 320) await page.evaluate(() => document.documentElement.style.fontSize = '200%');
  const cards = page.locator('.lobby-participant-card');
  await expect(cards).toHaveCount(count);
  for (const card of await cards.all()) {
   await card.locator('img').evaluate(async image => {
    if (!(image instanceof HTMLImageElement)) throw new Error('Expected a participant portrait image');
    await image.decode();
   });
   const metrics = await card.evaluate(card => {
    const art = card.querySelector<HTMLElement>('.lobby-center-art')!;
    const name = card.querySelector<HTMLElement>('.lobby-center-name')!;
    const image = card.querySelector('img')!;
    const ar = art.getBoundingClientRect(), nr = name.getBoundingClientRect(), ir = image.getBoundingClientRect();
    const matrix = new DOMMatrix(getComputedStyle(art).transform);
    const glyphs = matrix.multiply(new DOMMatrix(getComputedStyle(name).transform));
    const footer = getComputedStyle(art, '::after'), stroke = getComputedStyle(art, '::before');
    const footerHeight = parseFloat(footer.height);
    // Original 600x800 art: both eyes and the face, not opaque-edge crop proxies.
    const face = image.src.includes('/reimu/') ? { x0:332, x1:442, y0:120, y1:210 } : { x0:210, x1:350, y0:135, y1:235 };
    const points = [[face.x0,face.y0],[face.x1,face.y0],[face.x0,face.y1],[face.x1,face.y1]].map(([x,y]) => ({ x:ir.x+x*ir.width/600, y:ir.y+y*ir.height/800 }));
    const faceInside = points.every(({x,y}) => {
     const left = ar.x + ar.width/2 - art.offsetWidth/2 + matrix.c*(y-ar.y-ar.height/2);
     return y >= ar.top && y < ar.bottom-footerHeight && x >= left && x <= left+art.offsetWidth;
    });
    const sliceCenter = ar.x + ar.width/2 + matrix.c*(nr.y+nr.height/2-ar.y-ar.height/2);
    return { centerError: nr.x+nr.width/2-sliceCenter, netSkew:glyphs.c, nameOwned:name.parentElement===art, clip:getComputedStyle(art).overflow, strokeWidth:stroke.borderTopWidth, strokeColor:stroke.borderTopColor, foreground:getComputedStyle(art).color, strokeZ:Number(stroke.zIndex), nameZ:Number(getComputedStyle(name).zIndex), pointerEvents:stroke.pointerEvents, faceInside, artScale:ir.width/art.offsetWidth, footerHeight, frameHeight:art.offsetHeight, nameBackground:getComputedStyle(name).backgroundColor, bottomError:ar.bottom-nr.bottom };
   });
   await test.step('name_center_follows_skewed_footer_slice', async () => {
    expect(metrics.nameOwned).toBe(true);
    expect(Math.abs(metrics.centerError)).toBeLessThan(2);
    expect(Math.abs(metrics.netSkew)).toBeLessThan(0.00001);
    expect(Math.abs(metrics.bottomError)).toBeLessThan(2);
    expect(metrics.nameBackground).toBe('rgba(0, 0, 0, 0)');
   });
   await test.step('foreground_frame_stays_above_footer', async () => {
    expect(metrics.strokeWidth).toBe('1px');
    expect(metrics.strokeColor).toBe(metrics.foreground);
    expect(metrics.strokeZ).toBeGreaterThan(metrics.nameZ);
    expect(metrics.pointerEvents).toBe('none');
    expect(metrics.clip).toBe('hidden');
    expect(metrics.footerHeight).toBeLessThanOrEqual(metrics.frameHeight * .30 + 1);
   });
   await test.step('original_face_visible_above_integrated_footer', async () => {
    expect(metrics.faceInside).toBe(true);
    expect(metrics.artScale).toBeCloseTo(1.7, 1);
   });
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath(`original-faces-${count}p-${theme}-${width}.png`), fullPage: true });
  const trigger = cards.first();
  await trigger.focus(); await page.keyboard.press('Space');
  await expect(page.getByRole('dialog', { name: 'N'.repeat(64) })).toBeVisible();
  await page.keyboard.press('Escape'); await expect(trigger).toBeFocused();
 });
}

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
 await page.route('**/assets/characters/player-red/icon.webp', route => route.fulfill({ path: 'tests/fixtures/task12-characters/player-red/icon.webp' }));
 if (theme === 'light') await installUiDiagnostics(page);
 await page.goto('/admin');
 await expect(page.getByRole('heading', { name: 'Rooms', exact: true })).toBeVisible();
 await expect(page.getByRole('region', { name: 'Room detail' })).not.toBeVisible();
 await page.getByRole('link', { name: /Night Market/ }).click();
 await expect(page).toHaveURL(/\/admin\/rooms\/123456$/);
 await expect(page.getByRole('heading', { name: 'Night Market', exact: true })).toBeVisible();
 await expect(page.getByRole('complementary', { name: 'Room management' })).not.toBeVisible();
 await expect(page.getByRole('button', { name: 'Deselect', exact: true })).toBeVisible();
 await expect(page.getByRole('button', { name: 'Start Match', exact: true })).toBeDisabled();
 await expect(page.getByRole('region', { name: 'Selected roster' })).toHaveCount(0);
 await expect(page.locator('.participants-section[data-slot="card"]')).toBeVisible();
 await expect(page.locator('.lifecycle-section[data-slot="card"]')).toBeVisible();
 await page.getByText('Room settings', { exact: false }).click();
 await page.getByLabel('Room name', { exact: true }).fill('Edited room');
 await page.getByRole('button', { name: 'Save settings', exact: true }).click();
 await expect.poll(() => mutations).toContain('PATCH /api/v1/admin/rooms/123456');
 const afterSaveAxe = await new AxeBuilder({ page }).analyze();
 await testInfo.attach('admin-after-save-axe', { body: JSON.stringify(afterSaveAxe, null, 2), contentType: 'application/json' });
 expect(afterSaveAxe.violations).toEqual([]);
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
