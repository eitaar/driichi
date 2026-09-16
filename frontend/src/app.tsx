import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  Check,
  LockKey,
  SpeakerHigh,
  UserCircle,
  WarningCircle,
} from "@phosphor-icons/react";
import { gsap } from "gsap";
import { useGSAP } from "@gsap/react";
import { api, ApiProblem, type HumanCharacter, type ProblemDetails, type RoomLookup } from "./api";
import { TileVignette } from "./pixi-vignette";
import { navigate, routeForPath, type Route } from "./routes";
import "./styles.css";

type MotionQuery = MediaQueryList & { addListener?: (listener: () => void) => void; removeListener?: (listener: () => void) => void };

function useReducedMotion() {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)") as MotionQuery;
    const update = () => setReduced(query.matches);
    update();
    query.addEventListener?.("change", update);
    query.addListener?.(update);
    return () => {
      query.removeEventListener?.("change", update);
      query.removeListener?.(update);
    };
  }, []);
  return reduced;
}

export function App() {
  const [route, setRoute] = useState<Route>(() => routeForPath(window.location.pathname));
  useEffect(() => {
    const update = () => setRoute(routeForPath(window.location.pathname));
    window.addEventListener("popstate", update);
    return () => window.removeEventListener("popstate", update);
  }, []);

  if (route.kind === "entry") return <EntryShell />;
  if (route.kind === "room") return <RoomRoute joinCode={route.joinCode} />;
  if (route.kind === "admin") return <AdminLogin />;
  return <NotFound />;
}

function RouteLink({ href, children, className = "", onClick }: { href: string; children: React.ReactNode; className?: string; onClick?: () => void }) {
  return (
    <a
      className={className}
      href={href}
      onClick={(event) => {
        if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
        event.preventDefault();
        onClick?.();
        navigate(href);
      }}
    >
      {children}
    </a>
  );
}

function Brand() {
  return (
    <RouteLink href="/" className="brand" aria-label="Double Riichi home">
      <span className="brand-mark" aria-hidden="true">二</span>
      <span>DOUBLE RIICHI</span>
    </RouteLink>
  );
}

function Topbar({ action }: { action?: React.ReactNode }) {
  return (
    <header className="topbar">
      <Brand />
      <nav aria-label="Primary navigation">
        <span className="nav-context">SELF-HOSTED ROOM SERVICE</span>
        {action}
      </nav>
    </header>
  );
}

function EntryShell() {
  const shellRef = useRef<HTMLDivElement>(null);
  const reduced = useReducedMotion();
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const codeInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    codeInputRef.current?.focus();
  }, []);

  useGSAP(() => {
    if (reduced) return;
    const reveals = gsap.utils.toArray<HTMLElement>("[data-entry-reveal]");
    gsap.fromTo(reveals, { opacity: 0, y: 18 }, {
      opacity: 1,
      y: 0,
      duration: 0.72,
      stagger: 0.08,
      ease: "power3.out",
      clearProps: "transform",
    });
  }, { scope: shellRef, dependencies: [reduced], revertOnUpdate: true });

  function openRoom(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const normalized = code.trim();
    if (!/^\d{6}$/.test(normalized)) {
      setError("Enter the six-digit room code.");
      return;
    }
    setError("");
    navigate(`/room/${normalized}`);
  }

  return (
    <div ref={shellRef} className="app-shell entry-shell" data-testid="entry-shell" data-motion={reduced ? "static" : "cinematic"}>
      <Topbar action={<RouteLink href="/admin/login" className="nav-link">Admin sign in <ArrowUpRight aria-hidden="true" weight="regular" /></RouteLink>} />
      <main className="entry-main">
        <section className="entry-copy" aria-labelledby="entry-heading">
          <p className="eyebrow" data-entry-reveal>SELF-HOSTED RIICHI ROOMS</p>
          <h1 id="entry-heading" aria-label="Your table is live." data-entry-reveal>Your table<br /><span>is live.</span></h1>
          <p className="entry-lede" data-entry-reveal>Open a room by code, or sign in to host the next match.</p>
          <form className="lookup-form" onSubmit={openRoom} data-entry-reveal>
            <label htmlFor="room-code">Room code</label>
            <div className="field-row">
              <input
                ref={codeInputRef}
                id="room-code"
                name="room-code"
                value={code}
                onChange={(event) => {
                  setCode(event.target.value.replace(/\D/g, "").slice(0, 6));
                  setError("");
                }}
                inputMode="numeric"
                autoComplete="off"
                maxLength={6}
                placeholder="000000"
                autoFocus
                aria-describedby={error ? "room-code-error" : "room-code-hint"}
              />
              <button type="submit" className="button button-primary">Open room <ArrowRight aria-hidden="true" weight="regular" /></button>
            </div>
            <p id="room-code-hint" className="field-hint">Use the code shared by your host.</p>
            {error && <p id="room-code-error" className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{error}</p>}
          </form>
          <p className="entry-note" data-entry-reveal><span className="note-rule" aria-hidden="true" />Rooms stay local. Matches stay yours.</p>
        </section>
        <section className="entry-visual" data-entry-reveal aria-label="Mahjong tile vignette">
          <TileVignette />
          <div className="visual-caption"><span>ROOM SIGNAL</span><span>01 / OPEN TABLE</span></div>
        </section>
      </main>
      <footer className="site-footer"><span>DOUBLE RIICHI / ENTRY</span><span>NO ACCOUNT REQUIRED</span></footer>
    </div>
  );
}

