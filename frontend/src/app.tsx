import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  ArrowLeft, ArrowRight, ArrowUpRight, Check, Copy, Gear, LockKey, Plus, SignOut,
  SpeakerHigh, WarningCircle, X,
} from "@phosphor-icons/react";
import { gsap } from "gsap";
import { useGSAP } from "@gsap/react";
import { api, ApiProblem, type AdminRoomDetail, type AdminRoomSummary, type BotTokenRecord, type CreatedBotToken, type GameMode, type HumanCharacter, type ProblemDetails, type RoomParticipant, type TimeControl } from "./api";
import { TileVignette } from "./pixi-vignette";
import { GameplaySurface } from "./game/gameplay";
import { useGameStore, type Transport } from "./game/store";
import type { ProjectedState, RoomSnapshot } from "./game/types";
import { navigate, routeForPath, type Route } from "./routes";
import "./styles.css";

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
  const [queryClient] = useState(() => new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 0, refetchOnWindowFocus: true } } }));
  return <QueryClientProvider client={queryClient}><AppRoutes /></QueryClientProvider>;
}

function AppRoutes() {
  const [route, setRoute] = useState<Route>(() => routeForPath(window.location.pathname));
  useEffect(() => { const update = () => setRoute(routeForPath(window.location.pathname)); window.addEventListener("popstate", update); return () => window.removeEventListener("popstate", update); }, []);
  if (route.kind === "entry") return <EntryShell />;
  if (route.kind === "room") return <RoomRoute joinCode={route.joinCode} />;
  if (route.kind === "lobby") return <HumanLobby joinCode={route.joinCode} />;
  if (route.kind === "admin-login") return <AdminLogin />;
  if (route.kind === "admin") return <AdminWorkspace routeCode={route.joinCode} />;
  return <NotFound />;
}

function RouteLink({ href, children, className = "", onClick }: { href: string; children: ReactNode; className?: string; onClick?: () => void }) {
  return <a className={className} href={href} onClick={(event) => { if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return; event.preventDefault(); onClick?.(); navigate(href); }}>{children}</a>;
}
function Brand() { return <RouteLink href="/" className="brand" aria-label="Double Riichi home"><span className="brand-mark" aria-hidden="true">二</span><span>DOUBLE RIICHI</span></RouteLink>; }
function Topbar({ action }: { action?: ReactNode }) { return <header className="topbar"><Brand /><nav aria-label="Primary navigation"><span className="nav-context">SELF-HOSTED ROOM SERVICE</span>{action}</nav></header>; }

function EntryShell() {
  const shellRef = useRef<HTMLDivElement>(null); const reduced = useReducedMotion(); const [code, setCode] = useState(""); const [error, setError] = useState(""); const codeInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => { codeInputRef.current?.focus(); }, []);
  useGSAP(() => { if (reduced) return; const reveals = gsap.utils.toArray<HTMLElement>("[data-entry-reveal]"); gsap.fromTo(reveals, { opacity: 0, y: 18 }, { opacity: 1, y: 0, duration: .72, stagger: .08, ease: "power3.out", clearProps: "transform" }); }, { scope: shellRef, dependencies: [reduced], revertOnUpdate: true });
  function openRoom(event: React.FormEvent<HTMLFormElement>) { event.preventDefault(); const normalized = code.trim(); if (!/^\d{6}$/.test(normalized)) { setError("Enter the six-digit room code."); return; } setError(""); navigate(`/room/${normalized}`); }
  return <div ref={shellRef} className="app-shell entry-shell" data-testid="entry-shell" data-motion={reduced ? "static" : "cinematic"}>
    <Topbar action={<RouteLink href="/admin/login" className="nav-link">Admin sign in <ArrowUpRight aria-hidden="true" weight="regular" /></RouteLink>} />
    <main className="entry-main"><section className="entry-copy" aria-labelledby="entry-heading"><p className="eyebrow" data-entry-reveal>SELF-HOSTED RIICHI ROOMS</p><h1 id="entry-heading" aria-label="Your table is live." data-entry-reveal>Your table<br /><span>is live.</span></h1><p className="entry-lede" data-entry-reveal>Open a room by code, or sign in to host the next match.</p><form className="lookup-form" onSubmit={openRoom} data-entry-reveal><label htmlFor="room-code">Room code</label><div className="field-row"><input ref={codeInputRef} id="room-code" name="room-code" value={code} onChange={(event) => { setCode(event.target.value.replace(/\D/g, "").slice(0, 6)); setError(""); }} inputMode="numeric" autoComplete="off" maxLength={6} placeholder="000000" aria-describedby={error ? "room-code-error" : "room-code-hint"} /><button type="submit" className="button button-primary">Open room <ArrowRight aria-hidden="true" weight="regular" /></button></div><p id="room-code-hint" className="field-hint">Use the code shared by your host.</p>{error && <p id="room-code-error" className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{error}</p>}</form><p className="entry-note" data-entry-reveal><span className="note-rule" aria-hidden="true" />Rooms stay local. Matches stay yours.</p></section><section className="entry-visual" data-entry-reveal aria-label="Mahjong tile vignette"><TileVignette /><div className="visual-caption"><span>ROOM SIGNAL</span><span>OPEN TABLE</span></div></section></main><footer className="site-footer"><span>DOUBLE RIICHI / ENTRY</span><span>NO ACCOUNT REQUIRED</span></footer>
  </div>;
}

