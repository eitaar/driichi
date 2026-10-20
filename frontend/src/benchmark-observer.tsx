import { useEffect, useId, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, ApiProblem, problemFrom, type BenchmarkLive } from "./api";
import { clearAdminSession } from "./admin-session";
import type { RunAnalytics } from "./benchmark-analytics";
import { Button } from "./components/ui/button";
import { Card, CardHeader, CardTitle, CardAction, CardContent, CardFooter } from "./components/ui/card";
import { Field, FieldLabel } from "./components/ui/field";
import { Eye } from "@phosphor-icons/react/dist/csr/Eye";
import { Trophy } from "@phosphor-icons/react/dist/csr/Trophy";
import { Info } from "@phosphor-icons/react/dist/csr/Info";
import { ArrowClockwise } from "@phosphor-icons/react/dist/csr/ArrowClockwise";
import { ArrowsOut } from "@phosphor-icons/react/dist/csr/ArrowsOut";
import { ThreeTable } from "./game/three-table";
import { seatPositions } from "./game/orientation";
import type { ProjectedState } from "./game/types";
import { tileLabel } from "./game/tiles";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectGroup, SelectItem } from "./components/ui/select";
import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "./components/ui/table";

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
  const [open, setOpen] = useState(true);
  const [observation, setObservation] = useState(() => empty(lifetime));
  const [now, setNow] = useState(Date.now);
  const [focusedParticipantId, setFocusedParticipantId] = useState<string | null>(null);
  const [focusMenuOpen, setFocusMenuOpen] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [details, setDetails] = useState(false);
  const [fullscreenNote, setFullscreenNote] = useState<string | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const reopen = useRef<HTMLButtonElement>(null);
  const previousExpanded = useRef(false);
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
      setFocusMenuOpen(false);
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
      }
    };
    document.addEventListener("fullscreenchange", changed);
    return () => { document.removeEventListener("fullscreenchange", changed); };
  }, []);

  useEffect(() => {
    if (previousExpanded.current && !expanded) (open ? trigger : reopen).current?.focus();
    previousExpanded.current = expanded;
  }, [expanded, open]);

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
    setExpanded(false);
  };
  const focusItems = [{ value: "", label: "Default view" },
    ...(focusedParticipantId && !focused ? [{ value: focusedParticipantId, label: "Selected Participant — awaiting current Seat" }] : []),
    ...(projection?.players?.map(player => ({ value: player.participant_id, label: `${player.display_name} · Seat ${player.seat + 1}` })) ?? [])];
  const players = projection?.players ?? [];
  const dealer = players.find(player => player.seat === projection?.dealer);
  const number = (value: unknown) => typeof value === "number" && Number.isFinite(value) ? value.toLocaleString("en") : "Unknown";
  return <div ref={host} className="benchmark-observer" data-expanded={expanded} onKeyDown={event => {
      if (event.key === "Escape" && !event.defaultPrevented && expanded && !focusMenuOpen) { event.preventDefault(); void exit(); }
    }}>
      <section className="benchmark-observation-grid" aria-label="Observation">
      <Card className="benchmark-observation"><CardHeader className="benchmark-observer-toolbar">
        <CardTitle><h2><Eye aria-hidden="true" />Observation</h2></CardTitle>
        <CardAction className="benchmark-observer-actions">
        <Button variant="outline" aria-label="Refresh table" disabled={!open} onClick={() => refresh.current()}><ArrowClockwise aria-hidden="true" /></Button>
        <Button ref={trigger} variant="outline" aria-label={expanded ? "Exit fullscreen" : "Fullscreen table"} disabled={!open && !expanded} onClick={async () => {
          if (expanded) { await exit(); return; }
          setExpanded(true); setFullscreenNote(null);
          try { if (!host.current?.requestFullscreen) throw new Error(); await host.current.requestFullscreen(); }
          catch { setFullscreenNote("Browser fullscreen unavailable. Expanded in-page view."); }
        }}><ArrowsOut aria-hidden="true" /></Button>
        <Button variant="outline" aria-expanded={details} onClick={() => setDetails(!details)}><Info aria-hidden="true" />Details</Button>
        </CardAction>
        {open && status !== "Fresh" && <p role="status">Polling · {status}</p>}
        {fullscreenNote && <p role="status">{fullscreenNote}</p>}
      </CardHeader><CardContent>
      {details && <div className="benchmark-observer-details">
        <p>Revision {snapshot?.revision ?? "Unknown"} · GET compatibility mode; SSE unavailable</p>
        {open && status === "Fresh" && <p>Polling · {status}{current.reading ? " · Updating" : ""}</p>}
        <p>Last successful update: {current.lastSnapshotAt === null ? "Not yet received" : new Date(current.lastSnapshotAt).toISOString()}</p>
        {projection && <details><summary>Current hands as text</summary><ul>{players.map(player => <li key={player.participant_id}>{player.display_name}: {player.hand?.map(tileLabel).join(", ")}</li>)}</ul></details>}
        {open && <Button variant="outline" onClick={() => setOpen(false)}>Close observation</Button>}
      </div>}
      {current.error && <p role="alert">{current.error}</p>}
      {open && !snapshot && !current.failed && <p role="status">Connecting to live table…</p>}
      {snapshot && !projection && <p>{snapshot.benchmark?.status === "running" ? "Waiting for next Match." : "Run ended. No active Match."}</p>}
      {open ? <><Field orientation="horizontal" className="benchmark-observer-focus"><FieldLabel htmlFor={focusId}>Focus Participant</FieldLabel>
            <Select items={focusItems} open={focusMenuOpen} onOpenChange={setFocusMenuOpen} value={focusedParticipantId ?? ""} onValueChange={value => {
              if (typeof value !== "string") return;
              setFocusedParticipantId(value || null);
            }}>
              <SelectTrigger id={focusId} className="w-full min-w-0"><SelectValue /></SelectTrigger>
              <SelectContent portalContainer={host}><SelectGroup>{focusItems.map(item => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent>
            </Select>
          </Field>
          <p className="sr-only">{focused ? `Focused: ${focused.display_name} · Seat ${focused.seat + 1}` : "Default view · No focused current Seat"}</p>
        <div ref={tableVisual} className="benchmark-live"><ThreeTable projection={projection} viewSeat={seat} room={null} surface="replay" reducedMotion /></div>
        </> : <Button ref={reopen} variant="outline" onClick={() => setOpen(true)}>Open observation</Button>}
      </CardContent><CardFooter className="benchmark-observation-status">
        <span aria-label="Elapsed since Run start: unknown">--:--:-- elapsed</span>
        <span>{current.lastSnapshotAt === null ? "Not yet received" : `${new Date(current.lastSnapshotAt).toISOString().slice(11, 19)} UTC`}</span>
      </CardFooter></Card>
      <div className="benchmark-observer-rail">
        <Card role="region" aria-label="Run leaderboard" className="benchmark-observer-leaderboard"><CardHeader><CardTitle><h2><Trophy aria-hidden="true" />Run leaderboard</h2></CardTitle></CardHeader><CardContent>
          {analytics.kind === "invalid" ? <p>Standings unavailable: {analytics.reason}</p>
            : analytics.completed === 0 ? <p>No completed Matches yet.</p> : <>
              <p className="text-sm font-normal text-muted-foreground">{analytics.provisional ? "Provisional · " : ""}{analytics.completed} completed Matches · Persisted results only</p>
              <Table aria-label="Run leaderboard" className="tabular-nums"><TableHeader><TableRow><TableHead>Participant</TableHead><TableHead className="whitespace-normal">Mean rank</TableHead><TableHead className="whitespace-normal">First-place rate</TableHead><TableHead className="whitespace-normal">Net score</TableHead></TableRow></TableHeader><TableBody>{[...analytics.participants].sort((a, b) => (a.averageRank ?? Infinity) - (b.averageRank ?? Infinity)
                || (a.participantId < b.participantId ? -1 : a.participantId > b.participantId ? 1 : 0)).map(player => <TableRow key={player.participantId}>
                  <TableCell className="whitespace-normal">{player.displayName}</TableCell>
                  <TableCell>{player.averageRank?.toFixed(2) ?? "Unavailable"}</TableCell>
                  <TableCell>{player.firstPlaceRate === null ? "Unavailable" : `${(player.firstPlaceRate * 100).toFixed(1)}%`}</TableCell>
                  <TableCell>{player.cumulativeNet.toLocaleString()}</TableCell>
                </TableRow>)}</TableBody></Table>
            </>}
        </CardContent></Card>
        <Card role="region" aria-label="Current Match" className="benchmark-observer-current-match"><CardHeader><CardTitle><h2>Current Match</h2></CardTitle></CardHeader><CardContent>
          {!projection ? <p>{open ? "No current Match." : "Observation closed."}</p> : <>
            <dl className="benchmark-observer-match-facts">
              <div><dt>Round</dt><dd>{typeof projection.round === "string" && typeof projection.kyoku === "number" ? `${projection.round} ${number(projection.kyoku)}` : "Unknown"}</dd></div>
              <div className="benchmark-observer-dealer"><dt>Dealer</dt><dd>{dealer?.display_name ?? "Unknown"}</dd></div>
              <div><dt>Honba</dt><dd>{number(projection.honba)}</dd></div>
              <div><dt>Kyotaku</dt><dd>{number(projection.kyotaku)}</dd></div>
              <div><dt>Wall</dt><dd>{number(projection.remaining_wall)}</dd></div>
            </dl>
            <Table aria-label="Current Match points"><TableHeader><TableRow><TableHead>Participant</TableHead><TableHead>Seat</TableHead><TableHead>Points</TableHead></TableRow></TableHeader><TableBody>
              {players.map(player => <TableRow key={player.participant_id}><TableCell>{player.display_name}</TableCell><TableCell>{player.seat + 1}</TableCell><TableCell>{number(player.score)}</TableCell></TableRow>)}
            </TableBody></Table>
          </>}
        </CardContent></Card>
      </div></section>
    </div>;
}
