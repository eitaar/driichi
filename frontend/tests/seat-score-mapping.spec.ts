import { expect, test } from "@playwright/test";
import base from "./fixtures/task12-projection.json" with { type: "json" };
import oracle from "./fixtures/seat-score.json" with { type: "json" };

test.use({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });

// Catches Seat-indexed scores/names drifting apart during viewer or Match rotation.
for (const entry of oracle.cases) {
  for (const surface of ["Live", "Replay", "Admin"] as const) {
    test(`${surface} ${entry.mode} viewer ${entry.viewer}: identity, Seat and score survive rotation`, async ({ page }) => {
      const mode = entry.mode as keyof typeof base.projections;
      const participants = oracle.participants.slice(0, mode === "3p-red-east" ? 3 : 4);
      const state = (updated: boolean) => ({ ...base.projections[mode],
        audience: surface === "Live" ? "player" : surface === "Admin" ? "benchmark_admin" : "replay_admin",
        viewer_seat: updated ? entry.rotatedViewer : entry.viewer,
        decision: undefined,
        players: participants.map(p => ({ ...base.projections[mode].players[p.seat],
          ...p, seat: updated ? (mode === "3p-red-east" ? p.rotatedSeat3! : p.rotatedSeat4) : p.seat,
          score: updated ? p.updatedScore : p.score,
          hand: surface === "Live" && p.seat !== entry.viewer ? undefined : [1, 29, 49],
        })),
      });
      const room = { ...base.room_envelopes[mode], roster: participants, match_players: participants };
      let current = state(false);
      if (surface === "Live") {
        await page.addInitScript(({ room, state }) => {
          const Native = window.WebSocket;
          class Socket {
            static OPEN = 1;
            readyState = 1;
            onopen: (() => void) | null = null;
            onmessage: ((e: { data: string }) => void) | null = null;
            onclose: (() => void) | null = null;
            constructor(...args: ConstructorParameters<typeof WebSocket>) {
              if (!String(args[0]).endsWith("/ws/v1/rooms/123456/human")) return new Native(...args) as unknown as Socket;
              Object.assign(window, { scoreSocket: this });
              setTimeout(() => { this.onopen?.(); this.emit({ type: "snapshot", room, state }); }, 0);
            }
            emit(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
            send() {}
            close() {}
          }
          window.WebSocket = Socket as unknown as typeof WebSocket;
          sessionStorage.setItem("driichi:participant:123456", state.players.find(p => p.seat === state.viewer_seat)!.participant_id);
        }, { room, state: current });
        await page.goto("/room/123456/lobby");
      } else if (surface === "Replay") {
        await page.route("**/api/v1/admin/replays/SEAT-SCORE", route => route.fulfill({ json: {
          match_id: "SEAT-SCORE", source: "ranked", room_name: "Synthetic Seat evidence", game_mode: mode,
          started_at: "2026-09-16T00:00:00Z", completed_at: "2026-09-16T00:10:00Z", file_size: 512,
          availability: "available", replay_available: true, players: participants,
          frames: [false, true].map((updated, event_index) => ({ event_index, visible_event: { type: "start_kyoku" }, visible_state: state(updated), auxiliary_events: [] })),
        } }));
        await page.goto("/admin/replays/SEAT-SCORE");
      } else {
        const run = { run_id: "seat-score", room_code: "123456", game_mode: mode, target: 2, completed: 0, status: "running", reason: null, failed_match_id: null,
          roster: participants.map(p => ({ ...p, initial_seat: p.seat, participant_kind: "built_in_bot", character_id: null })), matches: [] };
        const benchmark = { run_id: "seat-score", target: 2, completed: 0, stop_requested: false, status: "running" };
        await page.route("**/api/v1/admin/benchmark/runs/seat-score", route => route.fulfill({ json: run }));
        await page.route("**/api/v1/admin/rooms/123456", route => route.fulfill({ json: { ...room, benchmark } }));
        await page.route("**/api/v1/admin/benchmark/rooms/123456/live", route => route.fulfill({ json: { revision: current === initial ? 1 : 2, benchmark, projection: current } }));
        const initial = current;
        await page.goto("/admin/benchmark/runs/seat-score");
        await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
        await page.getByRole("button", { name: "Details", exact: true }).click();
        await expect(page.getByText(/Polling · Fresh/)).toBeVisible();
        await page.getByLabel("Focus Participant").click();
        const focused = participants[entry.viewer];
        await page.getByRole("option", { name: `${focused.display_name} · Seat ${focused.seat + 1}`, exact: true }).click();
      }
      const table = page.getByTestId("three-table");
      const check = async (updated: boolean) => {
        await expect(table).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
        for (const position of ["bottom", "right", "top", "left"] as const) {
          const name = entry[position];
          const frame = table.locator(`.table-player-frame[data-position="${position}"]`);
          if (name === null) { await expect(frame).toHaveCount(0); continue; }
          await expect(frame).toContainText(name);
          const p = participants.find(p => p.display_name === name)!;
          await expect(table.getByRole("list", { name: "Table scores" })).toContainText(`${name}: ${(updated ? p.updatedScore : p.score).toLocaleString("en-US")}`);
        }
        await page.screenshot({ path: test.info().outputPath(`${surface}-${mode}-viewer${entry.viewer}-${updated ? "rotated" : "initial"}.png`), fullPage: surface !== "Admin" });
      };
      await check(false);
      current = state(true);
      if (surface === "Live") {
        await page.evaluate(({ room, state }) => {
          (window as unknown as { scoreSocket: { emit(value: unknown): void } }).scoreSocket.emit({ type: "room_update", room, state });
        }, { room, state: current });
      } else if (surface === "Replay") await page.getByRole("button", { name: "Next Event", exact: true }).click();
      else await page.getByRole("button", { name: "Refresh table" }).click();
      await check(true);
    });
  }
}
