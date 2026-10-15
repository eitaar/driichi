import { lazy, Suspense, useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, ApiProblem, problemFrom, type AdminRoomDetail, type GameMode, type TimeControl } from "./api";
import { AppShell } from "./components/app-shell";
import { DesktopTableBoundary } from "./components/desktop-table-boundary";
const LiveObserver = lazy(() => import("./benchmark-observer").then(module => ({ default: module.BenchmarkObserver })));
import { navigate } from "./routes";
import { validateLoadedRuns, benchmarkReadiness, filterLoadedRuns, matchingStopRequested, replayAvailability, useBenchmarkEvidence } from "./benchmark-data";
import { BenchmarkMatchDetail } from "./benchmark-match-detail";
import { Alert, AlertTitle, AlertDescription } from "./components/ui/alert";
import { Button } from "./components/ui/button";
import { Field, FieldGroup, FieldLabel, FieldDescription } from "./components/ui/field";
import { Input } from "./components/ui/input";
import { Dialog, DialogTrigger, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "./components/ui/dialog";
import { AlertDialog, AlertDialogTrigger, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "./components/ui/alert-dialog";
import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "./components/ui/table";
import { Pagination, PaginationContent, PaginationItem } from "./components/ui/pagination";
const BenchmarkCharts = lazy(() => import("./benchmark-charts").then(module => ({ default: module.BenchmarkCharts })));
import "./benchmark.css";

function Link({ href, children }: { href: string; children: ReactNode }) {
  return <a href={href} className="nav-link" onClick={event => {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault(); navigate(href);
  }}>{children}</a>;
}
function ErrorMessage({ error, retry }: { error: unknown; retry?: () => void }) {
  return <div><p className="form-error" role="alert">{problemFrom(error).detail}</p>{retry && <Button variant="outline" onClick={retry}>Retry</Button>}</div>;
}

function StopRun({ runId, code, stopRequested, snapshotVersion, refresh }: {
  runId: string; code: string; stopRequested: boolean | null; snapshotVersion: number;
  refresh(): Promise<boolean | null>;
}) {
  const [open, setOpen] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [uncertain, setUncertain] = useState(false);
  const inFlight = useRef(false);
  useEffect(() => { if (!inFlight.current && stopRequested !== null) setUncertain(false); }, [stopRequested, snapshotVersion]);
  const stop = useMutation({
    mutationFn: async () => {
      const room = await api.stopBenchmarkRun(code);
      if (matchingStopRequested({ run_id: runId, room_code: code, status: "running" }, room) !== true) {
        throw new ApiProblem({ detail: "Stop was not confirmed for this Run. Refresh before a new intent." });
      }
      setAccepted(true);
    },
    onError: () => { setUncertain(true); },
    onSettled: async (_data, error) => {
      if (error instanceof ApiProblem && error.status === 401) { inFlight.current = false; return; }
      try {
        const pending = await refresh();
        if (pending !== null) { setAccepted(pending); setUncertain(false); }
      } finally { inFlight.current = false; }
    },
    retry: false,
  });
  const pending = accepted || stopRequested === true;
  const unknown = !pending && (uncertain || stopRequested === null);
  const disabled = stop.isPending || pending || unknown;
  return <div className="flex flex-col gap-2">
    {pending && <p role="status">Stopping after current Match</p>}
    {unknown && <p role="status">Stop state unknown. Refresh authoritative Room detail before another Stop.</p>}
    {stop.isError && <ErrorMessage error={stop.error} />}
    {stop.error instanceof ApiProblem && stop.error.status === 409 && <p role="alert">Run changed. Review refreshed state and confirm a new Stop intent.</p>}
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger render={<Button variant="outline" disabled={disabled} />}>Stop after current Match</AlertDialogTrigger>
      <AlertDialogContent><AlertDialogHeader><AlertDialogTitle>Finish current Match, then stop</AlertDialogTitle><AlertDialogDescription>The current Match may finish and be recorded. No further Matches will be scheduled after the server accepts Stop.</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel>Cancel</AlertDialogCancel><AlertDialogAction disabled={disabled} onClick={() => {
          if (inFlight.current || disabled || stopRequested !== false) return;
          inFlight.current = true; setOpen(false); stop.mutate();
        }}>Confirm Stop</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
    <p className="field-hint">The current server has no revision-guarded or durable idempotent Stop contract.</p>
  </div>;
}

export function BenchmarkControls({ room, pending, perform, refreshRoom }: {
  room: AdminRoomDetail; pending: boolean; perform: (task: () => Promise<unknown>) => void;
  refreshRoom(): Promise<AdminRoomDetail | undefined>;
}) {
  const [target, setTarget] = useState(1);
  const progress = room.benchmark;
  const blockers = benchmarkReadiness(room, target);
  return <section className="flat-section benchmark-controls" aria-label="Benchmark control">
    <h2>Benchmark Run</h2>
    {progress ? <>
      <p role="status">{progress.status} · {progress.completed} / {progress.target} completed Matches</p>
      {progress.status === "running" && <StopRun key={progress.run_id} runId={progress.run_id} code={room.join_code} stopRequested={typeof progress.stop_requested === "boolean" ? progress.stop_requested : null} snapshotVersion={room.revision} refresh={async () => matchingStopRequested({ run_id: progress.run_id, room_code: room.join_code, status: progress.status }, await refreshRoom())} />}
      <Link href={`/admin/benchmark/runs/${encodeURIComponent(progress.run_id)}`}>View Run and live table</Link>
    </> : <form onSubmit={event => { event.preventDefault(); if (!pending && blockers.length === 0) perform(() => api.startBenchmarkRun(room.join_code, target)); }}>
      <FieldGroup><Field data-invalid={!Number.isInteger(target) || target < 1 || target > 1000}>
        <FieldLabel htmlFor="benchmark-target">Matches in Run</FieldLabel>
        <Input id="benchmark-target" type="number" min="1" max="1000" step="1" value={target} aria-invalid={!Number.isInteger(target) || target < 1 || target > 1000} aria-describedby="benchmark-readiness" onChange={event => setTarget(Number(event.target.value))} />
        <FieldDescription>No automatic fill. Select a fixed connected software roster below.</FieldDescription>
      </Field></FieldGroup>
      <div id="benchmark-readiness" aria-live="polite"><h3>Ready Panel</h3>{blockers.length ? <ul>{blockers.map((blocker, index) => <li key={index}>{blocker}</li>)}</ul> : <p>Ready: exact connected eligible roster selected.</p>}</div>
      <Button type="submit" disabled={pending || blockers.length > 0}>Start Run</Button>
    </form>}
  </section>;
}

export function BenchmarkWorkspace({ runId }: { runId?: string }) {
  return <AppShell variant="admin"><div className="app-shell workspace-shell benchmark-shell">
    <div className="benchmark-main">{runId ? <RunDetail key={runId} runId={runId} /> : <RunList />}</div>
  </div></AppShell>;
}

function RunList() {
  const runs = useQuery({ queryKey: ["admin", "benchmark", "runs"], queryFn: async ({ signal }) => validateLoadedRuns(await api.listBenchmarkRuns(signal)), refetchOnMount: "always", refetchOnWindowFocus: "always" });
  const [name, setName] = useState("");
  const [mode, setMode] = useState<GameMode>("4p-red-east");
  const [timing, setTiming] = useState<TimeControl>("casual");
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [filterMode, setFilterMode] = useState("");
  const [offset, setOffset] = useState(0);
  const create = useMutation({ mutationFn: api.createBenchmarkRoom, onSuccess: room => navigate(`/admin/rooms/${room.join_code}`) });
  const page = filterLoadedRuns(runs.data ?? [], search, status, filterMode, offset);
  return <>
    <h1>Benchmarks</h1><p>Compare a fixed bot roster over 1–1,000 completed Matches.</p>
    <Dialog><DialogTrigger render={<Button />}>New Run</DialogTrigger><DialogContent>
      <DialogHeader><DialogTitle>New Run</DialogTitle><DialogDescription>Create a Room, then connect and explicitly select bots in Admin before starting.</DialogDescription></DialogHeader>
      <form onSubmit={event => { event.preventDefault(); if (name.trim() && !create.isPending) create.mutate({ room_name: name.trim(), game_mode: mode, time_control: timing }); }}>
        <FieldGroup>
          <Field><FieldLabel htmlFor="benchmark-room-name">Room name</FieldLabel><Input id="benchmark-room-name" value={name} maxLength={64} required onChange={event => setName(event.target.value)} /></Field>
          <Field><FieldLabel htmlFor="benchmark-mode">Game mode</FieldLabel><select id="benchmark-mode" value={mode} onChange={event => setMode(event.target.value as GameMode)}><ModeOptions /></select></Field>
          <Field><FieldLabel htmlFor="benchmark-timing">Time control</FieldLabel><select id="benchmark-timing" value={timing} onChange={event => setTiming(event.target.value as TimeControl)}><option value="casual">Casual</option><option value="riichi_dev">Riichi.dev</option><option value="unlimited">Unlimited</option></select></Field>
          {create.isError && <ErrorMessage error={create.error} />}
          <Button type="submit" disabled={create.isPending || !name.trim()}>Create benchmark room</Button>
        </FieldGroup>
      </form>
    </DialogContent></Dialog>
    <section className="benchmark-history"><h2>Run history</h2>
      <p>Search and paging apply only to the loaded legacy snapshot. Newest-first server order is retained; timestamps and efficient server pagination are unavailable.</p>
      <Button variant="outline" disabled={runs.isFetching} onClick={() => { void runs.refetch({ cancelRefetch: false }); }}>Refresh</Button>
      <FieldGroup className="benchmark-filters">
        <Field><FieldLabel htmlFor="run-search">Search loaded Runs</FieldLabel><Input id="run-search" type="search" value={search} onChange={event => { setSearch(event.target.value); setOffset(0); }} /></Field>
        <Field><FieldLabel htmlFor="run-status">Status</FieldLabel><select id="run-status" value={status} onChange={event => { setStatus(event.target.value); setOffset(0); }}><option value="">All statuses</option>{["running", "completed", "stopped", "interrupted", "failed"].map(value => <option key={value}>{value}</option>)}</select></Field>
        <Field><FieldLabel htmlFor="run-mode">Mode</FieldLabel><select id="run-mode" value={filterMode} onChange={event => { setFilterMode(event.target.value); setOffset(0); }}><option value="">All modes</option><ModeOptions /></select></Field>
      </FieldGroup>
      {runs.isLoading && <p role="status">Loading Runs…</p>}
      {runs.isError ? <ErrorMessage error={runs.error} retry={() => { void runs.refetch({ cancelRefetch: false }); }} /> : runs.data && <>
        <Table aria-label="Loaded Run history"><TableHeader><TableRow><TableHead>Room / Run</TableHead><TableHead>Bots</TableHead><TableHead>Mode</TableHead><TableHead>Status</TableHead><TableHead>Completed / target</TableHead></TableRow></TableHeader><TableBody>
          {page.items.map(run => <TableRow key={run.run_id}><TableCell><Link href={`/admin/benchmark/runs/${encodeURIComponent(run.run_id)}`}>{run.room_code} · {run.run_id}</Link></TableCell><TableCell>{run.roster.map(p => p.display_name).join(", ")}</TableCell><TableCell>{run.game_mode}</TableCell><TableCell>{run.status}</TableCell><TableCell>{run.completed} / {run.target}</TableCell></TableRow>)}
        </TableBody></Table>
        {page.items.length === 0 && <p>No matching loaded Runs.</p>}
        <Pagination aria-label="Loaded Run pages"><PaginationContent>
          <PaginationItem><Button variant="outline" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 25))}>Previous page</Button></PaginationItem>
          <PaginationItem><span>{page.items.length ? offset + 1 : 0}–{page.items.length ? offset + page.items.length : 0} of {page.total}</span></PaginationItem>
          <PaginationItem><Button variant="outline" disabled={offset + 25 >= page.total} onClick={() => setOffset(offset + 25)}>Next page</Button></PaginationItem>
        </PaginationContent></Pagination>
      </>}
    </section>
  </>;
}
function ModeOptions() {
  return <><option value="4p-red-east">4p red East</option><option value="4p-red-half">4p red half</option><option value="3p-red-east">3p red East (no MJAI)</option><option value="3p-red-half">3p red half (no MJAI)</option></>;
}

