/// <reference types="../node_modules/@types/node" />

import { expect, test as base, type BrowserContext, type Page } from "@playwright/test";
import { resolve } from "node:path";
import { closeSync, fsyncSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import { performance, monitorEventLoopDelay, type ELDHistogram, type EventLoopUtilization } from "node:perf_hooks";
import { setTimeout as wait } from "node:timers/promises";
import { startRealServer, type RealServerHarness } from "./support/real-server";

const evidenceRoot = resolve(process.cwd(), "../.superpowers/sdd/2026-10-02-authoritative-kyoku-results/task-8-native1/completion-native1/browser");
base.describe.configure({ mode: "serial", timeout: 240_000 });
// Real finite-clock gameplay uses the platform renderer, as the real-server config does.
base.use({ launchOptions: { args: [] } });

type Snapshot = { type: string; room: { phase: string; roster: Array<{ participant_id: string; seat: number }>; kyoku_end: null | { id: { match_id: string; kyoku_sequence: number; step_index: number }; content: Record<string, unknown>; required: string[]; confirmed: string[]; published_at_ms: number; deadline_at_ms: number; remaining_ms: number }; result: unknown }; state: null | { viewer_seat: number; players: Array<{ seat: number; hand?: number[]; score: number }>; decision: null | { decision_id: string; actions: Array<{ action_id: string; action: unknown }> }; kyoku_results?: { wins: Array<{ event_index: number; seat: number; concealed: number[]; winning_tile: number; winning_tile_index: number; win_points: number; evaluation: unknown }>; settlement: unknown } } };
type ObservedSnapshot = Snapshot & { socket_generation: number; socket_created_host_at_ms: number; received_at_ms: number; message_index: number };
type Human = { context: BrowserContext; page: Page; id: string; name: string; socketKey: string; messages: string[]; sent: string[]; sentAt: number[]; snapshots: ObservedSnapshot[] };
type ResultStep = NonNullable<Snapshot["room"]["kyoku_end"]>;
type ReconnectUiSample = { epoch_ms: number; status: string; phase: string | null; surface: "win" | "settlement" | "standings" | "table" | null; standings_count: number; step: ResultStep | null; labels: string[] };

function reconnectUiHasValidPublication(sample: ReconnectUiSample, first: ResultStep, ownId: string, received: Snapshot["room"][]): boolean {
  const step = sample.step;
  if (!step) return true;
  if (sample.standings_count !== 0 || step.id.match_id !== first.id.match_id || step.id.kyoku_sequence !== first.id.kyoku_sequence
    || step.deadline_at_ms - step.published_at_ms !== 5000
    || !received.some(room => room.phase === sample.phase && isDeepStrictEqual(room.kyoku_end, step))) return false;
  if (step.id.step_index === 0) return step.published_at_ms === first.published_at_ms
    && step.deadline_at_ms === first.deadline_at_ms && !step.required.includes(ownId);
  return [1, 2].includes(step.id.step_index) && step.published_at_ms >= first.published_at_ms && step.required.includes(ownId);
}

function reconnectUiMatchesCurrentPublication(sample: ReconnectUiSample, first: ResultStep, ownId: string, received: Snapshot["room"][]): boolean {
  if (sample.status !== "connected" || !reconnectUiHasValidPublication(sample, first, ownId, received)) return false;
  const step = sample.step;
  if (!step) return received.some(room => room.phase === sample.phase && room.kyoku_end === null)
    && sample.labels.length === 0 && (sample.phase === "post_match" ? sample.surface === "standings" && sample.standings_count === 1 : sample.phase === "playing" && sample.surface === "table" && sample.standings_count === 0);
  if (sample.epoch_ms >= step.deadline_at_ms || step.remaining_ms <= 0) return false;
  const action = !step.required.includes(ownId) ? "Dismiss" : step.confirmed.includes(ownId) ? "Waiting for players" : "Continue";
  return step.id.step_index === 2
    ? sample.surface === "settlement" && isDeepStrictEqual(sample.labels, ["Pause for review", "Skip animation", action])
    : sample.surface === "win" && isDeepStrictEqual(sample.labels, [action]);
}

function checkReconnectUiMatchesCurrentPublication() {
  const first: ResultStep = { id: { match_id: "oracle-match", kyoku_sequence: 7, step_index: 0 }, content: {}, required: ["other"], confirmed: [], published_at_ms: 1000, deadline_at_ms: 6000, remaining_ms: 4000 };
  const step0: ReconnectUiSample = { epoch_ms: 2000, status: "connected", phase: "playing", surface: "win", standings_count: 0, step: first, labels: ["Dismiss"] };
  const step1: ReconnectUiSample = { ...step0, epoch_ms: 6002, step: { ...first, id: { ...first.id, step_index: 1 }, published_at_ms: 6001, deadline_at_ms: 11001, required: ["own", "other"] }, labels: ["Continue"] };
  const step2: ReconnectUiSample = { ...step1, epoch_ms: 11002, surface: "settlement", step: { ...step1.step!, id: { ...first.id, step_index: 2 }, published_at_ms: 11001, deadline_at_ms: 16001 }, labels: ["Pause for review", "Skip animation", "Continue"] };
  const received: Snapshot["room"][] = [step0, step1, step2].map(sample => ({ phase: "playing", roster: [], kyoku_end: sample.step, result: null }));
  received.push({ phase: "post_match", roster: [], kyoku_end: null, result: {} });
  const matches = (sample: ReconnectUiSample) => reconnectUiMatchesCurrentPublication(sample, first, "own", received);
  const valid = (sample: ReconnectUiSample) => reconnectUiHasValidPublication(sample, first, "own", received);
  for (const sample of [step0, step1, step2]) expect(matches(sample)).toBe(true);
  expect(matches({ ...step2, epoch_ms: 20000, phase: "post_match", surface: "standings", standings_count: 1, step: null, labels: [] })).toBe(true);
  expect(matches({ ...step0, step: null, surface: "table", labels: [] }), "missing publication cannot impersonate a ready table").toBe(false);
  received.push({ phase: "playing", roster: [], kyoku_end: null, result: null });
  expect(matches({ ...step2, epoch_ms: 20000, surface: "table", step: null, labels: [] })).toBe(true);
  expect(valid({ ...step0, labels: [], status: "connecting" })).toBe(true);
  for (const bad of [
    { ...step0, step: { ...first, id: { ...first.id, match_id: "foreign" } } },
    { ...step0, step: { ...first, id: { ...first.id, kyoku_sequence: 8 } } },
    { ...step0, step: { ...first, id: { ...first.id, step_index: 2 } } },
    { ...step1, step: { ...step1.step!, published_at_ms: 5999, deadline_at_ms: 10999 } },
    { ...step0, step: { ...first, published_at_ms: 1001, deadline_at_ms: 6001 } },
    { ...step0, step: { ...first, required: ["own"] } },
    { ...step1, step: { ...step1.step!, required: ["other"] } },
  ]) expect([bad, step1].every(valid), "later readiness cannot erase earlier invalid publication").toBe(false);
  for (const bad of [
    { ...step0, step: { ...first, id: { ...first.id, match_id: "foreign" } } },
    { ...step0, step: { ...first, id: { ...first.id, kyoku_sequence: 8 } } },
    { ...step0, step: { ...first, id: { ...first.id, step_index: 2 } } },
    { ...step1, step: { ...step1.step!, published_at_ms: 5999, deadline_at_ms: 10999 } },
    { ...step0, step: { ...first, published_at_ms: 1001, deadline_at_ms: 6001 } },
    { ...step0, step: { ...first, required: ["own"] } },
    { ...step1, step: { ...step1.step!, required: ["other"] } },
    { ...step0, labels: ["Continue"] }, { ...step1, labels: ["Dismiss"] },
    { ...step0, epoch_ms: 6000 }, { ...step1, status: "disconnected" },
    { ...step2, labels: ["Continue"] }, { ...step2, epoch_ms: 16001 },
    { ...step2, surface: "win" as const }, { ...step2, step: { ...step2.step!, content: { fabricated: true } } },
    { ...step2, step: { ...step2.step!, published_at_ms: 11002, deadline_at_ms: 16002 } },
    { ...step2, phase: "post_match", surface: "settlement" as const, step: null, labels: [] },
    { ...step2, standings_count: 1 },
  ]) expect(matches(bad)).toBe(false);
}

base("reconnect UI oracle permits an authoritative settlement after slow rendering", () => {
  const first: ResultStep = { id: { match_id: "oracle-match", kyoku_sequence: 7, step_index: 0 }, content: {}, required: ["other"], confirmed: [], published_at_ms: 1000, deadline_at_ms: 6000, remaining_ms: 4000 };
  const settlement: ReconnectUiSample = { epoch_ms: 11002, status: "connected", phase: "playing", surface: "settlement", standings_count: 0, step: { ...first, id: { ...first.id, step_index: 2 }, published_at_ms: 11001, deadline_at_ms: 16001, required: ["own", "other"] }, labels: ["Pause for review", "Skip animation", "Continue"] };
  const received: Snapshot["room"][] = [{ phase: "playing", roster: [], kyoku_end: settlement.step, result: null }];
  expect(reconnectUiHasValidPublication(settlement, first, "own", received)).toBe(true);
  expect(reconnectUiMatchesCurrentPublication(settlement, first, "own", received)).toBe(true);
  checkReconnectUiMatchesCurrentPublication();
});

base("expired result confirmation needs no UI readiness", async ({ page, context }) => {
  const human: Human = { context, page, id: "own", name: "Own", socketKey: "", messages: [], sent: [], sentAt: [], snapshots: [] };
  const expired: ResultStep = { id: { match_id: "oracle-match", kyoku_sequence: 7, step_index: 0 }, content: {}, required: ["own"], confirmed: [], published_at_ms: 0, deadline_at_ms: 0, remaining_ms: 0 };
  expect(await confirmStep([human], expired)).toEqual(["own:expired"]);
});

base("verified build refuses omitted producer inputs", async () => {
  const root = resolve(evidenceRoot, `proof-control-${randomUUID()}`);
  mkdirSync(root, { recursive: true });
  const snapshot = resolve(root, "source.json");
  const proofPath = resolve(root, "proof.json");
  writeFileSync(snapshot, "{}");
  writeFileSync(proofPath, JSON.stringify({ sourceBefore: snapshot, sourceAfter: snapshot, buildCommand: snapshot, binaryReceipt: snapshot }));
  const previous = process.env.DRIICHI_TASK8_VERIFIED_BUILD;
  process.env.DRIICHI_TASK8_VERIFIED_BUILD = proofPath;
  try {
    await expect(startRealServer({ task8: { fixture: "double-ron", evidenceRoot: root } })).rejects.toThrow("source coverage/custody mismatch");
  } finally {
    if (previous === undefined) delete process.env.DRIICHI_TASK8_VERIFIED_BUILD;
    else process.env.DRIICHI_TASK8_VERIFIED_BUILD = previous;
    rmSync(root, { recursive: true, force: true });
  }
});

function findFreshMatchStarted(messages: string[], cutoff: number, oldMatchId: string): { message_index: number; match_id: string } | null {
  for (let index = cutoff; index < messages.length; index++) {
    const frame = JSON.parse(messages[index]) as Record<string, unknown>;
    if (frame.type !== "room_update" || !frame.event || typeof frame.event !== "object" || Array.isArray(frame.event)) continue;
    const event = frame.event as Record<string, unknown>;
    if (event.type !== "match_started") continue;
    if (Object.keys(frame).sort().join() !== "event,room,state,type" || Object.keys(event).sort().join() !== "match_id,type"
      || typeof event.match_id !== "string" || !event.match_id.trim() || event.match_id === oldMatchId) continue;
    const room = frame.room as Snapshot["room"] | null;
    const state = frame.state as Snapshot["state"];
    if (!room || room.phase !== "playing" || !state || !state.decision || typeof state.decision.decision_id !== "string"
      || !state.decision.decision_id || !Array.isArray(state.decision.actions)) continue;
    return { message_index: index, match_id: event.match_id };
  }
  return null;
}

function checkFindFreshMatchStarted() {
  const valid = { type: "room_update", event: { type: "match_started", match_id: "new-match" }, room: { phase: "playing" }, state: { decision: { decision_id: "actual-decision", actions: [] } } };
  const text = JSON.stringify(valid);
  expect(findFreshMatchStarted([text], 0, "old-match")).toEqual({ message_index: 0, match_id: "new-match" });
  expect(findFreshMatchStarted([text], 1, "old-match")).toBeNull();
  for (const bad of [
    { ...valid, event: { type: "match_started", match_id: "old-match" } },
    { ...valid, event: { type: "match_started" } },
    { ...valid, event: { type: "match_started", match_id: " " } },
    { ...valid, type: "game_update" }, { ...valid, extra: true },
    { ...valid, event: { ...valid.event, extra: true } },
    { ...valid, event: { type: "phase_changed", phase: "playing" } },
    { ...valid, state: { decision: null } }, { ...valid, room: { phase: "post_match" } },
  ]) expect(findFreshMatchStarted([JSON.stringify(bad)], 0, "old-match")).toBeNull();
}

type ReconnectPhase = { host_epoch_ms: number; host_monotonic_ms: number; operation_id: number; phase: string; details: unknown };

function recordReconnectPhase(events: ReconnectPhase[], inFlight: Set<number>, operationId: number, phase: string, details: unknown = null) {
  events.push({ host_epoch_ms: Date.now(), host_monotonic_ms: performance.now(), operation_id: operationId, phase, details });
  if (phase === "host_issue") {
    if (events.slice(0, -1).some(event => event.operation_id === operationId)) throw new Error("duplicate reconnect operation identity");
    inFlight.add(operationId);
  } else {
    if (!events.some(event => event.operation_id === operationId && event.phase === "host_issue")) throw new Error("unissued reconnect operation identity");
    if (phase === "host_completed" || phase === "host_error") {
      if (!inFlight.delete(operationId)) throw new Error("reconnect operation already settled");
    }
  }
}

async function observeReconnectEvaluation<T>(events: ReconnectPhase[], inFlight: Set<number>, operationId: number, evaluate: () => Promise<T>): Promise<T> {
  recordReconnectPhase(events, inFlight, operationId, "host_issue");
  try {
    const sample = await evaluate();
    recordReconnectPhase(events, inFlight, operationId, "host_completed");
    return sample;
  } catch (error) {
    recordReconnectPhase(events, inFlight, operationId, "host_error", error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error));
    throw error;
  }
}

