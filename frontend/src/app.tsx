import { Component, lazy, Suspense, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient, QueryClient, QueryClientProvider, QueryCache, MutationCache } from "@tanstack/react-query";
import { ArrowLeft } from "@phosphor-icons/react/dist/csr/ArrowLeft";
import { ArrowRight } from "@phosphor-icons/react/dist/csr/ArrowRight";
import { Check } from "@phosphor-icons/react/dist/csr/Check";
import { Copy } from "@phosphor-icons/react/dist/csr/Copy";
import { LockKey } from "@phosphor-icons/react/dist/csr/LockKey";
import { Plus } from "@phosphor-icons/react/dist/csr/Plus";
import { SpeakerHigh } from "@phosphor-icons/react/dist/csr/SpeakerHigh";
import { WarningCircle } from "@phosphor-icons/react/dist/csr/WarningCircle";
import { X } from "@phosphor-icons/react/dist/csr/X";
import { Button } from "./components/ui/button";
import { LobbyParticipantCard } from "./components/lobby-participant-card";
import { Sidebar, SidebarContent, SidebarProvider } from "./components/ui/sidebar";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "./components/ui/table";
import { Alert, AlertDescription } from "./components/ui/alert";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "./components/ui/field";
import { Input } from "./components/ui/input";
import { RadioGroup, RadioGroupItem } from "./components/ui/radio-group";
import { api, ApiProblem, type AdminRoomDetail, type BotTokenRecord, type CreatedBotToken, type GameMode, type HumanCharacter, type ProblemDetails, type RoomParticipant, type TimeControl } from "./api";
import { useGameStore, type Transport } from "./game/store";
import { decodeCharacterAsset } from "./game/assets";
import { decodeRoomFinalStandings } from "./game/results";
import type { RoomSnapshot } from "./game/types";
import { navigate, routeForPath, type Route } from "./routes";
import { clearAdminSession } from "./admin-session";
import { AppShell } from "./components/app-shell";
import { DesktopTableBoundary, DesktopTableFallback } from "./components/desktop-table-boundary";
import { ThemeProvider } from "./components/theme-provider";
import { Toaster, toast } from "./components/ui/toast";
import "./styles.css";
import "./pre-match.css";
import "./admin.css";

// Optional surfaces must not gate the socket-owning HumanLobby's first commit.
const GameplaySurface = lazy(() => import("./game/gameplay").then(module => ({ default: module.GameplaySurface })));
const ReplayWorkspace = lazy(() => import("./replay").then(module => ({ default: module.ReplayWorkspace })));
const BenchmarkWorkspace = lazy(() => import("./benchmark").then(module => ({ default: module.BenchmarkWorkspace })));
const BenchmarkControls = lazy(() => import("./benchmark").then(module => ({ default: module.BenchmarkControls })));

class SurfaceLoader extends Component<{ label: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    if (this.state.failed) return <div className="panel-state" role="alert"><p>{this.props.label} could not be loaded.</p><a className="button button-secondary" href={window.location.href}>Reload page</a></div>;
    return <Suspense fallback={<div role="status"><LoadingPanel label={`Loading ${this.props.label}`} /></div>}>{this.props.children}</Suspense>;
  }
}

type MotionQuery = MediaQueryList & { addListener?: (listener: () => void) => void; removeListener?: (listener: () => void) => void };

function prefersReducedMotion() { return typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches; }
function useReducedMotion() {
  const [reduced, setReduced] = useState(prefersReducedMotion);
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)") as MotionQuery;
    const update = () => setReduced(query.matches);
    if (query.addEventListener) { query.addEventListener("change", update); return () => query.removeEventListener("change", update); }
    query.addListener?.(update); return () => query.removeListener?.(update);
  }, []);
  return reduced;
}

export function App() {
  const [queryClient] = useState(() => {
    const onAuthError = (error: unknown) => { if (error instanceof ApiProblem && error.status === 401) clearAdminSession(client); };
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 0, refetchOnWindowFocus: true } },
      queryCache: new QueryCache({ onError: (error, query) => { if (query.queryKey[0] === "admin") onAuthError(error); } }),
      mutationCache: new MutationCache({ onError: onAuthError }),
    });
    return client;
  });
  return <QueryClientProvider client={queryClient}><ThemeProvider><Toaster><AppRoutes /></Toaster></ThemeProvider></QueryClientProvider>;
}

function AppRoutes() {
  const [route, setRoute] = useState<Route>(() => routeForPath(window.location.pathname));
  useEffect(() => { const update = () => setRoute(routeForPath(window.location.pathname)); window.addEventListener("popstate", update); return () => window.removeEventListener("popstate", update); }, []);
  if (route.kind === "entry") return <EntryShell />;
  if (route.kind === "room") return <RoomRoute joinCode={route.joinCode} />;
  if (route.kind === "lobby") return <HumanLobby joinCode={route.joinCode} />;
  if (route.kind === "admin-login") return <AdminLogin />;
  if (route.kind === "admin-replays") return <SurfaceLoader key="replay" label="Replay"><ReplayWorkspace /></SurfaceLoader>;
  if (route.kind === "admin-replay") return <SurfaceLoader key="replay" label="Replay"><ReplayWorkspace matchId={route.matchId} /></SurfaceLoader>;
  if (route.kind === "admin-benchmark") return <SurfaceLoader key="benchmark" label="Benchmark"><BenchmarkWorkspace runId={route.runId} /></SurfaceLoader>;
  if (route.kind === "admin") return <AppShell variant="admin"><AdminWorkspace routeCode={route.joinCode} /></AppShell>;
  return <NotFound />;
}

function RouteLink({ href, children, className = "", onClick, ...props }: React.ComponentProps<'a'> & { href: string; onClick?: () => void }) {
  return <a {...props} className={className} href={href} onClick={(event) => { if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return; event.preventDefault(); onClick?.(); navigate(href); }}>{children}</a>;
}

function EntryShell() {
  const [code, setCode] = useState(""); const [error, setError] = useState(""); const codeInputRef = useRef<HTMLInputElement>(null);
  const [characters, setCharacters] = useState<HumanCharacter[]>([]);
  useEffect(() => { let active = true; void api.listHumanCharacters().then(items => { if (active) setCharacters(items.slice(0, 2)); }).catch(() => {}); return () => { active = false; }; }, []);
  useEffect(() => { codeInputRef.current?.focus(); }, []);
  function openRoom(event: React.FormEvent<HTMLFormElement>) { event.preventDefault(); const normalized = code.trim(); if (!/^\d{6}$/.test(normalized)) { setError("Enter the six-digit room code."); return; } setError(""); navigate(`/room/${normalized}`); }
  return <div className="app-shell entry-shell" data-testid="entry-shell" data-motion="static">
    <AppShell variant="public">{null}</AppShell>
    <main className="entry-main">
      <section className="entry-copy" aria-labelledby="entry-heading">
        <h1 id="entry-heading">Join a room</h1><p className="entry-lede">Enter the room code from your host.</p>
        <form className="lookup-form" onSubmit={openRoom} noValidate>
          <FieldGroup>
            <Field data-invalid={Boolean(error)}>
              <FieldLabel htmlFor="room-code">Room code</FieldLabel>
              <Input ref={codeInputRef} id="room-code" name="room-code" value={code} onChange={(event) => { setCode(event.target.value.replace(/\D/g, "").slice(0, 6)); setError(""); }} inputMode="numeric" autoComplete="off" maxLength={6} placeholder="000000" required aria-invalid={Boolean(error)} aria-describedby={error ? "room-code-error" : "room-code-hint"} />
              <FieldDescription id="room-code-hint">Use the code shared by your host.</FieldDescription>
              {error && <FieldError id="room-code-error">{error}</FieldError>}
            </Field>
            <Button type="submit">Open room <ArrowRight data-icon="inline-end" aria-hidden="true" /></Button>
          </FieldGroup>
        </form>
      </section>
      <section className="entry-welcome" aria-hidden="true"><div className="entry-cast">{characters.map(character => <CharacterImage key={character.id} character={character} kind="portrait" />)}</div><p className="entry-title">Double Riichi</p><p>Your next game starts here.</p></section>
    </main><footer className="site-footer"><span>No account required.</span></footer>
  </div>;
}