function RoomRoute({ joinCode }: { joinCode: string }) {
  const [state, setState] = useState<{ status: "loading" | "ready" | "error"; room?: import("./api").RoomLookup; characters?: HumanCharacter[]; problem?: ProblemDetails }>({ status: "loading" });
  const [retry, setRetry] = useState(0);
  useEffect(() => { let active = true; setState({ status: "loading" }); void api.lookupRoom(joinCode).then(async (room) => { if (!active) return; if (!room.join_allowed) { setState({ status: "ready", room, characters: [] }); return; } try { const characters = (await api.listHumanCharacters()).sort((left, right) => left.name.localeCompare(right.name)); if (active) setState({ status: "ready", room, characters }); } catch (error) { if (active) setState({ status: "ready", room, problem: problemFrom(error) }); } }).catch((error) => { if (active) setState({ status: "error", problem: problemFrom(error) }); }); return () => { active = false; }; }, [joinCode, retry]);
  return <div className="app-shell route-shell"><Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Room code</RouteLink>} />{state.status === "loading" && <LoadingRoom />}{state.status === "error" && <ProblemState title="Room unavailable" problem={state.problem} actionLabel="Try another room" />}{state.status === "ready" && state.room && <RoomJoin room={state.room} joinCode={joinCode} characters={state.characters ?? []} characterProblem={state.problem} onRetryCharacters={() => setRetry((value) => value + 1)} />}</div>;
}
function LoadingRoom() { return <main className="route-main loading-state" aria-busy="true" aria-live="polite"><div className="loading-copy"><span className="eyebrow">ROOM LOOKUP</span><h1>Loading room</h1><p>Checking the host for room details.</p></div><div className="loading-lines" aria-hidden="true"><span /><span /><span /></div></main>; }
function ProblemState({ title, problem, actionLabel }: { title: string; problem?: ProblemDetails; actionLabel: string }) { return <main className="route-main status-main"><div className="status-icon" aria-hidden="true"><WarningCircle weight="regular" /></div><p className="eyebrow">REQUEST STATUS</p><h1>{title}</h1><p className="status-detail" role="alert">{problem?.detail ?? problem?.title ?? "The host could not complete the request."}</p>{problem?.request_id && <p className="request-id">Request ID <code>{problem.request_id}</code></p>}<RouteLink href="/" className="button button-secondary">{actionLabel} <ArrowLeft aria-hidden="true" weight="regular" /></RouteLink></main>; }