async function checkObserveReconnectEvaluation() {
  const events: ReconnectPhase[] = []; const inFlight = new Set<number>();
  await observeReconnectEvaluation(events, inFlight, 1, async () => { recordReconnectPhase(events, inFlight, 1, "browser_start", { browser_epoch_ms: 1 }); return true; });
  const error = new Error("recorder rejection control");
  await expect(observeReconnectEvaluation(events, inFlight, 2, async () => { recordReconnectPhase(events, inFlight, 2, "browser_start"); throw error; })).rejects.toBe(error);
  expect(events.map(event => event.phase)).toEqual(["host_issue", "browser_start", "host_completed", "host_issue", "browser_start", "host_error"]);
  expect(inFlight.size).toBe(0);
  expect(() => recordReconnectPhase(events, inFlight, 3, "browser_start")).toThrow("unissued reconnect operation identity");
  expect(events.at(-1)?.operation_id).toBe(3);
}

function reconnectApiEvent(name: string, severity: string, message: unknown): "started" | "succeeded" | "failed" | null {
  if (name !== "api" || severity !== "info") return null;
  if (message === "=> page.evaluate started") return "started";
  if (message === "<= page.evaluate succeeded") return "succeeded";
  if (message === "<= page.evaluate failed") return "failed";
  return null;
}

function reconnectDelayStats(histogram: Pick<ELDHistogram, "count" | "min" | "max" | "mean" | "exceeds" | "percentile">) {
  const sampled = histogram.count > 0;
  const finite = (value: number) => sampled && Number.isFinite(value) && value >= 0 ? value : null;
  return { count: histogram.count, unit: "ns", min: finite(histogram.min), max: finite(histogram.max), mean: finite(histogram.mean), p50: sampled ? finite(histogram.percentile(50)) : null, p99: sampled ? finite(histogram.percentile(99)) : null, exceeds: histogram.exceeds };
}

function reconnectUtilization(value: EventLoopUtilization) {
  if (![value.active, value.idle, value.utilization].every(Number.isFinite) || value.active < 0 || value.idle < 0 || value.utilization < 0 || value.utilization > 1) throw new Error("invalid reconnect event loop utilization");
  return { active_ms: value.active, idle_ms: value.idle, utilization: value.utilization };
}

function checkReconnectSchedulingProjection() {
  for (const [message, event] of [["=> page.evaluate started", "started"], ["<= page.evaluate succeeded", "succeeded"], ["<= page.evaluate failed", "failed"]] as const) expect(reconnectApiEvent("api", "info", message)).toBe(event);
  for (const [name, severity, message] of [["protocol", "info", "=> page.evaluate started"], ["api", "verbose", "=> page.evaluate started"], ["api", "info", "Authorization: secret body=private"], ["api", "info", new Error("secret")]] as const) expect(reconnectApiEvent(name, severity, message)).toBeNull();
  const none = reconnectDelayStats({ count: 0, min: 9223372036854776000, max: 0, mean: NaN, exceeds: 0, percentile: () => { throw new Error("no sample percentile queried"); } });
  expect(none).toEqual({ count: 0, unit: "ns", min: null, max: null, mean: null, p50: null, p99: null, exceeds: 0 });
  expect(reconnectDelayStats({ count: 1, min: NaN, max: Infinity, mean: -1, exceeds: 0, percentile: () => NaN })).toMatchObject({ min: null, max: null, mean: null, p50: null, p99: null });
  expect(reconnectUtilization({ active: 1, idle: 2, utilization: 1 / 3 })).toEqual({ active_ms: 1, idle_ms: 2, utilization: 1 / 3 });
  expect(() => reconnectUtilization({ active: -1, idle: 0, utilization: 0 })).toThrow("invalid reconnect event loop utilization");
}

