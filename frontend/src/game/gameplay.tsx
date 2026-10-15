import { Component, lazy, Suspense, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import {
  actionCandidates,
  actionGroupLabel,
  actionKind,
  actionTile,
  describeAction,
  type ActionKind,
} from "./actions";
import { AudioManager, type AudioSettings, type VoiceKind } from "./audio";
import { decodeCharacterAsset } from "./assets";
import "./live-table.css";
import { isDora } from "./dora";
import { TableAppearanceSettings, useTableAppearance } from "./table-appearance";
import { seatPositionFor, seatPositions } from "./orientation";
import { useGameStore, type Transport } from "./store";
import type {
  ProjectedDecision,
  ProjectedPlayer,
  ProjectedState,
  RoomPlayerSnapshot,
  RoomSnapshot,
  VisibleAction,
} from "./types";
import { tileAssetUrl, tileLabel } from "./tiles";
import { useGameplayViewportSupport } from "./viewport";
import {
  decodeKyokuEndStep,
  resultStepKey,
  type KyokuEndStep,
  type KyokuSettlement,
} from "./results";
import { DrawResult, RoundWinSurface, ScoreTransfer, ResultsPanel, type AssetState } from "./result-presentation";
import { Button } from "../components/ui/button";
import { Sheet, SheetTrigger, SheetContent, SheetTitle, SheetDescription } from "../components/ui/sheet";
import "./result.css";

const ThreeTable = lazy(() => import("./three-table").then(module => ({ default: module.ThreeTable })));

class TableLoader extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    if (this.state.failed) return <div className="three-table" role="status">3D table unavailable.</div>;
    return <Suspense fallback={<div className="three-table" role="status" aria-label="3D table loading">Preparing 3D table</div>}>{this.props.children}</Suspense>;
  }
}

export interface GameplayProps {
  room: RoomSnapshot | null;
  projection: ProjectedState | null;
  status: "connecting" | "connected" | "reconnecting" | "closed" | "error";
  reason: string;
  commandError: string;
  connectionGeneration: number;
  send: Transport;
  reducedMotion: boolean;
  participantId?: string | null;
  /** Leave the local result presentation without changing the authoritative room phase. */
  onLeaveResults?: () => void;
}

function projectionPlayer(
  projection: ProjectedState | null,
  seat: number,
): ProjectedPlayer | undefined {
  return projection?.players?.find((player) => player.seat === seat);
}

function ownSeat(projection: ProjectedState | null): number {
  return projection?.audience === "player" &&
    typeof projection.viewer_seat === "number"
    ? projection.viewer_seat
    : 0;
}

function roster(room: RoomSnapshot | null): RoomPlayerSnapshot[] {
  if (!room) return [];
  return room.roster?.length ? room.roster : (room.match_players ?? []);
}

function characterForSeat(
  room: RoomSnapshot | null,
  seat: number,
): string | null {
  return (
    roster(room).find((player) => player.seat === seat)?.character_id ?? null
  );
}

function uniqueCharacterIds(room: RoomSnapshot | null): string[] {
  return [
    ...new Set(
      roster(room)
        .map((player) => player.character_id)
        .filter((id): id is string => Boolean(id)),
    ),
  ];
}

export function preloadRosterAssets(ids: string[]): Promise<AssetState> {
  return Promise.all(
    ids.map(async (id) => {
      try {
        await Promise.all(
          (["portrait", "icon"] as const).map((kind) =>
            decodeCharacterAsset(id, kind),
          ),
        );
        return [id, true] as const;
      } catch {
        return [id, false] as const;
      }
    }),
  ).then((entries) => Object.fromEntries(entries));
}

function eventPayload(event: unknown): Record<string, unknown> {
  if (!event || typeof event !== "object") return {};
  const object = event as Record<string, unknown>;
  if (
    typeof object.type === "string" &&
    object.event &&
    typeof object.event === "object"
  )
    return eventPayload(object.event);
  const key = Object.keys(object)[0];
  const value = key ? object[key] : undefined;
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : object;
}

function eventKind(event: unknown): string | undefined {
  if (typeof event === "string") return event.toLowerCase();
  if (!event || typeof event !== "object") return undefined;
  const object = event as Record<string, unknown>;
  if (typeof object.type === "string") return object.type.toLowerCase();
  return Object.keys(object)[0]?.toLowerCase();
}

