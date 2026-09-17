import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Pause, Play, Trash, WarningCircle } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, problemFrom, type ProblemDetails, type ReplayFrame, type ReplaySummary, type ReplayView } from "./api";
import { navigate } from "./routes";
import { PixiTable } from "./game/pixi-table";
import type { ProjectedState, RoomSnapshot } from "./game/types";

const replayKeys = {
  list: (offset: number) => ["admin", "replays", offset] as const,
  view: (matchId: string) => ["admin", "replay", matchId] as const,
};

function ReplayTopbar() {
  return (
    <header className="topbar">
      <a className="brand" href="/" onClick={(event) => { event.preventDefault(); navigate("/"); }} aria-label="Double Riichi home">
        <span className="brand-mark" aria-hidden="true">二</span>
        <span>DOUBLE RIICHI</span>
      </a>
      <nav aria-label="Replay navigation">
        <a className="nav-link" href="/admin" onClick={(event) => { event.preventDefault(); navigate("/admin"); }}>Rooms <ArrowLeft aria-hidden="true" weight="regular" /></a>
        <a className="nav-link" href="/admin/replays" onClick={(event) => { event.preventDefault(); navigate("/admin/replays"); }}>Replay library</a>
        <button className="text-button" onClick={() => { void api.logoutAdmin().finally(() => navigate("/admin/login")); }}>Sign out</button>
      </nav>
    </header>
  );
}

function ProblemMessage({ problem, title = "Replay unavailable" }: { problem: ProblemDetails; title?: string }) {
  return (
    <section className="replay-state" role="alert">
      <WarningCircle aria-hidden="true" weight="regular" />
      <p className="eyebrow">REQUEST STATUS</p>
      <h2>{title}</h2>
      <p>{problem.detail ?? problem.title ?? "The host could not load this Replay."}</p>
      {problem.request_id && <code className="request-id">Request ID {problem.request_id}</code>}
      <button className="button button-secondary" onClick={() => navigate("/admin/replays")}>
        Back to Replay library <ArrowLeft aria-hidden="true" weight="regular" />
      </button>
    </section>
  );
}

function ReplayMeta({ replay }: { replay: ReplaySummary }) {
  return (
    <dl className="replay-meta">
      <div><dt>Source</dt><dd>{replay.source === "ranked" ? "Ranked" : "Room"}</dd></div>
      <div><dt>Mode</dt><dd>{replay.game_mode}</dd></div>
      <div><dt>Completed</dt><dd><time dateTime={replay.completed_at}>{replay.completed_at}</time></dd></div>
      <div><dt>File</dt><dd>{replay.file_size.toLocaleString()} bytes</dd></div>
    </dl>
  );
}

function ReplayList({ onView }: { onView: (matchId: string) => void }) {
  const [offset, setOffset] = useState(0);
  const query = useQuery({ queryKey: replayKeys.list(offset), queryFn: () => api.listAdminReplays(offset, 50) });
  const queryClient = useQueryClient();
  const [deleteReplay, setDeleteReplay] = useState<ReplaySummary | null>(null);
  const deletion = useMutation({
    mutationFn: (matchId: string) => api.deleteAdminReplay(matchId),
    onSuccess: () => {
      setDeleteReplay(null);
      void queryClient.invalidateQueries({ queryKey: ["admin", "replays"] });
    },
  });
  if (query.isLoading) return <main className="replay-main"><ReplayHeader /><ReplayLoading /></main>;
  if (query.isError) return <main className="replay-main"><ReplayHeader /><ProblemMessage problem={problemFrom(query.error)} title="Replay library unavailable" /></main>;
  const data = query.data;
  if (!data) return <main className="replay-main"><ReplayHeader /><ReplayLoading /></main>;
  return (
    <main className="replay-main">
      <ReplayHeader />
      {data.replays.length === 0 ? (
        <section className="replay-empty"><p className="eyebrow">ARCHIVE</p><h2>No Replays yet.</h2><p>Completed Replays will appear here when Replay saving is enabled.</p></section>
      ) : (
        <>
          <section className="replay-list" aria-label="Saved Replays">
            {data.replays.map((replay) => <ReplayRow key={replay.match_id} replay={replay} onView={onView} onDelete={setDeleteReplay} />)}
          </section>
          <nav className="replay-pagination" aria-label="Replay pages">
            <span className="state-label">Showing {data.offset + 1}–{data.offset + data.replays.length} of {data.total}</span>
            <div>
              <button className="button button-secondary small-button" disabled={data.offset === 0 || query.isFetching} onClick={() => setOffset(Math.max(0, data.offset - data.limit))}>Previous</button>
              <button className="button button-secondary small-button" disabled={!data.has_more || query.isFetching} onClick={() => setOffset(data.offset + data.limit)}>Next</button>
            </div>
          </nav>
        </>
      )}
      {deleteReplay && <ReplayDeleteDialog replay={deleteReplay} pending={deletion.isPending} error={deletion.error} onCancel={() => setDeleteReplay(null)} onConfirm={() => deletion.mutate(deleteReplay.match_id)} />}
    </main>
  );
}

