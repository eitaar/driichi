import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { BenchmarkCharts } from "./benchmark-charts";
import type { RunAnalytics } from "./benchmark-analytics";

const plot = vi.hoisted(() => ({ domain: [] as number[], axes: [] as string[], geometry: { cx: 0, cy: 0, outerRadius: 0 } }));
vi.mock("./components/ui/chart", () => ({
  ChartContainer: ({ children, className }: { children: ReactNode; className: string }) => <div className={className}>{children}<div className="house-chart-legend" /></div>,
  ChartTooltip: () => null, ChartTooltipContent: () => null,
  ChartLegend: () => null, ChartLegendContent: () => null,
}));
vi.mock("recharts", () => ({
  RadarChart: ({ children, data, cx, cy, outerRadius }: { children: ReactNode; data: { name: string }[]; cx: number; cy: number; outerRadius: number }) => { plot.axes = data.map(p => p.name); plot.geometry = { cx, cy, outerRadius }; return <svg>{children}</svg>; },
  PolarRadiusAxis: ({ domain }: { domain: number[] }) => { plot.domain = domain; return null; },
  LineChart: () => null, BarChart: () => null, Radar: () => null, PolarGrid: () => null, PolarAngleAxis: () => null,
  LabelList: () => null, Symbols: () => null, Bar: () => null, CartesianGrid: () => null, Line: () => null, ReferenceLine: () => null, XAxis: () => null, YAxis: () => null,
}));
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); Reflect.deleteProperty(SVGElement.prototype, "getBBox"); });
it("measures legend/text, retains the full-roster domain when filtering, and releases measurement work", () => {
  const observers: { callback(): void; elements: Set<Element>; disconnect: ReturnType<typeof vi.fn> }[] = [];
  vi.stubGlobal("ResizeObserver", class {
    elements = new Set<Element>();
    disconnect = vi.fn();
    constructor(public callback: () => void) { observers.push(this); }
    observe(element: Element) { this.elements.add(element); }
    unobserve(element: Element) { this.elements.delete(element); }
  });
  const frames = new Map<number, FrameRequestCallback>(); let next = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { frames.set(++next, callback); return next; });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    return { x: 0, y: 0, left: 0, right: 400, width: 400, height: 320, top: this.classList.contains("house-chart-legend") ? 280 : 0, bottom: 320, toJSON() {} };
  });
  Object.defineProperty(SVGElement.prototype, "getBBox", { configurable: true, value: () => ({ width: 60, height: 18 }) });
  const analytics: Extract<RunAnalytics, { kind: "ready" }> = { kind: "ready", completed: 100, provisional: false, participants: ["Alpha", "Beta", "Gamma", "Delta"].map((displayName, i) => ({
    participantId: `p${i}`, displayName, averageRank: i + 1, firstPlaceRate: .2, cumulativeNet: 0, rankCounts: i === 0 ? [50, 20, 20, 10] : [20, 30, 30, 20], points: [],
  })) };
  const { unmount } = render(<BenchmarkCharts analytics={analytics} onOpenMatch={vi.fn()} />);
  const measure = () => act(() => { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback => callback(0)); });
  expect(document.querySelector(".radar-label-metric")?.namespaceURI).toBe("http://www.w3.org/2000/svg");
  measure();
  expect(plot.domain).toEqual([0, 75]);
  expect(plot.geometry).toEqual({ cx: 200, cy: 136, outerRadius: 102 });
  fireEvent.click(screen.getByRole("button", { name: "Rank distribution options" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Rank distribution Alpha" }));
  measure();
  expect(plot.axes).toEqual(["Beta", "Gamma", "Delta"]);
  expect(plot.domain).toEqual([0, 75]);
  expect(document.body).not.toHaveTextContent(/\(p[0-4]\)/);
  const radarObserver = observers.filter(observer => [...observer.elements].some(element => element.classList.contains("radar-host"))).at(-1)!;
  expect(radarObserver.elements.size).toBe(6); // Host, legend, four full-roster label metrics.
  radarObserver.callback();
  const measurementFrame = next;
  expect(frames.has(measurementFrame)).toBe(true);
  unmount();
  expect(radarObserver.disconnect).toHaveBeenCalledOnce();
  expect(frames.has(measurementFrame)).toBe(false);
  const lastFrame = next;
  radarObserver.callback();
  expect(next).toBe(lastFrame);
});