function voiceEvents(
  events: unknown[],
  room: RoomSnapshot | null,
): Array<{ characterId: string | null; kind: VoiceKind }> {
  return events.flatMap((event) => {
    const kind = eventKind(event);
    const payload = eventPayload(event);
    const actor = typeof payload.actor === "number" ? payload.actor : undefined;
    const characterId =
      actor === undefined ? null : characterForSeat(room, actor);
    if (
      kind === "chi" ||
      kind === "pon" ||
      kind === "kita" ||
      kind === "kakan" ||
      kind === "ankan" ||
      kind === "daiminkan"
    )
      return [
        {
          characterId,
          kind:
            kind === "daiminkan" || kind === "ankan" || kind === "kakan"
              ? "kan"
              : kind,
        } as { characterId: string | null; kind: VoiceKind },
      ];
    if (kind === "reach" || kind === "reach_accepted")
      return [{ characterId, kind: "riichi" }];
    if (kind === "hora") {
      const target =
        typeof payload.target === "number" ? payload.target : actor;
      return [{ characterId, kind: target === actor ? "tsumo" : "ron" }];
    }
    return [];
  });
}

function decisionRemaining(
  decision: ProjectedDecision | null | undefined,
): number | null {
  if (!decision) return null;
  if (typeof decision.remaining_ms === "number")
    return Math.max(0, decision.remaining_ms);
  if (typeof decision.duration_ms === "number")
    return Math.max(0, decision.duration_ms);
  return null;
}

function formatSeconds(milliseconds: number | null): string {
  if (milliseconds === null) return "—";
  return `${Math.ceil(milliseconds / 1000)}`;
}