function ReplayHeader() {
  return <header className="replay-heading"><div><p className="eyebrow">ADMIN / ARCHIVE</p><h1>Replay library</h1><p>Review complete Matches without exposing canonical MJSON to the browser.</p></div><span className="state-label">NEWEST FIRST</span></header>;
}

function ReplayLoading() {
  return <section className="replay-loading" aria-busy="true" aria-live="polite"><span className="state-label">Loading Replays</span><div className="loading-lines" aria-hidden="true"><span /><span /><span /></div></section>;
}

function ReplayRow({ replay, onView, onDelete }: { replay: ReplaySummary; onView: (matchId: string) => void; onDelete: (replay: ReplaySummary) => void }) {
  const unavailable = replay.availability !== "available";
  return (
    <article className={`replay-row${unavailable ? " is-unavailable" : ""}`}>
      <div className="replay-row-main"><span className="state-label">{replay.source === "ranked" ? "RANKED" : "ROOM"}</span><h2>{replay.room_name ?? "Ranked Match"}</h2><code>{replay.match_id}</code></div>
      <div className="replay-row-detail"><span>{replay.game_mode}</span><time dateTime={replay.completed_at}>{replay.completed_at}</time><span className={unavailable ? "replay-status is-bad" : "replay-status"}>{unavailable ? (replay.availability === "too_large" ? "Too large" : "Unavailable") : "Ready to view"}</span></div>
      <div className="replay-row-actions"><button className="button button-secondary small-button" aria-label={`View Replay ${replay.match_id}`} disabled={unavailable} onClick={() => onView(replay.match_id)}>View <ArrowRight aria-hidden="true" weight="regular" /></button><button className="text-button danger-text" aria-label={`Delete Replay ${replay.match_id}`} onClick={() => onDelete(replay)}><Trash aria-hidden="true" weight="regular" />Delete</button></div>
    </article>
  );
}

function ReplayDeleteDialog({ replay, pending, error, onCancel, onConfirm }: { replay: ReplaySummary; pending: boolean; error: unknown; onCancel: () => void; onConfirm: () => void }) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const cancelRef = useRef(onCancel);
  cancelRef.current = onCancel;
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    returnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    try { if (!dialog.open) dialog.showModal(); } catch { dialog.setAttribute("open", ""); }
    (dialog.querySelector<HTMLElement>("button") ?? dialog).focus();
    const closeOnEscape = (event: Event) => {
      if (event instanceof KeyboardEvent && event.key !== "Escape") return;
      event.preventDefault();
      cancelRef.current();
    };
    dialog.addEventListener("cancel", closeOnEscape);
    dialog.addEventListener("keydown", closeOnEscape);
    return () => {
      dialog.removeEventListener("cancel", closeOnEscape);
      dialog.removeEventListener("keydown", closeOnEscape);
      if (dialog.open) { if (typeof dialog.close === "function") dialog.close(); else dialog.removeAttribute("open"); }
      else dialog.removeAttribute("open");
      if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus();
    };
  }, []);
  return <dialog ref={dialogRef} className="confirm-dialog replay-delete-dialog" aria-labelledby="replay-delete-heading"><div className="dialog-top"><p className="eyebrow">DESTRUCTIVE ACTION</p><button className="icon-button" aria-label="Close dialog" onClick={onCancel}>×</button></div><h2 id="replay-delete-heading">Delete this Replay?</h2><p>Delete the saved timeline for <strong>{replay.room_name ?? "Ranked Match"}</strong>? This cannot be undone.</p>{error != null && <p className="form-error" role="alert">{problemFrom(error).detail ?? "The Replay could not be deleted."}</p>}<div className="dialog-actions"><button className="button button-secondary" onClick={onCancel} disabled={pending}>Cancel</button><button className="button button-primary" onClick={onConfirm} disabled={pending}>{pending ? "Deleting Replay" : "Confirm delete"}</button></div></dialog>;
}