function RoomRoute({ joinCode }: { joinCode: string }) {
  const [state, setState] = useState<{ status: "loading" | "ready" | "error"; room?: import("./api").RoomLookup; characters?: HumanCharacter[]; problem?: ProblemDetails }>({ status: "loading" });
  const [retry, setRetry] = useState(0);
  useEffect(() => { let active = true; setState({ status: "loading" }); void api.lookupRoom(joinCode).then(async (room) => { if (!active) return; if (!room.join_allowed) { setState({ status: "ready", room, characters: [] }); return; } try { const characters = (await api.listHumanCharacters()).sort((left, right) => left.name.localeCompare(right.name)); if (active) setState({ status: "ready", room, characters }); } catch (error) { if (active) setState({ status: "ready", room, problem: problemFrom(error) }); } }).catch((error) => { if (active) setState({ status: "error", problem: problemFrom(error) }); }); return () => { active = false; }; }, [joinCode, retry]);
  return <div className="app-shell route-shell pre-match-shell"><AppShell variant="public">{null}</AppShell>{state.status === "loading" && <LoadingRoom />}{state.status === "error" && <ProblemState title="Room unavailable" problem={state.problem} actionLabel="Try another room" />}{state.status === "ready" && state.room && <RoomJoin key={joinCode} room={state.room} joinCode={joinCode} characters={state.characters ?? []} characterProblem={state.problem} onRetryCharacters={() => setRetry((value) => value + 1)} />}</div>;
}
function LoadingRoom() { return <main className="route-main loading-state" aria-busy="true" aria-live="polite"><div className="loading-copy"><span className="eyebrow">ROOM LOOKUP</span><h1>Loading room</h1><p>Checking the host for room details.</p></div><div className="loading-lines" aria-hidden="true"><span /><span /><span /></div></main>; }
function ProblemState({ title, problem, actionLabel }: { title: string; problem?: ProblemDetails; actionLabel: string }) { return <main className="route-main status-main"><div className="status-icon" aria-hidden="true"><WarningCircle weight="regular" /></div><p className="eyebrow">REQUEST STATUS</p><h1>{title}</h1><p className="status-detail" role="alert">{problem?.detail ?? problem?.title ?? "The host could not complete the request."}</p>{problem?.request_id && <p className="request-id">Request ID <code>{problem.request_id}</code></p>}<RouteLink href="/" className="button button-secondary">{actionLabel} <ArrowLeft aria-hidden="true" weight="regular" /></RouteLink></main>; }