function useDecisionTimer(
  decision: ProjectedDecision | null | undefined,
  paused = false,
): number | null {
  const [remaining, setRemaining] = useState(() => decisionRemaining(decision));
  useEffect(() => {
    const initial = decisionRemaining(decision);
    if (initial === null || paused) {
      setRemaining(initial);
      return;
    }
    const deadline = Date.now() + initial;
    const update = () => setRemaining(Math.max(0, deadline - Date.now()));
    update();
    // The UI displays whole seconds; avoid waking the gameplay tree four
    // times per second while the table is rendering an exact-target motion.
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [decision?.decision_id, decision?.remaining_ms, decision?.duration_ms, paused]);
  return remaining;
}

function useRosterPreload(
  room: RoomSnapshot | null,
  connectionGeneration: number,
): AssetState {
  const ids = uniqueCharacterIds(room);
  const key = ids.slice().sort().join(",");
  const [state, setState] = useState<AssetState>({});
  useEffect(() => {
    let active = true;
    if (!ids.length) {
      setState({});
      return () => {
        active = false;
      };
    }
    setState({});
    void preloadRosterAssets(ids).then((loaded) => {
      if (active) setState(loaded);
    });
    return () => {
      active = false;
    };
  }, [key, connectionGeneration]);
  return state;
}

function useVoiceManager(
  events: unknown[],
  eventToken: number,
  room: RoomSnapshot | null,
  assets: AssetState,
): AudioManager {
  const managerRef = useRef<AudioManager | null>(null);
  if (!managerRef.current) managerRef.current = new AudioManager();
  useEffect(() => {
    const manager = managerRef.current!;
    let unlocked = false;
    const unlock = () => {
      if (unlocked) return;
      unlocked = true;
      void manager.unlock();
      document.removeEventListener("pointerdown", unlock);
      document.removeEventListener("keydown", unlock);
    };
    document.addEventListener("pointerdown", unlock, { once: true });
    document.addEventListener("keydown", unlock, { once: true });
    return () => {
      document.removeEventListener("pointerdown", unlock);
      document.removeEventListener("keydown", unlock);
      manager.destroy();
    };
  }, []);
  useEffect(() => {
    if (eventToken > 0)
      managerRef.current?.playVoices(
        voiceEvents(events, room).filter(
          (event) => event.characterId && assets[event.characterId] === true,
        ),
      );
  }, [eventToken, events, room, assets]);
  return managerRef.current;
}

function AudioSettingsPanel({ manager, appearance }: { manager: AudioManager; appearance: ReturnType<typeof useTableAppearance> }) {
  const [settings, setSettings] = useState<AudioSettings>(
    () => manager.settings,
  );
  const update = (patch: Partial<AudioSettings>) =>
    setSettings(manager.setSettings(patch));
  return (
    <details className="audio-settings">
      <summary>Settings</summary>
      <div className="audio-settings-body">
        <label htmlFor="audio-master">Master</label>
        <input
          id="audio-master"
          aria-label="Master"
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={settings.master}
          onChange={(event) => update({ master: Number(event.target.value) })}
        />
        <label htmlFor="audio-sfx">SFX</label>
        <input
          id="audio-sfx"
          aria-label="SFX"
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={settings.sfx}
          onChange={(event) => update({ sfx: Number(event.target.value) })}
        />
        <label htmlFor="audio-voice">Voice</label>
        <input
          id="audio-voice"
          aria-label="Voice"
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={settings.voice}
          onChange={(event) => update({ voice: Number(event.target.value) })}
        />
        <label className="audio-toggle">
          <input
            type="checkbox"
            checked={settings.voiceEnabled}
            onChange={(event) => update({ voiceEnabled: event.target.checked })}
          />
          Voice enabled
        </label>
        <TableAppearanceSettings settings={appearance} />
      </div>
    </details>
  );
}

function GameplayControls({
  status,
  manager,
  appearance,
}: {
  status: GameplayProps["status"];
  manager: AudioManager;
  appearance: ReturnType<typeof useTableAppearance>;
}) {
  return (
    <div className="gameplay-controls">
      <div className={`connection-state connection-${status}`}>
        <span className="state-dot" aria-hidden="true" />
        <span>{status}</span>
      </div>
      <AudioSettingsPanel manager={manager} appearance={appearance} />
    </div>
  );
}

function GameplayToast({
  status,
  commandError,
  actionError,
}: {
  status: GameplayProps["status"];
  commandError: string;
  actionError: string;
}) {
  return (
    <div className="gameplay-toast-stack">
      {(status === "reconnecting" || status === "error") && (
        <p className="gameplay-toast" role="status">
          {status === "error"
            ? "Network connection interrupted… Retrying while the last authoritative table state remains visible."
            : "Reconnecting… The last authoritative table state remains visible."}
        </p>
      )}
      {commandError && (
        <p className="gameplay-toast" role="alert">
          Action rejected: {commandError}
        </p>
      )}
      {actionError && (
        <p className="gameplay-toast" role="alert">
          Action rejected: {actionError}. Retry while this Decision remains
          open.
        </p>
      )}
    </div>
  );
}

export function handActionMap(
  hand: number[],
  actions: VisibleAction[],
): Array<VisibleAction | undefined> {
  const used = new Set<string>();
  return hand.map((tile) => {
    const action = actions.find(
      (candidate) =>
        !used.has(candidate.action_id) && actionTile(candidate.action) === tile,
    );
    if (action) used.add(action.action_id);
    return action;
  });
}

function TileHitLayer({
  hand,
  actions,
  indicators,
  disabled,
  onAction,
}: {
  hand: number[];
  actions: VisibleAction[];
  indicators: number[];
  disabled: boolean;
  onAction: (actions: VisibleAction[]) => void;
}) {
  const mapped = handActionMap(hand, actions);
  return (
    <div className="table-hit-layer" role="group" aria-label="Your concealed hand">
      {hand.map((tile, index) => {
        const action = mapped[index];
        const candidates = action ? actions.filter((candidate) => actionTile(candidate.action) === tile) : [];
        return (
          <button
            key={`${tile}-${index}`}
            type="button"
            className={`table-tile-hit${action ? " is-legal" : ""}${isDora(tile, indicators) ? " is-dora" : ""}`}
            data-action-id={candidates.length === 1 ? action?.action_id : ""}
            aria-label={`${action ? `${actionGroupLabel(actionKind(action.action))} ${tileLabel(tile)}` : `Your ${tileLabel(tile)}`}${isDora(tile, indicators) ? ", Dora" : ""}`}
            disabled={disabled || !action}
            onClick={() => onAction(candidates)}
          >
            <img src={tileAssetUrl(tile)} alt="" />
          </button>
        );
      })}
    </div>
  );
}

function CandidatePopup({
  actions,
  onAction,
  onClose,
  id,
  disabled,
}: {
  actions: VisibleAction[];
  onAction: (action: VisibleAction) => void;
  onClose: () => void;
  id: string;
  disabled: boolean;
}) {
  const popupRef = useRef<HTMLDialogElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const popup = popupRef.current;
    if (!popup) return;
    const returnFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    try {
      if (!popup.open) popup.showModal();
    } catch {
      popup.setAttribute("open", "");
    }
    const focusableSelector = [
      "button:not([disabled])",
      "[href]",
      "input:not([disabled])",
      "select:not([disabled])",
      "textarea:not([disabled])",
      "[tabindex]:not([tabindex=\"-1\"])",
    ].join(",");
    const focusable = () =>
      Array.from(popup.querySelectorAll<HTMLElement>(focusableSelector));
    const initialFocus =
      popup.querySelector<HTMLButtonElement>(
        ".candidate-list button:not([disabled])",
      ) ?? popup.querySelector<HTMLButtonElement>(".candidate-popup-head .text-button");
    initialFocus?.focus();
    const onCancel = (event: Event) => {
      event.preventDefault();
      closeRef.current();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const elements = focusable();
      if (elements.length === 0) {
        event.preventDefault();
        return;
      }
      const first = elements[0];
      const last = elements[elements.length - 1];
      const active = document.activeElement;
      if (event.shiftKey) {
        if (active === first || !popup.contains(active)) {
          event.preventDefault();
          last.focus();
        }
      } else if (active === last || !popup.contains(active)) {
        event.preventDefault();
        first.focus();
      }
    };
    popup.addEventListener("cancel", onCancel);
    popup.addEventListener("keydown", onKeyDown);
    return () => {
      popup.removeEventListener("cancel", onCancel);
      popup.removeEventListener("keydown", onKeyDown);
      if (popup.open) popup.close();
      else popup.removeAttribute("open");
      if (returnFocus?.isConnected) returnFocus.focus();
    };
  }, []);
  useEffect(() => {
    const popup = popupRef.current;
    if (!popup) return;
    const firstCandidate = popup.querySelector<HTMLButtonElement>(
      ".candidate-list button:not([disabled])",
    );
    if (!disabled) {
      firstCandidate?.focus();
      return;
    }
    if (document.activeElement instanceof HTMLButtonElement && document.activeElement.disabled) {
      popup.querySelector<HTMLButtonElement>(".candidate-popup-head .text-button")?.focus();
    }
  }, [disabled]);
  return (
    <dialog
      ref={popupRef}
      id={id}
      className="candidate-popup"
      aria-modal="true"
      aria-label="Choose a legal candidate"
      aria-busy={disabled ? "true" : "false"}
    >
      <div className="candidate-popup-head">
        <span>Choose a line</span>
        <button type="button" className="text-button" onClick={onClose}>
          Close
        </button>
      </div>
      <div className="candidate-list">
        {actions.map((action) => (
          <button
            type="button"
            className="button button-secondary"
            key={action.action_id}
            data-action-id={action.action_id}
            disabled={disabled}
            onClick={() => onAction(action)}
          >
            {describeAction(action)}
          </button>
        ))}
      </div>
    </dialog>
  );
}

