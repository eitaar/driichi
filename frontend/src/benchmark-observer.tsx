import { useEffect, useId, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, ApiProblem, problemFrom, type BenchmarkLive } from "./api";
import { clearAdminSession } from "./admin-session";
import type { RunAnalytics } from "./benchmark-analytics";
import { Button } from "./components/ui/button";
import { Field, FieldGroup, FieldLabel } from "./components/ui/field";
import { ThreeTable } from "./game/three-table";
import { seatPositions } from "./game/orientation";
import type { ProjectedState } from "./game/types";
import { tileLabel } from "./game/tiles";

export function focusedSeat(projection: ProjectedState | null, participantId: string | null): number | undefined {
  return projection?.players?.find(player => player.participant_id === participantId)?.seat;
}

function validProjection(projection: ProjectedState | null): boolean {
  if (projection === null) return true;
  const players = projection?.players;
  const count = seatPositions(projection?.mode).length;
  return projection?.audience === "benchmark_admin" && Array.isArray(players) && players.length === count
    && players.every(player => player && typeof player.participant_id === "string" && player.participant_id.length > 0
      && typeof player.display_name === "string" && Number.isInteger(player.seat) && player.seat >= 0 && player.seat < count
      && (player.hand === undefined || (Array.isArray(player.hand) && player.hand.every(Number.isInteger))))
    && new Set(players.map(player => player.participant_id)).size === count
    && new Set(players.map(player => player.seat)).size === count;
}

type Observation = {
  lifetime: string; snapshot: BenchmarkLive | null; lastSnapshotAt: number | null;
  failed: boolean; reading: boolean; error: string | null;
};
const empty = (lifetime: string): Observation => ({ lifetime, snapshot: null, lastSnapshotAt: null, failed: false, reading: false, error: null });

