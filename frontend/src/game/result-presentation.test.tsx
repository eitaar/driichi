import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../components/theme-provider";
import { DrawResult, MatchVictoryPage, MobileResultSummary, RoundWinSurface, ScoreTransfer } from "./result-presentation";
import type { RoomSnapshot } from "./types";
import type { KyokuSettlement, WinResult } from "./results";

const room = { game_mode: "4p-red-east", phase: "playing", roster: [0, 1, 2, 3].map(seat => ({ participant_id: `p${seat}`, seat, display_name: `Player ${seat}`, character_id: null })) } as RoomSnapshot;
const win: WinResult = { event_index: 1, winner: "p0", seat: 0, target: 1, tsumo: false, concealed: [0, 1, 16, 17, 18, 36, 40, 44, 48, 52, 56, 60, 64, 68], melds: [], winning_tile: 16, winning_tile_index: 2, yaku: [{ id: 1, name: "riichi", han: 1 }], evaluation: { Ordinary: { han: 1, fu: 30 } }, win_points: 1000 };
const settlement: KyokuSettlement = { changes: [0, 1, 2, 3].map(seat => ({ participant_id: `p${seat}`, seat, before: 25000, delta: seat === 0 ? 1000 : seat === 1 ? -1000 : 0, after: seat === 0 ? 26000 : seat === 1 ? 24000 : 25000 })), pot_before: 0, pot_after: 0, draw_reason: "suucha_riichi" };

function finalRoom(count: 3 | 4): RoomSnapshot {
  const scores = count === 3 ? [-1200, 68200, 38000] : [-1200, 68200, 18000, 15000];
  const roster = room.roster!.slice(0, count).map(player => ({ ...player, character_id: `character-${player.seat}` }));
  return { ...room, phase: "post_match", game_mode: count === 3 ? "3p-red-east" : "4p-red-east", roster: [...roster].reverse(), result: {
    mode: count === 3 ? "ThreePlayerRedEast" : "FourPlayerRedEast", final_scores: scores,
    players: roster.map((player, seat) => ({ ...player, kind: "Human", final_score: scores[seat], rank: 1 + scores.filter(score => score > scores[seat]).length })),
  } } as RoomSnapshot;
}

describe("MatchVictoryPage", () => {
  it.each([3, 4] as const)("renders authoritative %ip standings and the rank-one Participant's original art", count => {
    render(<ThemeProvider><MatchVictoryPage room={finalRoom(count)} onReturn={() => {}} notice={null} /></ThemeProvider>);
    const standings = screen.getByRole("list", { name: "Final standings" });
    const rows = within(standings).getAllByRole("listitem");
    expect(rows).toHaveLength(count);
    expect(rows[0]).toHaveTextContent("1Player 168,200");
    expect(rows.at(-1)).toHaveTextContent("Player 0-1,200");
    expect(screen.getByRole("img", { name: "Player 1 portrait" })).toHaveAttribute("src", "/assets/characters/character-1/portrait.webp");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Continue|Review/ })).not.toBeInTheDocument();
  });
  it("keeps supplied recovery notices and Return beside the real Theme action", () => {
    localStorage.removeItem("driichi-theme");
    const onReturn = vi.fn();
    render(<ThemeProvider><MatchVictoryPage room={finalRoom(4)} onReturn={onReturn} notice={<p role="alert">Connection lost. Reconnecting.</p>} /></ThemeProvider>);
    expect(screen.getByRole("alert")).toHaveTextContent("Connection lost. Reconnecting.");
    const button = screen.getByRole("button", { name: "Return to room" });
    const theme = screen.getByRole("button", { name: "Theme: System. Switch to Light" });
    expect(button.parentElement).toBe(theme.parentElement);
    fireEvent.click(theme);
    expect(screen.getByRole("button", { name: "Theme: Light. Switch to Dark" })).toBeVisible();
    fireEvent.click(button);
    expect(onReturn).toHaveBeenCalledTimes(1);
    localStorage.removeItem("driichi-theme");
  });
  it.each(["absent", "malformed", "mismatched roster"])("fails closed for %s standings without suppressing recovery", invalid => {
    const current = finalRoom(4);
    if (invalid === "absent") current.result = undefined;
    else if (invalid === "malformed") current.result = { ...current.result as object, final_scores: [1, 2] };
    else current.roster = current.roster!.map(player => ({ ...player, participant_id: `other-${player.seat}` }));
    render(<ThemeProvider><MatchVictoryPage room={current} onReturn={() => {}} notice={<p role="alert">Return and reconnect to recover results.</p>} /></ThemeProvider>);
    expect(screen.queryByTestId("results-panel")).not.toBeInTheDocument();
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeVisible();
    expect(screen.getByRole("button", { name: "Return to room" })).toBeVisible();
  });
});