function ActionDeck({
  decision,
  pending,
  disabled,
  motionActive,
  onAction,
}: {
  decision: ProjectedDecision | null | undefined;
  pending: boolean;
  disabled: boolean;
  motionActive: boolean;
  onAction: (action: VisibleAction) => void;
}) {
  const [popupKind, setPopupKind] = useState<
    "riichi_discard" | "chi" | "pon" | "kan" | "nuki" | null
  >(null);
  const popupId = `${useId()}-candidate-dialog`;
  const grouped = useMemo(() => actionCandidates(decision), [decision]);
  const remaining = useDecisionTimer(decision, motionActive);
  useEffect(() => {
    setPopupKind(null);
  }, [decision?.decision_id]);
  if (!decision) return null;
  const simpleByKind = new Map<ActionKind, VisibleAction>();
  grouped.simple.forEach((action) => {
    const kind = actionKind(action.action);
    if (!simpleByKind.has(kind)) simpleByKind.set(kind, action);
  });
  const candidateButtons = (["riichi_discard", "chi", "pon", "kan", "nuki"] as const).flatMap(
    (kind) => {
      const candidates = kind === "riichi_discard"
        ? grouped.riichiDiscard
        : grouped.candidates.get(kind) ?? [];
      if (!candidates.length) return [];
      return [{ kind, candidates }];
    },
  );
  return (
    <div
      className="action-deck"
      data-testid="action-deck"
      aria-label="Legal actions"
      aria-busy={pending ? "true" : "false"}
    >
      <div className="action-deck-label">
        <span className="eyebrow">DECISION / {decision.kind}</span>
        <strong>{pending ? "Sending action" : "Choose one"}</strong>
        <span
          data-testid="decision-timer"
          aria-label={`Decision timer: ${formatSeconds(remaining)} seconds`}
        >
          {remaining === null ? "TIMER —" : `${formatSeconds(remaining)}s`}
        </span>
      </div>
      <div className="action-buttons">
        {(["ron", "tsumo", "pass", "abortive_draw"] as ActionKind[]).map(
          (kind) => {
            const action = simpleByKind.get(kind);
            return action ? (
              <button
                key={kind}
                type="button"
                className={`button ${kind === "pass" ? "button-secondary" : "button-primary"}`}
                data-action-id={action.action_id}
                disabled={disabled}
                onClick={() => onAction(action)}
              >
                {actionGroupLabel(kind)}
              </button>
            ) : null;
          },
        )}
        {candidateButtons.map(({ kind, candidates }) => {
          const multiple = candidates.length > 1;
          return (
            <button
              key={kind}
              type="button"
              className="button button-secondary"
              data-action-id={candidates.length === 1 ? candidates[0].action_id : ""}
              disabled={disabled}
              aria-haspopup={multiple ? "dialog" : undefined}
              aria-expanded={multiple ? popupKind === kind : undefined}
              aria-controls={multiple ? popupId : undefined}
              onClick={() =>
                candidates.length === 1
                  ? onAction(candidates[0])
                  : setPopupKind(kind)
              }
            >
              {candidates.length === 1
                ? actionGroupLabel(kind)
                : `${actionGroupLabel(kind)} (${candidates.length})`}
            </button>
          );
        })}
      </div>
      {popupKind && (
        <CandidatePopup
          id={popupId}
          actions={popupKind === "riichi_discard"
            ? grouped.riichiDiscard
            : grouped.candidates.get(popupKind) ?? []}
          disabled={disabled}
          onAction={onAction}
          onClose={() => setPopupKind(null)}
        />
      )}
    </div>
  );
}