export function BenchmarkObserver({ runId, roomCode, analytics }: { runId: string; roomCode: string; analytics: RunAnalytics }) {
  const client = useQueryClient();
  const lifetime = JSON.stringify([runId, roomCode]);
  const [open, setOpen] = useState(false);
  const [observation, setObservation] = useState(() => empty(lifetime));
  const [now, setNow] = useState(Date.now);
  const [focusedParticipantId, setFocusedParticipantId] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [fullscreenNote, setFullscreenNote] = useState<string | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const tableVisual = useRef<HTMLDivElement>(null);
  const refresh = useRef(() => {});
  const focusId = useId();

  useEffect(() => {
    setObservation(empty(lifetime));
    if (!open) return;
    let active = true;
    let inFlight = false;
    let pendingRefresh = false;
    let appliedRevision = -1;
    let terminal = false;
    const controller = new AbortController();
    const purge = () => {
      active = false;
      controller.abort();
      window.clearInterval(tick);
      refresh.current = () => {};
      setObservation(empty(lifetime));
      setFocusedParticipantId(null);
    };
    // shortcut: current backend has GET only; replace compatibility Polling when authorized SSE ships.
    const read = async () => {
      if (!active || terminal) return;
      if (inFlight) { pendingRefresh = true; return; }
      inFlight = true;
      setObservation(value => ({ ...value, reading: true }));
      try {
        const snapshot = await api.getBenchmarkLive(roomCode, controller.signal);
        if (!active) return;
        const valid = snapshot?.benchmark?.run_id === runId && Number.isSafeInteger(snapshot.revision)
          && snapshot.revision >= 0 && snapshot.revision >= appliedRevision
          && ["running", "completed", "stopped", "failed", "interrupted"].includes(snapshot.benchmark.status)
          && validProjection(snapshot.projection);
        if (!valid) {
          setObservation(value => ({ ...value, snapshot: null }));
          throw new Error("Current Run observation could not be verified.");
        }
        appliedRevision = snapshot.revision;
        terminal = snapshot.benchmark?.status !== "running";
        const at = Date.now();
        setNow(at);
        setObservation({ lifetime, snapshot: terminal ? { ...snapshot, projection: null } : snapshot,
          lastSnapshotAt: at, failed: false, reading: false, error: null });
      } catch (error) {
        if (!active) return;
        if (error instanceof ApiProblem && (error.status === 401 || error.status === 403)) {
          purge(); clearAdminSession(client); return;
        }
        setObservation(value => ({ ...value, failed: true, reading: false, error: problemFrom(error).detail ?? "Observation failed." }));
      } finally {
        inFlight = false;
        if (active) {
          setObservation(value => ({ ...value, reading: false }));
          if (pendingRefresh) { pendingRefresh = false; void read(); }
        }
      }
    };
    let elapsed = 0;
    const tick = window.setInterval(() => {
      setNow(Date.now());
      elapsed += 1;
      if (elapsed % 5 === 0) void read();
    }, 1000);
    refresh.current = () => { void read(); };
    window.addEventListener("admin-session-cleared", purge);
    void read();
    return () => { purge(); window.removeEventListener("admin-session-cleared", purge); };
  }, [open, lifetime, runId, roomCode, client]);

  useEffect(() => {
    const changed = () => {
      if (document.fullscreenElement !== host.current) {
        setExpanded(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("fullscreenchange", changed);
    return () => { document.removeEventListener("fullscreenchange", changed); };
  }, []);

  const current = open && observation.lifetime === lifetime ? observation : empty(lifetime);
  const snapshot = current.snapshot;
  const projection = snapshot?.projection ?? null;
  const seat = focusedSeat(projection, focusedParticipantId);
  const focused = projection?.players?.find(player => player.seat === seat);
  const stale = current.failed || current.lastSnapshotAt === null || now - current.lastSnapshotAt >= 45000;
  const status = !snapshot ? (current.failed ? "Stale" : "Connecting") : snapshot.benchmark?.status !== "running"
    ? snapshot.benchmark?.status : stale ? "Stale" : !projection ? "Waiting" : "Fresh";
  const exit = async () => {
    if (document.fullscreenElement === host.current) await document.exitFullscreen();
    setExpanded(false); trigger.current?.focus();
  };
  return <details className="admin-disclosure" onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>Live table — all current hands</summary>
    {open && <div ref={host} className="benchmark-observer" data-expanded={expanded} onKeyDown={event => {
      if (event.key === "Escape" && expanded) { event.preventDefault(); void exit(); }
    }}>
      <div className="benchmark-observer-toolbar">
        <p role="status">Polling · {status}{current.reading ? " · Updating" : ""}</p>
        <Button variant="outline" onClick={() => refresh.current()}>Refresh table</Button>
        <Button ref={trigger} variant="outline" onClick={async () => {
          if (expanded) { await exit(); return; }
          setExpanded(true); setFullscreenNote(null);
          try { if (!host.current?.requestFullscreen) throw new Error(); await host.current.requestFullscreen(); }
          catch { setFullscreenNote("Browser fullscreen unavailable. Expanded in-page view."); }
        }}>{expanded ? "Exit fullscreen" : "Fullscreen table"}</Button>
      </div>
      {fullscreenNote && <p role="status">{fullscreenNote}</p>}
      <p>Last successful update: {current.lastSnapshotAt === null ? "Not yet received" : new Date(current.lastSnapshotAt).toISOString()}</p>
      {current.error && <p role="alert">{current.error}</p>}
      {!snapshot && !current.failed && <p role="status">Connecting to live table…</p>}
      {snapshot && <p className="field-hint">Read-only Admin view · Revision {snapshot.revision} · GET compatibility mode; SSE unavailable</p>}
      {snapshot && !projection && <p>{snapshot.benchmark?.status === "running" ? "Waiting for next Match." : "Run ended. No active Match."}</p>}
      <div className="benchmark-observer-grid">
        <div>
          <FieldGroup><Field><FieldLabel htmlFor={focusId}>Focus Participant</FieldLabel>
            <select id={focusId} value={focusedParticipantId ?? ""} onChange={event => {
              setFocusedParticipantId(event.target.value || null);
              if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
                tableVisual.current?.animate?.([{ opacity: 0.94 }, { opacity: 1 }], { duration: 150 });
              }
            }}>
              <option value="">Default view</option>
              {focusedParticipantId && !focused && <option value={focusedParticipantId}>Selected Participant — awaiting current Seat</option>}
              {projection?.players?.map(player => <option key={player.participant_id} value={player.participant_id}>{player.display_name} ({player.participant_id}) · Seat {player.seat + 1}</option>)}
            </select>
          </Field></FieldGroup>
          <p>{focused ? `Focused: ${focused.display_name} · Seat ${focused.seat + 1}` : "Default view · No focused current Seat"}</p>
          <div ref={tableVisual} className="benchmark-live replay-table-wrap"><ThreeTable projection={projection} viewSeat={seat} room={null} surface="replay" reducedMotion /></div>
        </div>
        <section aria-label="Run leaderboard"><h2>Run leaderboard</h2>
          {analytics.kind === "invalid" ? <p>Standings unavailable: {analytics.reason}</p>
            : analytics.completed === 0 ? <p>No completed Matches yet.</p> : <>
              <p>{analytics.provisional ? "Provisional · " : ""}{analytics.completed} completed Matches · Persisted results only</p>
              <ol>{[...analytics.participants].sort((a, b) => (a.averageRank ?? Infinity) - (b.averageRank ?? Infinity)
                || (a.participantId < b.participantId ? -1 : a.participantId > b.participantId ? 1 : 0)).map(player => <li key={player.participantId}>
                  <strong>{player.displayName} ({player.participantId})</strong>
                  <div>Mean rank: {player.averageRank?.toFixed(2) ?? "Unavailable"}</div>
                  <div>First-place rate: {player.firstPlaceRate === null ? "Unavailable" : `${(player.firstPlaceRate * 100).toFixed(1)}%`}</div>
                  <div>Net score: {player.cumulativeNet.toLocaleString()}</div>
                </li>)}</ol>
            </>}
        </section>
      </div>
      {projection && <details><summary>Current hands as text</summary><ul>{projection.players?.map(player => <li key={player.participant_id}>{player.display_name} ({player.participant_id}): {player.hand?.map(tileLabel).join(", ")}</li>)}</ul></details>}
    </div>}
  </details>;
}
