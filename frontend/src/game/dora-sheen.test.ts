import { afterEach, describe, expect, it, vi } from "vitest";
import { doraSheenFrame, startDoraSheen, type DoraSheenClock } from "./dora-sheen";

function clock(): DoraSheenClock {
  return { sweep: { value: -10 }, strength: { value: 0 }, listeners: new Set() };
}

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

describe("ordinary-Dora face sheen", () => {
  it("uses an absolute two-second clock with only a 600ms active pass", () => {
    expect(doraSheenFrame(0, true, false, true)).toMatchObject({ phase: 0, waitMs: 0 });
    expect(doraSheenFrame(300, true, false, true)).toMatchObject({ phase: 0.5, waitMs: 0 });
    expect(doraSheenFrame(600, true, false, true)).toMatchObject({ phase: -10, waitMs: 1400 });
    expect(doraSheenFrame(2300, true, false, true)).toMatchObject({ phase: 0.5, waitMs: 0 });
    for (const time of [-1, NaN, Infinity]) expect(doraSheenFrame(time, true, false, true).waitMs).toBeNull();
    expect(doraSheenFrame(300, false, false, true)).toMatchObject({ phase: -10, strength: 0, waitMs: null });
    expect(doraSheenFrame(300, true, false, false)).toMatchObject({ phase: -10, strength: 0, waitMs: null });
    expect(doraSheenFrame(300, true, true, true)).toEqual({ phase: 0.5, strength: 0.28, waitMs: null });
  });

  it("stops DOM updates and invalidations during gaps, catches up after visibility, and cancels on cleanup", () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame", "cancelAnimationFrame", "performance"] });
    let hidden = false;
    vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
    const host = document.createElement("div");
    const shared = clock();
    const invalidate = vi.fn();
    shared.listeners.add(invalidate);
    const stop = startDoraSheen(host, shared, true, false);
    vi.advanceTimersByTime(700);
    expect(shared.sweep.value).toBe(-10);
    expect(host.style.getPropertyValue("--dora-sheen-opacity")).toBe("0");
    const callsAtGap = invalidate.mock.calls.length;
    vi.advanceTimersByTime(1200);
    expect(invalidate).toHaveBeenCalledTimes(callsAtGap);
    vi.advanceTimersByTime(200);
    expect(shared.sweep.value).toBeGreaterThan(0);
    expect(shared.sweep.value).toBeLessThan(1);
    expect(Number(host.dataset.sheenPhase)).toBe(shared.sweep.value);
    hidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
    expect(shared.strength.value).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(2000);
    hidden = false;
    document.dispatchEvent(new Event("visibilitychange"));
    expect(shared.sweep.value).toBeCloseTo(1 / 6, 1);
    stop();
    expect(vi.getTimerCount()).toBe(0);
    const callsAfterStop = invalidate.mock.calls.length;
    document.dispatchEvent(new Event("visibilitychange"));
    vi.advanceTimersByTime(2000);
    expect(invalidate).toHaveBeenCalledTimes(callsAfterStop);
    expect(host.style.getPropertyValue("--dora-sheen-opacity")).toBe("");
  });

  it.each([[false, false], [true, true]])("schedules no animation when enabled=%s reduced=%s", (enabled, reduced) => {
    vi.useFakeTimers();
    vi.spyOn(document, "hidden", "get").mockReturnValue(false);
    const stop = startDoraSheen(document.createElement("div"), clock(), enabled, reduced);
    expect(vi.getTimerCount()).toBe(0);
    stop();
  });
});
