import { useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, ApiProblem, problemFrom, type AdminRoomDetail, type BenchmarkRun, type GameMode, type TimeControl } from "./api";
import { ThreeTable } from "./game/three-table";
import { tileLabel } from "./game/tiles";
import { navigate } from "./routes";
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
  return <div className="app-shell workspace-shell benchmark-shell">
    <header className="topbar"><Link href="/admin">DOUBLE RIICHI</Link><nav aria-label="Benchmark navigation"><Link href="/admin">Rooms</Link><Link href="/admin/benchmark">Benchmarks</Link><button className="text-button" onClick={() => { void api.logoutAdmin().finally(() => navigate("/admin/login")); }}>Sign out</button></nav></header>
    <main className="benchmark-main">{runId ? <RunDetail key={runId} runId={runId} /> : <RunList />}</main>
  </div>;
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
  const query = useQuery({ queryKey: ["admin", "benchmark", "run", runId], queryFn: () => api.getBenchmarkRun(runId), refetchInterval: query => query.state.data?.status === "running" ? 2000 : false });
  if (query.isError) return <ErrorMessage error={query.error} retry={() => { void query.refetch(); }} />;
  if (!query.data) return <p role="status">Loading Run…</p>;
  const run = query.data;
  return <>
    <h1>Benchmark Run</h1><p className="benchmark-identity">{run.run_id}</p>
    <p role="status">{run.status} · {run.completed} / {run.target} completed Matches</p>
    {run.reason && <p>{run.reason}</p>}
    {run.failed_match_id && <p className="field-hint">Incomplete attempt {run.failed_match_id} — excluded from statistics.</p>}
    {run.status === "running" && <><Link href={`/admin/rooms/${run.room_code}`}>Room controls</Link><LiveObserver code={run.room_code} /></>}
    <RunStatistics run={run} />
    {run.matches.length > 0 && <details className="admin-disclosure"><summary>Per-Match results</summary><div className="benchmark-table-scroll"><table aria-label="Per-Match results"><thead><tr><th scope="col">Match</th><th scope="col">Participant ID</th><th scope="col">Seat</th><th scope="col">Rank</th><th scope="col">Final score</th><th scope="col">Cumulative net score</th></tr></thead><tbody>{run.matches.flatMap(game => game.results.map(player => <tr key={`${game.match_id}:${player.participant_id}`}><td><Link href={`/admin/replays/${encodeURIComponent(game.match_id)}`}>{game.sequence}</Link></td><td>{player.participant_id}</td><td>{player.seat + 1}</td><td>{player.rank}</td><td>{player.final_score.toLocaleString()}</td><td>{run.statistics?.find(stat => stat.participant_id === player.participant_id)?.cumulative_net_scores[game.sequence - 1]?.toLocaleString()}</td></tr>))}</tbody></table></div></details>}
  </>;
}

function LiveObserver({ code }: { code: string }) {
  const [open, setOpen] = useState(false);
  const live = useQuery({ queryKey: ["admin", "benchmark", "live", code], queryFn: () => api.getBenchmarkLive(code), enabled: open,
    refetchInterval: query => query.state.error instanceof ApiProblem && query.state.error.status === 401 ? false : 1000,
    refetchIntervalInBackground: false,
  });
  const projection = live.data?.projection;
  return <details className="admin-disclosure" onToggle={event => setOpen(event.currentTarget.open)}><summary>Live table — all current hands</summary>
    {open && (live.isError ? <ErrorMessage error={live.error} retry={() => { void live.refetch(); }} /> : !live.data ? <p role="status">Connecting to live table…</p> : projection ? <>
      <p className="field-hint">Read-only Admin view · Revision {live.data.revision} · Updates every second</p>
      <div className="benchmark-live replay-table-wrap"><ThreeTable projection={projection} room={null} surface="replay" reducedMotion /></div>
      <details><summary>Current hands as text</summary><ul>{projection.players?.map(player => <li key={player.participant_id}>{player.display_name} ({player.participant_id}): {player.hand?.map(tileLabel).join(", ")}</li>)}</ul></details>
    </> : <p>No active Match.</p>)}
  </details>;
}

const SERIES_COLORS = ["#354a66", "#a72e3f", "#2762ab", "#865a10"];
function RunStatistics({ run }: { run: BenchmarkRun }) {
  const stats = run.statistics ?? [];
  if (run.completed === 0) return <section className="benchmark-statistics"><h2>Statistics</h2><p>No completed Matches yet.</p></section>;
  const range = Math.max(1, ...stats.flatMap(stat => stat.cumulative_net_scores.map(Math.abs)));
  return <section className="benchmark-statistics"><h2>Statistics</h2>
    <figure><svg viewBox="0 0 640 240" role="img" aria-label="Cumulative score relative to table average">
      <title>Cumulative score relative to table average</title><desc>Match sequence on the horizontal axis, cumulative net score on the vertical axis. Exact values are in Per-Match results.</desc>
      <line x1="36" y1="112" x2="620" y2="112" stroke="#bcc6cf" />
      <text x="4" y="20">{range.toLocaleString()}</text><text x="4" y="218">{(-range).toLocaleString()}</text><text x="36" y="238">0</text><text x="620" y="238" textAnchor="end">Match {run.completed}</text>
      {stats.map((stat, index) => <polyline key={stat.participant_id} fill="none" stroke={SERIES_COLORS[index % 4]} strokeWidth="2" strokeDasharray={index === 0 ? undefined : `${8 + index * 2} ${index * 2}`} points={[0, ...stat.cumulative_net_scores].map((score, sequence) => `${36 + sequence / Math.max(1, run.completed) * 584},${112 - score / range * 92}`).join(" ")} />)}
    </svg><figcaption>Cumulative score relative to table average. Only persisted completed Matches count.</figcaption></figure>
    <div className="benchmark-table-scroll"><table aria-label="Participant statistics"><thead><tr><th scope="col">Participant</th><th scope="col">Participant ID</th><th scope="col">Mean rank</th><th scope="col">First-place rate</th><th scope="col">Net score</th></tr></thead><tbody>{stats.map((stat, index) => <tr key={stat.participant_id}><th scope="row"><span className="benchmark-series-key" style={{ color: SERIES_COLORS[index % 4] }}>{stat.display_name}</span></th><td>{stat.participant_id}</td><td>{stat.average_rank?.toFixed(2) ?? "—"}</td><td>{stat.first_place_rate === null ? "—" : `${(stat.first_place_rate * 100).toFixed(1)}%`}</td><td>{stat.cumulative_net_scores.at(-1)?.toLocaleString() ?? "—"}</td></tr>)}</tbody></table></div>
  </section>;
}
