import { test as base } from '@playwright/test';
export { expect } from '@playwright/test';
export type { Page, Route, Locator } from '@playwright/test';

// Validation-only observer: records console/pageerror/request failures, workers and a
// final-state snapshot. It installs no renderer, network or readiness replacement, and
// adds no interaction instrumentation; opt-in UI diagnostics live in
// `./ui-diagnostics.ts` and are installed explicitly by the specs that need them.
export const test = base.extend<{ task7bEvidence: void }>({
  task7bEvidence: [async ({ context, page }, use, info) => {
    const events: object[] = [];
    const observe = (target: typeof page) => {
      target.on('console', message => events.push({ type: 'console', level: message.type(), text: message.text(), url: target.url() }));
      target.on('pageerror', error => events.push({ type: 'pageerror', message: error.message, stack: error.stack }));
      target.on('requestfailed', request => events.push({ type: 'requestfailed', url: request.url(), method: request.method(), failure: request.failure() }));
      target.on('response', response => { if (response.status() >= 400) events.push({ type: 'http-error', url: response.url(), status: response.status() }); });
      target.on('worker', worker => events.push({ type: 'native-worker', url: worker.url() }));
    };
    observe(page); context.on('page', observe);
    await use();
    for (const [index, target] of context.pages().entries()) {
      if (target.isClosed()) continue;
      try {
        await target.screenshot({ path: info.outputPath(`final-page-${index}.png`), fullPage: true, timeout: 10000 });
        events.push({ type: 'final-state', url: target.url(), viewport: target.viewportSize(), geometry: await target.evaluate(() => ({ width: innerWidth, height: innerHeight, scrollWidth: document.documentElement.scrollWidth, fontSize: getComputedStyle(document.documentElement).fontSize, theme: document.documentElement.className, canvases: document.querySelectorAll('canvas').length, ready: [...document.querySelectorAll('[data-render-ready]')].map(el => el.getAttribute('data-render-ready')) })) });
      } catch (error) { events.push({ type: 'capture-error', message: String(error) }); }
    }
    await info.attach('task7b-events', { body: JSON.stringify(events, null, 2), contentType: 'application/json' });
  }, { auto: true }],
});
