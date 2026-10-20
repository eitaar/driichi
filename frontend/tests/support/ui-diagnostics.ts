import type { Page, TestInfo } from '@playwright/test';

/**
 * Opt-in UI interaction diagnostic. Installed explicitly by the specs that need
 * it; never by the shared observer fixture, and never in product code.
 *
 * Hot path records identity and DOM attributes only. Computed style and animation
 * state are read from a requestAnimationFrame sampler (outside event dispatch) and
 * from the final snapshot, so observation does not force synchronous style flushes
 * inside the pointer/focus ordering it is measuring.
 *
 * Flushes are time-coalesced so serialising the buffer cannot turn into quadratic
 * main-thread work during a transition: rows append in O(1) and the whole buffer is
 * stringified at most a few times per second, plus once on pagehide and on stop().
 *
 * Each full navigation creates a fresh in-page buffer, so records are flushed to
 * sessionStorage per document and merged at collection time. A multi-document or
 * partially-flushed receipt reports `incomplete` against the epoch witness instead
 * of looking complete.
 *
 * The sampler window opens only on a tracked `disabled` attribute change; it runs
 * for 500 ms and records one frame sample per animation frame.
 */
const KEY = 'task7bUiDiagnostics';
const epochKey = `${KEY}:documents`;
const FLUSH_DELAY_MS = 250;

