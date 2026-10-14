import {expect,test} from '@playwright/test';
import path from 'node:path';
test('missing registered portrait keeps the selected character and join action usable',async({page})=>{
 await page.route('**/api/v1/characters/human',route=>route.fulfill({json:[{id:'player-red',name:'Red Player'}]}));
 await page.route('**/api/v1/rooms/123456',route=>route.fulfill({json:{room_name:'Night Market',game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4}}));
 await page.route('**/assets/characters/player-red/portrait.webp',route=>route.fulfill({status:404,body:'missing test asset'}));
 await page.goto('/room/123456');
 await page.getByRole('radio',{name:'Red Player'}).check();
 await expect(page.locator('.character-stage .asset-fallback')).toBeVisible();
 await page.getByLabel('Display name').fill('Mika');
 await expect(page.getByRole('button',{name:'Join room'})).toBeEnabled();
});
test('primary action keeps readable contrast throughout enabling', async ({ page }) => {
 await page.goto('/');
 const button = page.getByRole('button', { name: 'Open room' });
 const ratios = await button.evaluate(async element => {
  const button = element as HTMLButtonElement;
  const luminance = (color: string) => {
   const channels = color.match(/[\d.]+/g)!.slice(0, 3).map(Number).map(value => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
   });
   return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  };
  button.disabled = true;
  await Promise.all(button.getAnimations().map(animation => animation.finished));
  getComputedStyle(button).color;
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
test('narrow entry and character choice remain scrollable without horizontal overflow',async({page})=>{
 await page.setViewportSize({width:390,height:844});
 await page.route('**/api/v1/characters/human',route=>route.fulfill({json:[{id:'player-red',name:'Red Player'}]}));
 await page.route('**/api/v1/rooms/123456',route=>route.fulfill({json:{room_name:'Night Market',game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4}}));
 await page.goto('/');
 expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
 await page.getByLabel('Room code').fill('123456');await page.getByRole('button',{name:'Open room'}).click();
 await page.getByRole('radio',{name:'Red Player'}).check();
 await page.getByLabel('Display name').fill('Mika');
 await page.getByRole('button',{name:'Join room'}).scrollIntoViewIfNeeded();
 await expect(page.getByRole('button',{name:'Join room'})).toBeInViewport();
 expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});
for(const size of [{width:1024,height:600},{width:1440,height:900},{width:1920,height:1080}]){
 test(`pre-match split layout ${size.width}`,async({page},info)=>{
  await page.setViewportSize(size);
  await page.route('**/api/v1/characters/human',route=>route.fulfill({json:[{id:'player-red',name:'Red Player'},{id:'tsumogiri-bot',name:'Tsumogiri'}]}));
  await page.route('**/assets/characters/*/*.webp',route=>{const url=new URL(route.request().url());const parts=url.pathname.split('/');return route.fulfill({path:path.resolve('tests/fixtures/task12-characters',parts.at(-2)!,parts.at(-1)!)});});
  await page.route('**/api/v1/rooms/123456',route=>route.fulfill({json:{room_name:'Night Market',game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4}}));
  await page.goto('/');
  await expect(page.getByRole('heading',{name:'Join a room'})).toBeVisible();
  await expect(page.locator('.entry-cast img')).toHaveCount(2);
  expect(await page.locator('.entry-shell').evaluate(e=>getComputedStyle(e).backgroundColor)).toBe('rgb(238, 240, 242)');
  await page.screenshot({path:info.outputPath('entry.png')});
  await page.getByLabel('Room code').fill('123456');await page.getByRole('button',{name:'Open room'}).click();
  await page.getByRole('radio',{name:'Red Player'}).check();
  await page.getByLabel('Display name').fill('Mika');
  const art=await page.locator('.character-stage').boundingBox();const form=await page.locator('.join-form').boundingBox();
  expect(art!.x+art!.width).toBeLessThan(form!.x);
  const join=await page.getByRole('button',{name:'Join room'}).boundingBox();expect(join!.y+join!.height).toBeLessThanOrEqual(size.height);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.screenshot({path:info.outputPath('character.png')});
 });
}