function RoomJoin({ room, joinCode, characters, characterProblem, onRetryCharacters }: { room: import("./api").RoomLookup; joinCode: string; characters: HumanCharacter[]; characterProblem?: ProblemDetails; onRetryCharacters: () => void }) {
  const [step, setStep] = useState<"room-name" | "character">("room-name");
  const [nameError, setNameError] = useState("");
  const headingRef = useRef<HTMLHeadingElement>(null);
  const activeRef = useRef(true);
  useEffect(() => { activeRef.current = true; return () => { activeRef.current = false; }; }, []);
  useEffect(() => { if (step === "character") headingRef.current?.focus(); }, [step]);
  const [nickname, setNickname] = useState(""); const [selectedId, setSelectedId] = useState(""); const [voiceStatus, setVoiceStatus] = useState("Preview riichi voice"); const [joinState, setJoinState] = useState<{ status: "idle" | "joining" | "error" | "success"; problem?: ProblemDetails }>({ status: "idle" }); const selected = characters.find((character) => character.id === selectedId); const canJoin = room.join_allowed && nickname.trim().length > 0 && Boolean(selected) && joinState.status !== "joining";
  async function join(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!room.join_allowed) return;
    if (step === "room-name") {
      if (!nickname.trim()) { setNameError("Enter a display name."); return; }
      setNameError(""); setStep("character"); return;
    }
    if (!canJoin || !selected) return;
    setJoinState({ status: "joining" });
    try {
      const result = await api.joinHuman(joinCode, nickname.trim(), selected.id);
      if (!activeRef.current) return;
      sessionStorage.setItem(`driichi:participant:${joinCode}`, result.participant_id);
      setJoinState({ status: "success" });
    } catch (error) { if (activeRef.current) setJoinState({ status: "error", problem: problemFrom(error) }); }
  }
  const audioRef = useRef<HTMLAudioElement | null>(null);
  useEffect(() => { setVoiceStatus('Preview riichi voice'); return () => { audioRef.current?.pause(); audioRef.current = null; }; }, [selectedId, step]);
  function previewVoice() {
    if (!selected) return;
    audioRef.current?.pause();
    if (!window.Audio) { setVoiceStatus('Voice preview is unavailable'); return; }
    const audio = new Audio(`/assets/characters/${encodeURIComponent(selected.id)}/voices/riichi.ogg`);
    audioRef.current = audio;
    setVoiceStatus('Loading riichi voice');
    audio.onended = () => { if (audioRef.current === audio) setVoiceStatus('Preview riichi voice'); };
    void audio.play().then(() => { if (audioRef.current === audio) setVoiceStatus('Playing riichi voice'); }).catch(() => { if (audioRef.current === audio) setVoiceStatus('Voice preview is unavailable'); });
  }
  if (joinState.status === "success") return <JoinSuccess roomName={room.room_name} joinCode={joinCode} />;
  return <main className="route-main room-main" data-join-step={step}>
    {!room.join_allowed ? <Alert role="status"><AlertDescription>This room is full. Ask the host for another code.</AlertDescription></Alert> : <>
      <form className="join-form" onSubmit={join} noValidate aria-busy={joinState.status === "joining"}>
        <h1 id="room-heading" ref={headingRef} tabIndex={-1}>{step === "room-name" ? `Join ${room.room_name}` : "Choose your character"}</h1>
        <FieldGroup>
          {step === "room-name" ? <>
            <p>Step 1 of 2 · Room and name</p>
            <details className="join-room-details"><summary>Room details</summary><p>{room.game_mode} · {room.phase}</p><p>{room.participant_count} / {room.participant_limit} participants</p></details>
            <Field data-invalid={Boolean(nameError)}>
              <FieldLabel htmlFor="display-name">Display name</FieldLabel>
              <Input id="display-name" value={nickname} onChange={(event) => { setNickname(event.target.value); setNameError(""); }} maxLength={64} autoComplete="nickname" autoFocus required aria-invalid={Boolean(nameError)} aria-describedby={nameError ? "display-name-error" : "display-name-hint"} />
              <FieldDescription id="display-name-hint">This name is shown to the room.</FieldDescription>
              {nameError && <FieldError id="display-name-error">{nameError}</FieldError>}
            </Field>
            <Button type="submit">Continue <ArrowRight data-icon="inline-end" aria-hidden="true" /></Button>
          </> : <>
            <p>Step 2 of 2 · Character</p>
            <FieldSet disabled={joinState.status === "joining"}>
              <FieldLegend id="character-legend">Character</FieldLegend>
              {characterProblem && <><Alert variant="destructive"><AlertDescription>{characterProblem.detail ?? "Characters could not be loaded."}</AlertDescription></Alert><Button type="button" variant="outline" onClick={onRetryCharacters}>Retry character list</Button></>}
              {characters.length === 0 && !characterProblem && <FieldDescription>Characters are not available yet.</FieldDescription>}
              <Field data-invalid={joinState.status === "error"}>
                <RadioGroup className="character-list" disabled={joinState.status === "joining"} aria-labelledby="character-legend" aria-required="true" aria-invalid={joinState.status === "error"} aria-describedby={joinState.status === "error" ? "join-error" : "character-hint"} value={selectedId} onValueChange={(value) => setSelectedId(String(value))}>
                  {characters.map((character) => <FieldLabel key={character.id} className="join-character-option"><RadioGroupItem value={character.id} /><CharacterImage character={character} kind="icon" /><span>{character.name}</span></FieldLabel>)}
                </RadioGroup>
                <FieldDescription id="character-hint">Choose a character to enable Join room. Characters are cosmetic and do not affect play.</FieldDescription>
              </Field>
            </FieldSet>
            {selected && <><Button type="button" variant="outline" onClick={previewVoice}><SpeakerHigh data-icon="inline-start" aria-hidden="true" />{voiceStatus}</Button><span className="sr-only" role="status">{voiceStatus}</span></>}
            {joinState.status === "error" && <Alert id="join-error" variant="destructive"><AlertDescription>{joinState.problem?.detail ?? "The host could not join you to this room."}</AlertDescription></Alert>}
            <div className="join-actions"><Button type="button" variant="outline" disabled={joinState.status === "joining"} onClick={() => setStep("room-name")}><ArrowLeft data-icon="inline-start" aria-hidden="true" />Back</Button><Button type="submit" disabled={!canJoin}>{joinState.status === "joining" ? "Joining room" : "Join room"}<ArrowRight data-icon="inline-end" aria-hidden="true" /></Button></div>
          </>}
        </FieldGroup>
      </form>
      {step === "character" && <section className="character-stage" aria-label="Selected character">{selected ? <><CharacterImage key={selected.id} character={selected} kind="portrait" /><p>{selected.name}</p></> : <p>Choose a character to preview their portrait.</p>}</section>}
    </>}
  </main>;
}
function JoinSuccess({ roomName, joinCode }: { roomName: string; joinCode: string }) { return <main className="route-main status-main success-main"><div className="status-icon success-icon" aria-hidden="true"><Check weight="regular" /></div><p className="eyebrow">JOIN ACCEPTED</p><h1>You're in {roomName}.</h1><p className="status-detail">Your Guest Session is ready. Enter the Lobby to follow the host.</p><RouteLink href={`/room/${joinCode}/lobby`} className="button button-primary">Enter Lobby <ArrowRight aria-hidden="true" weight="regular" /></RouteLink></main>; }

function CharacterImage({ character, kind }: { character: { id: string; name: string }; kind: "icon" | "portrait" }) { const [failed, setFailed] = useState(false); if (failed) return <span className={`character-asset character-asset-${kind} asset-fallback`} role="img" aria-hidden={kind === "icon" ? true : undefined} aria-label={kind === "portrait" ? `${character.name} portrait unavailable` : undefined}><WarningCircle aria-hidden="true" weight="regular" /></span>; return <img className={`character-asset character-asset-${kind}`} src={`/assets/characters/${encodeURIComponent(character.id)}/${kind === "icon" ? "icon" : "portrait"}.webp`} alt={kind === "portrait" ? `${character.name} portrait` : ""} onError={() => setFailed(true)} />; }

function AdminLogin() {
  const [username, setUsername] = useState(""); const [password, setPassword] = useState(""); const [state, setState] = useState<{ status: "idle" | "loading" | "error" | "success"; problem?: ProblemDetails }>({ status: "idle" });
  async function submit(event: React.FormEvent<HTMLFormElement>) { event.preventDefault(); setState({ status: "loading" }); try { await api.loginAdmin(username, password); setPassword(""); setState({ status: "success" }); toast.add({ title: "Signed in", type: "success" }); } catch (error) { setState({ status: "error", problem: problemFrom(error) }); } }
  if (state.status === "success") return <div className="app-shell route-shell"><AppShell variant="public">{null}</AppShell><main className="route-main status-main success-main"><div className="status-icon success-icon" aria-hidden="true"><Check weight="regular" /></div><p className="eyebrow">ADMIN ACCESS</p><h1>Admin session active.</h1><p className="status-detail">The host session is ready. Room controls continue in the Admin surface.</p><RouteLink href="/admin" className="button button-primary">Open Admin <ArrowRight aria-hidden="true" weight="regular" /></RouteLink></main></div>;
  return <div className="app-shell route-shell"><AppShell variant="public">{null}</AppShell><main className="route-main admin-main"><section className="admin-copy"><span className="admin-lock" aria-hidden="true"><LockKey weight="regular" /></span><p className="eyebrow">HOST ACCESS</p><h1>Sign in to host.</h1><p>Manage rooms from the private Admin surface.</p></section><form className="admin-form" onSubmit={submit}><div className="form-block"><label htmlFor="admin-username">Username</label><input id="admin-username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" /></div><div className="form-block"><label htmlFor="admin-password">Password</label><input id="admin-password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" autoFocus /></div>{state.status === "error" && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{state.problem?.detail ?? "The host could not sign you in."}</p>}<button type="submit" className="button button-primary" disabled={state.status === "loading"}>{state.status === "loading" ? "Signing in" : "Sign in"} <ArrowRight aria-hidden="true" weight="regular" /></button><p className="security-note"><LockKey aria-hidden="true" weight="regular" />Session cookies stay in the browser.</p></form></main></div>;
}

const queryKeys = { rooms: ["admin", "rooms"] as const, room: (code: string) => ["admin", "room", code] as const, tokens: ["admin", "tokens"] as const };
function AdminWorkspace({ routeCode }: { routeCode?: string }) {
  const queryClient = useQueryClient(); const [selectedCode, setSelectedCode] = useState(routeCode ?? ""); const [createOpen, setCreateOpen] = useState(false);
  const rooms = useQuery({ queryKey: queryKeys.rooms, queryFn: api.listAdminRooms }); const tokens = useQuery({ queryKey: queryKeys.tokens, queryFn: api.listBotTokens });
  useEffect(() => { if (routeCode) setSelectedCode(routeCode); else if (!selectedCode && rooms.data?.[0]) setSelectedCode(rooms.data[0].join_code); }, [routeCode, selectedCode, rooms.data]);
  const detail = useQuery({ queryKey: queryKeys.room(selectedCode), queryFn: () => api.getAdminRoom(selectedCode), enabled: /^\d{6}$/.test(selectedCode), refetchInterval: 2000, refetchIntervalInBackground: false });
  const invalidate = () => { void queryClient.invalidateQueries({ queryKey: queryKeys.rooms }); if (selectedCode) void queryClient.invalidateQueries({ queryKey: queryKeys.room(selectedCode) }); };
  const action = useMutation({ mutationFn: (task: () => Promise<unknown>) => task(), onSuccess: invalidate });
  const create = useMutation({ mutationFn: api.createAdminRoom, onSuccess: (room) => { void queryClient.invalidateQueries({ queryKey: queryKeys.rooms }); setSelectedCode(room.join_code); setCreateOpen(false); navigate(`/admin/rooms/${room.join_code}`); } });
  return <div className="app-shell workspace-shell" data-admin-view={routeCode ? "detail" : "list"}><SidebarProvider className="admin-workspace"><Sidebar collapsible="none" className="workspace-rail" role="complementary" aria-label="Room management"><SidebarContent><div className="workspace-rail-head"><h1>Rooms</h1></div><details className="admin-disclosure create-room-disclosure"><summary>Create room</summary><button className="button button-secondary" onClick={() => setCreateOpen(true)}>Set up room <Plus aria-hidden="true" weight="regular" /></button></details>{rooms.isLoading && <p className="state-label" aria-busy="true">Loading Rooms</p>}{rooms.isError && <ProblemInline error={rooms.error} />}{rooms.data?.length === 0 && <div className="empty-state"><p>No Rooms yet.</p><button className="button button-secondary" onClick={() => setCreateOpen(true)}>Create a Room <Plus aria-hidden="true" weight="regular" /></button></div>}<nav className="room-list" aria-label="Admin Rooms"><Table><TableHeader className="sr-only"><TableRow><TableHead>Room</TableHead></TableRow></TableHeader><TableBody>{rooms.data?.map((room) => <TableRow key={room.join_code}><TableCell><RouteLink href={`/admin/rooms/${room.join_code}`} className="room-list-item" aria-current={routeCode === room.join_code ? "page" : undefined} onClick={() => setSelectedCode(room.join_code)}><span><strong>{room.room_name}</strong><small>{room.join_code} · {room.game_mode} · {room.phase}</small></span></RouteLink></TableCell></TableRow>)}</TableBody></Table></nav><TokenPanel tokens={tokens.data ?? []} loading={tokens.isLoading} error={tokens.error} action={action} queryClient={queryClient} /></SidebarContent></Sidebar><section className="workspace-main" aria-label="Room detail">{routeCode && <RouteLink href="/admin" className="admin-back">Back to Rooms</RouteLink>}{!selectedCode && !rooms.isLoading && <EmptyDetail onCreate={() => setCreateOpen(true)} />}{selectedCode && detail.isLoading && <LoadingPanel label="Loading Room detail" />}{selectedCode && detail.isError && <ProblemPanel title="Room detail unavailable" error={detail.error} />}{detail.data && <RoomDetailPanel key={detail.data.join_code} room={detail.data} action={action} onDeleted={() => { void queryClient.invalidateQueries({ queryKey: queryKeys.rooms }); setSelectedCode(""); navigate("/admin"); }} />}</section></SidebarProvider><CreateRoomDialog open={createOpen} onClose={() => setCreateOpen(false)} mutation={create} /></div>;
}

function EmptyDetail({ onCreate }: { onCreate: () => void }) { return <section className="empty-detail"><p className="eyebrow">ROOM DETAIL</p><h2>Choose a Room.</h2><p>Open a Room from the rail, or create the next one.</p><button className="button button-primary" onClick={onCreate}>Create Room <Plus aria-hidden="true" weight="regular" /></button></section>; }
function LoadingPanel({ label }: { label: string }) { return <div className="panel-state" aria-busy="true"><span className="state-label">{label}</span><div className="loading-lines" aria-hidden="true"><span /><span /><span /></div></div>; }
function ProblemInline({ error }: { error: unknown }) { const problem = problemFrom(error); return <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{problem.detail ?? problem.title ?? "The host could not complete the request."}</p>; }
function ProblemPanel({ title, error }: { title: string; error: unknown }) { const problem = problemFrom(error); return <section className="empty-detail"><p className="eyebrow">REQUEST STATUS</p><h2>{title}</h2><ProblemInline error={problem} />{problem.request_id && <p className="request-id">Request ID <code>{problem.request_id}</code></p>}<button className="button button-secondary" onClick={() => window.location.reload()}>Retry <ArrowRight aria-hidden="true" weight="regular" /></button></section>; }

type ActionMutation = { isPending: boolean; isError: boolean; error: unknown; mutate: (task: () => Promise<unknown>) => void };
type CreateMutation = { isPending: boolean; isError: boolean; error: unknown; mutate: (input: { room_name: string; game_mode: GameMode; time_control: TimeControl; replay_save: boolean }) => void };
function RoomDetailPanel({ room, action, onDeleted }: { room: AdminRoomDetail; action: ActionMutation; onDeleted: () => void }) {
  const [settings, setSettings] = useState({ room_name: room.room_name, game_mode: room.game_mode, time_control: room.time_control, replay_save: room.replay_save, participant_limit: room.participant_limit }); const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => setSettings({ room_name: room.room_name, game_mode: room.game_mode, time_control: room.time_control, replay_save: room.replay_save, participant_limit: room.participant_limit }), [room.room_name, room.game_mode, room.time_control, room.replay_save, room.participant_limit]);
  const lobby = room.phase === "lobby" && !room.benchmark; const seats = room.game_mode.startsWith("3p") ? 3 : 4;
  const run = (task: () => Promise<unknown>) => action.mutate(task);
  const readyCount = room.participants.filter((participant) => participant.selected && (participant.kind !== "human" || participant.ready)).length;
  return <section className="room-detail" aria-labelledby="room-detail-heading">
    <header className="detail-heading"><div><h1 id="room-detail-heading">{room.room_name}</h1><p className="detail-lede">{room.join_code} · {titleCasePhase(room.phase)} · {room.game_mode} · Revision {room.revision}</p></div>{!room.benchmark_mode && <RouteLink href={`/room/${room.join_code}/lobby`} className="detail-lobby-link">Open lobby</RouteLink>}</header>
    <section className="flat-section participants-section"><div className="section-heading"><h2>Participants <span className="state-label">{room.selected_count} of {seats} selected</span></h2>{room.benchmark_mode ? <button className="button button-secondary small-button" disabled={!lobby || action.isPending || room.participant_count >= room.participant_limit} onClick={() => run(() => api.addBenchmarkBot(room.join_code))}>Add Built-in Bot</button> : <button className="button button-secondary small-button" disabled={!lobby || action.isPending || room.selected_count >= seats} onClick={() => run(() => api.fillWithBots(room.join_code))}>Fill with Bots</button>}</div>{room.participants.length === 0 ? <p className="field-hint">No Participants have joined this Room.</p> : <div className="participant-list">{room.participants.map((participant) => <ParticipantRow key={participant.participant_id} participant={participant} room={room} disabled={!lobby || action.isPending} onAction={run} />)}</div>}</section>
    {room.benchmark_mode ? <><SurfaceLoader label="Benchmark"><BenchmarkControls room={room} pending={action.isPending} perform={run} /></SurfaceLoader>{action.isError && <ProblemInline error={action.error} />}</> : <section className="flat-section lifecycle-section"><div className="section-heading"><h2>Match control</h2></div><div className="action-row"><button className="button button-primary" disabled={!lobby || action.isPending || room.selected_count !== seats || readyCount !== seats} onClick={() => run(() => api.startRoom(room.join_code))}>Start Match <ArrowRight aria-hidden="true" weight="regular" /></button><p className="start-note">{readyCount} of {seats} selected players ready</p><button className="button button-secondary" disabled={room.phase !== "post_match" || action.isPending} onClick={() => run(() => api.rematchRoom(room.join_code))}>Rematch</button><button className="button button-secondary" disabled={room.phase !== "post_match" || action.isPending} onClick={() => run(() => api.backToLobby(room.join_code))}>Back to Lobby</button></div>{action.isError && <ProblemInline error={action.error} />}</section>}
    <RosterRail room={room} seats={seats} />
    <details className="admin-disclosure settings-disclosure"><summary>Room settings <span className="state-label">{lobby ? "Editable" : "Locked"}</span></summary><FieldGroup className="settings-grid"><Field data-disabled={!lobby}><FieldLabel htmlFor="room-name">Room name</FieldLabel><Input id="room-name" value={settings.room_name} disabled={!lobby} onChange={(event) => setSettings({ ...settings, room_name: event.target.value })} /></Field><Field data-disabled={!lobby}><FieldLabel htmlFor="game-mode">Game mode</FieldLabel><select id="game-mode" value={settings.game_mode} disabled={!lobby} onChange={(event) => setSettings({ ...settings, game_mode: event.target.value as GameMode })}><option value="4p-red-east">4p red East</option><option value="4p-red-half">4p red half</option><option value="3p-red-east">3p red East</option><option value="3p-red-half">3p red half</option></select></Field><Field data-disabled={!lobby}><FieldLabel htmlFor="time-control">Time control</FieldLabel><select id="time-control" value={settings.time_control} disabled={!lobby} onChange={(event) => setSettings({ ...settings, time_control: event.target.value as TimeControl })}><option value="casual">Casual 30 / 10</option><option value="riichi_dev">Riichi.dev</option><option value="unlimited">Unlimited</option></select></Field><Field data-disabled={!lobby}><FieldLabel htmlFor="participant-limit">Participant limit</FieldLabel><Input id="participant-limit" type="number" min={seats} max={32} value={settings.participant_limit} disabled={!lobby} onChange={(event) => setSettings({ ...settings, participant_limit: Number(event.target.value) })} /></Field></FieldGroup><label className="check-row"><input type="checkbox" checked={settings.replay_save} disabled={!lobby || room.benchmark_mode} onChange={(event) => setSettings({ ...settings, replay_save: event.target.checked })} />Save Replay</label><button className="button button-secondary" disabled={!lobby || action.isPending} onClick={() => run(() => api.patchAdminRoom(room.join_code, settings))}>{action.isPending ? "Saving settings" : "Save settings"}</button><button className="button button-secondary danger-button" disabled={room.phase === "playing" || action.isPending} onClick={() => setConfirmDelete(true)}>Delete Room</button></details>
    <ConfirmDialog open={confirmDelete} title="Delete Room?" description={`Delete ${room.room_name}? Connected Participants will lose this Room Session.`} confirmLabel="Confirm delete" onClose={() => setConfirmDelete(false)} onConfirm={() => { setConfirmDelete(false); run(async () => { await api.deleteAdminRoom(room.join_code); onDeleted(); }); }} />
  </section>;
}

