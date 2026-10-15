import { lazy, Suspense, useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, problemFrom, type AdminRoomDetail, type GameMode, type TimeControl } from "./api";
import { AppShell } from "./components/app-shell";
import { DesktopTableBoundary } from "./components/desktop-table-boundary";
const LiveObserver = lazy(() => import("./benchmark-live-observer").then(module => ({ default: module.BenchmarkLiveObserver })));
import { navigate } from "./routes";
import { deriveRunAnalytics } from "./benchmark-analytics";
import { BenchmarkMatchDetail } from "./benchmark-match-detail";
import { Alert, AlertTitle, AlertDescription } from "./components/ui/alert";
import { Button } from "./components/ui/button";
const BenchmarkCharts = lazy(() => import("./benchmark-charts").then(module => ({ default: module.BenchmarkCharts })));
import "./benchmark.css";

function Link({ href, children }: { href: string; children: ReactNode }) {
  return <a href={href} className="nav-link" onClick={event => {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault(); navigate(href);
  }}>{children}</a>;
}
function ErrorMessage({ error, retry }: { error: unknown; retry?: () => void }) {
  return <div><p className="form-error" role="alert">{problemFrom(error).detail}</p>{retry && <button className="button button-secondary" onClick={retry}>Retry</button>}</div>;
}

export function BenchmarkControls({ room, pending, perform }: {
  room: AdminRoomDetail; pending: boolean; perform: (task: () => Promise<unknown>) => void;
}) {
  const [target, setTarget] = useState(1);
  const seats = room.game_mode.startsWith("3p") ? 3 : 4;
  const progress = room.benchmark;
  const valid = Number.isInteger(target) && target >= 1 && target <= 1000;
  const rosterReady = room.selected_count === seats && room.participants.filter(p => p.selected).every(p => p.presence === "connected" && p.kind !== "human");
  return <section className="flat-section benchmark-controls" aria-label="Benchmark control">
    <h3>Benchmark Run</h3>
    {progress ? <>
      <p role="status">{progress.status} · {progress.completed} / {progress.target} completed Matches{progress.stop_requested && progress.status === "running" ? " · Stopping after current Match" : ""}</p>
      <div className="action-row"><button className="button button-secondary" disabled={pending || progress.status !== "running" || progress.stop_requested} onClick={() => perform(() => api.stopBenchmarkRun(room.join_code))}>Stop after current Match</button><Link href={`/admin/benchmark/runs/${encodeURIComponent(progress.run_id)}`}>View Run and live table</Link></div>
    </> : <form className="action-row" onSubmit={event => { event.preventDefault(); if (valid && rosterReady) perform(() => api.startBenchmarkRun(room.join_code, target)); }}>
      <div className="form-block"><label htmlFor="benchmark-target">Matches in Run</label><input id="benchmark-target" type="number" min="1" max="1000" step="1" value={target} onChange={event => setTarget(Number(event.target.value))} /></div>
      <button className="button button-primary" type="submit" disabled={pending || !valid || !rosterReady}>Start Run</button>
      <p className="field-hint">Explicitly select {seats} connected bots. No automatic fill.</p>
    </form>}
  </section>;
}

export function BenchmarkWorkspace({ runId }: { runId?: string }) {
  return <AppShell variant="admin"><div className="app-shell workspace-shell benchmark-shell">
    <main className="benchmark-main">{runId ? <RunDetail key={runId} runId={runId} /> : <RunList />}</main>
  </div></AppShell>;
}