function RoomJoin({ room, joinCode, characters, characterProblem, onRetryCharacters }: { room: import("./api").RoomLookup; joinCode: string; characters: HumanCharacter[]; characterProblem?: ProblemDetails; onRetryCharacters: () => void }) {
  const [nickname, setNickname] = useState(""); const [selectedId, setSelectedId] = useState(""); const [voiceStatus, setVoiceStatus] = useState("Preview riichi voice"); const [joinState, setJoinState] = useState<{ status: "idle" | "joining" | "error" | "success"; problem?: ProblemDetails }>({ status: "idle" }); const selected = characters.find((character) => character.id === selectedId); const canJoin = room.join_allowed && nickname.trim().length > 0 && Boolean(selected) && joinState.status !== "joining";
  async function join(event: React.FormEvent<HTMLFormElement>) { event.preventDefault(); if (!canJoin || !selected) return; setJoinState({ status: "joining" }); try { const result = await api.joinHuman(joinCode, nickname.trim(), selected.id); sessionStorage.setItem(`driichi:participant:${joinCode}`, result.participant_id); setJoinState({ status: "success" }); } catch (error) { setJoinState({ status: "error", problem: problemFrom(error) }); } }
  function previewVoice() { if (!selected) return; const AudioConstructor = window.Audio; if (!AudioConstructor) { setVoiceStatus("Voice preview is unavailable"); return; } const audio = new AudioConstructor(`/assets/characters/${encodeURIComponent(selected.id)}/voices/riichi.ogg`); void audio.play().then(() => setVoiceStatus("Playing riichi voice")).catch(() => setVoiceStatus("Voice preview is unavailable")); }
  if (joinState.status === "success") return <JoinSuccess roomName={room.room_name} joinCode={joinCode} />;
  return <main className="route-main room-main"><section className="room-intro" aria-labelledby="room-heading"><div><p className="eyebrow">ROOM / {joinCode}</p><h1 id="room-heading">Join {room.room_name}</h1><p className="room-lede">Choose a display name and a character before you enter the room.</p></div><dl className="room-facts"><div><dt>Mode</dt><dd>{room.game_mode}</dd></div><div><dt>Phase</dt><dd>{room.phase}</dd></div><div><dt>Players</dt><dd>{room.participant_count} / {room.participant_limit} participants</dd></div></dl></section>{!room.join_allowed && <div className="inline-notice" role="status">This room is full. Ask the host for another code.</div>}{room.join_allowed && <form className="join-form" onSubmit={join}><div className="form-block"><label htmlFor="display-name">Display name</label><input id="display-name" value={nickname} onChange={(event) => setNickname(event.target.value)} maxLength={64} autoComplete="nickname" autoFocus /><p className="field-hint">This name is shown to the room.</p></div><fieldset className="character-fieldset"><legend>Character</legend>{characterProblem && <><p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{characterProblem.detail ?? "Characters could not be loaded."}</p><button type="button" className="button button-secondary retry-button" onClick={onRetryCharacters}>Retry character list <ArrowRight aria-hidden="true" weight="regular" /></button></>}{characters.length === 0 && !characterProblem && <p className="field-hint">Characters are not available yet.</p>}<div className="character-list" role="radiogroup" aria-label="Choose a character">{characters.map((character) => <label key={character.id} className={`character-option${selectedId === character.id ? " is-selected" : ""}`}><input className="character-radio" type="radio" name="character" value={character.id} checked={selectedId === character.id} onChange={() => setSelectedId(character.id)} /><CharacterImage character={character} kind="icon" /><span>{character.name}</span>{selectedId === character.id && <Check aria-hidden="true" weight="regular" />}</label>)}</div></fieldset>{selected && <div className="selected-character"><CharacterImage character={selected} kind="portrait" /><div><span className="eyebrow">SELECTED CHARACTER</span><strong>{selected.name}</strong><button type="button" className="voice-button" onClick={previewVoice}><SpeakerHigh aria-hidden="true" weight="regular" />{voiceStatus}</button></div></div>}{joinState.status === "error" && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{joinState.problem?.detail ?? "The host could not join you to this room."}</p>}<button type="submit" className="button button-primary join-button" disabled={!canJoin}>{joinState.status === "joining" ? "Joining room" : "Join room"} <ArrowRight aria-hidden="true" weight="regular" /></button></form>}</main>;
}
function JoinSuccess({ roomName, joinCode }: { roomName: string; joinCode: string }) { return <main className="route-main status-main success-main"><div className="status-icon success-icon" aria-hidden="true"><Check weight="regular" /></div><p className="eyebrow">JOIN ACCEPTED</p><h1>You're in {roomName}.</h1><p className="status-detail">Your Guest Session is ready. Enter the Lobby to follow the host.</p><RouteLink href={`/room/${joinCode}/lobby`} className="button button-primary">Enter Lobby <ArrowRight aria-hidden="true" weight="regular" /></RouteLink></main>; }

function CharacterImage({ character, kind }: { character: { id: string; name: string }; kind: "icon" | "portrait" }) { const [failed, setFailed] = useState(false); if (failed) return <span className={`character-asset character-asset-${kind} asset-fallback`} aria-label={`${character.name} portrait unavailable`}><WarningCircle aria-hidden="true" weight="regular" /></span>; return <img className={`character-asset character-asset-${kind}`} src={`/assets/characters/${encodeURIComponent(character.id)}/${kind === "icon" ? "icon" : "portrait"}.webp`} alt={kind === "portrait" ? `${character.name} portrait` : ""} onError={() => setFailed(true)} />; }

function AdminLogin() {
  const [username, setUsername] = useState(""); const [password, setPassword] = useState(""); const [state, setState] = useState<{ status: "idle" | "loading" | "error" | "success"; problem?: ProblemDetails }>({ status: "idle" });
  async function submit(event: React.FormEvent<HTMLFormElement>) { event.preventDefault(); setState({ status: "loading" }); try { await api.loginAdmin(username, password); setPassword(""); setState({ status: "success" }); } catch (error) { setState({ status: "error", problem: problemFrom(error) }); } }
  if (state.status === "success") return <div className="app-shell route-shell"><Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Entry</RouteLink>} /><main className="route-main status-main success-main"><div className="status-icon success-icon" aria-hidden="true"><Check weight="regular" /></div><p className="eyebrow">ADMIN ACCESS</p><h1>Admin session active.</h1><p className="status-detail">The host session is ready. Room controls continue in the Admin surface.</p><RouteLink href="/admin" className="button button-primary">Open Admin <ArrowRight aria-hidden="true" weight="regular" /></RouteLink></main></div>;
  return <div className="app-shell route-shell"><Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Entry</RouteLink>} /><main className="route-main admin-main"><section className="admin-copy"><span className="admin-lock" aria-hidden="true"><LockKey weight="regular" /></span><p className="eyebrow">HOST ACCESS</p><h1>Sign in to host.</h1><p>Manage rooms from the private Admin surface.</p></section><form className="admin-form" onSubmit={submit}><div className="form-block"><label htmlFor="admin-username">Username</label><input id="admin-username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" /></div><div className="form-block"><label htmlFor="admin-password">Password</label><input id="admin-password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" autoFocus /></div>{state.status === "error" && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{state.problem?.detail ?? "The host could not sign you in."}</p>}<button type="submit" className="button button-primary" disabled={state.status === "loading"}>{state.status === "loading" ? "Signing in" : "Sign in"} <ArrowRight aria-hidden="true" weight="regular" /></button><p className="security-note"><LockKey aria-hidden="true" weight="regular" />Session cookies stay in the browser.</p></form></main></div>;
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
  return <div className="app-shell workspace-shell"><Topbar action={<div className="workspace-actions"><RouteLink href="/" className="nav-link">Entry <ArrowUpRight aria-hidden="true" weight="regular" /></RouteLink><button className="text-button" onClick={() => { void api.logoutAdmin().finally(() => navigate("/admin/login")); }}><SignOut aria-hidden="true" weight="regular" />Sign out</button></div>} /><main className="admin-workspace"><aside className="workspace-rail"><div className="workspace-rail-head"><div><p className="eyebrow">CONTROL ROOM</p><h1>Admin rooms</h1></div><button className="icon-button" aria-label="Create Room" onClick={() => setCreateOpen(true)}><Plus aria-hidden="true" weight="regular" /></button></div>{rooms.isLoading && <p className="state-label" aria-busy="true">Loading Rooms</p>}{rooms.isError && <ProblemInline error={rooms.error} />}{rooms.data?.length === 0 && <div className="empty-state"><p>No Rooms yet.</p><button className="button button-secondary" onClick={() => setCreateOpen(true)}>Create a Room <Plus aria-hidden="true" weight="regular" /></button></div>}<nav className="room-list" aria-label="Admin Rooms">{rooms.data?.map((room) => <RouteLink key={room.join_code} href={`/admin/rooms/${room.join_code}`} className={`room-list-item${selectedCode === room.join_code ? " is-active" : ""}`} onClick={() => setSelectedCode(room.join_code)}><span><strong>{room.room_name}</strong><small>{room.join_code} / {room.game_mode}</small></span><span className="state-label">{room.phase}</span></RouteLink>)}</nav></aside><section className="workspace-main">{!selectedCode && !rooms.isLoading && <EmptyDetail onCreate={() => setCreateOpen(true)} />}{selectedCode && detail.isLoading && <LoadingPanel label="Loading Room detail" />}{selectedCode && detail.isError && <ProblemPanel title="Room detail unavailable" error={detail.error} />}{detail.data && <RoomDetailPanel room={detail.data} action={action} onDeleted={() => { void queryClient.invalidateQueries({ queryKey: queryKeys.rooms }); setSelectedCode(""); navigate("/admin"); }} />}</section></main><TokenPanel tokens={tokens.data ?? []} loading={tokens.isLoading} error={tokens.error} action={action} queryClient={queryClient} /><CreateRoomDialog open={createOpen} onClose={() => setCreateOpen(false)} mutation={create} /></div>;
}

