import type { ConsoleMessage, Page } from "@playwright/test";

export interface RendererStallEvent {
  phase: "start" | "end";
  realm: "window" | "worker";
  timeOrigin: number;
  now: number;
  hostObservedAt: number;
}

// Capture only the standard renderer init boundary; no production test API.
export async function captureRenderer(page: Page, key: string) {
  await page.evaluate(key => {
    if (typeof Worker === "undefined") return;
    const original = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (...args: unknown[]) {
      const data = args[0] as { type?: string; canvas?: unknown } | null;
      if (data?.type === "init" && data.canvas instanceof OffscreenCanvas) {
        (window as unknown as Record<string, Worker>)[key] = this;
      }
      return Reflect.apply(original, this, args);
    };
  }, key);
}

// Test-only GPU boundary: retain the prepared renderer's actual draw and frame.
function installDrawStall(prefix: string) {
  const prototype = WebGL2RenderingContext.prototype;
  const original = prototype.drawElements;
  let armed = false;
  let stalled = false;
  const originalMessage = self.onmessage;
  // EventTarget orders an existing onmessage before a subsequently added listener.
  self.onmessage = function (event) {
    if (event.data?.type === prefix) { armed = true; return; }
    return originalMessage?.call(self, event);
  };
  prototype.drawElements = function (mode, count, type, offset) {
    if (armed && !stalled) {
      stalled = true;
      const start = performance.now();
      const realm = typeof document === "undefined" ? "worker" : "window";
      const emit = (phase: string, now: number) => console.debug(prefix + JSON.stringify({ phase, realm, timeOrigin: performance.timeOrigin, now }));
      emit("start", start);
      while (performance.now() - start < 7500) { /* synchronous renderer work */ }
      emit("end", performance.now());
    }
    return original.call(this, mode, count, type, offset);
  };
}

export async function prepareRendererStall(page: Page, prefix: string) {
  const events: RendererStallEvent[] = [];
  const observe = (message: ConsoleMessage) => {
    if (!message.text().startsWith(prefix)) return;
    const event = JSON.parse(message.text().slice(prefix.length)) as RendererStallEvent;
    if (!["start", "end"].includes(event.phase) || !["window", "worker"].includes(event.realm)
      || !Number.isFinite(event.timeOrigin) || !Number.isFinite(event.now)) throw new Error("Invalid renderer stall marker");
    // Chromium also forwards Worker console through Page; retain one observation.
    if (!events.some(previous => previous.phase === event.phase && previous.realm === event.realm && previous.timeOrigin === event.timeOrigin && previous.now === event.now)) {
      events.push({ ...event, hostObservedAt: Date.now() });
    }
  };
  page.on("console", observe);
  const renderer = page.workers().find(worker => /\/three-table-worker(?:-[\w-]+)?\.js(?:\?.*)?$/.test(worker.url()));
  if (renderer) {
    renderer.on("console", observe);
    await renderer.evaluate(installDrawStall, prefix);
  } else {
    await page.evaluate(installDrawStall, prefix);
  }
  return events;
}

export async function armRendererStall(page: Page, prefix: string) {
  await page.evaluate(prefix => {
    const renderer = (window as unknown as Record<string, Worker>)[prefix];
    if (renderer) renderer.postMessage({ type: prefix });
    else window.dispatchEvent(new MessageEvent("message", { data: { type: prefix } }));
  }, prefix);
}

export async function reconnectDuringRendererStall(page: Page, socketKey: string) {
  await page.evaluate(socketKey => {
    const scope = window as unknown as Record<string, unknown>;
    const socket = scope[socketKey] as WebSocket | undefined;
    if (!socket || socket.readyState !== WebSocket.OPEN) throw new Error("Missing owned open Human socket");
    socket.close();
  }, socketKey);
}