function eventKind(event: unknown): string {
  if (!event || typeof event !== "object") return "event";
  const object = event as Record<string, unknown>;
  return typeof object.type === "string" ? object.type : Object.keys(object)[0] ?? "event";
}

function readableKind(kind: string): string {
  return kind.replaceAll("_", " ").replace(/\b\w/g, (value) => value.toUpperCase());
}

function auxiliaryKind(event: unknown): string {
  if (!event || typeof event !== "object") return "auxiliary event";
  const key = Object.keys(event as Record<string, unknown>)[0];
  return key ? readableKind(key) : "Auxiliary event";
}

function frameLog(frame: ReplayFrame): string[] {
  const before = frame.auxiliary_events.filter((event) => event.phase === "before").map((event) => `Before / ${auxiliaryKind(event.event)}`);
  const after = frame.auxiliary_events.filter((event) => event.phase === "after").map((event) => `After / ${auxiliaryKind(event.event)}`);
  return [...before, readableKind(eventKind(frame.visible_event)), ...after];
}

function kyokuValues(frames: ReplayFrame[]): string[] {
  return [...new Set(frames.map((frame) => frame.visible_state.kyoku).filter((value): value is string | number => typeof value === "string" || typeof value === "number").map(String))];
}

function genericReplayPresentation(replay: ReplayView): boolean {
  return replay.source === "ranked" || replay.players.some((player) => !player.character_id || /missing/i.test(player.character_id));
}

function replayRoom(replay: ReplayView): RoomSnapshot {
  const sourceHasCharacters = replay.source === "room" && !genericReplayPresentation(replay);
  const players = replay.players.map((player) => ({
    participant_id: player.participant_id,
    display_name: player.display_name,
    kind: player.participant_kind,
    seat: player.seat,
    character_id: sourceHasCharacters ? player.character_id : null,
    controller: "replay",
  }));
  return {
    join_code: "replay",
    room_name: replay.room_name ?? "Ranked Match",
    game_mode: replay.game_mode,
    phase: "post_match",
    revision: 0,
    participants: [],
    match_players: players,
    roster: players,
    result: null,
  };
}

function useReducedMotionPreference(): boolean {
  const [reduced, setReduced] = useState(() => typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(query.matches);
    query.addEventListener?.("change", update);
    return () => query.removeEventListener?.("change", update);
  }, []);
  return reduced;
}