function RoomRoute({ joinCode }: { joinCode: string }) {
  const [state, setState] = useState<{ status: "loading" | "ready" | "error"; room?: RoomLookup; characters?: HumanCharacter[]; problem?: ProblemDetails }>({ status: "loading" });

  useEffect(() => {
    let active = true;
    setState({ status: "loading" });
    void api.lookupRoom(joinCode).then(async (room) => {
      if (!active) return;
      if (!room.join_allowed) {
        setState({ status: "ready", room, characters: [] });
        return;
      }
      try {
        const characters = (await api.listHumanCharacters()).sort((left, right) => left.name.localeCompare(right.name));
        if (active) setState({ status: "ready", room, characters });
      } catch (error) {
        if (active) setState({ status: "ready", room, problem: problemFrom(error) });
      }
    }).catch((error) => {
      if (active) setState({ status: "error", problem: problemFrom(error) });
    });
    return () => { active = false; };
  }, [joinCode]);

  return (
    <div className="app-shell route-shell">
      <Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Room code</RouteLink>} />
      {state.status === "loading" && <LoadingRoom />}
      {state.status === "error" && <ProblemState title="Room unavailable" problem={state.problem} actionLabel="Try another room" />}
      {state.status === "ready" && state.room && <RoomJoin room={state.room} joinCode={joinCode} characters={state.characters ?? []} characterProblem={state.problem} />}
    </div>
  );
}

function LoadingRoom() {
  return (
    <main className="route-main loading-state" aria-busy="true" aria-live="polite">
      <div className="loading-copy"><span className="eyebrow">ROOM LOOKUP</span><h1>Loading room</h1><p>Checking the host for room details.</p></div>
      <div className="loading-lines" aria-hidden="true"><span /><span /><span /></div>
    </main>
  );
}

function ProblemState({ title, problem, actionLabel }: { title: string; problem?: ProblemDetails; actionLabel: string }) {
  return (
    <main className="route-main status-main">
      <div className="status-icon" aria-hidden="true"><WarningCircle weight="regular" /></div>
      <p className="eyebrow">REQUEST STATUS</p>
      <h1>{title}</h1>
      <p className="status-detail" role="alert">{problem?.detail ?? problem?.title ?? "The host could not complete the request."}</p>
      {problem?.request_id && <p className="request-id">Request ID <code>{problem.request_id}</code></p>}
      <RouteLink href="/" className="button button-secondary">{actionLabel} <ArrowLeft aria-hidden="true" weight="regular" /></RouteLink>
    </main>
  );
}

