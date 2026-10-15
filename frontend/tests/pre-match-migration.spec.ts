import {expect,test} from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.beforeEach(async({page})=>{
 await page.route('**/api/v1/characters/human',route=>route.fulfill({json:[{id:'player-red',name:'Red Player'},{id:'tsumogiri-bot',name:'Tsumogiri'}]}));
 await page.route('**/api/v1/rooms/123456',route=>route.fulfill({json:{room_name:'Night Market',game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4}}));
});

test('missing registered portrait and voice keep the selection and join action usable',async({page})=>{
 await page.route('**/assets/characters/player-red/portrait.webp',route=>route.fulfill({status:404,body:'missing test asset'}));
 await page.route('**/assets/characters/player-red/voices/riichi.ogg',route=>route.fulfill({status:404,body:'missing test asset'}));
 await page.goto('/room/123456');
 await page.getByLabel('Display name').fill('Mika');
 await page.getByRole('button',{name:'Continue'}).click();
 await page.getByRole('radio',{name:'Red Player'}).check();
 await expect(page.locator('.character-stage .asset-fallback')).toBeVisible();
 await page.getByRole('button',{name:'Preview riichi voice'}).click();
 await expect(page.getByRole('button',{name:'Voice preview is unavailable'})).toBeVisible();
 await expect(page.getByRole('radio',{name:'Red Player'})).toBeChecked();
 await expect(page.getByRole('button',{name:'Join room'})).toBeEnabled();
});

test('primary action keeps readable contrast throughout enabling', async ({ page }) => {
 await page.goto('/');
 const button = page.getByRole('button', { name: 'Open room' });
 const ratios = await button.evaluate(async element => {
  const button = element as HTMLButtonElement;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d', {willReadFrequently:true})!;
  const luminance = (color: string) => {
   context.fillStyle = color;
   context.fillRect(0,0,1,1);
   const channels = Array.from(context.getImageData(0,0,1,1).data).slice(0,3).map(value => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
   });
   return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  };
  button.disabled = true;
  await Promise.all(button.getAnimations().map(animation => animation.finished));
  void getComputedStyle(button).color;
  button.disabled = false;
  const ratios: number[] = [];
  const start = performance.now();
  do {
   await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
   const style = getComputedStyle(button);
   const foreground = luminance(style.color), background = luminance(style.backgroundColor);
   ratios.push((Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05));
  } while (performance.now() - start < 300);
  return ratios;
 });
 expect(ratios.length).toBeGreaterThan(0);
 expect(Math.min(...ratios)).toBeGreaterThanOrEqual(4.5);
});

for(const theme of ['light','dark']){
 test(`Home and both Join steps keep accessible controls in ${theme}`,async({page})=>{
  await page.addInitScript(theme=>localStorage.setItem('driichi-theme',theme),theme);
  await page.goto('/');
  await expect(page.locator('html')).toHaveClass(theme === 'dark' ? 'dark' : '');
  expect((await new AxeBuilder({page}).analyze()).violations).toEqual([]);
  await page.getByLabel('Room code').fill('123456');
  await page.getByRole('button',{name:'Open room'}).click();
  await page.getByRole('button',{name:'Continue'}).click();
  await expect(page.getByLabel('Display name')).toHaveAttribute('aria-invalid','true');
  expect((await new AxeBuilder({page}).analyze()).violations).toEqual([]);
  await page.getByLabel('Display name').fill('Mika');
  await page.getByRole('button',{name:'Continue'}).click();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('radio',{name:'Red Player'})).toBeFocused();
  await page.keyboard.press('Space');
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('radio',{name:'Tsumogiri'})).toBeChecked();
  expect((await new AxeBuilder({page}).analyze()).violations).toEqual([]);
 });
}

