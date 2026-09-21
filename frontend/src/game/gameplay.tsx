import { useEffect, useId, useMemo, useRef, useState } from "react";
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
import { ThreeTable, type PortraitEffect } from "./three-table";
import { buildMatchSceneLayout, type MatchSceneLayout } from "./three-table-layout";
import { projectLocalHandHitTargets } from "./table-hit-targets";
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
import { tileAssetUrl, tileIdsFromValue, tileLabel } from "./tiles";
import { useGameplayViewportSupport } from "./viewport";

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

export interface RoundWinYaku {
  name: string;
  han?: number;
}

export interface RoundWinEffect extends PortraitEffect {
  /** The physical winning tile from the authoritative Hora event, when exposed. */
  winningTile?: number;
  /** The winner's visible concealed hand, plus the winning tile when it is exposed. */
  hand?: number[];
  /** Visible meld tiles kept separate from the concealed hand. */
  melds?: number[][];
  /** Yaku pairs are copied from the authoritative Hora event without translation. */
  yaku?: RoundWinYaku[];
  scores?: number[];
  delta?: number[];
}

function numericArray(value: unknown): number[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const numbers = value.filter(
    (entry): entry is number => typeof entry === "number" && Number.isFinite(entry),
  );
  return numbers.length === value.length ? numbers : undefined;
}

function roundWinYaku(value: unknown): RoundWinYaku[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const entries = value.flatMap((entry): RoundWinYaku[] => {
    if (Array.isArray(entry)) {
      const [name, han] = entry;
      if (typeof name !== "string" || !name.trim()) return [];
      return [{
        name: name.trim(),
        ...(typeof han === "number" && Number.isFinite(han) ? { han } : {}),
      }];
    }
    if (entry && typeof entry === "object") {
      const object = entry as Record<string, unknown>;
      const name = typeof object.name === "string"
        ? object.name
        : typeof object.yaku === "string"
          ? object.yaku
          : "";
      if (!name.trim()) return [];
      const han = typeof object.han === "number" && Number.isFinite(object.han)
        ? object.han
        : undefined;
      return [{ name: name.trim(), ...(han === undefined ? {} : { han }) }];
    }
    return [];
  });
  return entries.length > 0 ? entries : undefined;
}

function visibleWinnerHand(
  projection: ProjectedState | null | undefined,
  actor: number,
  winningTile: number | undefined,
): { hand?: number[]; melds?: number[][] } {
  const player = projection?.players?.find((candidate) => candidate.seat === actor);
  const hand = tileIdsFromValue(player?.hand).filter((tile) => tile >= 0 && tile < 136);
  const melds = Array.isArray(player?.melds)
    ? player.melds
        .map((meld) => tileIdsFromValue(meld?.tiles).filter((tile) => tile >= 0 && tile < 136))
        .filter((tiles) => tiles.length > 0)
    : [];
  if (hand.length === 0) {
    return melds.length > 0 ? { melds } : {};
  }
  const visibleHand = hand.slice();
  // A private projection can expose the concealed tiles while the Hora event carries
  // the final tile. Add that tile only when the server has not already included it.
  if (winningTile !== undefined && visibleHand.length < 14 && !visibleHand.includes(winningTile)) {
    visibleHand.push(winningTile);
  }
  return { hand: visibleHand, ...(melds.length > 0 ? { melds } : {}) };
}