function ParticipantRow({ participant, room, disabled, onAction }: { participant: RoomParticipant; room: AdminRoomDetail; disabled: boolean; onAction: (task: () => Promise<unknown>) => void }) { const [confirmKick, setConfirmKick] = useState(false); const selectedCharacter = { id: participant.character_id, name: participant.character_id }; return <article className="participant-row"><CharacterImage character={selectedCharacter} kind="icon" /><div className="participant-main"><strong>{participant.display_name}</strong><span className="state-label">{participant.kind}</span></div><dl className="participant-axes"><div><dt>Identity</dt><dd>{participant.participant_id.slice(0, 8)}</dd></div><div><dt>Presence</dt><dd>{participant.presence}</dd></div><div><dt>Selection</dt><dd>{participant.selected ? "selected" : "unselected"}</dd></div><div><dt>Ready</dt><dd>{participant.ready ? "ready" : "not ready"}</dd></div><div><dt>Controller</dt><dd>{participant.controller.replaceAll("_", " ")}</dd></div></dl><div className="participant-actions"><button className="text-button" disabled={disabled || participant.selected} onClick={() => onAction(() => api.selectParticipant(room.join_code, participant.participant_id))}>Select</button><button className="text-button" disabled={disabled || !participant.selected} onClick={() => onAction(() => api.deselectParticipant(room.join_code, participant.participant_id))}>Deselect</button><button className="text-button danger-text" disabled={disabled} onClick={() => setConfirmKick(true)}>Kick</button></div><ConfirmDialog open={confirmKick} title="Kick Participant?" description={`Remove ${participant.display_name} from this Room?`} confirmLabel="Confirm kick" onClose={() => setConfirmKick(false)} onConfirm={() => { setConfirmKick(false); onAction(() => api.kickParticipant(room.join_code, participant.participant_id)); }} /></article>; }
function RosterRail({ room, seats }: { room: AdminRoomDetail; seats: number }) { const roster = room.roster.length ? room.roster : room.match_players;
  if (room.phase === "lobby") return <section className="roster-rail" aria-label="Selected roster"><h2>Selected roster</h2><ul>{room.participants.filter(participant => participant.selected).map(participant => <li key={participant.participant_id}>{participant.display_name}</li>)}</ul></section>;
  return <aside className="roster-rail" aria-label="Selected roster"><div className="section-heading"><div><p className="eyebrow">{room.phase === "post_match" ? "PREVIOUS MATCH SEATS" : "MATCH SEATS"}</p><h2>{seats} seats</h2></div><span className="state-label">LIVE</span></div><div className="seat-list">{Array.from({ length: seats }, (_, seat) => { const player = roster.find((entry) => entry.seat === seat); return <div className={`seat-row${player ? " is-filled" : ""}`} key={seat}><span className="seat-number">{String(seat + 1).padStart(2, "0")}</span>{player ? <><CharacterImage character={{ id: player.character_id ?? "", name: player.display_name }} kind="portrait" /><span><strong>{player.display_name}</strong><small>{player.controller.replaceAll("_", " ")}</small></span></> : <span className="empty-seat">Seat open</span>}</div>; })}</div></aside>; }