function RoomJoin({ room, joinCode, characters, characterProblem }: { room: RoomLookup; joinCode: string; characters: HumanCharacter[]; characterProblem?: ProblemDetails }) {
  const [nickname, setNickname] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [voiceStatus, setVoiceStatus] = useState("Preview riichi voice");
  const [joinState, setJoinState] = useState<{ status: "idle" | "joining" | "error" | "success"; problem?: ProblemDetails }>({ status: "idle" });
  const selected = characters.find((character) => character.id === selectedId);
  const canJoin = room.join_allowed && nickname.trim().length > 0 && Boolean(selected) && joinState.status !== "joining";

  async function join(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canJoin || !selected) return;
    setJoinState({ status: "joining" });
    try {
      await api.joinHuman(joinCode, nickname.trim(), selected.id);
      setJoinState({ status: "success" });
    } catch (error) {
      setJoinState({ status: "error", problem: problemFrom(error) });
    }
  }

  function previewVoice() {
    if (!selected) return;
    const AudioConstructor = window.Audio;
    if (!AudioConstructor) {
      setVoiceStatus("Voice preview is unavailable");
      return;
    }
    const audio = new AudioConstructor(`/assets/characters/${encodeURIComponent(selected.id)}/voices/riichi.ogg`);
    void audio.play().then(() => setVoiceStatus("Playing riichi voice")).catch(() => setVoiceStatus("Voice preview is unavailable"));
  }

  if (joinState.status === "success") {
    return <JoinSuccess roomName={room.room_name} />;
  }

  return (
    <main className="route-main room-main">
      <section className="room-intro" aria-labelledby="room-heading">
        <div>
          <p className="eyebrow">ROOM / {joinCode}</p>
          <h1 id="room-heading">Join {room.room_name}</h1>
          <p className="room-lede">Choose a display name and a character before you enter the room.</p>
        </div>
        <dl className="room-facts">
          <div><dt>Mode</dt><dd>{room.game_mode}</dd></div>
          <div><dt>Phase</dt><dd>{room.phase}</dd></div>
          <div><dt>Players</dt><dd>{room.participant_count} / {room.participant_limit} participants</dd></div>
        </dl>
      </section>
      {!room.join_allowed && <div className="inline-notice" role="status">This room is full. Ask the host for another code.</div>}
      {room.join_allowed && (
        <form className="join-form" onSubmit={join}>
          <div className="form-block">
            <label htmlFor="display-name">Display name</label>
            <input id="display-name" value={nickname} onChange={(event) => setNickname(event.target.value)} maxLength={64} autoComplete="nickname" autoFocus />
            <p className="field-hint">This name is shown to the room.</p>
          </div>
          <fieldset className="character-fieldset">
            <legend>Character</legend>
            {characterProblem && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{characterProblem.detail ?? "Characters could not be loaded."}</p>}
            {characters.length === 0 && !characterProblem && <p className="field-hint">Characters are not available yet.</p>}
            <div className="character-list" role="listbox" aria-label="Choose a character">
              {characters.map((character) => (
                <button
                  type="button"
                  key={character.id}
                  className={`character-option${selectedId === character.id ? " is-selected" : ""}`}
                  aria-selected={selectedId === character.id}
                  onClick={() => setSelectedId(character.id)}
                >
                  <CharacterImage character={character} kind="icon" />
                  <span>{character.name}</span>
                  {selectedId === character.id && <Check aria-hidden="true" weight="regular" />}
                </button>
              ))}
            </div>
          </fieldset>
          {selected && <div className="selected-character">
            <CharacterImage character={selected} kind="portrait" />
            <div><span className="eyebrow">SELECTED CHARACTER</span><strong>{selected.name}</strong><button type="button" className="voice-button" onClick={previewVoice}><SpeakerHigh aria-hidden="true" weight="regular" />{voiceStatus}</button></div>
          </div>}
          {joinState.status === "error" && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{joinState.problem?.detail ?? "The host could not join you to this room."}</p>}
          <button type="submit" className="button button-primary join-button" disabled={!canJoin}>{joinState.status === "joining" ? "Joining room" : "Join room"} <ArrowRight aria-hidden="true" weight="regular" /></button>
        </form>
      )}
    </main>
  );
}

