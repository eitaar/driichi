import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BenchmarkCharts } from "./benchmark-charts";
import type { RunAnalytics } from "./benchmark-analytics";
import type { ReactNode } from "react";

// Plot layout/animation timers are outside the cycle contract.
vi.mock("./components/ui/chart", () => ({ ChartContainer: () => null, ChartTooltip: () => null, ChartTooltipContent: () => null, ChartLegend: () => null, ChartLegendContent: () => null }));
// Native select isolates the cycle contract, including selecting the same stable ID again.
vi.mock("./components/ui/select", () => ({
  Select: ({ value, onValueChange, items, children }: { value: string; onValueChange(value: string): void; items: { value: string; label: string }[]; children: ReactNode }) => <><select aria-label={items.some(item => item.value === "p0") ? "Timer Participant" : "Timer window"} value={value} onChange={event => onValueChange(event.target.value)}>{items.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}</select>{children}</>,
  SelectTrigger: () => null, SelectValue: () => null, SelectContent: () => null, SelectGroup: () => null, SelectItem: () => null,
}));
function data(count = 3): Extract<RunAnalytics, { kind: "ready" }> {
  return { kind: "ready", completed: 1, provisional: true, participants: Array.from({ length: count }, (_, i) => ({
    participantId: `p${i}`, displayName: "Duplicate", averageRank: i + 1, firstPlaceRate: Number(i === 0), cumulativeNet: 0, rankCounts: [1, 0, 0],
    points: [{ sequence: 1, matchId: "exact-match", net: 0, cumulative: 0, movingRank: i + 1, movingN: 1 }],
  })) };
}
const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));
function setup(count = 3) {
  const view = render(<BenchmarkCharts analytics={data(count)} onOpenMatch={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Per-Match net score options" }));
  return { ...view, select: screen.getByLabelText("Timer Participant"), card: screen.getByRole("region", { name: "Per-Match net score" }).closest('[data-chart-family="net"]')! };
}
beforeEach(() => { vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} }); vi.useFakeTimers(); Object.defineProperty(document, "hidden", { configurable: true, value: false }); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
it("cycles only at 8000ms and wraps stable IDs despite duplicate names", () => {
  const { select } = setup();
  advance(7999); expect(select).toHaveValue("p0");
  advance(1); expect(select).toHaveValue("p1");
  advance(8000); expect(select).toHaveValue("p2");
  advance(8000); expect(select).toHaveValue("p0");
});
it("cycles at 8s across equivalent authoritative refreshes every 5s", () => {
  const { select, rerender } = setup();
  let elapsed = 0;
  for (const time of [5000, 7999, 8000, 10000, 15000, 15999, 16000, 20000, 23999, 24000]) {
    advance(time - elapsed); elapsed = time;
    if (time % 5000 === 0) {
      const refreshed = data();
      refreshed.participants.reverse(); // Incoming order and object identity are not the canonical roster order.
      refreshed.participants.forEach(p => { p.cumulativeNet = time; });
      rerender(<BenchmarkCharts analytics={refreshed} onOpenMatch={vi.fn()} />);
    }
    expect(select).toHaveValue(`p${Math.floor(time / 8000) % 3}`);
  }
});
it("restarts for changed roster IDs, recovers a removed selection, and stops for a single roster", () => {
  const { select, rerender, unmount } = setup();
  advance(5000);
  const replacement = data();
  replacement.participants[1].participantId = "p9";
  rerender(<BenchmarkCharts analytics={replacement} onOpenMatch={vi.fn()} />);
  advance(7999); expect(select).toHaveValue("p0");
  advance(1); expect(select).toHaveValue("p2");
  const removed = { ...replacement, participants: replacement.participants.filter(p => p.participantId !== "p2") };
  rerender(<BenchmarkCharts analytics={removed} onOpenMatch={vi.fn()} />);
  expect(select).toHaveValue("p0");
  advance(7999); expect(select).toHaveValue("p0");
  advance(1); expect(select).toHaveValue("p9");
  rerender(<BenchmarkCharts analytics={data(1)} onOpenMatch={vi.fn()} />);
  expect(select).toHaveValue("p0"); advance(16000); expect(select).toHaveValue("p0");
  unmount(); expect(vi.getTimerCount()).toBe(0);
});
it("Pause/Resume and every manual selection start a fresh interval", () => {
  const { select } = setup();
  advance(7000); fireEvent.click(screen.getByRole("button", { name: "Pause cycling" }));
  advance(16000); expect(select).toHaveValue("p0");
  fireEvent.click(screen.getByRole("button", { name: "Resume cycling" }));
  advance(7999); expect(select).toHaveValue("p0"); advance(1); expect(select).toHaveValue("p1");
  advance(7000); fireEvent.change(select, { target: { value: "p1" } });
  advance(7999); expect(select).toHaveValue("p1"); advance(1); expect(select).toHaveValue("p2");
  fireEvent.change(select, { target: { value: "p0" } });
  advance(7999); expect(select).toHaveValue("p0"); advance(1); expect(select).toHaveValue("p1");
});
it("suspends pointer reading and focus, and restarts after leaving/blur", () => {
  const { card, select } = setup();
  advance(7000); fireEvent.pointerEnter(card); advance(16000); expect(select).toHaveValue("p0");
  fireEvent.pointerLeave(card); advance(7999); expect(select).toHaveValue("p0"); advance(1); expect(select).toHaveValue("p1");
  fireEvent.focus(select); advance(16000); expect(select).toHaveValue("p1");
  fireEvent.blur(select, { relatedTarget: document.body }); advance(7999); expect(select).toHaveValue("p1"); advance(1); expect(select).toHaveValue("p2");
});
it("suspends hidden pages, refreshes visibility intervals, preserves selection across refresh and cleans up", () => {
  const add = vi.spyOn(document, "addEventListener"), remove = vi.spyOn(document, "removeEventListener");
  const { select, rerender, unmount } = setup();
  fireEvent.change(select, { target: { value: "p2" } });
  rerender(<BenchmarkCharts analytics={data()} onOpenMatch={vi.fn()} />);
  expect(select).toHaveValue("p2"); advance(7000);
  Object.defineProperty(document, "hidden", { configurable: true, value: true }); fireEvent(document, new Event("visibilitychange"));
  advance(16000); expect(select).toHaveValue("p2");
  Object.defineProperty(document, "hidden", { configurable: true, value: false }); fireEvent(document, new Event("visibilitychange"));
  advance(7999); expect(select).toHaveValue("p2"); advance(1); expect(select).toHaveValue("p0");
  const listeners = add.mock.calls.filter(([type]) => type === "visibilitychange");
  unmount(); expect(vi.getTimerCount()).toBe(0);
  for (const [, listener] of listeners) expect(remove).toHaveBeenCalledWith("visibilitychange", listener);
  add.mockRestore(); remove.mockRestore();
});
it("leaves no cycle timer or visibility listener for a single Participant", () => {
  const add = vi.spyOn(document, "addEventListener");
  const intervals = vi.spyOn(window, "setInterval"), clear = vi.spyOn(window, "clearInterval");
  const { select, unmount } = setup(1); advance(24000); expect(select).toHaveValue("p0");
  expect(add.mock.calls.filter(([type]) => type === "visibilitychange")).toHaveLength(0);
  expect(intervals).not.toHaveBeenCalled();
  unmount(); expect(clear).not.toHaveBeenCalled();
  intervals.mockRestore(); clear.mockRestore(); add.mockRestore();
});