function portraitFromEvents(
  events: unknown[],
  room: RoomSnapshot | null,
  projection?: ProjectedState | null,
): RoundWinEffect | null {
  // The server's event array is already audience-filtered. The first Hora is the
  // only centered result moment; event order is the multi-Ron resolution order.
  for (const event of events) {
    if (eventKind(event) !== "hora") continue;
    const payload = eventPayload(event);
    const actor = typeof payload.actor === "number" ? payload.actor : undefined;
    const target = typeof payload.target === "number" ? payload.target : undefined;
    if (actor === undefined || target === undefined) continue;
    const winningTile = typeof payload.pai === "number"
      ? payload.pai
      : typeof payload.tile === "number"
        ? payload.tile
        : undefined;
    const winner = roster(room).find((player) => player.seat === actor);
    const delta = numericArray(payload.delta ?? payload.deltas);
    const details = visibleWinnerHand(projection, actor, winningTile);
    return {
      // Keep the result visible even when the Character Pack asset is missing.
      characterId: winner?.character_id ?? "",
      displayName: winner?.display_name ?? `Player ${actor + 1}`,
      result: target === actor ? "Tsumo" : "Ron",
      han: typeof payload.han === "number" ? payload.han : undefined,
      fu: typeof payload.fu === "number" ? payload.fu : undefined,
      ...(typeof payload.limit === "string" && payload.limit.trim().length > 0
        ? { limit: payload.limit.trim() }
        : {}),
      points:
        typeof payload.points === "number"
          ? payload.points
          : delta && typeof delta[actor] === "number"
            ? delta[actor]
            : undefined,
      winningTile,
      ...details,
      yaku: roundWinYaku(payload.yaku),
      scores: numericArray(payload.scores),
      delta,
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
  layout,
  disabled,
  onAction,
}: {
  hand: number[];
  actions: VisibleAction[];
  layout: MatchSceneLayout | null;
  disabled: boolean;
  onAction: (action: VisibleAction) => void;
}) {
  const mapped = handActionMap(hand, actions);
  const projected = useMemo(
    () => projectLocalHandHitTargets(layout ?? { players: [], tiles: [], wallCount: 0 }),
    [layout],
  );
  const bounds = projected.bounds;
  const hasProjectedBounds = projected.targets.length > 0 && bounds.width > 0 && bounds.height > 0;
  return (
    <div
      className="table-hit-layer"
      role="group"
      aria-label="Your concealed hand"
      style={
        hasProjectedBounds
          ? {
              left: `${bounds.left * 100}%`,
              top: `${bounds.top * 100}%`,
              right: "auto",
              bottom: "auto",
              width: `${bounds.width * 100}%`,
              height: `${bounds.height * 100}%`,
            }
          : undefined
      }
    >
      {mapped.map((action, index) => {
        const target = projected.targets[index];
        const targetStyle = target && hasProjectedBounds
          ? {
              left: `${((target.rect.left - bounds.left) / bounds.width) * 100}%`,
              top: `${((target.rect.top - bounds.top) / bounds.height) * 100}%`,
              width: `${(target.rect.width / bounds.width) * 100}%`,
              height: `${(target.rect.height / bounds.height) * 100}%`,
            }
          : undefined;
        return (
          <button
            key={target?.tileKey ?? `${hand[index]}-${index}`}
            type="button"
            className={`table-tile-hit${action ? " is-legal" : ""}`}
            data-action-id={action?.action_id ?? ""}
            data-tile-key={target?.tileKey ?? ""}
            style={targetStyle}
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
          onAction={onAction}
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
  className = "",
}: {
  characterId: string | null | undefined;
  name: string;
  available: boolean;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const portraitClass = `results-winner-portrait${className ? ` ${className}` : ""}`;
  const initial = Array.from(name.trim())[0]?.toUpperCase() ?? "?";
  if (!characterId || !available || failed)
    return (
      <span
        className={`${portraitClass} asset-fallback`}
        role="img"
        aria-label={`${name} portrait unavailable`}
      >
        {initial}
      </span>
    );
  return (
    <img
      className={portraitClass}
      src={`/assets/characters/${encodeURIComponent(characterId)}/portrait.webp`}
      alt={`${name} portrait`}
      onError={() => setFailed(true)}
    />
  );
}

function yakuLabel(name: string): string {
  return name
    .replaceAll("_", " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/(^|\s)\S/g, (letter) => letter.toUpperCase());
}

function roundPoints(value: number | undefined): string {
  return value === undefined ? "—" : value.toLocaleString();
}

function RoundWinTiles({
  hand,
  melds,
  winningTile,
}: {
  hand?: number[];
  melds?: number[][];
  winningTile?: number;
}) {
  if (!hand?.length && !melds?.length && winningTile === undefined) return null;
  return (
    <div className="round-win-hand" role="group" aria-label="Winning hand">
      <div className="round-win-hand-label">
        <span className="state-label">VISIBLE HAND</span>
        {!hand?.length && <span className="round-win-hand-note">Some tiles remain private.</span>}
      </div>
      {hand && hand.length > 0 && (
        <div
          className="round-win-tile-row"
          role="group"
          aria-label={`${hand.length} visible winning hand tiles`}
        >
          {hand.map((tile, index) => (
            <span
              className={`round-win-tile${tile === winningTile ? " is-winning" : ""}`}
              key={`${tile}-${index}`}
            >
              <img src={tileAssetUrl(tile)} alt={tileLabel(tile)} />
            </span>
          ))}
        </div>
      )}
      {melds && melds.length > 0 && (
        <div className="round-win-melds" role="group" aria-label="Visible melds">
          {melds.map((meld, meldIndex) => (
            <span className="round-win-meld" key={`meld-${meldIndex}`}>
              {meld.map((tile, tileIndex) => (
                <img key={`${tile}-${tileIndex}`} src={tileAssetUrl(tile)} alt={tileLabel(tile)} />
              ))}
            </span>
          ))}
        </div>
      )}
      {!hand?.length && winningTile !== undefined && (
        <span className="round-win-hand-note">Winning tile: {tileLabel(winningTile)}</span>
      )}
    </div>
  );
}

export function RoundWinSurface({
  effect,
  assets,
  reducedMotion = false,
}: {
  effect: RoundWinEffect;
  assets: AssetState;
  reducedMotion?: boolean;
}) {
  const yaku = effect.yaku ?? [];
  return (
    <section
      className="round-win-surface"
      data-testid="round-win-surface"
      data-motion={reducedMotion ? "static" : "cinematic"}
      aria-labelledby="round-win-heading"
      role="status"
      aria-live="polite"
    >
      <div className="round-win-identity">
        <ResultPortrait
          characterId={effect.characterId}
          name={effect.displayName}
          available={effect.characterId ? assets[effect.characterId] !== false : false}
          className="round-win-portrait"
        />
        <div className="round-win-copy">
          <p className="eyebrow">ROUND WIN / {effect.result}</p>
          <h2 id="round-win-heading">{effect.displayName}</h2>
          <span className="round-win-method">{effect.result === "Tsumo" ? "Self-draw" : "Discard win"}</span>
        </div>
      </div>
      <dl className="round-win-facts" aria-label="Round win facts">
        {effect.han !== undefined && <div><dt>Han</dt><dd>{effect.han}</dd></div>}
        {effect.fu !== undefined && <div><dt>Fu</dt><dd>{effect.fu}</dd></div>}
        {effect.limit && <div><dt>Limit</dt><dd>{effect.limit}</dd></div>}
        {effect.points !== undefined && <div><dt>Points</dt><dd>{roundPoints(effect.points)}</dd></div>}
      </dl>
      {yaku.length > 0 && (
        <div className="round-win-yaku" role="group" aria-label="Authoritative yaku">
          <span className="state-label">YAKU</span>
          <ul>
            {yaku.map((entry, index) => (
              <li key={`${entry.name}-${index}`}>
                {yakuLabel(entry.name)}{entry.han === undefined ? "" : ` · ${entry.han} han`}
              </li>
            ))}
          </ul>
        </div>
      )}
      <RoundWinTiles hand={effect.hand} melds={effect.melds} winningTile={effect.winningTile} />
    </section>
  );
}

function resultNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function resultSeat(
  player: Record<string, unknown>,
  fallbackSeat: number | undefined,
  index: number,
): number {
  const seat = resultNumber(player.seat);
  return typeof seat === "number" && Number.isInteger(seat) && seat >= 0
    ? seat
    : fallbackSeat ?? index;
}

function resultDelta(
  player: Record<string, unknown>,
  index: number,
  result: Record<string, unknown> | null,
  fallbackSeat?: number,
): number | undefined {
  for (const key of ["delta", "deltas", "final_delta", "score_delta"]) {
    const direct = resultNumber(player[key]);
    if (direct !== undefined) return direct;
  }
  const seat = resultSeat(player, fallbackSeat, index);
  for (const key of ["deltas", "delta", "final_deltas", "score_deltas"]) {
    const values = result?.[key];
    if (Array.isArray(values)) {
      const delta = resultNumber(values[seat]) ?? resultNumber(values[index]);
      if (delta !== undefined) return delta;
    }
  }
  return undefined;
}

function resultScore(
  player: Record<string, unknown>,
  index: number,
  result: Record<string, unknown> | null,
  fallbackSeat?: number,
): number | undefined {
  const direct = resultNumber(player.final_score);
  if (direct !== undefined) return direct;
  const scores = result?.final_scores;
  if (!Array.isArray(scores)) return undefined;
  const seat = resultSeat(player, fallbackSeat, index);
  return resultNumber(scores[seat]) ?? resultNumber(scores[index]);
}

export function ResultsPanel({
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
      <section className="results-panel" data-testid="results-panel" aria-labelledby="results-heading">
        <p className="eyebrow">POST-MATCH</p>
        <h2 id="results-heading">Match complete</h2>
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
  const roomRoster = roster(room);
  const firstRoster = roomRoster.find(
    (entry) => entry.participant_id === firstId,
  );
  const firstName =
    typeof first.display_name === "string" ? first.display_name : "First place";
  const firstAsset = firstRoster?.character_id;
  const firstSeat = firstRoster?.seat;
  const firstScore = resultScore(first, 0, result, firstSeat);
  const firstDelta = resultDelta(first, 0, result, firstSeat);
  const mode =
    typeof room?.game_mode === "string" && room.game_mode.trim().length > 0
      ? room.game_mode
      : typeof result?.mode === "string"
        ? result.mode
        : undefined;
  return (
    <section className="results-panel" data-testid="results-panel" aria-labelledby="results-heading">
      <div className="results-heading">
        <div>
          <p className="eyebrow">POST-MATCH / FINAL</p>
          <h2 id="results-heading">Standings</h2>
        </div>
        <span className="state-label">{ordered.length} PLAYERS</span>
      </div>
      <div
        className="results-winner"
        role="group"
        aria-label={`First place: ${firstName}`}
      >
        <ResultPortrait
          characterId={firstAsset}
          name={firstName}
          available={firstAsset ? assets[firstAsset] !== false : false}
          className="results-winner-hero-portrait"
        />
        <div className="results-winner-copy">
          <span className="results-winner-rank">01 / FIRST PLACE</span>
          <h3>{firstName}</h3>
          <strong>{firstScore === undefined ? "—" : firstScore.toLocaleString()}</strong>
          <span>FINAL POINTS{firstDelta === undefined ? "" : ` · ${firstDelta >= 0 ? "+" : ""}${firstDelta.toLocaleString()} DELTA`}</span>
        </div>
      </div>
      <dl className="results-facts" aria-label="Final match facts">
        {mode && <div><dt>Mode</dt><dd>{mode}</dd></div>}
        {typeof room?.replay_available === "boolean" && (
          <div><dt>Replay</dt><dd>{room.replay_available ? "Available" : "Not saved"}</dd></div>
        )}
      </dl>
      <ol className="results-list" aria-label="Final standings">
        {ordered.map((player, index) => {
          const id =
            typeof player.participant_id === "string"
              ? player.participant_id
              : `${index}`;
          const name =
            typeof player.display_name === "string"
              ? player.display_name
              : "Unknown player";
          const rosterEntry = roomRoster.find(
            (entry) => entry.participant_id === id,
          );
          const score = resultScore(player, index, result, rosterEntry?.seat);
          const delta = resultDelta(player, index, result, rosterEntry?.seat);
          const controller = rosterEntry?.controller ?? "";
          const characterId = rosterEntry?.character_id;
          const autoLabel = controller.includes("permanent_auto")
            ? "Permanent Auto"
            : controller.includes("temporary_auto")
              ? "Temporary Auto"
              : undefined;
          const rank = Number(player.rank ?? index + 1);
          return (
            <li key={id} className={rank === 1 ? "is-first" : undefined}>
              <span className="result-rank" aria-label={`Rank ${rank}`}>
                {String(rank).padStart(2, "0")}
              </span>
              <ResultPortrait
                characterId={characterId}
                name={name}
                available={characterId ? assets[characterId] !== false : false}
                className="results-list-portrait"
              />
              <span className="result-name">
                {name}
                {autoLabel && <small>{autoLabel}</small>}
              </span>
              <span className="result-score">
                <strong>{score === undefined ? "—" : score.toLocaleString()}</strong>
                {delta !== undefined && <small className={delta >= 0 ? "is-positive" : "is-negative"}>{delta >= 0 ? "+" : ""}{delta.toLocaleString()}</small>}
              </span>
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
  const sceneLayout = useMemo(
    () => (projection ? buildMatchSceneLayout(projection, room) : null),
    [projection, room],
  );
  const riichiMode = grouped.riichiDiscard.length > 0;
  const legalDiscardActions = riichiMode
    ? grouped.riichiDiscard
    : grouped.discard;
  const [portraitEffect, setPortraitEffect] = useState<RoundWinEffect | null>(
    null,
  );
  const portraitEventRef = useRef<number | null>(null);
  useEffect(() => {
    if (portraitEventRef.current === eventToken) return;
    portraitEventRef.current = eventToken;
    const candidate = portraitFromEvents(storeEvents, room, projection);
    if (candidate) setPortraitEffect(candidate);
  }, [eventToken, projection, room, storeEvents]);
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
  const supported = useGameplayViewportSupport();
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
            <ThreeTable
              projection={projection}
              room={room}
              animations={animations}
              reducedMotion={reducedMotion}
              portraitEffect={portraitEffect}
              onAnimationConsumed={(id: number) =>
                useGameStore.getState().consumeAnimations([id])
              }
              onAnimationCancelled={(id: number) =>
                useGameStore.getState().cancelAnimations([id])
              }
              surface="live"
            />
            <TileHitLayer
              hand={ownPlayer?.hand ?? []}
              actions={legalDiscardActions}
              layout={sceneLayout}
              disabled={inputDisabled}
              onAction={submit}
            />
            {portraitEffect && room?.phase !== "post_match" && (
              <div className="round-win-overlay">
                <RoundWinSurface
                  effect={portraitEffect}
                  assets={assets}
                  reducedMotion={reducedMotion}
                />
              </div>
            )}
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