function TokenPanel({ tokens, loading, error, action, queryClient }: { tokens: BotTokenRecord[]; loading: boolean; error: unknown; action: ActionMutation; queryClient: ReturnType<typeof useQueryClient> }) {
  const [name, setName] = useState(""); const [secret, setSecret] = useState<CreatedBotToken | null>(null); const [revoke, setRevoke] = useState<BotTokenRecord | null>(null);
  const tokenNameRef = useRef<HTMLInputElement>(null);
  const create = () => { action.mutate(() => api.createBotToken(name.trim()).then((created) => { setName(""); setSecret(created); void queryClient.invalidateQueries({ queryKey: queryKeys.tokens }); })); };
  return <section className="token-panel"><details className="admin-disclosure"><summary>Bot tokens</summary><p className="field-hint">Raw secrets appear once. They are never listed again.</p><FieldGroup className="token-create"><Field><FieldLabel htmlFor="token-name">Token name</FieldLabel><Input ref={tokenNameRef} id="token-name" value={name} maxLength={64} onChange={(event) => setName(event.target.value)} placeholder="e.g. night-market-runner" /><Button disabled={!name.trim() || action.isPending} onClick={create}>{action.isPending ? "Creating token" : "Create token"}<Plus data-icon="inline-end" aria-hidden="true" /></Button></Field></FieldGroup>{loading && <p className="state-label" aria-busy="true">Loading Tokens</p>}{error != null && <ProblemInline error={error} />}{!loading && !error && tokens.length === 0 && <p className="field-hint">No Bot Tokens have been created.</p>}{tokens.length > 0 && <div className="token-list">{tokens.map((token) => <article className="token-row" key={token.token_id}><div><strong>{token.name}</strong><span className="state-label">{token.state}</span></div><code>{token.token_id}</code><time dateTime={token.created_at}>{token.created_at}</time><button className="text-button danger-text" disabled={token.state !== "active" || action.isPending} aria-label={`Revoke ${token.name}`} onClick={() => setRevoke(token)}>Revoke</button></article>)}</div>}{action.isError && <ProblemInline error={action.error} />}</details><OneTimeTokenDialog token={secret} onClose={() => { setSecret(null); window.setTimeout(() => tokenNameRef.current?.focus(), 0); }} /><ConfirmDialog open={Boolean(revoke)} title="Revoke Bot Token?" description={`Revoke ${revoke?.name ?? "this Token"}? Every connection using it will lose access.`} confirmLabel="Confirm revoke" onClose={() => setRevoke(null)} onConfirm={() => { if (!revoke) return; const token = revoke; setRevoke(null); action.mutate(() => api.revokeBotToken(token.token_id).then((result) => { void queryClient.invalidateQueries({ queryKey: queryKeys.tokens }); return result; })); }} /></section>;
}
function ModalDialog({ open, onClose, className, labelledBy, children }: { open: boolean; onClose: () => void; className: string; labelledBy: string; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  const closeRef = useRef(onClose);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  closeRef.current = onClose;
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || !open) return;
    returnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    try { dialog.showModal(); } catch { dialog.setAttribute("open", ""); }
    const focusTarget = dialog.querySelector<HTMLElement>("[data-dialog-autofocus]") ?? dialog.querySelector<HTMLElement>("button, input, select, textarea");
    focusTarget?.focus();
    const closeOnEscape = (event: Event) => { if (event instanceof KeyboardEvent && event.key !== "Escape") return; event.preventDefault(); closeRef.current(); };
    dialog.addEventListener("cancel", closeOnEscape);
    dialog.addEventListener("keydown", closeOnEscape);
    return () => {
      dialog.removeEventListener("cancel", closeOnEscape);
      dialog.removeEventListener("keydown", closeOnEscape);
      if (dialog.open) {
        if (typeof dialog.close === "function") dialog.close();
        else dialog.removeAttribute("open");
      }
      if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus();
    };
  }, [open]);
  if (!open) return null;
  return <dialog ref={ref} className={className} aria-labelledby={labelledBy}>{children}</dialog>;
}

