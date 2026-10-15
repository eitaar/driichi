import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DrawResult, MobileResultSummary, RoundWinSurface, ScoreTransfer } from "./result-presentation";
import type { RoomSnapshot } from "./types";
import type { KyokuSettlement, WinResult } from "./results";

const room = { game_mode: "4p-red-east", phase: "playing", roster: [0, 1, 2, 3].map(seat => ({ participant_id: `p${seat}`, seat, display_name: `Player ${seat}`, character_id: null })) } as RoomSnapshot;
const win: WinResult = { event_index: 1, winner: "p0", seat: 0, target: 1, tsumo: false, concealed: [0, 1, 16, 17, 18, 36, 40, 44, 48, 52, 56, 60, 64, 68], melds: [], winning_tile: 16, winning_tile_index: 2, yaku: [{ id: 1, name: "riichi", han: 1 }], evaluation: { Ordinary: { han: 1, fu: 30 } }, win_points: 1000 };
const settlement: KyokuSettlement = { changes: [0, 1, 2, 3].map(seat => ({ participant_id: `p${seat}`, seat, before: 25000, delta: seat === 0 ? 1000 : seat === 1 ? -1000 : 0, after: seat === 0 ? 26000 : seat === 1 ? 24000 : 25000 })), pot_before: 0, pot_after: 0, draw_reason: "suucha_riichi" };

describe("result-presentation", () => {
  it("shows only the supplied winner hand and exact yaku/points", () => {
    render(<RoundWinSurface result={win} room={room} assets={{}} />);
    expect(within(screen.getByRole("group", { name: "14 visible winning hand tiles" })).getAllByRole("img")).toHaveLength(14);
    expect(screen.getByLabelText("Authoritative yaku")).toHaveTextContent("Riichi1 han");
    expect(screen.getByText("1,000")).toBeVisible();
    expect(screen.queryByText("Player 1")).not.toBeInTheDocument();
  });
  it("exposes before, signed delta and after while refusing duplicate or mismatched identities", () => {
    const view = render(<ScoreTransfer settlement={settlement} room={room} />);
    const rows = within(screen.getByLabelText("Score transfer")).getAllByRole("listitem");
    expect(within(rows[0]).getByLabelText("Before 25,000")).toBeVisible();
    expect(within(rows[0]).getByLabelText("Change +1,000")).toBeVisible();
    expect(within(rows[0]).getByLabelText("After 26,000")).toBeVisible();
    expect(within(rows[1]).getByLabelText("Change −1,000")).toBeVisible();
    view.rerender(<ScoreTransfer settlement={{ ...settlement, changes: settlement.changes.map(() => settlement.changes[0]) }} room={room} />);
    expect(screen.queryByLabelText("Score transfer")).not.toBeInTheDocument();
    view.rerender(<ScoreTransfer settlement={{ ...settlement, changes: settlement.changes.map(entry => ({ ...entry, seat: (entry.seat + 1) % 4 })) }} room={room} />);
    expect(screen.queryByLabelText("Score transfer")).not.toBeInTheDocument();
  });
  it("lets the reader switch Draw views immediately without progression", () => {
    render(<DrawResult settlement={settlement} room={room} />);
    expect(screen.getByRole("button", { name: "Result" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("heading", { name: "suucha_riichi" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Scores" }));
    expect(screen.getByLabelText("Score transfer")).toContainElement(screen.getByLabelText("Change +1,000"));
    expect(screen.queryByRole("button", { name: "Continue" })).not.toBeInTheDocument();
  });
  it("labels a phone Draw truthfully even when no reason is supplied", () => {
    render(<MobileResultSummary room={{ ...room, kyoku_end: { id: { match_id: "m", kyoku_sequence: 1, step_index: 0 }, content: { Draw: { ...settlement, draw_reason: undefined } }, published_at_ms: 0, deadline_at_ms: 5000, remaining_ms: 5000, required: [], confirmed: [] } }} />);
    expect(screen.getByRole("heading", { name: "Draw" })).toBeVisible();
  });
  it("expands authorized phone win metadata without a live confirmation", async () => {
    const current = { ...room, kyoku_end: { id: { match_id: "m", kyoku_sequence: 1, step_index: 0 }, content: { Win: win }, published_at_ms: 0, deadline_at_ms: 5000, remaining_ms: 5000, required: ["p0"], confirmed: [] } };
    render(<MobileResultSummary room={current} />);
    expect(screen.getByRole("heading", { name: "Player 0 · Ron" })).toBeVisible();
    expect(screen.queryByLabelText("Winning hand")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Expand" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Winning hand")).toBeVisible();
    expect(within(dialog).getByText("1,000")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Continue" })).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
  });
});