function useResultRemaining(step: KyokuEndStep | null, receivedDeadline: number | null): number {
  const key = step ? resultStepKey(step.id) : "";
  const clock = useRef({ key: "", deadline: 0 });
  const [remaining, setRemaining] = useState(step?.remaining_ms ?? 0);
  useEffect(() => {
    const now = performance.now();
    const deadline = receivedDeadline ?? now + (step?.remaining_ms ?? 0);
    clock.current = {
      key,
      deadline: clock.current.key === key ? Math.min(clock.current.deadline, deadline) : deadline,
    };
    const update = () => setRemaining(Math.max(0, Math.ceil(clock.current.deadline - performance.now())));
    update();
    const timer = window.setInterval(update, 50);
    return () => window.clearInterval(timer);
  }, [key, step?.remaining_ms, receivedDeadline]);
  return remaining;
}

function SettlementPresentation({ settlement, room, remaining, reducedMotion, paused, setPaused, skipped, setSkipped }: {
  settlement: KyokuSettlement; room: RoomSnapshot | null; remaining: number; reducedMotion: boolean;
  paused: number | null; setPaused: (value: number | null) => void; skipped: boolean; setSkipped: (value: boolean) => void;
}) {
  const progress = reducedMotion || skipped ? 1 : Math.min(1, (5000 - remaining) / 3000);
  return <>
    <ScoreTransfer settlement={settlement} room={room} progress={reducedMotion || skipped ? 1 : paused ?? progress} />
    <div className="round-win-actions">
      <button type="button" className="button" aria-pressed={paused !== null} onClick={() => setPaused(paused === null ? progress : null)}>{paused === null ? "Pause for review" : "Resume"}</button>
      <button type="button" className="button" onClick={() => setSkipped(true)}>Skip animation</button>
    </div>
  </>;
}