function OneTimeTokenDialog({ token, onClose }: { token: CreatedBotToken | null; onClose: () => void }) { const [copied, setCopied] = useState(false); useEffect(() => { setCopied(false); }, [token]); if (!token) return null; return <ModalDialog open={Boolean(token)} onClose={onClose} className="confirm-dialog secret-dialog" labelledBy="secret-heading"><div className="dialog-top"><p className="eyebrow">ONE-TIME SECRET</p><button className="icon-button" aria-label="Close token" onClick={onClose}><X aria-hidden="true" weight="regular" /></button></div><h2 id="secret-heading">Copy this Bot Token now.</h2><p>This value will not be shown again and is removed when this dialog closes.</p><code className="secret-value">{token.token}</code><div className="dialog-actions"><button className="button button-secondary" onClick={async () => { await navigator.clipboard?.writeText(token.token); setCopied(true); }}>{copied ? "Copied" : "Copy Token"} <Copy aria-hidden="true" weight="regular" /></button><button data-dialog-autofocus className="button button-primary" onClick={onClose}>Close token <Check aria-hidden="true" weight="regular" /></button></div></ModalDialog>; }
function ConfirmDialog({ open, title, description, confirmLabel, onClose, onConfirm }: { open: boolean; title: string; description: string; confirmLabel: string; onClose: () => void; onConfirm: () => void }) { return <ModalDialog open={open} onClose={onClose} className="confirm-dialog" labelledBy="confirm-heading"><div className="dialog-top"><p className="eyebrow">CONFIRMATION</p><button className="icon-button" aria-label="Close dialog" onClick={onClose}><X aria-hidden="true" weight="regular" /></button></div><h2 id="confirm-heading">{title}</h2><p>{description}</p><div className="dialog-actions"><button data-dialog-autofocus className="button button-secondary" onClick={onClose}>Cancel</button><button className="button button-primary" onClick={onConfirm}>{confirmLabel}</button></div></ModalDialog>; }
function CreateRoomDialog({ open, onClose, mutation }: { open: boolean; onClose: () => void; mutation: CreateMutation }) { const [name, setName] = useState(""); const [mode, setMode] = useState<GameMode>("4p-red-east"); const [timeControl, setTimeControl] = useState<TimeControl>("casual"); return <ModalDialog open={open} onClose={onClose} className="confirm-dialog create-dialog" labelledBy="create-heading"><div className="dialog-top"><p className="eyebrow">NEW ROOM</p><button className="icon-button" aria-label="Close dialog" onClick={onClose}><X aria-hidden="true" weight="regular" /></button></div><h2 id="create-heading">Create a Room.</h2><div className="form-block"><label htmlFor="new-room-name">Room name</label><input id="new-room-name" value={name} onChange={(event) => setName(event.target.value)} data-dialog-autofocus /></div><div className="form-block"><label htmlFor="new-room-mode">Game mode</label><select id="new-room-mode" value={mode} onChange={(event) => setMode(event.target.value as GameMode)}><option value="4p-red-east">4p red East</option><option value="4p-red-half">4p red half</option><option value="3p-red-east">3p red East</option><option value="3p-red-half">3p red half</option></select></div><div className="form-block"><label htmlFor="new-room-time">Time control</label><select id="new-room-time" value={timeControl} onChange={(event) => setTimeControl(event.target.value as TimeControl)}><option value="casual">Casual 30 / 10</option><option value="riichi_dev">Riichi.dev</option><option value="unlimited">Unlimited</option></select></div>{mutation.isError && <ProblemInline error={mutation.error} />}<div className="dialog-actions"><button className="button button-secondary" onClick={onClose}>Cancel</button><button className="button button-primary" disabled={!name.trim() || mutation.isPending} onClick={() => mutation.mutate({ room_name: name.trim(), game_mode: mode, time_control: timeControl, replay_save: true })}>{mutation.isPending ? "Creating Room" : "Create Room"}</button></div></ModalDialog>; }

type SocketRoom = RoomSnapshot;
type HumanSocketMessage = { type: string; room?: SocketRoom; state?: unknown; event?: unknown; code?: string; status?: string; decision_id?: string; action_id?: string };

const TERMINAL_CLOSE_REASONS = new Set([
  "connected_elsewhere",
  "room_deleted",
  "server_shutdown",
  "slow_consumer",
  "token_revoked",
  "session_expired",
]);
const CLOSE_REASON_BY_CODE: Record<number, string> = {
  4001: "connected_elsewhere",
  4002: "room_deleted",
  4003: "server_shutdown",
  4005: "slow_consumer",
  4006: "session_expired",
};