function ReplayViewer({ replay }: { replay: ReplayView }) {
  const [position, setPosition] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [rate, setRate] = useState(1);
  const reducedMotion = useReducedMotionPreference();
  const frames = replay.frames;
  const frame = frames[position] ?? frames[0];
  const room = useMemo(() => replayRoom(replay), [replay]);
  const genericPresentation = genericReplayPresentation(replay);
  const kyokus = useMemo(() => kyokuValues(frames), [frames]);
  useEffect(() => {
    if (!playing || frames.length < 2) return;
    const timer = window.setTimeout(() => {
      setPosition((current) => {
        if (current >= frames.length - 1) {
          setPlaying(false);
          return current;
        }
        return current + 1;
      });
    }, 900 / rate);
    return () => window.clearTimeout(timer);
  }, [frames.length, playing, position, rate]);
  if (!frame) return <section className="replay-state"><h2>Replay has no frames.</h2><p>The saved timeline is empty.</p></section>;
  const statusText = frame.auxiliary_events.length > 0
    ? frame.auxiliary_events.map((event) => `${auxiliaryKind(event.event)} / ${event.phase}`).join(" / ")
    : readableKind(eventKind(frame.visible_event));
  const selectPosition = (next: number) => { setPlaying(false); setPosition(Math.max(0, Math.min(frames.length - 1, next))); };
  return (
    <>
      <section className="replay-viewer" aria-label="Replay viewer">
        <div className="replay-viewer-head"><div><span className="state-label">EVENT {position + 1} / {frames.length}</span><h2>{readableKind(eventKind(frame.visible_event))}</h2></div><span className="replay-live-state">{genericPresentation ? "GENERIC / SILENT" : "ROOM ASSETS"}</span></div>
        <div className="replay-table-wrap"><PixiTable projection={frame.visible_state as ProjectedState} room={room} reducedMotion={reducedMotion} /></div>
        <div className="replay-status-toast" role="status" aria-live="polite"><span className="state-label">EVENT SIGNAL</span><strong>{statusText}</strong></div>
        <div className="replay-controls" aria-label="Replay controls">
          {!playing ? <button className="button button-primary" onClick={() => setPlaying(frames.length > 1)}><Play aria-hidden="true" weight="fill" />Play</button> : <button className="button button-primary" onClick={() => setPlaying(false)}><Pause aria-hidden="true" weight="fill" />Pause</button>}
          <button className="button button-secondary" aria-label="Previous Event" disabled={position === 0} onClick={() => selectPosition(position - 1)}><ArrowLeft aria-hidden="true" weight="regular" />Previous Event</button>
          <button className="button button-secondary" aria-label="Next Event" disabled={position >= frames.length - 1} onClick={() => selectPosition(position + 1)}>Next Event <ArrowRight aria-hidden="true" weight="regular" /></button>
          <div className="replay-speed" aria-label="Playback speed">{[0.5, 1, 2, 4].map((value) => <button key={value} type="button" className={`speed-button${rate === value ? " is-active" : ""}`} aria-pressed={rate === value} onClick={() => setRate(value)}>{value}x</button>)}</div>
          <label className="replay-jump">Jump to Kyoku<select aria-label="Jump to Kyoku" value={String(frame.visible_state.kyoku ?? "")} onChange={(event) => { const target = event.target.value; const next = frames.findIndex((candidate) => String(candidate.visible_state.kyoku ?? "") === target); if (next >= 0) selectPosition(next); }}><option value="">Current</option>{kyokus.map((kyoku) => <option value={kyoku} key={kyoku}>Kyoku {kyoku}</option>)}</select></label>
        </div>
      </section>
      <section className="replay-event-log" role="log" aria-label="Replay event log" aria-live="off"><div className="section-heading"><div><p className="eyebrow">TIMELINE</p><h3>Event log</h3></div><span className="state-label">SERVER FRAMES</span></div><ol>{frames.map((entry, index) => <li key={entry.event_index} className={index === position ? "is-current" : ""}><button type="button" onClick={() => selectPosition(index)}><span>{String(index + 1).padStart(3, "0")}</span><span>{frameLog(entry).join(" / ")}</span></button></li>)}</ol></section>
    </>
  );
}

function ReplayDetail({ matchId }: { matchId: string }) {
  const query = useQuery({ queryKey: replayKeys.view(matchId), queryFn: () => api.getAdminReplay(matchId) });
  if (query.isLoading) return <main className="replay-main"><section className="replay-loading" aria-busy="true"><span className="state-label">Loading Replay</span></section></main>;
  if (query.isError) return <main className="replay-main"><ProblemMessage problem={problemFrom(query.error)} /></main>;
  if (!query.data) return <main className="replay-main"><ProblemMessage problem={{ code: "replay_unavailable", detail: "The Replay could not be loaded." }} /></main>;
  return <main className="replay-main replay-detail-main"><button className="back-link" onClick={() => navigate("/admin/replays")}><ArrowLeft aria-hidden="true" weight="regular" />Back to Replay library</button><header className="replay-heading replay-detail-heading"><div><p className="eyebrow">ADMIN / REPLAY</p><h1>{query.data.room_name ?? "Ranked Match"} Replay</h1><p>{query.data.match_id} · {query.data.completed_at}</p></div><ReplayMeta replay={query.data} /></header><ReplayViewer replay={query.data} /></main>;
}

export function ReplayWorkspace({ matchId }: { matchId?: string }) {
  return <div className="app-shell workspace-shell replay-shell"><ReplayTopbar />{matchId ? <ReplayDetail matchId={matchId} /> : <ReplayList onView={(id) => navigate(`/admin/replays/${encodeURIComponent(id)}`)} />}</div>;
}
