import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { BenchmarkMatchDetail } from "./benchmark-match-detail";
import type { BenchmarkMatchEvidence, ReplayAvailability } from "./benchmark-types";
const roster = ["a", "b", "c"].map((participant_id, initial_seat) => ({ participant_id, initial_seat, display_name: "Same bot", participant_kind: "built_in_bot", character_id: null }));
const match = (replay_availability: ReplayAvailability): BenchmarkMatchEvidence => ({ match_id: "match/a", sequence: 7, replay_availability, results: roster.map((p, seat) => ({ participant_id: p.participant_id, seat, rank: seat + 1, final_score: 45000 - seat * 10000 })) });
it.each(["unknown", "available", "unavailable", "too_large"] as const)("retains real placements and points for %s without fabricating a Replay", (state) => {
  render(<BenchmarkMatchDetail match={match(state)} roster={roster} onClose={vi.fn()} />);
  expect(screen.getByRole("dialog")).toHaveTextContent("45,000");
  expect(screen.getByRole("table", { name: "Match placements" })).toHaveTextContent("Same bot");
  expect(screen.getAllByRole("cell", { name: "Same bot" })).toHaveLength(3);
  expect(screen.getByRole("table", { name: "Match placements" })).not.toHaveTextContent(/\([abc]\)/);
  expect(screen.getByRole("dialog")).toHaveTextContent("match/a");
  if (state === "available") expect(screen.getByRole("link", { name: "Open Replay" })).toHaveAttribute("href", "/admin/replays/match%2Fa");
  else expect(screen.queryByRole("link", { name: "Open Replay" })).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent(state === "unknown" ? "not verified" : state === "too_large" ? "too large" : state === "unavailable" ? "unavailable" : "available");
});
it("offers Retry only with a real metadata retry boundary and closes the shared detail surface", () => {
  const retry = vi.fn(), close = vi.fn();
  render(<BenchmarkMatchDetail match={match("unknown")} roster={roster} onClose={close} onRetryReplay={retry} />);
  fireEvent.click(screen.getByRole("button", { name: "Retry Replay status" }));
  expect(retry).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(close).toHaveBeenCalledOnce();
});