type BrowserTaskSample = { kind: "init" | "longtask"; time_origin_ms: number; start_time_ms: number; duration_ms: number | null; supported: boolean };
function browserTaskSample(value: unknown): BrowserTaskSample {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid browser task metadata");
  const v = value as BrowserTaskSample;
  if (Object.keys(v).sort().join() !== "duration_ms,kind,start_time_ms,supported,time_origin_ms"
    || ![v.time_origin_ms, v.start_time_ms].every(n => typeof n === "number" && Number.isFinite(n) && n >= 0) || typeof v.supported !== "boolean"
    || !(v.kind === "init" ? v.duration_ms === null : v.kind === "longtask" && v.supported && typeof v.duration_ms === "number" && Number.isFinite(v.duration_ms) && v.duration_ms > 0)) throw new Error("invalid browser task metadata");
  return v;
}
function checkBrowserTaskSample() {
  const valid = { kind: "longtask", time_origin_ms: 1000, start_time_ms: 2, duration_ms: 51, supported: true };
  expect(browserTaskSample(valid)).toEqual(valid);
  expect(browserTaskSample({ ...valid, kind: "init", duration_ms: null, supported: false }).supported).toBe(false);
  for (const invalid of [null, { ...valid, url: "private" }, { ...valid, kind: "secret" }, { ...valid, duration_ms: -1 }, { ...valid, duration_ms: null }, { ...valid, time_origin_ms: NaN }, { ...valid, supported: false }, { ...valid, kind: "init" }]) expect(() => browserTaskSample(invalid)).toThrow("invalid browser task metadata");
}

async function join(context: BrowserContext, harness: RealServerHarness, code: string, name: string): Promise<Human> {
  const page = await context.newPage();
  const socketKey = `task8-owned-human-socket:${randomUUID()}`;
  await page.addInitScript(key => {
    const NativeWebSocket = window.WebSocket;
    window.WebSocket = class extends NativeWebSocket {
      constructor(...args: ConstructorParameters<typeof WebSocket>) {
        super(...args);
        this.addEventListener("message", event => {
          if (typeof event.data !== "string") return;
          try {
            const frame = JSON.parse(event.data);
            if (frame?.type === "snapshot" && frame.room && frame.state !== undefined) {
              (window as unknown as Record<string, WebSocket>)[key] = this;
            }
          } catch { /* Non-Human/HMR frames are not owned by this probe. */ }
        });
      }
    };
  }, socketKey);
  const messages: string[] = [];
  const sent: string[] = [];
  const sentAt: number[] = [];
  const snapshots: ObservedSnapshot[] = [];
  let socketGeneration = 0;
  page.on("websocket", socket => {
    const generation = ++socketGeneration;
    const socketCreatedHostAt = Date.now();
    socket.on("framesent", ({ payload }) => { sent.push(payload.toString()); sentAt.push(Date.now()); });
    socket.on("framereceived", ({ payload }) => {
    const receivedAt = Date.now();
    const text = payload.toString();
    messages.push(text);
    const value = JSON.parse(text) as Partial<Snapshot>;
    // HumanSocket client consumes room_update/game_update as well as snapshot.
    // Keep the raw bytes and join only fields actually supplied by the server.
    const room = value.room ?? snapshots.at(-1)?.room;
    if (room && value.state !== undefined) snapshots.push({ type: value.type ?? "", room, state: value.state, socket_generation: generation, socket_created_host_at_ms: socketCreatedHostAt, received_at_ms: receivedAt, message_index: messages.length - 1 });
    });
  });
  await page.goto(`${harness.frontendOrigin}/room/${code}`);
  await page.getByLabel("Display name").fill(name);
  await page.locator("label.character-option").filter({ hasText: "Player Red" }).locator("input[type=radio]").check();
  await page.getByRole("button", { name: /^join room$/i }).click();
  await page.getByRole("link", { name: /enter lobby/i }).click();
  await expect(page.locator(".connection-state")).toHaveText("connected");
  const id = await page.evaluate(code => sessionStorage.getItem(`driichi:participant:${code}`), code);
  expect(id).toBeTruthy();
  return { context, page, id: id!, name, socketKey, messages, sent, sentAt, snapshots };
}

function latest(human: Human): ObservedSnapshot {
  const snapshot = human.snapshots.at(-1);
  if (!snapshot) throw new Error("missing actual Human WebSocket snapshot");
  return snapshot;
}

async function choose(human: Human, predicate: (action: unknown) => boolean) {
  const decision = latest(human).state?.decision;
  expect(decision, "actual current offered Decision").toBeTruthy();
  const offer = decision!.actions.find(offer => predicate(offer.action));
  expect(offer, "actual engine offered action").toBeTruthy();
  const before = human.messages.length;
  await human.page.locator(`[data-action-id="${offer!.action_id}"]`).first().click();
  await expect.poll(() => human.messages.slice(before).some(text => {
    const response = JSON.parse(text) as { type?: string; action_id?: string; status?: string };
    return response.type === "action_result" && response.action_id === offer!.action_id && response.status === "accepted";
  })).toBe(true);
}

async function confirmStep(humans: Human[], step: ResultStep): Promise<string[]> {
  const required = humans.filter(h => step.required.includes(h.id));
  // A server-expired publication cannot be confirmed, regardless of UI readiness.
  if (Date.now() >= step.deadline_at_ms) return required.map(h => `${h.id}:expired`);
  const baselines = new Map(required.map(h => [h.id, { sent: h.sent.length, snapshots: h.snapshots.length }]));
  const ownAck = (h: Human) => h.snapshots.slice(baselines.get(h.id)!.snapshots).some(s => {
    const ack = s.room.kyoku_end;
    return ack && JSON.stringify(ack.id) === JSON.stringify(step.id) && ack.confirmed.includes(h.id);
  });
  const sentExpected = (h: Human) => h.sent.slice(baselines.get(h.id)!.sent).some(t => {
    const m = JSON.parse(t) as { type?: string; step_id?: ResultStep["id"] };
    return m.type === "confirm_result" && JSON.stringify(m.step_id) === JSON.stringify(step.id);
  });
  return Promise.all(required.map(async human => {
    const sentBefore = baselines.get(human.id)!.sent;
    const snapshotsBefore = baselines.get(human.id)!.snapshots;
    const observe = `(async () => {
      const { useGameStore } = await import('/src/game/store.ts');
      const state = useGameStore.getState();
      const expected = ${JSON.stringify(step.id)};
      const current = state.room?.kyoku_end;
      const same = current && current.id.match_id === expected.match_id && current.id.kyoku_sequence === expected.kyoku_sequence && current.id.step_index === expected.step_index;
      const expired = Date.now() >= ${step.deadline_at_ms};
      const button = document.querySelector('.round-win-overlay > .round-win-actions > .round-win-continue');
      const rect = button?.getBoundingClientRect();
      const hit = rect && document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
      const visible = button?.isConnected && rect?.width > 0 && rect?.height > 0 && getComputedStyle(button).visibility === 'visible' && (hit === button || button.contains(hit));
      return { state, current, same, expired, button, visible };
    })()`;
    await expect.poll(() => human.page.evaluate(`(async () => {
      const { state, current, same, expired, button, visible } = await ${observe};
      return expired || (!same && current?.id.match_id === ${JSON.stringify(step.id.match_id)} && current?.id.kyoku_sequence === ${step.id.kyoku_sequence} && current?.id.step_index > ${step.id.step_index}) || (same && (current.confirmed.includes(${JSON.stringify(human.id)}) ||
        (state.status === 'connected' && current.required.includes(${JSON.stringify(human.id)}) && visible && !button.disabled)));
    })()`), { timeout: 30_000 }).toBe(true);
    const outcome = await human.page.evaluate(`(async () => {
      const { state, current, same, expired, button, visible } = await ${observe};
      if (expired) return 'expired';
      if (!same) return 'advanced';
      if (current.confirmed.includes(${JSON.stringify(human.id)})) return 'confirmed';
      if (state.status !== 'connected' || !current.required.includes(${JSON.stringify(human.id)}) || !visible || button.disabled) return 'not-ready';
      button.click();
      return 'clicked';
    })()`) as string;
    expect(outcome).not.toBe("not-ready");
    if (outcome === "clicked") await expect.poll(() => human.sent.length > sentBefore).toBe(true);
    const confirmations = human.sent.slice(sentBefore).map(t => JSON.parse(t) as { type?: string; step_id?: ResultStep["id"] }).filter(m => m.type === "confirm_result");
    expect(confirmations.map(m => m.step_id)).toEqual(outcome === "clicked" ? [step.id] : []);
    if (outcome === "clicked") {
      // Concurrent confirmations can coalesce both replies into the early next publication.
      await expect.poll(() => ownAck(human) || (
        required.every(sentExpected) &&
        human.snapshots.slice(snapshotsBefore).some(s => {
          if (s.received_at_ms < human.sentAt[sentBefore]) return false;
          const next = s.room.kyoku_end;
          return next
            ? next.id.match_id === step.id.match_id && next.id.kyoku_sequence === step.id.kyoku_sequence && next.id.step_index === step.id.step_index + 1 &&
              (next.published_at_ms < step.deadline_at_ms || required.filter(h => h !== human).every(ownAck))
            : step.id.step_index === 2 && s.room.phase === "post_match" && s.room.result != null &&
              (s.received_at_ms < step.deadline_at_ms || required.filter(h => h !== human).every(ownAck));
        })
      )).toBe(true);
    }
    return `${human.id}:${outcome}`;
  }));
}