describe("result-presentation", () => {
  it.each(["win", "final"])("hides successful and failed %s art only inside actual Drawer details (style contract, not pixels)", async kind => {
    const current = kind === "final" ? finalRoom(4) : { ...room, roster: room.roster!.map(player => ({ ...player, character_id: "original" })), kyoku_end: { id: { match_id: "m", kyoku_sequence: 1, step_index: 0 }, content: { Win: win }, published_at_ms: 0, deadline_at_ms: 5000, remaining_ms: 5000, required: [], confirmed: [] } };
    const view = render(<MobileResultSummary room={current} />);
    const mainArt = view.container.querySelector(".mobile-result .results-winner-portrait");
    fireEvent.click(screen.getByRole("button", { name: "Expand" }));
    const dialog = await screen.findByRole("dialog");
    const selector = kind === "final" ? ".results-winner-hero-portrait" : ".round-win-portrait";
    const art = dialog.querySelector(selector)!;
    expect(art.tagName).toBe("IMG");
    fireEvent.error(art);
    const fallback = dialog.querySelector(`${selector}.asset-fallback`)!;
    expect(fallback).not.toBeNull();
    expect(fallback.closest(".result-details")).not.toBeNull();
    const css = readFileSync(resolve(process.cwd(), "src/game/result.css"), "utf8");
    // Equal specificity to the fallback grid rule, later source order; no global art hide.
    const hide = css.match(/:root \.result-details \.round-win-portrait,\s*:root \.result-details \.results-winner-hero-portrait \{([^}]+)\}/);
    expect(hide?.[1]).toContain("display: none");
    expect(css.indexOf(hide?.[0] ?? "")).toBeGreaterThan(css.indexOf(":root .results-winner-portrait.asset-fallback"));
    if (kind === "win") {
      expect(mainArt).not.toBeNull();
      expect(mainArt!.closest(".result-details")).toBeNull();
    }
  });
  it("defines the shared Win modal single stretched track and bounded uncropped art contract", () => {
    const css = readFileSync(resolve(process.cwd(), "src/game/result.css"), "utf8");
    const modal = css.match(/\.gameplay-shell \.round-win-overlay\.result-win-modal \{([^}]+)\}/)![1];
    expect(modal).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(modal).toContain("justify-content: stretch");
    const surface = css.match(/\.result-win-modal \.round-win-surface \{([^}]+)\}/)![1];
    expect(surface).toContain("grid-template-columns: minmax(140px, 18rem) minmax(0, 1fr)");
    expect(surface).toContain("justify-self: stretch");
    expect(css).toContain("object-fit: contain");
    expect(css).toContain(".round-win-overlay.result-win-modal--replay");
  });
  it("keeps shared Tsumo details, reduced motion and portrait fallback without live actions", () => {
    const current = { ...room, roster: room.roster!.map(player => ({ ...player, character_id: "original" })) };
    const view = render(<RoundWinSurface result={{ ...win, tsumo: true, target: 0 }} room={current} assets={{}} reducedMotion />);
    expect(screen.getByTestId("round-win-surface")).toHaveAttribute("data-motion", "static");
    expect(screen.getByLabelText("Tsumo win details")).toHaveTextContent("1 han · 30 fu1,000 points");
    fireEvent.error(screen.getByRole("img", { name: "Player 0 portrait" }));
    expect(screen.getByRole("img", { name: "Player 0 portrait unavailable" })).toBeVisible();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    view.rerender(<RoundWinSurface result={{ ...win, seat: 1 }} room={current} assets={{}} />);
    expect(screen.queryByTestId("round-win-surface")).not.toBeInTheDocument();
  });
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
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.queryByText(/points/)).not.toBeInTheDocument();
  });
  it("uses only each current validated Win award and falls back when its Character cannot load", () => {
    const current = { ...room, roster: room.roster!.map(player => ({ ...player, character_id: "missing" })), kyoku_end: { id: { match_id: "m", kyoku_sequence: 1, step_index: 0 }, content: { Win: win }, published_at_ms: 0, deadline_at_ms: 5000, remaining_ms: 5000, required: [], confirmed: [] } };
    const view = render(<MobileResultSummary room={current} />);
    fireEvent.error(screen.getByRole("img", { name: "Player 0 portrait" }));
    expect(screen.getByRole("img", { name: "Player 0 portrait unavailable" })).toBeVisible();
    view.rerender(<MobileResultSummary room={{ ...current, kyoku_end: { ...current.kyoku_end, content: { Win: { ...win, winner: "p2", seat: 2, win_points: 7700 } } } }} />);
    expect(screen.getByRole("heading", { name: "Player 2 · Ron" })).toBeVisible();
    expect(screen.getByText("7,700 points")).toBeVisible();
    expect(screen.queryByText("1,000 points")).not.toBeInTheDocument();
    view.rerender(<MobileResultSummary room={{ ...current, kyoku_end: { ...current.kyoku_end, content: { Win: { ...win, seat: 2 } } } }} />);
    expect(screen.queryByRole("region", { name: "Match result" })).not.toBeInTheDocument();
  });
  it("expands authorized phone win metadata without a live confirmation", async () => {
    const current = { ...room, kyoku_end: { id: { match_id: "m", kyoku_sequence: 1, step_index: 0 }, content: { Win: win }, published_at_ms: 0, deadline_at_ms: 5000, remaining_ms: 5000, required: ["p0"], confirmed: [] } };
    render(<MobileResultSummary room={current} />);
    expect(screen.getByRole("heading", { name: "Player 0 · Ron" })).toBeVisible();
    expect(screen.getByText("1,000 points")).toBeVisible();
    expect(screen.getByRole("img", { name: "Player 0 portrait unavailable" })).toBeVisible();
    expect(screen.queryByLabelText("Winning hand")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Expand" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Winning hand")).toBeVisible();
    expect(within(dialog).getByText("1,000")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Continue" })).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
  });
});