test('local Back retains values while browser Back exits the same-URL Join',async({page})=>{
 await page.goto('/');
 await page.getByLabel('Room code').fill('123456');
 await page.getByRole('button',{name:'Open room'}).click();
 await page.getByLabel('Display name').fill('Mika');
 const historyLength=await page.evaluate(()=>history.length);
 await page.getByRole('button',{name:'Continue'}).click();
 await expect(page.getByRole('heading',{name:'Choose your character'})).toBeFocused();
 await page.getByRole('radio',{name:'Red Player'}).check();
 await page.getByRole('button',{name:'Back',exact:true}).click();
 await expect(page.getByLabel('Display name')).toHaveValue('Mika');
 await expect(page.getByLabel('Display name')).toBeFocused();
 await page.getByRole('button',{name:'Continue'}).click();
 await expect(page.getByRole('radio',{name:'Red Player'})).toBeChecked();
 await expect(page).toHaveURL(/\/room\/123456$/);
 expect(await page.evaluate(()=>history.length)).toBe(historyLength);
 await page.goBack();
 await expect(page).toHaveURL(/\/$/);
 await expect(page.getByRole('heading',{name:'Join a room'})).toBeVisible();
});

for(const width of [390,320]){
 test(`mobile Join reflows with long names and 200% text at ${width}px`,async({page})=>{
  await page.setViewportSize({width,height:844});
  await page.route('**/api/v1/characters/human',route=>route.fulfill({json:[{id:'player-red',name:'LongCharacterNameWithoutSpaces'.repeat(3)}]}));
  await page.route('**/api/v1/rooms/123456',route=>route.fulfill({json:{room_name:'LongRoomNameWithoutSpaces'.repeat(3),game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4}}));
  await page.goto('/');
  await page.addStyleTag({content:'html {font-size:200%}'});
  await expect(page.getByRole('button',{name:'Open room'})).toBeInViewport();
  const join=await page.locator('.entry-copy').boundingBox();
  const art=await page.locator('.entry-welcome').boundingBox();
  expect(join!.y+join!.height).toBeLessThanOrEqual(art!.y);
  await expect(page.getByLabel('Room code')).toBeFocused();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  await page.getByLabel('Room code').fill('123456');await page.getByRole('button',{name:'Open room'}).click();
  await page.getByLabel('Display name').fill('LongParticipantName'.repeat(3));
  await page.getByRole('button',{name:'Continue'}).click();
  await page.getByRole('radio').check();
  await page.getByRole('button',{name:'Join room'}).scrollIntoViewIfNeeded();
  await expect(page.getByRole('button',{name:'Join room'})).toBeInViewport();
  await expect(page.getByRole('button',{name:'Join room'})).toBeEnabled();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
 });
}

for(const size of [{width:1024,height:600},{width:1440,height:900},{width:1920,height:1080}]){
 test(`pre-match split layout ${size.width}`,async({page},info)=>{
  await page.setViewportSize(size);
  await page.route('**/assets/characters/*/*.webp',route=>{const url=new URL(route.request().url());const parts=url.pathname.split('/');return route.fulfill({path:`tests/fixtures/task12-characters/${parts.at(-2)!}/${parts.at(-1)!}`});});
  await page.goto('/');
  await expect(page.getByRole('heading',{name:'Join a room'})).toBeVisible();
  await expect(page.locator('.entry-cast img')).toHaveCount(2);
  await page.screenshot({path:info.outputPath('entry.png')});
  await page.getByLabel('Room code').fill('123456');await page.getByRole('button',{name:'Open room'}).click();
  await page.getByLabel('Display name').fill('Mika');
  await page.getByRole('button',{name:'Continue'}).click();
  await page.getByRole('radio',{name:'Red Player'}).check();
  const art=await page.locator('.character-stage').boundingBox();const form=await page.locator('.join-form').boundingBox();
  expect(art!.x+art!.width).toBeLessThan(form!.x);
  const join=await page.getByRole('button',{name:'Join room'}).boundingBox();expect(join!.y+join!.height).toBeLessThanOrEqual(size.height);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.screenshot({path:info.outputPath('character.png')});
 });
}