function RunDetail({ runId }: { runId: string }) {
  const [selectedMatchId, setSelectedMatchId] = useState<string | null>(null);
  const { query, evidence, refresh, refreshAfterMutation } = useBenchmarkEvidence(runId);
  const metadata = useQuery({ queryKey: ["admin", "benchmark", "replay-metadata", runId, selectedMatchId, 0, 50], queryFn: ({ signal }) => api.listAdminReplays(0, 50, signal), enabled: selectedMatchId !== null, retry: false });
  if (evidence.kind === "error") return <ErrorMessage error={evidence.error} retry={() => { void refresh(); }} />;
  if (evidence.kind === "loading" || !query.data) return <p role="status">Loading Run…</p>;
  const { run, analytics } = query.data;
  const selectedMatch = Array.isArray(run.matches) ? run.matches.find(match => match?.match_id === selectedMatchId) : undefined;
  const matchEvidence = selectedMatch ? { ...selectedMatch, replay_availability: metadata.isError ? "unknown" as const : replayAvailability(selectedMatch.match_id, metadata.data) } : null;
  return <>
    <h1>Benchmark Run</h1><p className="benchmark-identity">{run.run_id}</p>
    <p role="status">{run.status} · {run.completed} / {run.target} completed Matches</p>
    <Button variant="outline" disabled={query.isFetching} onClick={() => { void refresh(); }}>Refresh Run</Button>
    {run.reason && <p>{run.reason}</p>}
    {run.failed_match_id && <p className="field-hint">Incomplete attempt {run.failed_match_id} — excluded from statistics.</p>}
    {run.status === "interrupted" && <><Button disabled aria-describedby="resume-unavailable">Resume unavailable</Button><p id="resume-unavailable">The server does not publish safe Resume eligibility, roster/revision checks, or an accepted Resume operation. This Run remains Interrupted.</p></>}
    {run.status === "running" && <>
      <StopRun runId={runId} code={run.room_code} stopRequested={query.data.stopRequested} snapshotVersion={query.dataUpdatedAt} refresh={async () => { const result = await refreshAfterMutation(); return !result || result.isError ? null : result.data?.stopRequested ?? null; }} />
      <Link href={`/admin/rooms/${run.room_code}`}>Room controls</Link>
    </>}
    <DesktopTableBoundary fallback={null}><LiveObserver runId={runId} roomCode={run.room_code} analytics={analytics} /></DesktopTableBoundary>
    {analytics.kind === "invalid" ? <Alert variant="destructive"><AlertTitle>Invalid Match evidence</AlertTitle><AlertDescription>{analytics.reason}</AlertDescription><Button variant="outline" onClick={() => { void refresh(); }}>Retry</Button></Alert>
      : analytics.completed === 0 ? <section><h2>Statistics</h2><p>No completed Matches yet.</p></section>
      : <Suspense fallback={<p role="status">Loading charts…</p>}><BenchmarkCharts analytics={analytics} onOpenMatch={setSelectedMatchId} /></Suspense>}
    <BenchmarkMatchDetail match={analytics.kind === "ready" ? matchEvidence : null} roster={run.roster} onClose={() => setSelectedMatchId(null)} onRetryReplay={() => { void metadata.refetch({ cancelRefetch: false }); }} />
  </>;
}
