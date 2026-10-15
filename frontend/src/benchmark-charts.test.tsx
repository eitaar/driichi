import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { BenchmarkCharts } from "./benchmark-charts";
import type { RunAnalytics } from "./benchmark-analytics";

function analytics(n = 60): Extract<RunAnalytics, { kind: "ready" }> {
  return { kind: "ready", completed: n, provisional: n < 10, participants: ["z]{}", "a", "b"].map((participantId, i) => ({
    participantId, displayName: `Bot ${i}`, averageRank: n ? i + 1 : null, firstPlaceRate: n ? Number(i === 0) : null,
    cumulativeNet: n * (i === 0 ? 100 : -50), rankCounts: [n, 0, 0],
    points: Array.from({ length: n }, (_, j) => ({ sequence: j + 1, matchId: `m${j + 1}`, net: i === 0 ? 100 : -50, cumulative: (j + 1) * (i === 0 ? 100 : -50), movingRank: i + 1, movingN: Math.min(j + 1, 10) })),
  })) };
}
it("shows four families and keeps full-run cumulative values with a separately labeled recent-window net", () => {
  render(<BenchmarkCharts analytics={analytics()} onOpenMatch={vi.fn()} />);
  for (const name of ["Cumulative net score", "Moving-average rank", "Rank distribution", "Per-Match net score"]) expect(screen.getByRole("heading", { name })).toBeVisible();
  fireEvent.change(screen.getByLabelText("Cumulative net score window"), { target: { value: "50" } });
  expect(screen.getByText(/Window net:.*Bot 0.*5,000/)).toBeVisible();
  const statistics = screen.getByRole("table", { name: "Participant statistics" });
  expect(statistics).toHaveTextContent("6,000");
  expect(within(statistics).getByRole("columnheader", { name: "Rank 1 count / rate" })).toBeVisible();
  expect(within(statistics).getAllByText("60 / 100.0%")).toHaveLength(3);
  expect(screen.getByLabelText("Moving-average rank window")).toHaveValue("all");
  expect(screen.getByText("All completed Matches: n = 60; denominator per Participant.")).toBeVisible();
  expect(screen.queryByText("Rank 4")).not.toBeInTheDocument();
});
it("initializes the net Bot only when standings exist and preserves manual choices on refresh", () => {
  const { rerender } = render(<BenchmarkCharts analytics={analytics(0)} onOpenMatch={vi.fn()} />);
  rerender(<BenchmarkCharts analytics={analytics()} onOpenMatch={vi.fn()} />);
  expect(screen.getByLabelText("Net score Participant")).toHaveValue("z]{}");
  fireEvent.change(screen.getByLabelText("Net score Participant"), { target: { value: "a" } });
  fireEvent.click(screen.getByLabelText("Cumulative net score Bot 0 (z]{})"));
  rerender(<BenchmarkCharts analytics={analytics(61)} onOpenMatch={vi.fn()} />);
  expect(screen.getByLabelText("Net score Participant")).toHaveValue("a");
  expect(screen.getByLabelText("Cumulative net score Bot 0 (z]{})")).not.toBeChecked();
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
  expect(within(screen.getByRole("region", { name: "Rank distribution" })).queryByRole("button")).not.toBeInTheDocument();
});