function EmptyDetail({ onCreate }: { onCreate: () => void }) { return <section className="empty-detail"><p className="eyebrow">ROOM DETAIL</p><h2>Choose a Room.</h2><p>Open a Room from the rail, or create the next one.</p><button className="button button-primary" onClick={onCreate}>Create Room <Plus aria-hidden="true" weight="regular" /></button></section>; }
function LoadingPanel({ label }: { label: string }) { return <div className="panel-state" aria-busy="true"><span className="state-label">{label}</span><div className="loading-lines" aria-hidden="true"><span /><span /><span /></div></div>; }
function ProblemInline({ error }: { error: unknown }) { const problem = problemFrom(error); return <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{problem.detail ?? problem.title ?? "The host could not complete the request."}</p>; }
function ProblemPanel({ title, error }: { title: string; error: unknown }) { const problem = problemFrom(error); return <section className="empty-detail"><p className="eyebrow">REQUEST STATUS</p><h2>{title}</h2><ProblemInline error={problem} />{problem.request_id && <p className="request-id">Request ID <code>{problem.request_id}</code></p>}<button className="button button-secondary" onClick={() => window.location.reload()}>Retry <ArrowRight aria-hidden="true" weight="regular" /></button></section>; }

type ActionMutation = { isPending: boolean; isError: boolean; error: unknown; mutate: (task: () => Promise<unknown>) => void };
type CreateMutation = { isPending: boolean; isError: boolean; error: unknown; mutate: (input: { room_name: string; game_mode: GameMode; time_control: TimeControl; replay_save: boolean; participant_limit: number }) => void };
function RoomDetailPanel({ room, action, onDeleted }: { room: AdminRoomDetail; action: ActionMutation; onDeleted: () => void }) {
  const [settings, setSettings] = useState({ room_name: room.room_name, game_mode: room.game_mode, time_control: room.time_control, replay_save: room.replay_save, participant_limit: room.participant_limit }); const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => setSettings({ room_name: room.room_name, game_mode: room.game_mode, time_control: room.time_control, replay_save: room.replay_save, participant_limit: room.participant_limit }), [room.room_name, room.game_mode, room.time_control, room.replay_save, room.participant_limit]);
  const lobby = room.phase === "lobby"; const seats = room.game_mode.startsWith("3p") ? 3 : 4;
  const run = (task: () => Promise<unknown>) => action.mutate(task);
  return <section className="room-detail" aria-labelledby="room-detail-heading"><header className="detail-heading"><div><p className="eyebrow">ROOM / {room.join_code}</p><h2 id="room-detail-heading">{room.room_name}</h2><p className="detail-lede">Authoritative revision {room.revision}. Changes are polled while this page is visible.</p></div><div className="detail-phase"><span className="state-label">PHASE</span><strong>{titleCasePhase(room.phase)}</strong></div></header><div className="detail-layout"><div className="detail-column"><section className="flat-section"><div className="section-heading"><div><p className="eyebrow">CONFIGURATION</p><h3>Room settings</h3></div><span className="state-label">{lobby ? "EDITABLE" : "LOCKED"}</span></div><div className="settings-grid"><div className="form-block"><label htmlFor="room-name">Room name</label><input id="room-name" value={settings.room_name} disabled={!lobby} onChange={(event) => setSettings({ ...settings, room_name: event.target.value })} /></div><div className="form-block"><label htmlFor="game-mode">Game mode</label><select id="game-mode" value={settings.game_mode} disabled={!lobby} onChange={(event) => setSettings({ ...settings, game_mode: event.target.value as GameMode })}><option value="4p-red-east">4p red East</option><option value="4p-red-half">4p red half</option><option value="3p-red-east">3p red East</option><option value="3p-red-half">3p red half</option></select></div><div className="form-block"><label htmlFor="time-control">Time control</label><select id="time-control" value={settings.time_control} disabled={!lobby} onChange={(event) => setSettings({ ...settings, time_control: event.target.value as TimeControl })}><option value="casual">Casual 30 / 10</option><option value="riichi_dev">Riichi.dev</option><option value="unlimited">Unlimited</option></select></div><div className="form-block"><label htmlFor="participant-limit">Participant limit</label><input id="participant-limit" type="number" min={seats} max={32} value={settings.participant_limit} disabled={!lobby} onChange={(event) => setSettings({ ...settings, participant_limit: Number(event.target.value) })} /></div></div><label className="check-row"><input type="checkbox" checked={settings.replay_save} disabled={!lobby} onChange={(event) => setSettings({ ...settings, replay_save: event.target.checked })} />Save Replay</label><button className="button button-secondary" disabled={!lobby || action.isPending} onClick={() => run(() => api.patchAdminRoom(room.join_code, settings))}>{action.isPending ? "Saving settings" : "Save settings"}</button>{action.isError && <ProblemInline error={action.error} />}</section><section className="flat-section"><div className="section-heading"><div><p className="eyebrow">ROSTER / {room.selected_count} OF {seats}</p><h3>Participants</h3></div><button className="button button-secondary small-button" disabled={!lobby || action.isPending || room.selected_count >= seats} onClick={() => run(() => api.fillWithBots(room.join_code))}>Fill with Bots</button></div>{room.participants.length === 0 ? <p className="field-hint">No Participants have joined this Room.</p> : <div className="participant-list">{room.participants.map((participant) => <ParticipantRow key={participant.participant_id} participant={participant} room={room} disabled={!lobby || action.isPending} onAction={run} />)}</div>}</section><section className="flat-section lifecycle-section"><div className="section-heading"><div><p className="eyebrow">LIFECYCLE</p><h3>Match control</h3></div></div><div className="action-row"><button className="button button-primary" disabled={!lobby || action.isPending || room.selected_count !== seats} onClick={() => run(() => api.startRoom(room.join_code))}>Start Match <ArrowRight aria-hidden="true" weight="regular" /></button><button className="button button-secondary" disabled={room.phase !== "post_match" || action.isPending} onClick={() => run(() => api.rematchRoom(room.join_code))}>Rematch</button><button className="button button-secondary" disabled={room.phase !== "post_match" || action.isPending} onClick={() => run(() => api.backToLobby(room.join_code))}>Back to Lobby</button><button className="button button-secondary danger-button" disabled={room.phase === "playing" || action.isPending} onClick={() => setConfirmDelete(true)}>Delete Room</button></div></section></div><RosterRail room={room} seats={seats} /></div><ConfirmDialog open={confirmDelete} title="Delete Room?" description={`Delete ${room.room_name}? Connected Participants will lose this Room Session.`} confirmLabel="Confirm delete" onClose={() => setConfirmDelete(false)} onConfirm={() => { setConfirmDelete(false); run(async () => { await api.deleteAdminRoom(room.join_code); onDeleted(); }); }} /></section>;
}