function RunList() {
  const runs = useQuery({ queryKey: ["admin", "benchmark", "runs"], queryFn: api.listBenchmarkRuns, refetchInterval: 2000 });
  const [name, setName] = useState("");
  const [mode, setMode] = useState<GameMode>("4p-red-east");
  const [timing, setTiming] = useState<TimeControl>("casual");
  const create = useMutation({ mutationFn: api.createBenchmarkRoom, onSuccess: room => navigate(`/admin/rooms/${room.join_code}`) });
  return <>
    <h1>Benchmarks</h1><p>Compare a fixed bot roster over 1–1,000 completed Matches.</p>
    <form className="benchmark-create" onSubmit={event => { event.preventDefault(); if (name.trim()) create.mutate({ room_name: name.trim(), game_mode: mode, time_control: timing }); }}>
      <div className="form-block"><label htmlFor="benchmark-room-name">Room name</label><input id="benchmark-room-name" value={name} maxLength={64} required onChange={event => setName(event.target.value)} /></div>
      <div className="form-block"><label htmlFor="benchmark-mode">Game mode</label><select id="benchmark-mode" value={mode} onChange={event => setMode(event.target.value as GameMode)}><option value="4p-red-east">4p red East</option><option value="4p-red-half">4p red half</option><option value="3p-red-east">3p red East (no MJAI)</option><option value="3p-red-half">3p red half (no MJAI)</option></select></div>
      <div className="form-block"><label htmlFor="benchmark-timing">Time control</label><select id="benchmark-timing" value={timing} onChange={event => setTiming(event.target.value as TimeControl)}><option value="casual">Casual</option><option value="riichi_dev">Riichi.dev</option><option value="unlimited">Unlimited</option></select></div>
      <button className="button button-primary" disabled={create.isPending || !name.trim()}>Create benchmark room</button>
    </form>
    {create.isError && <ErrorMessage error={create.error} />}
    <section className="benchmark-history"><h2>Run history</h2>
      {runs.isLoading && <p role="status">Loading Runs…</p>}
      {runs.isError ? <ErrorMessage error={runs.error} retry={() => { void runs.refetch(); }} /> : runs.data?.length === 0 ? <p>No Runs yet. Create a room, connect bots, and select them before starting.</p> : <ul>{runs.data?.map(run => <li key={run.run_id}><Link href={`/admin/benchmark/runs/${encodeURIComponent(run.run_id)}`}>{run.room_code} · {run.game_mode} · {run.status} · {run.completed} / {run.target}</Link></li>)}</ul>}
    </section>
  </>;
}

function RunDetail({ runId }: { runId: string }) {
  const [selectedMatchId, setSelectedMatchId] = useState<string | null>(null);
  const query = useQuery({ queryKey: ["admin", "benchmark", "run", runId], queryFn: () => api.getBenchmarkRun(runId), refetchInterval: query => query.state.data?.status === "running" ? 2000 : false });
  if (query.isError) return <ErrorMessage error={query.error} retry={() => { void query.refetch(); }} />;
  if (!query.data) return <p role="status">Loading Run…</p>;
  const run = query.data;
  const analytics = deriveRunAnalytics(run);
  const selectedMatch = run.matches.find(match => match.match_id === selectedMatchId);
  // Legacy Run records carry no Replay availability; never infer it from a persisted score.
  const evidence = selectedMatch ? { ...selectedMatch, replay_availability: "unknown" as const } : null;
  return <>
    <h1>Benchmark Run</h1><p className="benchmark-identity">{run.run_id}</p>
    <p role="status">{run.status} · {run.completed} / {run.target} completed Matches</p>
    {run.reason && <p>{run.reason}</p>}
    {run.failed_match_id && <p className="field-hint">Incomplete attempt {run.failed_match_id} — excluded from statistics.</p>}
    {run.status === "running" && <><Link href={`/admin/rooms/${run.room_code}`}>Room controls</Link><DesktopTableBoundary fallback={null}><LiveObserver code={run.room_code} /></DesktopTableBoundary></>}
    {analytics.kind === "invalid" ? <Alert variant="destructive"><AlertTitle>Invalid Match evidence</AlertTitle><AlertDescription>{analytics.reason}</AlertDescription><Button variant="outline" onClick={() => { void query.refetch(); }}>Retry</Button></Alert>
      : analytics.completed === 0 ? <section><h2>Statistics</h2><p>No completed Matches yet.</p></section>
      : <Suspense fallback={<p role="status">Loading charts…</p>}><BenchmarkCharts analytics={analytics} onOpenMatch={setSelectedMatchId} /></Suspense>}
    <BenchmarkMatchDetail match={analytics.kind === "ready" ? evidence : null} roster={run.roster} onClose={() => setSelectedMatchId(null)} />
  </>;
}