export async function installUiDiagnostics(page: Page) {
  await page.addInitScript(({ key, epochKey, flushDelayMs }) => {
    const tracked = '[data-slot="select-trigger"],[data-slot="select-content"],[data-slot="drawer-popup"],.recharts-surface,[data-slot="button"].text-destructive';
    const records: Record<string, unknown>[] = [];
    const controller = new AbortController();
    const epoch = Number(sessionStorage.getItem(epochKey) ?? '0') + 1;
    try { sessionStorage.setItem(epochKey, String(epoch)); } catch { /* storage unavailable: in-memory only */ }
    let sampleUntilMs = 0;
    let rafHandle = 0;
    let truncated = false;
    let flushTimer = 0;
    const errors: string[] = [];

    const describeNode = (node: EventTarget | null) => node instanceof Element
      ? { tag: node.tagName, id: node.id, role: node.getAttribute('role'), label: node.getAttribute('aria-label'), text: node.textContent?.slice(0, 80) }
      : null;

    const flush = () => {
      if (flushTimer) { window.clearTimeout(flushTimer); flushTimer = 0; }
      // A failed write is recorded, not swallowed: a stale shorter snapshot must not
      // look like a complete document.
      try { sessionStorage.setItem(`${key}:${epoch}`, JSON.stringify({ epoch, url: location.href, truncated, records })); }
      catch (error) { const reason = `flush:${String(error)}`; if (!errors.includes(reason)) errors.push(reason); }
    };
    const scheduleFlush = () => {
      if (flushTimer) return;
      flushTimer = window.setTimeout(flush, flushDelayMs);
    };

    const push = (entry: Record<string, unknown>) => {
      if (records.length >= 10000) { truncated = true; return; }
      records.push({ time: performance.now(), ...entry });
      scheduleFlush();
    };

    const styleOf = (node: Element) => {
      const style = getComputedStyle(node);
      return {
        opacity: style.opacity, color: style.color, background: style.backgroundColor,
        transition: style.transitionProperty, duration: style.transitionDuration,
        animations: node.getAnimations().map(animation => ({
          state: animation.playState, time: animation.currentTime,
          property: animation instanceof CSSTransition ? animation.transitionProperty : null,
        })),
      };
    };

    // Hot path: metadata only, no style resolution.
    const recordMeta = (reason: string, node: Element, detail: Record<string, unknown> = {}) => push({
      reason, target: describeNode(node), active: describeNode(document.activeElement),
      disabled: node.matches(':disabled'), expanded: node.getAttribute('aria-expanded'),
      open: node.hasAttribute('data-open'), closed: node.hasAttribute('data-closed'), ...detail,
    });

    const sample = () => {
      document.querySelectorAll('[data-slot="button"].text-destructive').forEach(node => push({
        reason: 'transition-frame', target: describeNode(node), active: describeNode(document.activeElement), ...styleOf(node),
      }));
      rafHandle = performance.now() < sampleUntilMs ? requestAnimationFrame(sample) : 0;
    };

    for (const type of ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'click', 'focusin', 'focusout', 'transitionrun', 'transitionend', 'transitioncancel']) {
      document.addEventListener(type, event => {
        const node = event.target instanceof Element ? event.target.closest(tracked) : null;
        if (node || (event instanceof FocusEvent && event.target instanceof Element)) {
          recordMeta(type, node ?? event.target as Element, {
            trusted: event.isTrusted,
            pointerType: event instanceof PointerEvent ? event.pointerType : null,
            related: event instanceof FocusEvent ? describeNode(event.relatedTarget) : null,
          });
        }
      }, { capture: true, passive: true, signal: controller.signal });
    }

    const observer = new MutationObserver(mutations => {
      for (const mutation of mutations) {
        const node = mutation.target;
        if (!(node instanceof Element) || !node.matches(tracked)) continue;
        recordMeta(`attribute:${mutation.attributeName}`, node, { previous: mutation.oldValue });
        if (mutation.attributeName === 'disabled') {
          sampleUntilMs = performance.now() + 500;
          if (!rafHandle) rafHandle = requestAnimationFrame(sample);
        }
      }
    });
    observer.observe(document, { subtree: true, attributes: true, attributeOldValue: true, attributeFilter: ['disabled', 'aria-expanded', 'data-open', 'data-closed', 'aria-hidden', 'inert', 'style'] });

    window.addEventListener('pagehide', flush);

    Object.assign(window, {
      task7bUiDiagnostics: {
        stop: () => {
          controller.abort();
          observer.disconnect();
          cancelAnimationFrame(rafHandle);
          window.removeEventListener('pagehide', flush);
          flush();
          // Earlier documents come from storage; the current document is taken from
          // memory, which stays authoritative even when its last write failed.
          const documents: Record<string, unknown>[] = [];
          for (let index = 1; index < epoch; index++) {
            let raw: string | null = null;
            try { raw = sessionStorage.getItem(`${key}:${index}`); } catch (error) { errors.push(`getItem:${String(error)}`); }
            if (!raw) continue;
            try { documents.push(JSON.parse(raw)); } catch (error) { errors.push(`parse:${String(error)}`); }
          }
          documents.push({ epoch, url: location.href, truncated, records, errors });
          let finalStyles: Record<string, unknown>[] = [];
          try { finalStyles = [...document.querySelectorAll(tracked)].map(node => ({ target: describeNode(node), ...styleOf(node) })); }
          catch (error) { errors.push(`finalStyles:${String(error)}`); }
          const rows = documents.flatMap(doc => (doc.records as unknown[]) ?? []);
          const documentErrors = documents.flatMap(doc => (doc.errors as unknown[]) ?? []);
          const allErrors = [...new Set([...errors, ...documentErrors] as string[])];
          return {
            documents: documents.map(doc => ({ url: doc.url, rows: ((doc.records as unknown[]) ?? []).length, truncated: doc.truncated })),
            expectedDocuments: epoch,
            documentCount: documents.length,
            incomplete: documents.length < epoch || allErrors.length > 0,
            truncated: documents.some(doc => doc.truncated) || truncated,
            rows,
            finalStyles,
            errors: allErrors,
          };
        },
      },
    });
  }, { key: KEY, epochKey, flushDelayMs: FLUSH_DELAY_MS });
}

/**
 * Attach the diagnostic receipt only when the case actually installed it, and never
 * let diagnostic trouble fail an otherwise-passing test: collection problems are
 * reported inside the attachment as `incomplete: true`.
 */
export async function attachUiDiagnosticsIfInstalled(page: Page, testInfo: TestInfo) {
  if (page.isClosed()) {
    await testInfo.attach('ui-passive-diagnostics', {
      body: JSON.stringify({ unavailable: true, reason: 'page closed before collection', incomplete: true }, null, 2),
      contentType: 'application/json',
    });
    return;
  }
  let receipt: unknown;
  try {
    const installed = await page.evaluate(() => typeof (window as unknown as { task7bUiDiagnostics?: unknown }).task7bUiDiagnostics !== 'undefined');
    if (!installed) return;
    receipt = await page.evaluate(() => (window as unknown as { task7bUiDiagnostics: { stop(): unknown } }).task7bUiDiagnostics.stop());
  } catch (error) {
    receipt = { unavailable: true, reason: String(error), incomplete: true };
  }
  await testInfo.attach('ui-passive-diagnostics', { body: JSON.stringify(receipt, null, 2), contentType: 'application/json' });
}