function ParticipantRow({ participant, room, disabled, onAction }: { participant: RoomParticipant; room: AdminRoomDetail; disabled: boolean; onAction: (task: () => Promise<unknown>) => void }) { const [confirmKick, setConfirmKick] = useState(false); const selectedCharacter = { id: participant.character_id, name: participant.character_id }; return <article className="participant-row"><CharacterImage character={selectedCharacter} kind="icon" /><div className="participant-main"><strong>{participant.display_name}</strong><span className="state-label">{participant.kind}</span></div><dl className="participant-axes"><div><dt>Identity</dt><dd>{participant.participant_id.slice(0, 8)}</dd></div><div><dt>Presence</dt><dd>{participant.presence}</dd></div><div><dt>Selection</dt><dd>{participant.selected ? "selected" : "unselected"}</dd></div><div><dt>Ready</dt><dd>{participant.ready ? "ready" : "not ready"}</dd></div><div><dt>Controller</dt><dd>{participant.controller.replaceAll("_", " ")}</dd></div></dl><div className="participant-actions"><button className="text-button" disabled={disabled || participant.selected} onClick={() => onAction(() => api.selectParticipant(room.join_code, participant.participant_id))}>Select</button><button className="text-button" disabled={disabled || !participant.selected} onClick={() => onAction(() => api.deselectParticipant(room.join_code, participant.participant_id))}>Deselect</button><button className="text-button danger-text" disabled={disabled} onClick={() => setConfirmKick(true)}>Kick</button></div><ConfirmDialog open={confirmKick} title="Kick Participant?" description={`Remove ${participant.display_name} from this Room?`} confirmLabel="Confirm kick" onClose={() => setConfirmKick(false)} onConfirm={() => { setConfirmKick(false); onAction(() => api.kickParticipant(room.join_code, participant.participant_id)); }} /></article>; }
function RosterRail({ room, seats }: { room: AdminRoomDetail; seats: number }) { const roster = room.roster.length ? room.roster : room.match_players; return <aside className="roster-rail" aria-label="Selected roster"><div className="section-heading"><div><p className="eyebrow">SEAT RAIL</p><h3>{seats} seats</h3></div><span className="state-label">LIVE</span></div><div className="seat-list">{Array.from({ length: seats }, (_, seat) => { const player = roster.find((entry) => entry.seat === seat); return <div className={`seat-row${player ? " is-filled" : ""}`} key={seat}><span className="seat-number">{String(seat + 1).padStart(2, "0")}</span>{player ? <><CharacterImage character={{ id: player.character_id ?? "", name: player.display_name }} kind="portrait" /><span><strong>{player.display_name}</strong><small>{player.controller.replaceAll("_", " ")}</small></span></> : <span className="empty-seat">Seat open</span>}</div>; })}</div></aside>; }