function GameplayPlayerStatus({
  players,
  mode,
  viewerSeat,
  playerCount,
}: {
  players: ProjectedPlayer[];
  mode: string;
  viewerSeat?: number;
  playerCount?: number;
}) {
  const seats = seatPositions(mode, viewerSeat);
  const count = Number.isInteger(playerCount)
    ? Math.max(0, Math.min(playerCount as number, seats.length))
    : seats.length;
  const validSeats = new Set(seats.slice(0, count).map(({ seat }) => seat));
  const validPlayers = players.filter((player) => validSeats.has(player.seat));
  if (!validPlayers.length) return null;
  return (
    <ul className="visually-hidden" aria-label="Player status">
      {validPlayers
        .slice()
        .sort((left, right) => left.seat - right.seat)
        .map((player) => (
          <li key={player.participant_id}>
            <span>{player.display_name}</span>
            <span>
              Score {typeof player.score === "number" ? player.score.toLocaleString() : "—"}
            </span>
            <span>
              Seat position: {seatPositionFor(mode, player.seat, viewerSeat) ?? `seat ${player.seat + 1}`}
            </span>
            <span>{player.riichi ? "Riichi" : "Not riichi"}</span>
          </li>
        ))}
    </ul>
  );
}

export function GameplaySurface({
  room,
  projection,
  status,
  reason,
  commandError,
  connectionGeneration,
  send,
  reducedMotion,
  participantId,
  onLeaveResults,
}: GameplayProps) {
  const storeEvents = useGameStore((state) => state.lastEvents);
  const eventToken = useGameStore((state) => state.lastEventToken);
  const animations = useGameStore((state) => state.animationQueue);
  const pending = useGameStore((state) => state.pendingAction);
  const actionError = useGameStore((state) => state.actionError);
  const lastActionResult = useGameStore((state) => state.lastActionResult);
  const actionResultHistory = useGameStore((state) => state.actionResultHistory);
  const animationEnqueuedCount = useGameStore((state) => state.animationEnqueuedCount);
  const animationConsumedCount = useGameStore((state) => state.animationConsumedCount);
  const assets = useRosterPreload(room, connectionGeneration);
  const manager = useVoiceManager(storeEvents, eventToken, room, assets);
  const decision = projection?.decision;
  const viewer = ownSeat(projection);
  const ownPlayer = projectionPlayer(projection, viewer);
  const grouped = useMemo(() => actionCandidates(decision), [decision]);
  const [tileCandidates, setTileCandidates] = useState<VisibleAction[] | null>(null);
  useEffect(() => setTileCandidates(null), [decision?.decision_id]);
  // Ordinary discards remain table targets when both families are offered.
  // A Riichi-only decision still uses the table target as its concrete action;
  // when both are present, Riichi is rendered independently in ActionDeck so
  // neither authoritative action family hides the other.
  const legalDiscardActions = grouped.discard.length > 0
    ? grouped.discard
    : grouped.riichiDiscard;
  const appearance = useTableAppearance();
  const tableSheenRef = useRef<HTMLDivElement>(null);
  const resultDismissRef = useRef<HTMLButtonElement>(null);
  const step = decodeKyokuEndStep(room?.kyoku_end, room?.game_mode ?? projection?.mode ?? "");
  const receivedDeadline = useGameStore((state) => state.room === room ? state.resultDeadline : null);
  const resultRemaining = useResultRemaining(step, receivedDeadline);
  const stepKey = step ? resultStepKey(step.id) : "";
  const [dismissedStep, setDismissedStep] = useState("");
  const confirmedStep = useGameStore((state) => state.confirmedResultStep);
  const ownId = participantId ?? (projection?.audience === "player" ? ownPlayer?.participant_id : null);
  const canConfirm = Boolean(step && ownId && projection?.audience === "player" && step.required.includes(ownId));
  const confirmed = Boolean(step && (step.confirmed.includes(ownId ?? "") || (confirmedStep && resultStepKey(confirmedStep) === stepKey)));
  const confirmRef = useRef<HTMLButtonElement>(null);
  const supported = useGameplayViewportSupport();
  const confirmationReady = canConfirm && !confirmed && status === "connected" && resultRemaining > 0;
  const [reviewKey, setReviewKey] = useState("");
  const reviewAllowed = supported && canConfirm && status === "connected" && resultRemaining > 0;
  const reviewOpen = reviewAllowed && reviewKey === stepKey;
  const [drawView, setDrawView] = useState("result");
  const [settlementPaused, setSettlementPaused] = useState<number | null>(null);
  const [settlementSkipped, setSettlementSkipped] = useState(false);
  useEffect(() => {
    setDrawView("result");
    setSettlementPaused(null);
    setSettlementSkipped(false);
  }, [stepKey, reviewAllowed]);
  useEffect(() => {
    if (!reviewAllowed || reviewKey !== stepKey) setReviewKey("");
  }, [reviewAllowed, stepKey, reviewKey]);
  useEffect(() => {
    if (supported && !reviewOpen && (!canConfirm || confirmationReady)) confirmRef.current?.focus();
  }, [stepKey, confirmationReady, canConfirm, supported, reviewOpen]);
  const win = step && "Win" in step.content ? step.content.Win : null;
  const settlement = step && !("Win" in step.content)
    ? ("Settlement" in step.content ? step.content.Settlement : step.content.Draw)
    : null;
  useEffect(() => {
    if (pending && decision && pending.decisionId !== decision.decision_id)
      useGameStore.getState().clearPendingAction();
  }, [decision?.decision_id, pending]);
  useEffect(() => {
    if (room?.phase !== "post_match" || !onLeaveResults) return;
    const focusTimer = window.setTimeout(() => resultDismissRef.current?.focus(), 0);
    return () => window.clearTimeout(focusTimer);
  }, [onLeaveResults, room?.phase]);
  const submit = (action: VisibleAction) => {
    if (!decision || status !== "connected" || !decision.actions.some((candidate) => candidate.action_id === action.action_id)) return;
    const state = useGameStore.getState();
    if (state.pendingAction || state.actionResultHistory.some((result) => result.decision_id === decision.decision_id && result.status === "accepted")) return;
    if (state.projection?.decision && state.projection.decision.decision_id !== decision.decision_id) return;
    state.submitAction(decision.decision_id, action.action_id, send);
    setTileCandidates(null);
  };
  const closeMessage =
    reason === "connected_elsewhere"
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
  const mode = projection?.mode ?? room?.game_mode ?? "4p-red-east";
  const ownController = room?.participants.find((participant) => participant.participant_id === participantId)?.controller
    ?? roster(room).find((player) => player.seat === viewer)?.controller
    ?? "";
  const inputDisabled = room?.kyoku_end != null || Boolean(pending) || status !== "connected" || !decision
    || actionResultHistory.some((result) => result.decision_id === decision.decision_id && result.status === "accepted");
  const resultView = step && <>
    {win && <RoundWinSurface key={stepKey} result={win} room={room} assets={assets} reducedMotion={reducedMotion} />}
    {settlement && ("Draw" in step.content ? <DrawResult key={stepKey} settlement={settlement} room={room} view={drawView} onViewChange={setDrawView} /> : <SettlementPresentation key={stepKey} settlement={settlement} room={room} remaining={resultRemaining} reducedMotion={reducedMotion} paused={settlementPaused} setPaused={setSettlementPaused} skipped={settlementSkipped} setSkipped={setSettlementSkipped} />)}
  </>;
  const resultAction = step && (canConfirm ? <Button ref={confirmRef} className="round-win-continue" disabled={!confirmationReady} onClick={() => useGameStore.getState().confirmResult(step.id, send)}>{confirmed ? "Waiting for players" : "Continue"}</Button>
    : <Button ref={confirmRef} variant="outline" onClick={() => setDismissedStep(stepKey)}>Dismiss</Button>);
  return (
    <div
      ref={tableSheenRef}
      className="app-shell gameplay-shell"
      data-testid="gameplay-shell"
      data-gameplay-mode={mode}
      data-orientation-seat={viewer}
      data-motion={reducedMotion ? "static" : "cinematic"}
      data-room-revision={room?.revision ?? ""}
      data-human-controller={ownController}
      data-current-decision-id={decision?.decision_id ?? ""}
      data-last-action-result-status={lastActionResult?.status ?? ""}
      data-last-action-result-action-id={lastActionResult?.action_id ?? ""}
      data-action-result-history={JSON.stringify(actionResultHistory)}
      data-animation-enqueued-count={animationEnqueuedCount}
      data-animation-consumed-count={animationConsumedCount}
    >
      <GameplayControls status={status} manager={manager} appearance={appearance} />
      <GameplayToast
        status={status}
        commandError={commandError}
        actionError={actionError}
      />
      <GameplayPlayerStatus
        players={projection?.players ?? []}
        mode={mode}
        viewerSeat={projection?.audience === "player" ? viewer : undefined}
        playerCount={projection?.player_count}
      />
      {closeMessage ? (
        <main className="gameplay-main gameplay-blocking-main">
          <section className="gameplay-blocking-state" role="alert">
            <p className="eyebrow">TABLE UNAVAILABLE</p>
            <h1>Live table paused</h1>
            <p>{closeMessage}</p>
          </section>
        </main>
      ) : !supported ? (
        <main className="gameplay-guidance">
          <p className="eyebrow">DESKTOP TABLE REQUIRED</p>
          <h2>Widen this window to play.</h2>
          <p>
            Gameplay needs a landscape window at least 1024 × 600. The table
            will appear when the window is large enough.
          </p>
        </main>
      ) : (
        <main className="gameplay-main">
          <div className="table-letterbox">
            {!projection && (
              <div className="gameplay-sync-state" role="status" aria-live="polite">
                <p className="eyebrow">TABLE SYNC</p>
                <strong>Waiting for an authoritative projection</strong>
                <span>The table will synchronize when the host sends the next snapshot.</span>
              </div>
            )}
            <TableLoader><ThreeTable
              projection={projection}
              room={room}
              animations={animations}
              reducedMotion={reducedMotion}
              onAnimationConsumed={(id: number) =>
                useGameStore.getState().consumeAnimations([id])
              }
              onAnimationCancelled={(id: number) =>
                useGameStore.getState().cancelAnimations([id])
              }
              surface="live"
              sheenHost={tableSheenRef}
              appearance={appearance.appearance}
            /></TableLoader>
            <TileHitLayer
              hand={ownPlayer?.hand ?? []}
              actions={legalDiscardActions}
              indicators={projection?.dora_indicators ?? []}
              disabled={inputDisabled}
              onAction={(candidates) => {
                if (candidates.length === 1) submit(candidates[0]);
                else if (candidates.length > 1) setTileCandidates(candidates);
              }}
            />
            {tileCandidates && decision && (
              <CandidatePopup
                key={decision.decision_id}
                id="tile-candidate-dialog"
                actions={tileCandidates}
                disabled={inputDisabled}
                onAction={submit}
                onClose={() => setTileCandidates(null)}
              />
            )}
            {step && dismissedStep !== stepKey && room?.phase !== "post_match" && (
              <div className="round-win-overlay">
                {!reviewOpen && resultView}
                <span data-testid="result-timer" data-remaining-ms={resultRemaining} aria-label="Result time remaining" aria-live="off">{formatSeconds(resultRemaining)}s</span>
                <p key={stepKey} role="status">{resultRemaining === 0 ? "Result time expired. Waiting for the server." : confirmed ? "Waiting for players" : `${"Win" in step.content ? "Win" : "Draw" in step.content ? "Draw" : "Scores"} result ${step.id.step_index + 1} ready`}</p>
                <div className="round-win-actions">
                  {!reviewOpen && resultAction}
                  <Sheet open={reviewOpen} onOpenChange={open => setReviewKey(open ? stepKey : "")}>
                    <SheetTrigger render={<Button variant="outline" disabled={!reviewAllowed} />}>Review</SheetTrigger>
                    <SheetContent className="result-review overflow-y-auto" finalFocus={() => confirmRef.current && !confirmRef.current.disabled ? confirmRef.current : tableSheenRef.current?.querySelector<HTMLElement>("summary") ?? null}>
                      <SheetTitle>Review result</SheetTitle>
                      <SheetDescription>The server deadline continues during Review.</SheetDescription>
                      {reviewOpen && <><div className="result-details">{resultView}</div><p aria-label="Result time remaining">{formatSeconds(resultRemaining)}s</p>{resultAction}</>}
                    </SheetContent>
                  </Sheet>
                </div>
              </div>
            )}
            {step && dismissedStep === stepKey && <p role="status">Waiting for players to confirm</p>}
            {room?.kyoku_end == null && room?.phase !== "post_match" && (
              <ActionDeck
                decision={decision}
                pending={Boolean(pending)}
                disabled={inputDisabled}
                motionActive={animations.length > 0}
                onAction={submit}
              />
            )}
            {room?.phase === "post_match" && (
              <div className="results-overlay">
                <ResultsPanel room={room} assets={assets} />
                {onLeaveResults && (
                  <div className="results-actions">
                    <button
                      ref={resultDismissRef}
                      type="button"
                      className="button button-secondary"
                      data-testid="dismiss-results"
                      onClick={onLeaveResults}
                    >
                      Return to room
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        </main>
      )}
    </div>
  );
}

export {
  actionCandidates,
  voiceEvents,
  formatSeconds,
};