function terminalCloseReason(event: CloseEvent): string | null {
  const statedReason = event.reason?.trim() || "";
  if (TERMINAL_CLOSE_REASONS.has(statedReason)) return statedReason;
  const mappedReason = CLOSE_REASON_BY_CODE[event.code] || "";
  return TERMINAL_CLOSE_REASONS.has(mappedReason) ? mappedReason : null;
}

export function reconnectDelay(attempt: number, random = Math.random): number {
  const count = Math.max(1, Math.floor(attempt));
  const base = Math.min(10_000, 500 * 2 ** Math.min(count - 1, 4));
  if (count === 1) return base;
  const jitter = Math.round((random() * 2 - 1) * base * 0.2);
  return Math.min(10_000, Math.max(250, base + jitter));
}
function useHumanSocket(joinCode: string) {
  const room = useGameStore((state) => state.room);
  const projection = useGameStore((state) => state.projection);
  const status = useGameStore((state) => state.status);
  const reason = useGameStore((state) => state.reason);
  const commandError = useGameStore((state) => state.commandError);
  const connectionGeneration = useGameStore((state) => state.connectionGeneration);
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectRef = useRef<number | null>(null);
  const attemptRef = useRef(0);
  const generationRef = useRef(0);
  const stoppedRef = useRef(false);
  const participantKey = `driichi:participant:${joinCode}`;
  const [sessionReady, setSessionReady] = useState(false);
  const send = useCallback<Transport>((value) => {
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN || useGameStore.getState().status !== "connected") return false;
    socket.send(JSON.stringify(value));
    return true;
  }, []);
  useEffect(() => {
    stoppedRef.current = false;
    attemptRef.current = 0;
    useGameStore.getState().reset();
    useGameStore.getState().setTransport(send);
    setSessionReady(true);
    const connect = () => {
      if (stoppedRef.current || (socketRef.current && socketRef.current.readyState !== 3)) return;
      const generation = generationRef.current + 1;
      generationRef.current = generation;
      useGameStore.getState().setStatus(attemptRef.current ? "reconnecting" : "connecting");
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const socket = new WebSocket(`${protocol}//${window.location.host}/ws/v1/rooms/${joinCode}/human`);
      socketRef.current = socket;
      socket.onopen = () => {
        if (generationRef.current !== generation || socketRef.current !== socket) return;
        attemptRef.current = 0;
        useGameStore.getState().setStatus("connected");
        useGameStore.setState((state) => ({ connectionGeneration: state.connectionGeneration + 1, reason: "", commandError: "" }));
      };
      socket.onmessage = (event) => {
        if (generationRef.current !== generation || socketRef.current !== socket) return;
        try {
          const message = JSON.parse(event.data) as HumanSocketMessage;
          if (message.room && message.room.join_code !== joinCode) return;
          if (message.type === "snapshot") {
            useGameStore.getState().receiveSnapshot(message.room ?? null, message.state);
          } else if (message.type === "action_result") {
            useGameStore.getState().receiveActionResult(message);
            if (message.status === "rejected") useGameStore.getState().setCommandError("");
            if (message.room || message.state !== undefined) useGameStore.getState().receiveUpdate(message.room ?? null, message.state, message.event);
          } else if (message.type === "error") {
            useGameStore.getState().setCommandError(message.code ?? "request_failed");
          } else {
            useGameStore.getState().receiveUpdate(message.room ?? null, message.state, message.event ?? message);
          }
          if (message.room) {
            const saved = sessionStorage.getItem(participantKey);
            if (!saved) {
              const human = message.room.participants.filter((participant) => participant.kind === "human");
              if (human.length === 1) sessionStorage.setItem(participantKey, human[0].participant_id);
            }
          }
        } catch {
          useGameStore.getState().setCommandError("invalid_message");
        }
      };
      socket.onerror = () => {
        if (generationRef.current !== generation || socketRef.current !== socket) return;
        useGameStore.getState().setStatus("error", "network_error");
      };
      socket.onclose = (event) => {
        if (stoppedRef.current || generationRef.current !== generation || socketRef.current !== socket) return;
        const semanticReason = terminalCloseReason(event);
        if (semanticReason) {
          socketRef.current = null;
          useGameStore.getState().preserveForTerminal(semanticReason);
          if (["room_deleted", "session_expired", "token_revoked"].includes(semanticReason))
            sessionStorage.removeItem(participantKey);
          return;
        }
        attemptRef.current += 1;
        socketRef.current = null;
        useGameStore.getState().resetForReconnect();
        const delay = reconnectDelay(attemptRef.current);
        reconnectRef.current = window.setTimeout(() => { if (generationRef.current === generation && !stoppedRef.current) connect(); }, delay);
      };
    };
    connect();
    return () => {
      stoppedRef.current = true;
      generationRef.current += 1;
      if (reconnectRef.current) window.clearTimeout(reconnectRef.current);
      socketRef.current?.close();
      socketRef.current = null;
      useGameStore.getState().setTransport(null);
    };
  }, [joinCode]);
  return { room: sessionReady ? room : null, projection: sessionReady ? projection : null, status, reason, commandError, connectionGeneration, sessionReady, send };
}