function CharacterImage({ character, kind }: { character: HumanCharacter; kind: "icon" | "portrait" }) {
  const [failed, setFailed] = useState(false);
  const initials = character.name.trim().slice(0, 1).toUpperCase() || "?";
  if (failed) return <span className={`character-asset character-asset-${kind} asset-fallback`} aria-label={`${character.name} placeholder`}><UserCircle aria-hidden="true" weight="regular" /></span>;
  return <img className={`character-asset character-asset-${kind}`} src={`/assets/characters/${encodeURIComponent(character.id)}/${kind === "icon" ? "icon" : "portrait"}.webp`} alt={kind === "portrait" ? `${character.name} portrait` : ""} onError={() => setFailed(true)} data-initial={initials} />;
}

function JoinSuccess({ roomName }: { roomName: string }) {
  return (
    <main className="route-main status-main success-main">
      <div className="status-icon success-icon" aria-hidden="true"><Check weight="regular" /></div>
      <p className="eyebrow">JOIN ACCEPTED</p>
      <h1>You're in {roomName}.</h1>
      <p className="status-detail">Keep this tab open while the host starts the match.</p>
      <RouteLink href="/" className="button button-secondary">Return to entry <ArrowLeft aria-hidden="true" weight="regular" /></RouteLink>
    </main>
  );
}

function AdminLogin() {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [state, setState] = useState<{ status: "idle" | "loading" | "error" | "success"; problem?: ProblemDetails }>({ status: "idle" });

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setState({ status: "loading" });
    try {
      await api.loginAdmin(username, password);
      setState({ status: "success" });
      setPassword("");
    } catch (error) {
      setState({ status: "error", problem: problemFrom(error) });
    }
  }

  if (state.status === "success") {
    return <div className="app-shell route-shell"><Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Entry</RouteLink>} /><main className="route-main status-main success-main"><div className="status-icon success-icon" aria-hidden="true"><Check weight="regular" /></div><p className="eyebrow">ADMIN ACCESS</p><h1>Admin session active.</h1><p className="status-detail">The host session is ready. Room controls continue in the Admin surface.</p><RouteLink href="/" className="button button-secondary">Return to entry <ArrowLeft aria-hidden="true" weight="regular" /></RouteLink></main></div>;
  }

  return (
    <div className="app-shell route-shell">
      <Topbar action={<RouteLink href="/" className="nav-link"><ArrowLeft aria-hidden="true" weight="regular" />Entry</RouteLink>} />
      <main className="route-main admin-main">
        <section className="admin-copy"><span className="admin-lock" aria-hidden="true"><LockKey weight="regular" /></span><p className="eyebrow">HOST ACCESS</p><h1>Sign in to host.</h1><p>Manage rooms from the private Admin surface.</p></section>
        <form className="admin-form" onSubmit={submit}>
          <div className="form-block"><label htmlFor="admin-username">Username</label><input id="admin-username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" /></div>
          <div className="form-block"><label htmlFor="admin-password">Password</label><input id="admin-password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" autoFocus /></div>
          {state.status === "error" && <p className="form-error" role="alert"><WarningCircle aria-hidden="true" weight="regular" />{state.problem?.detail ?? "The host could not sign you in."}</p>}
          <button type="submit" className="button button-primary" disabled={state.status === "loading"}>{state.status === "loading" ? "Signing in" : "Sign in"} <ArrowRight aria-hidden="true" weight="regular" /></button>
          <p className="security-note"><LockKey aria-hidden="true" weight="regular" />Session cookies stay in the browser.</p>
        </form>
      </main>
    </div>
  );
}

function NotFound() {
  return <div className="app-shell route-shell"><Topbar action={<RouteLink href="/" className="nav-link">Entry <ArrowUpRight aria-hidden="true" weight="regular" /></RouteLink>} /><ProblemState title="Page not found" problem={{ detail: "That path is not part of the entry surface." }} actionLabel="Return to entry" /></div>;
}

function problemFrom(error: unknown): ProblemDetails {
  if (error instanceof ApiProblem) return error.problem;
  return { detail: "The host could not complete the request.", code: "request_failed" };
}