function TokenPanel({ tokens, loading, error, action, queryClient }: { tokens: BotTokenRecord[]; loading: boolean; error: unknown; action: ActionMutation; queryClient: ReturnType<typeof useQueryClient> }) { const [name, setName] = useState(""); const [secret, setSecret] = useState<CreatedBotToken | null>(null); const [revoke, setRevoke] = useState<BotTokenRecord | null>(null); const create = () => { action.mutate(() => api.createBotToken(name.trim()).then((created) => { setName(""); setSecret(created); void queryClient.invalidateQueries({ queryKey: queryKeys.tokens }); })); }; return <section className="token-panel"><div className="token-panel-heading"><div><p className="eyebrow">BOT TOKENS</p><h2>Credentials for agents</h2><p>Raw secrets appear once. They are never listed again.</p></div><Gear aria-hidden="true" weight="regular" /></div><div className="token-create"><label htmlFor="token-name">Token name</label><div className="field-row"><input id="token-name" value={name} maxLength={64} onChange={(event) => setName(event.target.value)} placeholder="e.g. night-market-runner" /><button className="button button-primary" disabled={!name.trim() || action.isPending} onClick={create}>{action.isPending ? "Creating token" : "Create token"} <Plus aria-hidden="true" weight="regular" /></button></div></div>{loading && <p className="state-label" aria-busy="true">Loading Tokens</p>}{error != null && <ProblemInline error={error} />}{!loading && !error && tokens.length === 0 && <p className="field-hint">No Bot Tokens have been created.</p>}{tokens.length > 0 && <div className="token-list">{tokens.map((token) => <article className="token-row" key={token.token_id}><div><strong>{token.name}</strong><span className="state-label">{token.state}</span></div><code>{token.token_id}</code><time dateTime={token.created_at}>{token.created_at}</time><button className="text-button danger-text" disabled={token.state !== "active" || action.isPending} aria-label={`Revoke ${token.name}`} onClick={() => setRevoke(token)}>Revoke</button></article>)}</div>}{action.isError && <ProblemInline error={action.error} />}<OneTimeTokenDialog token={secret} onClose={() => setSecret(null)} /><ConfirmDialog open={Boolean(revoke)} title="Revoke Bot Token?" description={`Revoke ${revoke?.name ?? "this Token"}? Every connection using it will lose access.`} confirmLabel="Confirm revoke" onClose={() => setRevoke(null)} onConfirm={() => { if (!revoke) return; const token = revoke; setRevoke(null); action.mutate(() => api.revokeBotToken(token.token_id).then((result) => { void queryClient.invalidateQueries({ queryKey: queryKeys.tokens }); return result; })); }} /></section>; }
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
function CreateRoomDialog({ open, onClose, mutation }: { open: boolean; onClose: () => void; mutation: CreateMutation }) { const [name, setName] = useState(""); const [mode, setMode] = useState<GameMode>("4p-red-east"); const [timeControl, setTimeControl] = useState<TimeControl>("casual"); const seats = mode.startsWith("3p") ? 3 : 4; return <ModalDialog open={open} onClose={onClose} className="confirm-dialog create-dialog" labelledBy="create-heading"><div className="dialog-top"><p className="eyebrow">NEW ROOM</p><button className="icon-button" aria-label="Close dialog" onClick={onClose}><X aria-hidden="true" weight="regular" /></button></div><h2 id="create-heading">Create a Room.</h2><div className="form-block"><label htmlFor="new-room-name">Room name</label><input id="new-room-name" value={name} onChange={(event) => setName(event.target.value)} data-dialog-autofocus /></div><div className="form-block"><label htmlFor="new-room-mode">Game mode</label><select id="new-room-mode" value={mode} onChange={(event) => setMode(event.target.value as GameMode)}><option value="4p-red-east">4p red East</option><option value="4p-red-half">4p red half</option><option value="3p-red-east">3p red East</option><option value="3p-red-half">3p red half</option></select></div><div className="form-block"><label htmlFor="new-room-time">Time control</label><select id="new-room-time" value={timeControl} onChange={(event) => setTimeControl(event.target.value as TimeControl)}><option value="casual">Casual 30 / 10</option><option value="riichi_dev">Riichi.dev</option><option value="unlimited">Unlimited</option></select></div>{mutation.isError && <ProblemInline error={mutation.error} />}<div className="dialog-actions"><button className="button button-secondary" onClick={onClose}>Cancel</button><button className="button button-primary" disabled={!name.trim() || mutation.isPending} onClick={() => mutation.mutate({ room_name: name.trim(), game_mode: mode, time_control: timeControl, replay_save: true, participant_limit: seats })}>{mutation.isPending ? "Creating Room" : "Create Room"}</button></div></ModalDialog>; }