function HumanLobby({ joinCode }: { joinCode: string }) {
  const { room, projection, status, reason, commandError, connectionGeneration, sessionReady, send } = useHumanSocket(joinCode);
  const reducedMotion = useReducedMotion();
  const participantId = sessionStorage.getItem(`driichi:participant:${joinCode}`);
  const participants = room?.participants ?? [];
  const selected = participants.filter((participant) => participant.selected);
  const own = participants.find((participant) => participant.participant_id === participantId)
    ?? (participants.filter((participant) => participant.kind === "human").length === 1
      ? participants.find((participant) => participant.kind === "human")
      : undefined);
  const seatCount = room?.game_mode.startsWith("3p") ? 3 : 4;
  const [preload, setPreload] = useState<"idle" | "loading" | "complete" | "error">("idle");
  const [resultsDismissed, setResultsDismissed] = useState(false);
  const reviewResultsRef = useRef<HTMLButtonElement>(null);
  const selectedIds = sessionReady ? selected.map((participant) => participant.character_id).filter(Boolean).sort().join(",") : "";
  const seatRoster = room ? (room.roster.length ? room.roster : room.match_players) : [];
  const seatByParticipant = new Map(seatRoster.map((player) => [player.participant_id, player.seat]));
  useEffect(() => {
    if (room?.phase !== "post_match") setResultsDismissed(false);
  }, [room?.phase]);
  useEffect(() => {
    if (!resultsDismissed || room?.phase !== "post_match") return;
    const focusTimer = window.setTimeout(() => reviewResultsRef.current?.focus(), 0);
    return () => window.clearTimeout(focusTimer);
  }, [resultsDismissed, room?.phase]);
  const dismissResults = useCallback(() => setResultsDismissed(true), []);
  useEffect(() => {
    let active = true;
    const ids = selectedIds ? selectedIds.split(",") : [];
    if (ids.length === 0) {
      setPreload("complete");
      return () => { active = false; };
    }
    setPreload("loading");
    let completionTimer: number | undefined;
    void Promise.all(ids.flatMap((id) => (["portrait", "icon"] as const).map((kind) => decodeCharacterAsset(id, kind))))
      .then(() => { completionTimer = window.setTimeout(() => { if (active) setPreload("complete"); }, 0); })
      .catch(() => { if (active) setPreload("error"); });
    return () => { active = false; if (completionTimer !== undefined) window.clearTimeout(completionTimer); };
  }, [selectedIds, connectionGeneration]);
  // Post-Match is an authoritative ready phase for the next match; the
  // result presentation is local-only and can be dismissed without changing it.
  const canReady = status === "connected"
    && (room?.phase === "lobby" || room?.phase === "post_match")
    && Boolean(own?.selected)
    && selected.length === seatCount && preload === "complete" && !own?.ready;
  const closeMessage = reason === "connected_elsewhere"
    ? "This Participant connected in another tab."
    : reason === "room_deleted"
      ? "The host deleted this Room."
      : reason === "server_shutdown"
        ? "The host service shut down this connection."
        : reason === "slow_consumer"
          ? "The connection was closed because it could not keep up."
          : reason === "token_revoked"
            ? "This Guest Session token is no longer valid."
            : reason === "session_expired"
              ? "This Guest Session has expired."
              : "";

  const readyCount = selected.filter(participant => participant.ready).length;
  const connectedCount = participants.filter(participant => participant.presence === "connected").length;
  const blockers = [
    ...(status !== "connected" ? ["Reconnect to set Ready."] : []),
    ...(!own ? ["Waiting for your Participant identity."] : !own.selected ? ["The host must select your Participant."] : []),
    ...(selected.length !== seatCount ? [`The host needs ${seatCount} selected Players.`] : []),
    ...(preload === "loading" ? ["Preloading every selected Character."] : preload === "error" ? ["A selected Character asset could not be preloaded."] : []),
  ];
  const tableVisible = room && (room.phase === "playing" || (room.phase === "post_match" && !resultsDismissed));
  const lobby = <div className="app-shell lobby-shell"><AppShell variant="public">{null}</AppShell><main className="lobby-workspace"><header className="lobby-heading"><div><h1>Lobby</h1><p className="detail-lede">Room {joinCode}{room ? ` · ${room.room_name}` : ""}</p>{room && <div className="lobby-counts" aria-label="Room summary"><span>{selected.length} / {seatCount} selected</span><span>{readyCount} / {selected.length} ready</span><span>{connectedCount} connected</span></div>}</div><div className="lobby-heading-actions">{room?.phase === "post_match" && resultsDismissed && <button ref={reviewResultsRef} type="button" className="button button-secondary small-button" data-testid="review-results" onClick={() => setResultsDismissed(false)}>Review final standings</button>}<div className={`connection-state connection-${status}`}><span className="state-dot" aria-hidden="true" />{status}</div></div></header>{closeMessage && <section className="inline-notice lobby-notice" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{closeMessage}<RouteLink href={`/room/${joinCode}`} className="button button-secondary small-button">Return to join</RouteLink></section>}{commandError && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />Room command rejected: {commandError}</p>}{status === "error" && !closeMessage && <ProblemInline error={{ detail: "The live Room connection failed. Reconnecting shortly.", code: reason }} />}{!room && <LoadingPanel label={status === "reconnecting" ? "Reconnecting to Room" : "Connecting to Room"} />}{room && <div className="lobby-grid"><section className="flat-section lobby-roster"><div className="section-heading"><div><h2>Participants</h2></div><p className="field-hint">Open a portrait for Participant details.</p></div>{participants.length === 0 ? <p className="field-hint">No Participants are connected yet.</p> : <ul className="lobby-participants" aria-label="Participants">{participants.slice().sort((left, right) => left.participant_id.localeCompare(right.participant_id)).map((participant) => <li key={participant.participant_id}><LobbyParticipantCard participant={participant} own={participant.participant_id === own?.participant_id} previousSeat={room.phase === "post_match" ? seatByParticipant.get(participant.participant_id) : undefined} /></li>)}</ul>}<div className="ready-block"><div><strong>{own?.ready ? "Ready for the next Match" : "Not ready"}</strong><div id="ready-explanation" className="field-hint" role="status">{blockers.map(message => <p key={message}>{message}</p>)}{blockers.length === 0 && <p>{own?.ready ? "Ready is set. Waiting for the host." : "The complete selected roster is available."}</p>}</div></div><Button disabled={!canReady} aria-describedby="ready-explanation" onClick={() => send({ type: "set_ready", preloaded_characters: selected.map((participant) => participant.character_id) })}>{own?.ready ? "Ready set" : preload === "loading" ? "Preloading roster" : "Set Ready"}<Check data-icon="inline-end" aria-hidden="true" /></Button></div><details className="lobby-blockers"><summary>Match readiness blockers</summary><ul>{selected.filter(participant => !participant.ready || participant.presence !== "connected").map(participant => <li key={participant.participant_id}>{participant.display_name}: {[!participant.ready ? "not ready" : "", participant.presence !== "connected" ? participant.presence : ""].filter(Boolean).join(", ")}</li>)}</ul>{selected.every(participant => participant.ready && participant.presence === "connected") && <p>No Participant readiness blockers.</p>}</details><details><summary>Room status</summary><LifecycleStatus phase={room.phase} /></details></section></div>}</main></div>;
  return tableVisible ? <AppShell variant="table"><DesktopTableBoundary fallback={<><DesktopTableFallback backHref={`/room/${joinCode}`} />{room.phase === "post_match" && <MobileFinalStandings room={room} onReturn={dismissResults} />}{lobby}</>}><SurfaceLoader label="Match"><GameplaySurface room={room} projection={projection} status={status} reason={reason} commandError={commandError} connectionGeneration={connectionGeneration} send={send} reducedMotion={reducedMotion} participantId={participantId} onLeaveResults={room.phase === "post_match" ? dismissResults : undefined} /></SurfaceLoader></DesktopTableBoundary></AppShell> : lobby;
}
function MobileFinalStandings({ room, onReturn }: { room: RoomSnapshot; onReturn: () => void }) {
  const result = decodeRoomFinalStandings(room);
  if (!result) return null;
  return <section className="flat-section" aria-labelledby="mobile-results-heading">
    <h2 id="mobile-results-heading">Final standings</h2>
    <ol className="flex flex-col gap-2" aria-label="Final standings">{result.players.slice().sort((left, right) => left.rank - right.rank).map((player) =>
      <li className="flex gap-4" key={player.participant_id}><span aria-label={`Rank ${player.rank}`}>{player.rank}</span><span>{player.display_name}</span><span>{player.final_score.toLocaleString()}</span></li>,
    )}</ol>
    <button type="button" className="button button-secondary" onClick={onReturn}>Return to Room</button>
  </section>;
}
function LifecycleStatus({ phase }: { phase: string }) { return <div className="lifecycle-status"><p className="eyebrow">ROOM LIFECYCLE</p><ol><li className={phase === "lobby" ? "is-current" : "is-complete"}>Lobby</li><li className={phase === "playing" ? "is-current" : phase === "post_match" ? "is-complete" : ""}>Playing</li><li className={phase === "post_match" ? "is-current" : ""}>Post-Match</li></ol>{phase === "playing" && <p className="field-hint">The Pixi table arrives in the next surface. Your connection remains authoritative.</p>}</div>; }

function titleCasePhase(phase: string) { return phase === "post_match" ? "Post-Match" : phase.charAt(0).toUpperCase() + phase.slice(1); }
function problemFrom(error: unknown): ProblemDetails { if (error instanceof ApiProblem) return error.problem; return { detail: "The host could not complete the request.", code: "request_failed" }; }
function NotFound() { return <div className="app-shell route-shell"><AppShell variant="public">{null}</AppShell><ProblemState title="Page not found" problem={{ detail: "That path is not part of the entry surface." }} actionLabel="Return to entry" /></div>; }
