import { useEffect, useId, useMemo, useRef, useState } from "react";
import {
  actionCandidates,
  actionGroupLabel,
  actionKind,
  describeAction,
  type ActionKind,
} from "./actions";
import { AudioManager, type AudioSettings, type VoiceKind } from "./audio";
import { decodeCharacterAsset } from "./assets";
import { PixiTable, type PortraitEffect } from "./pixi-table";
import { seatPositionFor } from "./orientation";
import { useGameStore, type Transport } from "./store";
import type {
  ProjectedDecision,
  ProjectedPlayer,
  ProjectedState,
  RoomPlayerSnapshot,
  RoomSnapshot,
  VisibleAction,
} from "./types";
import { tileLabel } from "./tiles";

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
}

interface AssetState {
  [characterId: string]: boolean;
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

function winLimitLabel(han: number): string | undefined {
  if (han >= 13) return "Yakuman";
  if (han >= 11) return "Sanbaiman";
  if (han >= 8) return "Baiman";
  if (han >= 6) return "Haneman";
  if (han >= 5) return "Mangan";
  return undefined;
}

function isManganOrHigher(event: Record<string, unknown>): boolean {
  const han = typeof event.han === "number" ? event.han : 0;
  const limit =
    typeof event.limit === "string" && event.limit.trim().length > 0;
  return han >= 5 || limit;
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

function portraitFromEvents(
  events: unknown[],
  room: RoomSnapshot | null,
): PortraitEffect | null {
  // The server's event array is already audience-filtered. First qualifying event is
  // the only centered result moment; event order is the multi-Ron resolution order.
  for (const event of events) {
    if (eventKind(event) !== "hora") continue;
    const payload = eventPayload(event);
    if (!isManganOrHigher(payload)) continue;
    const actor = typeof payload.actor === "number" ? payload.actor : 0;
    const characterId = characterForSeat(room, actor);
    if (!characterId) return null;
    const target = typeof payload.target === "number" ? payload.target : actor;
    return {
      characterId,
      displayName:
        roster(room).find((player) => player.seat === actor)?.display_name ??
        "",
      result: target === actor ? "Tsumo" : "Ron",
      han: typeof payload.han === "number" ? payload.han : undefined,
      fu: typeof payload.fu === "number" ? payload.fu : undefined,
      limit:
        typeof payload.limit === "string"
          ? payload.limit
          : winLimitLabel(typeof payload.han === "number" ? payload.han : 0),
      points:
        typeof payload.points === "number"
          ? payload.points
          : Array.isArray(payload.delta) &&
              typeof payload.delta[actor] === "number"
            ? (payload.delta[actor] as number)
            : undefined,
    };
  }
  return null;
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
): number | null {
  const [remaining, setRemaining] = useState(() => decisionRemaining(decision));
  useEffect(() => {
    const initial = decisionRemaining(decision);
    if (initial === null) {
      setRemaining(null);
      return;
    }
    const deadline = Date.now() + initial;
    const update = () => setRemaining(Math.max(0, deadline - Date.now()));
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [decision?.decision_id, decision?.remaining_ms, decision?.duration_ms]);
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

function AudioSettingsPanel({ manager }: { manager: AudioManager }) {
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
      </div>
    </details>
  );
}

function GameplayControls({
  status,
  manager,
}: {
  status: GameplayProps["status"];
  manager: AudioManager;
}) {
  return (
    <div className="gameplay-controls">
      <div className={`connection-state connection-${status}`}>
        <span className="state-dot" aria-hidden="true" />
        <span>{status}</span>
      </div>
      <AudioSettingsPanel manager={manager} />
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
    <div className="gameplay-toast-stack" aria-live="polite">
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

function actionTileForHand(action: VisibleAction): number | undefined {
  const actionValue = action.action;
  if (!actionValue || typeof actionValue !== "object") return undefined;
  const key = Object.keys(actionValue as Record<string, unknown>)[0];
  const payload = key
    ? (actionValue as Record<string, unknown>)[key]
    : undefined;
  return payload &&
    typeof payload === "object" &&
    typeof (payload as Record<string, unknown>).tile === "number"
    ? ((payload as Record<string, unknown>).tile as number)
    : undefined;
}

function handActionMap(
  hand: number[],
  actions: VisibleAction[],
): Array<VisibleAction | undefined> {
  const used = new Set<string>();
  return hand.map((tile) => {
    const action = actions.find(
      (candidate) =>
        !used.has(candidate.action_id) && actionTileForHand(candidate) === tile,
    );
    if (action) used.add(action.action_id);
    return action;
  });
}

function TileHitLayer({
  hand,
  actions,
  disabled,
  onAction,
}: {
  hand: number[];
  actions: VisibleAction[];
  disabled: boolean;
  onAction: (action: VisibleAction) => void;
}) {
  const mapped = handActionMap(hand, actions);
  return (
    <div className="table-hit-layer" role="group" aria-label="Your concealed hand">
      {mapped.map((action, index) => (
        <button
          key={`${hand[index]}-${index}`}
          type="button"
          className={`table-tile-hit${action ? " is-legal" : ""}`}
          data-action-id={action?.action_id ?? ""}
          style={
            {
              "--tile-index": index,
              "--tile-count": hand.length,
            } as React.CSSProperties
          }
          aria-label={
            action
              ? `${actionGroupLabel(actionKind(action.action))} ${tileLabel(hand[index])}`
              : `Your ${tileLabel(hand[index])}`
          }
          disabled={disabled || !action}
          onClick={() => {
            if (action) onAction(action);
          }}
        >
          {tileLabel(hand[index])}
        </button>
      ))}
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
  const popupRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const popup = popupRef.current;
    const returnFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const focusableSelector = [
      "button:not([disabled])",
      "[href]",
      "input:not([disabled])",
      "select:not([disabled])",
      "textarea:not([disabled])",
      "[tabindex]:not([tabindex=\"-1\"])",
    ].join(",");
    const focusable = () =>
      popup
        ? Array.from(popup.querySelectorAll<HTMLElement>(focusableSelector))
        : [];
    const initialFocus =
      popup?.querySelector<HTMLButtonElement>(
        ".candidate-list button:not([disabled])",
      ) ?? popup?.querySelector<HTMLButtonElement>(".candidate-popup-head .text-button");
    initialFocus?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
        return;
      }
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
        if (active === first || !popup?.contains(active)) {
          event.preventDefault();
          last.focus();
        }
      } else if (active === last || !popup?.contains(active)) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      if (returnFocus?.isConnected) returnFocus.focus();
    };
  }, []);
  return (
    <div
      ref={popupRef}
      id={id}
      className="candidate-popup"
      role="dialog"
      aria-modal="true"
      aria-label="Choose a legal candidate"
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
    </div>
  );
}

function ActionDeck({
  decision,
  remaining,
  pending,
  disabled,
  onAction,
}: {
  decision: ProjectedDecision | null | undefined;
  remaining: number | null;
  pending: boolean;
  disabled: boolean;
  onAction: (action: VisibleAction) => void;
}) {
  const [popupKind, setPopupKind] = useState<
    "chi" | "pon" | "kan" | "nuki" | null
  >(null);
  const popupId = `${useId()}-candidate-dialog`;
  const grouped = useMemo(() => actionCandidates(decision), [decision]);
  useEffect(() => {
    setPopupKind(null);
  }, [decision?.decision_id]);
  if (!decision) return null;
  const simpleByKind = new Map<ActionKind, VisibleAction>();
  grouped.simple.forEach((action) => {
    const kind = actionKind(action.action);
    if (!simpleByKind.has(kind)) simpleByKind.set(kind, action);
  });
  const candidateButtons = (["chi", "pon", "kan", "nuki"] as const).flatMap(
    (kind) => {
      const candidates = grouped.candidates.get(kind) ?? [];
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
          actions={grouped.candidates.get(popupKind) ?? []}
          disabled={disabled}
          onAction={(action) => {
            setPopupKind(null);
            onAction(action);
          }}
          onClose={() => setPopupKind(null)}
        />
      )}
    </div>
  );
}