type SocketRoom = RoomSnapshot;
type HumanSocketMessage = { type: string; room?: SocketRoom; state?: unknown; event?: unknown; code?: string; status?: string; decision_id?: string };
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
    if (socketRef.current?.readyState === WebSocket.OPEN) socketRef.current.send(JSON.stringify(value));
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
        const semantic: Record<number, string> = { 4001: "connected_elsewhere", 4002: "room_deleted", 4005: "slow_consumer", 4006: "session_expired" };
        const semanticReason = semantic[event.code] ?? event.reason;
        if (semanticReason && [4001, 4002, 4006].includes(event.code)) {
          useGameStore.getState().setStatus("closed", semanticReason);
          return;
        }
        attemptRef.current += 1;
        socketRef.current = null;
        useGameStore.getState().resetForReconnect();
        const delay = Math.min(10000, 500 * 2 ** Math.min(attemptRef.current - 1, 4));
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

function decodeCharacterAsset(id: string, kind: "portrait" | "icon"): Promise<void> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      const decoded = image.decode?.();
      if (decoded) decoded.then(() => resolve()).catch(reject);
      else resolve();
    };
    image.onerror = () => reject(new Error("asset_unavailable"));
    image.src = `/assets/characters/${encodeURIComponent(id)}/${kind}.webp`;
  });
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
  const selectedIds = sessionReady ? selected.map((participant) => participant.character_id).filter(Boolean).sort().join(",") : "";
  const seatRoster = room ? (room.roster.length ? room.roster : room.match_players) : [];
  const seatByParticipant = new Map(seatRoster.map((player) => [player.participant_id, player.seat]));
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
  }, [selectedIds]);
  const canReady = status === "connected" && room?.phase === "lobby" && Boolean(own?.selected)
    && selected.length === seatCount && preload === "complete" && !own?.ready;
  const closeMessage = reason === "connected_elsewhere"
    ? "This Participant connected in another tab."
    : reason === "room_deleted"
      ? "The host deleted this Room."
      : reason === "session_expired"
        ? "This Guest Session has expired."
        : reason === "slow_consumer"
          ? "The connection was closed because it could not keep up."
          : reason
            ? `The live Room connection reported ${reason}.`
            : "";

  if (room && (room.phase === "playing" || room.phase === "post_match")) {
    return <GameplaySurface room={room} projection={projection} status={status} reason={reason} commandError={commandError} connectionGeneration={connectionGeneration} send={send} reducedMotion={reducedMotion} />;
  }

  return <div className="app-shell lobby-shell"><Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Entry</RouteLink>} /><main className="lobby-workspace"><header className="lobby-heading"><div><p className="eyebrow">ROOM / {joinCode}</p><h1>{room ? `${room.room_name} Lobby` : "Room Lobby"}</h1><p className="detail-lede">Authoritative Room state, delivered live from the host.</p></div><div className={`connection-state connection-${status}`}><span className="state-dot" aria-hidden="true" />{status}</div></header>{closeMessage && <section className="inline-notice lobby-notice" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{closeMessage}<RouteLink href={`/room/${joinCode}`} className="button button-secondary small-button">Return to join</RouteLink></section>}{commandError && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />Room command rejected: {commandError}</p>}{status === "error" && !closeMessage && <ProblemInline error={{ detail: "The live Room connection failed. Reconnecting shortly.", code: reason }} />}{!room && <LoadingPanel label={status === "reconnecting" ? "Reconnecting to Room" : "Connecting to Room"} />}{room && <div className="lobby-grid"><section className="flat-section lobby-roster"><div className="section-heading"><div><p className="eyebrow">ROSTER / {selected.length} OF {seatCount}</p><h2>Participant rail</h2></div><span className="state-label">REV {room.revision}</span></div>{participants.length === 0 ? <p className="field-hint">No Participants are connected yet.</p> : <div className="lobby-participants">{participants.map((participant) => <LobbyParticipant key={participant.participant_id} participant={participant} own={participant.participant_id === own?.participant_id} seat={seatByParticipant.get(participant.participant_id)} />)}</div>}<div className="ready-block"><div><p className="eyebrow">YOUR READY STATE</p><strong>{own?.ready ? "Ready for the next Match" : "Not ready"}</strong><p className="field-hint">{preload === "loading" ? "Preloading every selected Character." : preload === "error" ? "A selected Character asset could not be preloaded." : selected.length === seatCount ? "The complete selected roster is available." : `The host needs ${seatCount} selected Players.`}</p></div><button className="button button-primary" disabled={!canReady} onClick={() => send({ type: "set_ready", preloaded_characters: selected.map((participant) => participant.character_id) })}>{own?.ready ? "Ready set" : preload === "loading" ? "Preloading roster" : "Set Ready"} <Check aria-hidden="true" weight="regular" /></button></div></section><section className="flat-section character-panel"><p className="eyebrow">CHARACTER</p><h2>Your selection</h2>{own ? <><CharacterImage character={{ id: own.character_id, name: own.display_name }} kind="portrait" /><strong>{own.character_id}</strong><label htmlFor="lobby-character">Character selection</label><select id="lobby-character" value={own.character_id} disabled><option value={own.character_id}>{own.character_id}</option></select><p className="field-hint">Character selection is set before joining and remains cosmetic.</p></> : <p className="field-hint">Waiting for your Participant identity.</p>}<LifecycleStatus phase={room.phase} /></section></div>}</main></div>;
}
function LobbyParticipant({ participant, own, seat }: { participant: RoomParticipant; own: boolean; seat?: number }) { return <article className={`lobby-participant${own ? " is-own" : ""}`}><CharacterImage character={{ id: participant.character_id, name: participant.display_name }} kind="icon" /><div><strong>{participant.display_name}{own ? " / you" : ""}</strong><span className="state-label">{participant.kind}</span></div><dl className="lobby-axes"><div><dt>Identity</dt><dd>{participant.participant_id.slice(0, 8)}</dd></div><div><dt>Presence</dt><dd>{participant.presence}</dd></div><div><dt>Selection</dt><dd>{participant.selected ? "selected" : "unselected"}</dd></div><div><dt>Seat</dt><dd>{seat === undefined ? "unassigned" : `Seat ${seat + 1}`}</dd></div><div><dt>Controller</dt><dd>{participant.controller.replaceAll("_", " ")}</dd></div></dl><span className={`ready-mark${participant.ready ? " is-ready" : ""}`}>{participant.ready ? "ready" : "not ready"}</span></article>; }
function LifecycleStatus({ phase }: { phase: string }) { return <div className="lifecycle-status"><p className="eyebrow">ROOM LIFECYCLE</p><ol><li className={phase === "lobby" ? "is-current" : "is-complete"}>Lobby</li><li className={phase === "playing" ? "is-current" : phase === "post_match" ? "is-complete" : ""}>Playing</li><li className={phase === "post_match" ? "is-current" : ""}>Post-Match</li></ol>{phase === "playing" && <p className="field-hint">The Pixi table arrives in the next surface. Your connection remains authoritative.</p>}</div>; }

function titleCasePhase(phase: string) { return phase === "post_match" ? "Post-Match" : phase.charAt(0).toUpperCase() + phase.slice(1); }
function problemFrom(error: unknown): ProblemDetails { if (error instanceof ApiProblem) return error.problem; return { detail: "The host could not complete the request.", code: "request_failed" }; }
function NotFound() { return <div className="app-shell route-shell"><Topbar action={<RouteLink href="/" className="nav-link">Entry <ArrowUpRight aria-hidden="true" weight="regular" /></RouteLink>} /><ProblemState title="Page not found" problem={{ detail: "That path is not part of the entry surface." }} actionLabel="Return to entry" /></div>; }
