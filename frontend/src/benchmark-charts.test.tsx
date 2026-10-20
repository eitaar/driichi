import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BenchmarkCharts } from "./benchmark-charts";
import type { RunAnalytics } from "./benchmark-analytics";

function analytics(n = 60): Extract<RunAnalytics, { kind: "ready" }> {
  return { kind: "ready", completed: n, provisional: n < 10, participants: ["z]{}", "a", "b"].map((participantId, i) => ({
    participantId, displayName: `Bot ${i}`, averageRank: n ? i + 1 : null, firstPlaceRate: n ? Number(i === 0) : null,
    cumulativeNet: n * (i === 0 ? 100 : -50), rankCounts: [n, 0, 0],
    points: Array.from({ length: n }, (_, j) => ({ sequence: j + 1, matchId: `m${j + 1}`, net: i === 0 ? 100 : -50, cumulative: (j + 1) * (i === 0 ? 100 : -50), movingRank: i + 1, movingN: Math.min(j + 1, 10) })),
  })) };
}
beforeEach(() => { vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} }); });
afterEach(() => { vi.unstubAllGlobals(); });
it("shows the hero and three support families while retaining full-run cumulative values", async () => {
  render(<BenchmarkCharts analytics={analytics()} onOpenMatch={vi.fn()} />);
  for (const name of ["Cumulative net score", "Moving-average rank", "Rank distribution", "Per-Match net score"]) expect(screen.getByRole("heading", { name, level: 2 })).toBeVisible();
  for (const name of ["Cumulative net score", "Moving-average rank", "Rank distribution", "Per-Match net score"]) {
    expect(screen.getByRole("button", { name: `${name} options` })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("region", { name: `${name} chart and options` })).toHaveAttribute("tabindex", "0");
  }
  expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  for (const name of ["Cumulative net score", "Moving-average rank", "Per-Match net score"]) {
    fireEvent.click(screen.getByRole("button", { name: `${name} options` }));
    expect(screen.getByRole("combobox", { name: `${name} window` })).toHaveTextContent("Recent 50");
  }
  expect(screen.getAllByText("n = 50 · Range: Recent 50.")).toHaveLength(3);
  expect(document.querySelectorAll(".benchmark-chart-hero [data-chart-family]")).toHaveLength(1);
  expect(document.querySelectorAll(".benchmark-chart-supporting [data-chart-family]")).toHaveLength(3);
  expect(screen.queryByRole("button", { name: "Performance" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("combobox", { name: "Cumulative net score window" }));
  const option = within(await screen.findByRole("listbox")).getByRole("option", { name: "Full Run" });
  fireEvent.pointerDown(option, { pointerType: "mouse" }); fireEvent.click(option);
  expect(screen.getByText("n = 60 · Range: Full Run.")).toBeVisible();
  expect(screen.queryByText(/Window net:/)).not.toBeInTheDocument();
  const statistics = screen.getByRole("table", { name: "Participant statistics" });
  expect(statistics).toHaveTextContent("6,000");
  expect(within(statistics).getByRole("columnheader", { name: "Rank 1 count / rate" })).toBeVisible();
  expect(within(statistics).getAllByText("60 / 100.0%")).toHaveLength(3);
  expect(screen.getByLabelText("Moving-average rank window")).toHaveTextContent("Recent 50");
  expect(screen.getByText("n = 60")).toBeVisible();
  expect(screen.queryByText("Rank 4")).not.toBeInTheDocument();
});
it("identifies the current range even when both windows contain every completed Match", async () => {
  render(<BenchmarkCharts analytics={analytics(6)} onOpenMatch={vi.fn()} />);
  expect(screen.getAllByText("n = 6 · Range: Recent 50.")).toHaveLength(3);
  const options = screen.getByRole("button", { name: "Cumulative net score options" });
  fireEvent.click(options);
  fireEvent.click(screen.getByRole("combobox", { name: "Cumulative net score window" }));
  const option = await screen.findByRole("option", { name: "Full Run" });
  fireEvent.pointerDown(option, { pointerType: "mouse" }); fireEvent.click(option);
  fireEvent.click(options);
  expect(screen.getByText("n = 6 · Range: Full Run.")).toBeVisible();
});
it("initializes the net Bot only when standings exist and preserves manual choices on refresh", async () => {
  const { rerender } = render(<BenchmarkCharts analytics={analytics(0)} onOpenMatch={vi.fn()} />);
  rerender(<BenchmarkCharts analytics={analytics()} onOpenMatch={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Per-Match net score options" }));
  fireEvent.click(screen.getByRole("button", { name: "Cumulative net score options" }));
  expect(screen.getByLabelText("Net score Participant")).toHaveTextContent("Bot 0");
  fireEvent.click(screen.getByRole("combobox", { name: "Net score Participant" }));
  const option = await screen.findByRole("option", { name: "Bot 1" });
  fireEvent.pointerDown(option, { pointerType: "mouse" }); fireEvent.click(option);
  fireEvent.click(screen.getByLabelText("Cumulative net score Bot 0"));
  for (const name of ["Per-Match net score", "Cumulative net score"]) {
    fireEvent.click(screen.getByRole("button", { name: `${name} options` }));
    expect(screen.getByRole("button", { name: `${name} options` })).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(screen.getByRole("button", { name: `${name} options` }));
  }
  rerender(<BenchmarkCharts analytics={analytics(61)} onOpenMatch={vi.fn()} />);
  expect(screen.getByLabelText("Net score Participant")).toHaveTextContent("Bot 1");
  expect(screen.getByLabelText("Cumulative net score Bot 0")).not.toBeChecked();
});
it("opens only identified Matches through keyboard-friendly actions and pages numeric history recent-first", () => {
  const open = vi.fn();
  render(<BenchmarkCharts analytics={analytics()} onOpenMatch={open} />);
  const table = screen.getByRole("table", { name: "Per-Match numeric results" });
  expect(within(table).getAllByRole("row")).toHaveLength(26);
  fireEvent.click(within(table).getByRole("button", { name: "View Match 60 details" }));
  expect(open).toHaveBeenLastCalledWith("m60");
  fireEvent.change(screen.getByLabelText("Selected Match sequence"), { target: { value: "7" } });
  fireEvent.click(screen.getByRole("button", { name: "View Match details" }));
  expect(open).toHaveBeenLastCalledWith("m7");
  fireEvent.click(screen.getByRole("button", { name: "Older Matches" }));
  expect(within(table).getByRole("button", { name: "View Match 35 details" })).toBeVisible();
  expect(within(screen.getByRole("region", { name: "Rank distribution" })).getAllByRole("button")).toEqual([screen.getByRole("button", { name: "Rank distribution options" })]);
});