function ResultPortrait({
  characterId,
  name,
  available,
}: {
  characterId: string | null | undefined;
  name: string;
  available: boolean;
}) {
  const [failed, setFailed] = useState(false);
  if (!characterId || !available || failed)
    return (
      <span
        className="results-winner-portrait asset-fallback"
        aria-label={`${name} portrait unavailable`}
      >
        {name.slice(0, 1).toUpperCase()}
      </span>
    );
  return (
    <img
      className="results-winner-portrait"
      src={`/assets/characters/${encodeURIComponent(characterId)}/portrait.webp`}
      alt={`${name} portrait`}
      onError={() => setFailed(true)}
    />
  );
}

function ResultsPanel({
  room,
  assets,
}: {
  room: RoomSnapshot | null;
  assets: AssetState;
}) {
  const result =
    room?.result && typeof room.result === "object"
      ? (room.result as Record<string, unknown>)
      : null;
  const players = Array.isArray(result?.players)
    ? result.players.filter(
        (value): value is Record<string, unknown> =>
          Boolean(value) && typeof value === "object",
      )
    : [];
  if (!players.length)
    return (
      <section className="results-panel" data-testid="results-panel">
        <p className="eyebrow">POST-MATCH</p>
        <h2>Match complete</h2>
        <p className="field-hint">
          Final standings are not available in this projection yet.
        </p>
      </section>
    );
  const ordered = players
    .slice()
    .sort((left, right) => Number(left.rank ?? 99) - Number(right.rank ?? 99));
  const first = ordered[0];
  const firstId =
    typeof first.participant_id === "string" ? first.participant_id : "";
  const firstRoster = roster(room).find(
    (entry) => entry.participant_id === firstId,
  );
  const firstName =
    typeof first.display_name === "string" ? first.display_name : "First place";
  const firstAsset = firstRoster?.character_id;
  return (
    <section className="results-panel" data-testid="results-panel">
      <div className="results-heading">
        <div>
          <p className="eyebrow">POST-MATCH / FINAL</p>
          <h2>Standings</h2>
        </div>
        <ResultPortrait
          characterId={firstAsset}
          name={firstName}
          available={firstAsset ? assets[firstAsset] !== false : false}
        />
      </div>
      <ol className="results-list">
        {ordered.map((player, index) => {
          const id =
            typeof player.participant_id === "string"
              ? player.participant_id
              : `${index}`;
          const name =
            typeof player.display_name === "string"
              ? player.display_name
              : "Unknown player";
          const score =
            typeof player.final_score === "number"
              ? player.final_score
              : Array.isArray(result?.final_scores) &&
                  typeof result?.final_scores[index] === "number"
                ? (result.final_scores[index] as number)
                : "—";
          const controller =
            roster(room).find((entry) => entry.participant_id === id)?.controller ?? "";
          const autoLabel = controller.includes("permanent_auto")
            ? "Permanent Auto"
            : controller.includes("temporary_auto")
              ? "Temporary Auto"
              : undefined;
          return (
            <li key={id}>
              <span className="result-rank">
                {String(Number(player.rank ?? index + 1)).padStart(2, "0")}
              </span>
              <span className="result-name">
                {name}
                {autoLabel && <small>{autoLabel}</small>}
              </span>
              <strong>
                {typeof score === "number" ? score.toLocaleString() : score}
              </strong>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function GameplayPlayerStatus({
  players,
  mode,
  viewerSeat,
}: {
  players: ProjectedPlayer[];
  mode: string;
  viewerSeat?: number;
}) {
  if (!players.length) return null;
  return (
    <ul className="visually-hidden" aria-label="Player status">
      {players
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
  const timer = useDecisionTimer(decision);
  const viewer = ownSeat(projection);
  const ownPlayer = projectionPlayer(projection, viewer);
  const grouped = useMemo(() => actionCandidates(decision), [decision]);
  const riichiMode = grouped.riichiDiscard.length > 0;
  const legalDiscardActions = riichiMode
    ? grouped.riichiDiscard
    : grouped.discard;
  const [portraitEffect, setPortraitEffect] = useState<PortraitEffect | null>(
    null,
  );
  const portraitEventRef = useRef<number | null>(null);
  useEffect(() => {
    if (portraitEventRef.current === eventToken) return;
    portraitEventRef.current = eventToken;
    const candidate = portraitFromEvents(storeEvents, room);
    if (candidate) setPortraitEffect(candidate);
  }, [eventToken, room, storeEvents]);
  useEffect(() => {
    if (!portraitEffect) return;
    const timeout = window.setTimeout(() => setPortraitEffect(null), 12000);
    return () => window.clearTimeout(timeout);
  }, [portraitEffect]);
  useEffect(() => {
    if (pending && decision && pending.decisionId !== decision.decision_id)
      useGameStore.getState().clearPendingAction();
  }, [decision?.decision_id, pending]);
  const submit = (action: VisibleAction) => {
    if (!decision || status !== "connected") return;
    useGameStore
      .getState()
      .submitAction(decision.decision_id, action.action_id, send);
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
  const supported =
    typeof window === "undefined" ||
    (window.innerWidth >= 1024 && window.innerHeight >= 600);
  const mode = projection?.mode ?? room?.game_mode ?? "4p-red-east";
  const ownController = room?.participants.find((participant) => participant.participant_id === participantId)?.controller
    ?? roster(room).find((player) => player.seat === viewer)?.controller
    ?? "";
  const inputDisabled = Boolean(pending) || status !== "connected" || !decision;
  return (
    <div
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
      <GameplayControls status={status} manager={manager} />
      <GameplayToast
        status={status}
        commandError={commandError}
        actionError={actionError}
      />
      <GameplayPlayerStatus
        players={projection?.players ?? []}
        mode={mode}
        viewerSeat={projection?.audience === "player" ? viewer : undefined}
      />
      {!supported ? (
        <main className="gameplay-guidance">
          <p className="eyebrow">DESKTOP TABLE REQUIRED</p>
          <h2>Widen this window to play.</h2>
          <p>
            Gameplay needs a landscape window at least 1024 × 600. The table
            will appear when the window is large enough.
          </p>
        </main>
      ) : closeMessage ? (
        <main className="gameplay-main">
          <section className="gameplay-blocking-state" role="alert">
            <p className="eyebrow">TABLE UNAVAILABLE</p>
            <h1>Live table paused</h1>
            <p>{closeMessage}</p>
          </section>
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
            <PixiTable
              projection={projection}
              room={room}
              animations={animations}
              reducedMotion={reducedMotion}
              portraitEffect={portraitEffect}
              onAnimationConsumed={(id: number) =>
                useGameStore.getState().consumeAnimations([id])
              }
            />
            <TileHitLayer
              hand={ownPlayer?.hand ?? []}
              actions={legalDiscardActions}
              disabled={inputDisabled}
              onAction={submit}
            />
            {room?.phase !== "post_match" && (
              <ActionDeck
                decision={decision}
                remaining={timer}
                pending={Boolean(pending)}
                disabled={inputDisabled}
                onAction={submit}
              />
            )}
            {room?.phase === "post_match" && (
              <div className="results-overlay">
                <ResultsPanel room={room} assets={assets} />
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
  portraitFromEvents,
  voiceEvents,
  isManganOrHigher,
  formatSeconds,
};