function retainResultGeometryBounds<T extends { width: number; height: number; dpr: number; rect: { width: number; height: number } }>(measured: T, observations: unknown[], viewport: { width: number; height: number }, dpr: number) {
  observations.push(measured);
  expect([measured.width, measured.height, measured.dpr]).toEqual([viewport.width, viewport.height, dpr]);
  expect(measured.rect.width).toBeGreaterThan(0); expect(measured.rect.height).toBeGreaterThan(0);
}

function checkRetainResultGeometryBounds() {
  for (const control of [{ elementMissing: false, elementConnectedAfterImport: true }, { elementMissing: true, elementConnectedAfterImport: false }, { elementMissing: false, elementConnectedAfterImport: false }]) {
    const observations: unknown[] = [];
    const bad = { ...control, width: 1920, height: 1080, dpr: 1, rect: { width: 0, height: 0 } };
    expect(() => retainResultGeometryBounds(bad, observations, { width: 1920, height: 1080 }, 1)).toThrow();
    expect(observations).toEqual([bad]);
  }
}

async function resultGeometry(page: Page, surface: "win" | "standings", viewport: { width: number; height: number }, dpr: number, liveStep?: ResultStep, observations: unknown[] = [], liveHuman?: Human, humans: Human[] = []) {
  const selector = surface === "win" ? ".round-win-surface" : ".results-panel";
  await expect(page.locator(selector)).toBeVisible();
  const measured = await page.evaluate(async ({ selector, liveStep }) => {
    const importStartedAtMs = Date.now();
    const storePath = "/src/game/store.ts";
    const store = liveStep ? await import(storePath) : null;
    const importEndedAtMs = Date.now();
    const clock = store?.useGameStore.getState().room?.kyoku_end ?? null;
    const element = document.querySelector<HTMLElement>(selector)!;
    const selectedAtMs = Date.now();
    const selectedIdentity = { tag: element.tagName, id: element.id, className: element.className, testId: element.dataset.testid ?? null };
    const rect = (node: Element) => { const r = node.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height }; };
    const buttons = [...document.querySelectorAll<HTMLButtonElement>(".round-win-actions button, .results-actions button")].map(button => {
      const r = button.getBoundingClientRect(); const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return { label: button.textContent, disabled: button.disabled, rect: rect(button), uncovered: hit === button || Boolean(hit && button.contains(hit)) };
    });
    return { selectedAtMs, selectedIdentity, elementMissing: false, elementConnectedBeforeImport: null, elementConnectedAfterImport: element.isConnected, importStartedAtMs, importEndedAtMs, initialStep: liveStep ?? null, pageUrl: location.href, selector, samplingPhases: ["store-import", "synchronous-store-element-buttons-and-rect"], epochMs: Date.now(), width: innerWidth, height: innerHeight, dpr: devicePixelRatio, rect: rect(element), text: element.textContent, winnerName: element.querySelector("#round-win-heading")?.textContent ?? null, label: element.getAttribute("aria-labelledby"), tiles: element.querySelectorAll(".round-win-tile-row img").length, standings: element.querySelectorAll(".results-list li").length, buttons, clock: clock ?? null, conflictingSurfaceCount: document.querySelectorAll(selector === ".round-win-surface" ? ".results-panel" : ".round-win-surface").length, fallbackLabels: [...element.querySelectorAll(".asset-fallback")].map(e => e.getAttribute("aria-label")) };
  }, { selector, liveStep });
  retainResultGeometryBounds(measured, observations, viewport, dpr);
  expect(measured.rect.left).toBeGreaterThanOrEqual(0); expect(measured.rect.top).toBeGreaterThanOrEqual(0);
  expect(measured.rect.right).toBeLessThanOrEqual(viewport.width); expect(measured.rect.bottom).toBeLessThanOrEqual(viewport.height);
  expect(measured.label).toBeTruthy(); expect(measured.conflictingSurfaceCount).toBe(0);
  if (surface === "win") { expect(measured.text).toContain("2,600"); expect(measured.tiles).toBe(14); }
  else { expect(measured.text).toContain("Final standings"); expect(measured.standings).toBeGreaterThanOrEqual(3); }
  for (const button of measured.buttons) { expect(button.label).toBeTruthy(); expect(button.uncovered).toBe(true); }
  for (const label of measured.fallbackLabels) expect(label).toMatch(/portrait unavailable/i);
  if (liveStep) {
    const actual = liveHuman?.snapshots.find(s => s.room.kyoku_end && isDeepStrictEqual(s.room.kyoku_end, measured.clock))?.room.kyoku_end;
    expect(actual, "Live DOM clock must be an actual received publication").toBeTruthy();
    expect(actual!.id.match_id).toBe(liveStep.id.match_id);
    expect(actual!.id.kyoku_sequence).toBe(liveStep.id.kyoku_sequence);
    expect([0, 1]).toContain(actual!.id.step_index);
    expect(actual!.content).toHaveProperty("Win");
    expect(actual!.deadline_at_ms - actual!.published_at_ms).toBe(5000);
    expect(measured.epochMs).toBeLessThan(actual!.deadline_at_ms);
    const winner = (actual!.content.Win as { winner: string }).winner;
    const human = humans.find(h => h.id === winner);
    expect(human, "published winner must belong to this actual Match").toBeTruthy();
    expect(measured.winnerName).toBe(human!.name);
  }
  return measured;
}

function remainingPreCaseRateWindow(previousCompletion: number | null, now: number): number {
  if (!Number.isFinite(now) || now < 0 || (previousCompletion !== null && (!Number.isFinite(previousCompletion) || previousCompletion < 0 || now < previousCompletion))) throw new Error("invalid pre-case monotonic observation");
  return previousCompletion === null ? 0 : Math.max(0, 61_000 - (now - previousCompletion));
}

function checkRemainingPreCaseRateWindow() {
  expect(remainingPreCaseRateWindow(null, 10)).toBe(0);
  expect(remainingPreCaseRateWindow(1000, 2000)).toBe(60_000);
  expect(remainingPreCaseRateWindow(1000, 62_000)).toBe(0);
  expect(remainingPreCaseRateWindow(1000, 63_000)).toBe(0);
  expect(remainingPreCaseRateWindow(1000, 61_999.5)).toBe(0.5);
  expect(Math.ceil(remainingPreCaseRateWindow(1000, 61_999.5))).toBe(1);
  expect(remainingPreCaseRateWindow(1000, 62_000)).toBe(0);
  expect(() => remainingPreCaseRateWindow(2000, 1000)).toThrow();
}

for (const fixture of ["double-ron", "double-ron-final"] as const) {
  let previousCompletedCase: number | null = null;
  const test = base.extend<{}, { harness: RealServerHarness }>({
    harness: [async ({}, use) => {
      const harness = await startRealServer({ task8: { fixture, evidenceRoot } });
      try { await use(harness); } finally { await harness.stop(); }
    }, { scope: "worker", timeout: 720_000 }],
  });
  test.describe(fixture, () => {
for (const [mode, count, ordinaryAfter] of [
  ["3p-red-east", 3, [94400, 105000, 102600]],
  ["4p-red-east", 4, [94200, 105200, 102600, 100000]],
] as const) {
  const cases = fixture === "double-ron-final" ? [
    { width: 1024, height: 600, dpr: 1 }, { width: 1280, height: 720, dpr: 1 },
    { width: 1600, height: 900, dpr: 1 }, { width: 1920, height: 1080, dpr: 1 },
    { width: 1024, height: 600, dpr: 2 },
  ] : [{ width: 1280, height: 720, dpr: 1 }];
  for (const viewport of cases) {
  test(`real_match_result_capture_publication_save_and_reconnect ${mode} double Ron ${viewport.width}x${viewport.height} DPR${viewport.dpr}`, async ({ browser, harness }, testInfo) => {
    const finalFixture = fixture === "double-ron-final";
    const beforeScores = Array<number>(count).fill(100000);
    if (finalFixture) beforeScores[0] = 1000;
    const after: number[] = [...ordinaryAfter];
    if (finalFixture) after[0] = count === 3 ? -4600 : -4800;
    const admin = await browser.newContext({ viewport, deviceScaleFactor: viewport.dpr, extraHTTPHeaders: { Origin: harness.frontendOrigin } });
    const humans: Human[] = [];
    const geometry: unknown[] = [];
    const geometryObservations: unknown[] = [];
    checkRetainResultGeometryBounds();
    const reconnectUiSamples: ReconnectUiSample[] = [];
    const reconnectPhases: ReconnectPhase[] = [];
    const reconnectInFlight = new Set<number>();
    const reconnectObserverErrors: unknown[] = [];
    const reconnectPrefix = `task8-reconnect-phase:${randomUUID()}:`;
    let reconnectOperationId = 0;
    let schedulingDealerJoinIndex: number | null = null;
    let schedulingLoggerOwned = false;
    let schedulingHistogram: ELDHistogram | null = null;
    let schedulingHistogramEnabled = false;
    let schedulingPollElu: EventLoopUtilization | null = null;
    const reconnectScheduling = {
      resolution_ms: 20, histogram_scope: `cumulative ${finalFixture ? "transport reconnect" : "reload"} plus poll; extrema not localized to poll`,
      enabled_at_ms: null as number | null, disabled_at_ms: null as number | null,
      api_events: [] as Array<{ host_epoch_ms: number; host_monotonic_ms: number; operation_id: number; event: "started" | "succeeded" | "failed" }>,
      snapshots: [] as Array<{ label: string; host_epoch_ms: number; host_monotonic_ms: number; enabled: boolean; histogram: ReturnType<typeof reconnectDelayStats> }>,
      poll_utilization: null as ReturnType<typeof reconnectUtilization> | null,
      browser_tasks: { installed_before_result_at_ms: null as number | null, scope: "selected dealer top document; callbacks are asynchronous, absent entries/attribution/cause UNKNOWN", limit: 4096, overflow_count: 0, error_count: 0,
        events: [] as Array<{ host_epoch_ms: number; data: BrowserTaskSample }> },
    };
    const schedulingSnapshot = (label: string) => {
      if (schedulingHistogram) reconnectScheduling.snapshots.push({ label, host_epoch_ms: Date.now(), host_monotonic_ms: performance.now(), enabled: schedulingHistogramEnabled, histogram: reconnectDelayStats(schedulingHistogram) });
    };
    const disableSchedulingHistogram = () => {
      if (schedulingHistogramEnabled) { schedulingHistogram!.disable(); schedulingHistogramEnabled = false; reconnectScheduling.disabled_at_ms = Date.now(); }
    };
    const reconnectPoll: { start_epoch_ms?: number; start_monotonic_ms?: number; end_epoch_ms?: number; end_monotonic_ms?: number; error?: unknown; in_flight_at_end?: number[] } = {};
    let faultCutoff: { epoch_ms: number; phases_count: number; samples_count: number; in_flight: number[] } | null = null;
    checkReconnectUiMatchesCurrentPublication();
    checkFindFreshMatchStarted();
    checkRemainingPreCaseRateWindow();
    const preCasePacing: { previous_completion_ms: number | null; before_monotonic_ms?: number; after_monotonic_ms?: number; before_epoch_ms?: number; after_epoch_ms?: number; requested_wait_ms?: number; waits: Array<{ before_monotonic_ms: number; before_epoch_ms: number; remaining_ms: number; requested_wait_ms: number; after_monotonic_ms?: number; after_epoch_ms?: number }> } = { previous_completion_ms: previousCompletedCase, waits: [] };
    const rematchObservation: { requested_at_ms?: number; before_message_index?: number; response_at_ms?: number; response_status?: number; response_body?: string; authoritative_frame?: { message_index: number; match_id: string; received_at_ms: number; socket_generation: number; raw: string } } = {};
    const progress: Array<{ step: string; epoch_ms: number }> = [];
    const mark = (step: string) => { progress.push({ step, epoch_ms: Date.now() }); console.log(`Task8 ${mode}: ${step}`); };
    let failed = false;
    try {
      checkReconnectSchedulingProjection();
      await checkObserveReconnectEvaluation();
      mark("recorder-path selfcheck PASS issue/start/completion/error/mismatch retained; synthetic recorder only");
      preCasePacing.before_monotonic_ms = performance.now();
      preCasePacing.before_epoch_ms = Date.now();
      preCasePacing.requested_wait_ms = remainingPreCaseRateWindow(previousCompletedCase, preCasePacing.before_monotonic_ms);
      while (true) {
        const before = performance.now();
        const remaining = remainingPreCaseRateWindow(previousCompletedCase, before);
        if (remaining === 0) break;
        const observation = { before_monotonic_ms: before, before_epoch_ms: Date.now(), remaining_ms: remaining, requested_wait_ms: Math.ceil(remaining) } as (typeof preCasePacing.waits)[number];
        preCasePacing.waits.push(observation);
        await wait(observation.requested_wait_ms);
        observation.after_monotonic_ms = performance.now(); observation.after_epoch_ms = Date.now();
      }
      preCasePacing.after_monotonic_ms = performance.now();
      preCasePacing.after_epoch_ms = Date.now();
      if (previousCompletedCase !== null) expect(preCasePacing.after_monotonic_ms - previousCompletedCase, "default sliding IP rate window naturally expired before any case HTTP").toBeGreaterThanOrEqual(61_000);
      mark("admin login");
      const login = await admin.request.post(`${harness.frontendOrigin}/api/v1/admin/login`, { data: { username: harness.adminUsername, password: harness.adminPassword } });
      expect(login.status()).toBe(200);
      mark("create room");
      const created = await admin.request.post(`${harness.frontendOrigin}/api/v1/admin/rooms`, { data: { room_name: `Task8 ${mode}`, game_mode: mode, time_control: "unlimited", replay_save: true } });
      expect(created.status()).toBe(201);
      const code = (await created.json() as { join_code: string }).join_code;
      for (let seat = 0; seat < count; seat++) {
        mark(`join Human ${seat}`);
        const human = await join(await browser.newContext({ viewport, deviceScaleFactor: viewport.dpr, logger: {
          isEnabled: (name, severity) => name === "api" && severity === "info" && schedulingLoggerOwned && seat === schedulingDealerJoinIndex && reconnectInFlight.has(reconnectOperationId),
          log: (name, severity, message) => {
            if (!schedulingLoggerOwned || seat !== schedulingDealerJoinIndex || !reconnectInFlight.has(reconnectOperationId)) return;
            const event = reconnectApiEvent(name, severity, message);
            if (event) reconnectScheduling.api_events.push({ host_epoch_ms: Date.now(), host_monotonic_ms: performance.now(), operation_id: reconnectOperationId, event });
          },
        } }), harness, code, `Task8 ${seat}`);
        humans.push(human);
        mark(`select Human ${seat}`);
        const selected = await admin.request.post(`${harness.frontendOrigin}/api/v1/admin/rooms/${code}/participants/${human.id}/select`);
        expect(selected.status()).toBe(200);
      }
      // Lobby canReady requires the complete selected roster, not one seat.
      for (const [seat, human] of humans.entries()) {
        mark(`ready Human ${seat}`);
        await expect(human.page.getByRole("button", { name: /set ready/i })).toBeEnabled();
        await human.page.getByRole("button", { name: /set ready/i }).click();
        await expect(human.page.getByText("Ready for the next Match", { exact: true })).toBeVisible();
      }
      checkBrowserTaskSample();
      mark("start Match");
      const started = await admin.request.post(`${harness.frontendOrigin}/api/v1/admin/rooms/${code}/start`);
      expect(started.status()).toBe(200);
      for (const human of humans) await expect(human.page.getByTestId("gameplay-shell")).toBeVisible();
      await expect.poll(() => humans.every(h => latest(h).state != null)).toBe(true);
      const bySeat = [...humans].sort((a, b) => latest(a).state!.viewer_seat - latest(b).state!.viewer_seat);
      expect(latest(bySeat[1]).state!.players.find(p => p.seat === 1)!.hand).toEqual([36,40,44,48,53,56,60,64,68,72,73,74,76]);
      expect(latest(bySeat[2]).state!.players.find(p => p.seat === 2)!.hand).toEqual([37,41,45,49,54,57,61,65,69,80,81,82,78].sort((a,b) => a-b));
      // Seats exist only after Match starts; install before any result-producing action.
      const dealerPage = bySeat[0].page;
      const browserTaskPrefix = `task8-browser-task:${randomUUID()}:`;
      dealerPage.on("console", message => {
        const text = message.text();
        if (!text.startsWith(browserTaskPrefix)) return;
        try {
          const data = browserTaskSample(JSON.parse(text.slice(browserTaskPrefix.length)));
          const tasks = reconnectScheduling.browser_tasks;
          if (tasks.events.length < tasks.limit) tasks.events.push({ host_epoch_ms: Date.now(), data });
          else tasks.overflow_count++;
        } catch { reconnectScheduling.browser_tasks.error_count++; }
      });
      await dealerPage.addInitScript(prefix => {
        if (window.top !== window) return;
        const supported = typeof PerformanceObserver !== "undefined" && PerformanceObserver.supportedEntryTypes.includes("longtask");
        const emit = (kind: string, start_time_ms: number, duration_ms: number | null) => console.debug(prefix + JSON.stringify({ kind, time_origin_ms: performance.timeOrigin, start_time_ms, duration_ms, supported }));
        emit("init", performance.now(), null);
        if (supported) new PerformanceObserver(list => {
          for (const entry of list.getEntries()) emit("longtask", entry.startTime, entry.duration);
        }).observe({ type: "longtask", buffered: true });
      }, browserTaskPrefix);
      reconnectScheduling.browser_tasks.installed_before_result_at_ms = Date.now();
      // Use the table's actual ready signal before starting finite result clocks.
      await Promise.all(bySeat.map(async human => {
        await expect(human.page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
        await human.page.evaluate(`(async () => { await import('/src/game/store.ts'); })()`);
      }));
      mark("offered dealer discard77");
      await choose(bySeat[0], action => typeof action === "object" && action != null && "discard" in action && (action as { discard: { tile: number } }).discard.tile === 77);
      await expect.poll(() => latest(bySeat[1]).state?.decision?.actions.some(a => typeof a.action === "object" && a.action != null && "Ron" in a.action)).toBe(true);
      mark("offered Ron seat1");
      await choose(bySeat[1], action => typeof action === "object" && action != null && "Ron" in action);
      mark("offered Ron seat2");
      let finalResponse = choose(bySeat[2], action => typeof action === "object" && action != null && "Ron" in action);
      if (count === 4) {
        finalResponse = finalResponse.then(() => latest(bySeat[3]).state?.decision ? choose(bySeat[3], action => action === "Pass") : undefined);
      }
      // Browser action completion can lag behind its real server publication.
      void finalResponse.catch(() => undefined);
      await expect.poll(() => latest(bySeat[0]).room.kyoku_end?.id.step_index, { timeout: 30_000 }).toBe(0);
      mark("first winner publication");
      const first = latest(bySeat[0]);
      expect(first.state!.decision).toBeNull();
      expect(first.room.result).toBeNull();
      expect(first.state!.kyoku_results!.wins.map(w => w.seat)).toEqual([1]);
      expect(first.state!.kyoku_results!.settlement).toBeNull();
      expect(first.state!.kyoku_results!.wins[0]).toMatchObject({ winning_tile: 77, winning_tile_index: 13, win_points: 2600, evaluation: { Ordinary: { han: 2, fu: 40, limit: null } } });
      expect(first.room.kyoku_end!.deadline_at_ms - first.room.kyoku_end!.published_at_ms).toBe(5000);
      mark("first winner reconnect");
      const generation = first.socket_generation;
      schedulingDealerJoinIndex = humans.indexOf(bySeat[0]);
      schedulingHistogram = monitorEventLoopDelay({ resolution: 20 });
      schedulingHistogram.enable(); schedulingHistogramEnabled = true; reconnectScheduling.enabled_at_ms = Date.now();
      schedulingSnapshot("beforeReload");
      const reconnectIssuedHostAt = Date.now();
      if (finalFixture) {
        await bySeat[0].page.evaluate(key => {
          const socket = (window as unknown as Record<string, WebSocket>)[key];
          if (!socket || socket.readyState !== WebSocket.OPEN) throw new Error("missing owned open Human socket");
          socket.close();
        }, bySeat[0].socketKey);
        mark("actual Human transport closed; automatic reconnect");
      } else {
        await bySeat[0].page.reload({ waitUntil: "domcontentloaded" });
        mark("reload DOM content loaded");
      }
      const reconnectCompletedHostAt = Date.now();
      const reconnectSnapshot = () => bySeat[0].snapshots.find(s => s.socket_generation > generation && s.type === "snapshot");
      await expect.poll(() => Boolean(reconnectSnapshot())).toBe(true);
      const reconnected = reconnectSnapshot()!;
      const publication = (index: number) => bySeat[0].snapshots.find(s => {
        const id = s.room.kyoku_end?.id;
        return id?.match_id === first.room.kyoku_end!.id.match_id && id.kyoku_sequence === first.room.kyoku_end!.id.kyoku_sequence && id.step_index === index;
      });
      const earliestFirst = publication(0);
      // Allowlisted scalars only; socket-created/received times are Node observations, not browser OPEN/receipt.
      mark(`reconnect clock observation ${JSON.stringify({
        kind: finalFixture ? "transport" : "reload",
        issued_host_at_ms: reconnectIssuedHostAt, completed_host_at_ms: reconnectCompletedHostAt,
        original_win0: {
          step_index: first.room.kyoku_end!.id.step_index,
          published_at_ms: first.room.kyoku_end!.published_at_ms, deadline_at_ms: first.room.kyoku_end!.deadline_at_ms,
          selected_host_observed_at_ms: first.received_at_ms, selected_wire_remaining_ms: first.room.kyoku_end!.remaining_ms,
          selected_generation: first.socket_generation, selected_message_index: first.message_index,
          earliest_host_observed_at_ms: earliestFirst?.received_at_ms ?? null,
          earliest_published_at_ms: earliestFirst?.room.kyoku_end?.published_at_ms ?? null,
          earliest_deadline_at_ms: earliestFirst?.room.kyoku_end?.deadline_at_ms ?? null,
          earliest_wire_remaining_ms: earliestFirst?.room.kyoku_end?.remaining_ms ?? null,
          required_count: first.room.kyoku_end!.required.length, confirmed_count: first.room.kyoku_end!.confirmed.length,
          own_required: first.room.kyoku_end!.required.includes(bySeat[0].id),
        },
        first_reconnect: {
          step_index: reconnected.room.kyoku_end?.id.step_index ?? null,
          published_at_ms: reconnected.room.kyoku_end?.published_at_ms ?? null, deadline_at_ms: reconnected.room.kyoku_end?.deadline_at_ms ?? null,
          wire_remaining_ms: reconnected.room.kyoku_end?.remaining_ms ?? null,
          socket_created_host_at_ms: reconnected.socket_created_host_at_ms, host_observed_at_ms: reconnected.received_at_ms,
          generation: reconnected.socket_generation, message_index: reconnected.message_index,
          required_count: reconnected.room.kyoku_end?.required.length ?? null, confirmed_count: reconnected.room.kyoku_end?.confirmed.length ?? null,
          own_required: reconnected.room.kyoku_end?.required.includes(bySeat[0].id) ?? null,
        },
      }, null, 2)}`);
      // Assert the first actual reconnect frame, not a later UI wait/latest frame.
      expect(reconnected.room.kyoku_end!.id).toEqual(first.room.kyoku_end!.id);
      expect(reconnected.room.kyoku_end!.published_at_ms).toBe(first.room.kyoku_end!.published_at_ms);
      expect(reconnected.room.kyoku_end!.deadline_at_ms).toBe(first.room.kyoku_end!.deadline_at_ms);
      expect(reconnected.received_at_ms).toBeLessThan(first.room.kyoku_end!.deadline_at_ms);
      expect(reconnected.room.kyoku_end!.remaining_ms).toBeGreaterThan(0);
      expect(reconnected.room.kyoku_end!.remaining_ms).toBeLessThan(first.room.kyoku_end!.remaining_ms);
      expect(reconnected.room.kyoku_end!.required).not.toContain(bySeat[0].id);
      expect(reconnected.state!.decision).toBeNull();
      expect(reconnected.room.result).toBeNull();
      expect(reconnected.state!.kyoku_results!.wins).toEqual(first.state!.kyoku_results!.wins);
      expect(reconnected.state!.kyoku_results!.settlement).toBeNull();
      if (finalFixture) {
        geometry.push({ surface: "Live WinOverlay", mode, viewport, measurement: await resultGeometry(bySeat[1].page, "win", viewport, viewport.dpr, first.room.kyoku_end!, geometryObservations, bySeat[1], humans) });
      }
      await finalResponse;
      // Server publications continue while the reloaded page initializes its renderer.
      const firstConfirmations = await confirmStep(bySeat, reconnected.room.kyoku_end!);
      mark(`first winner confirmations ${JSON.stringify(firstConfirmations)}`);
      await expect.poll(() => Boolean(publication(1))).toBe(true);
      const second = publication(1)!;
      const firstAcks = bySeat.flatMap(h => h.snapshots).some(s => {
        const step = s.room.kyoku_end;
        return step?.id.step_index === 0 && step.required.every(id => step.confirmed.includes(id));
      });
      if (!firstAcks && !firstConfirmations.every(o => o.endsWith(":clicked") || o.endsWith(":confirmed"))) expect(second.room.kyoku_end!.published_at_ms).toBeGreaterThanOrEqual(first.room.kyoku_end!.deadline_at_ms);
      expect(second.room.kyoku_end!.deadline_at_ms - second.room.kyoku_end!.published_at_ms).toBe(5000);
      expect(second.state!.kyoku_results!.wins.map(w => w.seat)).toEqual([1,2]);
      expect(second.state!.kyoku_results!.settlement).toBeNull();
      expect(second.room.kyoku_end!.required).toContain(bySeat[0].id);
      expect(second.state!.decision).toBeNull();
      expect(second.room.result).toBeNull();
      expect(second.state!.kyoku_results!.wins[1]).toMatchObject({ seat: 2, winning_tile: 77, winning_tile_index: 13, win_points: 2600, evaluation: { Ordinary: { han: 2, fu: 40, limit: null } } });
      mark("confirm second winner");
      mark(`second winner confirmations ${JSON.stringify(await confirmStep(bySeat, second.room.kyoku_end!))}`);
      await expect.poll(() => Boolean(publication(2))).toBe(true);
      const settled = publication(2)!;
      expect(settled.room.kyoku_end!.deadline_at_ms - settled.room.kyoku_end!.published_at_ms).toBe(5000);
      expect(settled.room.kyoku_end!.published_at_ms).toBeGreaterThan(second.room.kyoku_end!.published_at_ms);
      const settlement = settled.state!.kyoku_results!.settlement as { changes: Array<{ before: number; delta: number; after: number }> };
      expect(settlement.changes.map(c => c.after)).toEqual(after);
      expect(settlement.changes.map(c => c.before)).toEqual(beforeScores);
      expect(settlement.changes.every(c => c.before + c.delta === c.after)).toBe(true);
      mark("literal settlement verified");
      // Import first, then synchronously sample the current store, DOM and clock together.
      // DOM readiness cannot restart the original publication or freeze the next one.
      bySeat[0].page.on("console", message => {
        const text = message.text();
        if (!text.startsWith(reconnectPrefix)) return;
        try {
          const event = JSON.parse(text.slice(reconnectPrefix.length)) as { operation_id: number; phase: string; browser_epoch_ms: number; browser_monotonic_ms: number; error?: unknown };
          if (!Number.isInteger(event.operation_id) || !["browser_start", "browser_import_start", "browser_import_end", "browser_sample", "browser_error"].includes(event.phase) || !Number.isFinite(event.browser_epoch_ms) || !Number.isFinite(event.browser_monotonic_ms)) throw new Error("malformed owned reconnect phase");
          recordReconnectPhase(reconnectPhases, reconnectInFlight, event.operation_id, event.phase, event);
        } catch (error) {
          reconnectObserverErrors.push({ host_epoch_ms: Date.now(), text, error: error instanceof Error ? { name: error.name, message: error.message } : String(error) });
        }
      });
      reconnectPoll.start_epoch_ms = Date.now(); reconnectPoll.start_monotonic_ms = performance.now();
      schedulingSnapshot("pollStart"); schedulingPollElu = performance.eventLoopUtilization(); schedulingLoggerOwned = true;
      try {
        await expect.poll(async () => {
          const operationId = ++reconnectOperationId;
          const sample = await observeReconnectEvaluation(reconnectPhases, reconnectInFlight, operationId, () => bySeat[0].page.evaluate(`(async () => {
            const operation_id = ${operationId};
            const emit = (phase, error = undefined) => console.debug(${JSON.stringify(reconnectPrefix)} + JSON.stringify({ operation_id, phase, browser_epoch_ms: Date.now(), browser_monotonic_ms: performance.now(), error }));
            emit('browser_start');
            try {
              emit('browser_import_start');
              const { useGameStore } = await import('/src/game/store.ts');
              emit('browser_import_end');
              const state = useGameStore.getState();
              const visible = selector => { const node = document.querySelector(selector); const rect = node?.getBoundingClientRect(); return Boolean(rect?.width > 0 && rect?.height > 0 && getComputedStyle(node).visibility !== 'hidden'); };
              const sample = { epoch_ms: Date.now(), status: state.status, phase: state.room?.phase ?? null, step: state.room?.kyoku_end ?? null,
                surface: visible('.round-win-surface') ? 'win' : visible('.score-transfer') ? 'settlement' : visible('.results-panel') ? 'standings' : visible('.table-letterbox canvas') ? 'table' : null,
                standings_count: document.querySelectorAll('[data-testid="results-panel"]').length,
                labels: [...document.querySelectorAll('.round-win-overlay > .round-win-actions > button')].map(button => button.textContent) };
              emit('browser_sample');
              return sample;
            } catch (error) { emit('browser_error', { name: error.name, message: error.message, stack: error.stack }); throw error; }
          })()`)) as ReconnectUiSample;
          reconnectUiSamples.push(sample);
          expect(reconnectObserverErrors).toEqual([]);
          const received = bySeat[0].snapshots.filter(s => s.socket_generation > generation).map(s => s.room);
          expect(reconnectUiHasValidPublication(sample, first.room.kyoku_end!, bySeat[0].id, received), "each recorded nonnull sample preserves actual publication content/identity/clock/cohort and hides final standings").toBe(true);
          return reconnectUiMatchesCurrentPublication(sample, first.room.kyoku_end!, bySeat[0].id, received);
        }, { timeout: 30_000 }).toBe(true);
        expect(reconnectObserverErrors).toEqual([]);
      } catch (error) {
        reconnectPoll.error = error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error);
        throw error;
      } finally {
        reconnectPoll.end_epoch_ms = Date.now(); reconnectPoll.end_monotonic_ms = performance.now();
        reconnectPoll.in_flight_at_end = [...reconnectInFlight];
        schedulingSnapshot("pollFinally");
        reconnectScheduling.poll_utilization = reconnectUtilization(performance.eventLoopUtilization(schedulingPollElu!));
        disableSchedulingHistogram();
      }
      const ui = reconnectUiSamples.at(-1)!;
      mark(`reconnect UI observed ${ui.phase}/${ui.step?.id.step_index ?? "no-publication"} latency${ui.epoch_ms - first.room.kyoku_end!.published_at_ms}ms`);
      await testInfo.attach("actual-human-websocket-transcript", { body: Buffer.from(JSON.stringify({ progress, preCasePacing, reconnect_ui: ui, reconnectUiSamples, reconnectPhases, reconnectPoll, reconnectObserverErrors, reconnectScheduling, rematchObservation, humans: bySeat.map(h => ({ participant_id: h.id, messages: h.messages, sent: h.sent, observations: h.snapshots.map(s => ({ socket_generation: s.socket_generation, received_at_ms: s.received_at_ms, message_index: s.message_index })) })) })), contentType: "application/json" });
      if (finalFixture) {
        const held = settled;
        const finalStep = held.room.kyoku_end!;
        const wins = held.state!.kyoku_results!.wins;
        expect(held.room.phase).toBe("playing");
        expect(held.room.result).toBeNull();
        expect(held.state!.decision).toBeNull();
        mark("confirm final settlement");
        mark(`final settlement confirmations ${JSON.stringify(await confirmStep(bySeat, finalStep))}`);
        await expect.poll(() => latest(bySeat[0]).room.phase).toBe("post_match");
        const final = latest(bySeat[0]).room.result as { final_scores: number[]; players: Array<{ seat: number; rank: number; participant_id: string; final_score: number }> };
        expect(final.final_scores).toEqual(after);
        expect(final.players.map(p => p.rank)).toEqual(count === 3 ? [3,1,2] : [4,1,2,3]);
        expect(final.players.map(p => p.participant_id)).toEqual(bySeat.map(h => h.id));
        expect(final.players.map(p => p.final_score)).toEqual(after);
        for (const human of bySeat) await expect(human.page.getByTestId("results-panel")).toBeVisible();
        geometry.push({ surface: "Live final standings", mode, viewport, measurement: await resultGeometry(bySeat[0].page, "standings", viewport, viewport.dpr, undefined, geometryObservations) });
        await bySeat[0].page.setViewportSize({ width: 1023, height: 600 });
        await expect(bySeat[0].page.getByText("Widen this window to play.", { exact: true })).toBeVisible();
        await expect(bySeat[0].page.getByTestId("results-panel")).toHaveCount(0);
        await expect(bySeat[0].page.locator("[data-action-id]")).toHaveCount(0);
        expect(latest(bySeat[0]).room.result).toEqual(final);
        await bySeat[0].page.setViewportSize(viewport);
        geometry.push({ surface: "Live supported restoration", mode, viewport, measurement: await resultGeometry(bySeat[0].page, "standings", viewport, viewport.dpr, undefined, geometryObservations) });
        mark("saved Replay API");
        const matchId = finalStep.id.match_id;
        let replay: { frames: Array<{ event_index: number; visible_event: { type: string }; result: unknown; settlement: unknown }>; result: unknown } | undefined;
        await expect.poll(async () => {
          const response = await admin.request.get(`${harness.frontendOrigin}/api/v1/admin/replays/${matchId}`);
          if (response.status() !== 200) return response.status();
          replay = await response.json();
          return 200;
        }).toBe(200);
        expect(replay!.result).toEqual(final);
        expect(replay!.frames.filter(f => f.result != null).map(f => f.result)).toEqual(wins);
        expect(replay!.frames.filter(f => f.settlement != null).map(f => f.settlement)).toEqual([settlement]);
        const canonicalPaths = readdirSync(evidenceRoot, { recursive: true }).map(String).filter(p => p.endsWith(`_${matchId}.mjson`));
        expect(canonicalPaths).toHaveLength(1);
        const canonicalBytes = readFileSync(resolve(evidenceRoot, canonicalPaths[0]));
        expect(canonicalBytes.includes(0)).toBe(false);
        const canonical = canonicalBytes.toString("utf8").trim().split(/\r?\n/).map(line => JSON.parse(line) as { type: string });
        expect(replay!.frames.map(f => f.visible_event)).toEqual(canonical);
        expect(canonical.slice(-4).map(e => e.type)).toEqual(["hora", "hora", "end_kyoku", "end_game"]);
        expect(replay!.frames.map(f => f.event_index)).toEqual(canonical.map((_, i) => i));
        const unauthorized = await bySeat[0].context.request.get(`${harness.frontendOrigin}/api/v1/admin/replays/${matchId}`);
        expect(unauthorized.status()).toBe(401);
        mark("Admin Replay actual UI");
        const replayPage = await admin.newPage();
        await replayPage.goto(`${harness.frontendOrigin}/admin/replays/${matchId}`);
        await expect(replayPage.getByTestId("replay-viewer")).toBeVisible();
        const timeline = replayPage.getByRole("slider", { name: /replay position/i });
        const pause = replayPage.getByRole("button", { name: "Pause", exact: true });
        if (await pause.count()) await pause.click();
        await expect(replayPage.getByRole("button", { name: "Play", exact: true })).toBeVisible();
        await timeline.press("Home");
        for (let index = 0; index < wins[0].event_index; index++) await timeline.press("ArrowRight");
        await expect(replayPage.getByLabel("Ron win details")).toContainText("2,600");
        geometry.push({ surface: "Replay WinOverlay", mode, viewport, measurement: await resultGeometry(replayPage, "win", viewport, viewport.dpr, undefined, geometryObservations) });
        const sliderBefore = await timeline.inputValue();
        await replayPage.setViewportSize({ width: 1023, height: 600 });
        await expect(timeline).toHaveValue(sliderBefore);
        await expect(replayPage.getByRole("button", { name: "Play", exact: true })).toBeVisible();
        geometry.push({ surface: "Replay below threshold existing behavior", guidanceCount: await replayPage.getByText("Widen this window to play.", { exact: true }).count(), sliderBefore, currentSlider: await timeline.inputValue(), url: replayPage.url() });
        await replayPage.setViewportSize(viewport);
        geometry.push({ surface: "Replay supported restoration", mode, viewport, measurement: await resultGeometry(replayPage, "win", viewport, viewport.dpr, undefined, geometryObservations) });
        await timeline.press("End");
        await expect(replayPage.getByTestId("results-panel")).toBeVisible();
        geometry.push({ surface: "Replay final standings", mode, viewport, measurement: await resultGeometry(replayPage, "standings", viewport, viewport.dpr, undefined, geometryObservations) });
        await testInfo.attach("actual-live-replay-result-geometry", { body: Buffer.from(JSON.stringify({ matchId, mode, viewport, geometry, geometryObservations })), contentType: "application/json" });
        await testInfo.attach("actual-replay-final-viewport", { body: await replayPage.screenshot(), contentType: "image/png" });
        mark("Post-Match Ready and Rematch");
        for (const human of bySeat) {
          await human.page.getByTestId("dismiss-results").click();
          await expect(human.page.getByRole("button", { name: /set ready/i })).toBeEnabled();
          await human.page.getByRole("button", { name: /set ready/i }).click();
          await expect(human.page.getByText("Ready for the next Match", { exact: true })).toBeVisible();
        }
        const beforeRematch = bySeat[0].messages.length;
        rematchObservation.before_message_index = beforeRematch;
        rematchObservation.requested_at_ms = Date.now();
        const rematch = await admin.request.post(`${harness.frontendOrigin}/api/v1/admin/rooms/${code}/rematch`);
        rematchObservation.response_at_ms = Date.now();
        rematchObservation.response_status = rematch.status();
        rematchObservation.response_body = await rematch.text();
        expect(rematch.status()).toBe(200);
        await expect.poll(() => {
          const frame = findFreshMatchStarted(bySeat[0].messages, beforeRematch, matchId);
          if (!frame) return false;
          const observed = bySeat[0].snapshots.find(s => s.message_index === frame.message_index);
          expect(observed, "actual authoritative rematch frame receive observation").toBeTruthy();
          rematchObservation.authoritative_frame = { ...frame, received_at_ms: observed!.received_at_ms, socket_generation: observed!.socket_generation, raw: bySeat[0].messages[frame.message_index] };
          return true;
        }).toBe(true);
        await expect.poll(() => latest(bySeat[0]).state?.decision != null).toBe(true);
        expect(latest(bySeat[0]).room.phase).toBe("playing");
        expect(latest(bySeat[0]).room.result).toBeNull();
        expect(rematchObservation.authoritative_frame!.match_id).toBeTruthy();
        expect(rematchObservation.authoritative_frame!.match_id).not.toBe(matchId);
        const oldMessages = bySeat[0].messages.length;
        await bySeat[0].page.evaluate(`(async () => {
          const { useGameStore } = await import('/src/game/store.ts');
          useGameStore.getState().transport(${JSON.stringify({ type: "confirm_result", step_id: finalStep.id })});
        })()`);
        await expect.poll(() => bySeat[0].messages.slice(oldMessages).some(t => JSON.parse(t).type === "snapshot")).toBe(true);
        expect(latest(bySeat[0]).room.kyoku_end).toBeNull();
        expect(latest(bySeat[0]).state!.decision).not.toBeNull();
        expect(latest(bySeat[0]).room.result).toBeNull();
        await testInfo.attach("saved-authority-replay", { body: Buffer.from(JSON.stringify({ match_id: matchId, canonical, replay, wins, settlement, final, progress, preCasePacing, rematchObservation, reconnectUiSamples, reconnectPhases, reconnectPoll, reconnectObserverErrors, reconnectScheduling })), contentType: "application/json" });
        mark("final lifecycle save Replay verified");
      }
    } catch (error) {
      failed = true;
      faultCutoff = { epoch_ms: Date.now(), phases_count: reconnectPhases.length, samples_count: reconnectUiSamples.length, in_flight: [...reconnectInFlight] };
      const path = resolve(evidenceRoot, `failure-${mode}-${randomUUID()}.json`);
      const descriptor = openSync(path, "wx");
      try {
        writeFileSync(descriptor, JSON.stringify({ geometry, geometryObservations, viewport, progress, preCasePacing, reconnectUiSamples, reconnectPhases, reconnectPoll, reconnectObserverErrors, reconnectScheduling, faultCutoff, rematchObservation, primary_error: error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error), humans: humans.map(h => ({ participant_id: h.id, messages: h.messages, sent: h.sent, observations: h.snapshots.map(s => ({ socket_generation: s.socket_generation, received_at_ms: s.received_at_ms, message_index: s.message_index })) })) }), "utf8");
        fsyncSync(descriptor);
      } finally { closeSync(descriptor); }
      throw error;
    } finally {
      disableSchedulingHistogram();
      for (const context of [...humans.map(h => h.context), admin]) {
        try { await context.close(); }
        catch (error) { if (!failed) throw error; console.error("Task8 secondary context cleanup error", error); }
      }
      schedulingLoggerOwned = false;
      if (!failed) previousCompletedCase = performance.now();
      const supplemental = openSync(resolve(evidenceRoot, `reconnect-phases-${mode}-${randomUUID()}.json`), "wx");
      try {
        writeFileSync(supplemental, JSON.stringify({ mode, viewport, captured_at_ms: Date.now(), faultCutoff, supplemental_only_after_fault: failed, reconnectPhases, reconnectPoll, reconnectObserverErrors, reconnectScheduling, in_flight_at_capture: [...reconnectInFlight], reconnectUiSamples,
          raw_scope: "all handler-observed Human frames through natural context cleanup; no outbound FIFO/network-drain claim",
          humans: humans.map(h => ({ participant_id: h.id, messages: h.messages, sent: h.sent, sentAt: h.sentAt, observations: h.snapshots.map(s => ({ socket_generation: s.socket_generation, received_at_ms: s.received_at_ms, message_index: s.message_index })) })) }), "utf8");
        fsyncSync(supplemental);
      } finally { closeSync(supplemental); }
    }
  });
  }
}
  });
}
