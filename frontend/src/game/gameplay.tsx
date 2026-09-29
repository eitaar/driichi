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
import { isDora } from "./dora";
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
import "./result.css";

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
            aria-label={action ? `${actionGroupLabel(actionKind(action.action))} ${tileLabel(tile)}` : `Your ${tileLabel(tile)}`}
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
      {!hand?.length && <span className="round-win-hand-note">Some tiles remain private.</span>}
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
        <h2 id="round-win-heading">{effect.displayName}</h2>
      </div>
      <div className="round-win-details" aria-label={`${effect.result} win details`}>
        <RoundWinTiles hand={effect.hand} melds={effect.melds} winningTile={effect.winningTile} />
        {yaku.length > 0 && (
          <dl className="round-win-yaku" aria-label="Authoritative yaku">
            {yaku.map((entry, index) => (
              <div key={`${entry.name}-${index}`}><dt>{yakuLabel(entry.name)}</dt><dd>{entry.han === undefined ? "" : `${entry.han} han`}</dd></div>
            ))}
          </dl>
        )}
        {(effect.han !== undefined || effect.fu !== undefined || effect.limit || effect.points !== undefined) && (
          <div className="round-win-total">
            <p>{effect.han !== undefined && `${effect.han} han`}{effect.fu !== undefined && ` · ${effect.fu} fu`}{effect.limit && ` · ${effect.limit}`}</p>
            {effect.points !== undefined && <strong>{roundPoints(effect.points)} <small>points</small></strong>}
          </div>
        )}
      </div>
    </section>
  );
}

function ScoreTransfer({
  effect,
  room,
  reducedMotion,
}: {
  effect: RoundWinEffect;
  room: RoomSnapshot | null;
  reducedMotion: boolean;
}) {
  const [progress, setProgress] = useState(reducedMotion ? 1 : 0);
  useEffect(() => {
    if (reducedMotion) return;
    const start = performance.now();
    const timer = window.setInterval(() => setProgress(Math.min(1, (performance.now() - start) / 3000)), 50);
    return () => window.clearInterval(timer);
  }, [reducedMotion]);
  return (
    <section className="score-transfer" aria-label="Score transfer" role="status">
      <h2>Score transfer</h2>
      <ol>
        {roster(room).slice().sort((a, b) => a.seat - b.seat).map((player) => {
          const seat = player.seat;
          const final = effect.scores![seat];
          const delta = effect.delta![seat];
          const change = Math.round(delta * (reducedMotion ? 1 : progress));
          return (
            <li key={player.participant_id}>
              <span>{player.display_name}</span>
              <span>{delta >= 0 ? "+" : "−"}{Math.abs(change).toLocaleString()}</span>
              <strong>{(final - delta + change).toLocaleString()}</strong>
            </li>
          );
        })}
      </ol>
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
  const firstRoster = roomRoster.find((entry) => entry.participant_id === firstId);
  const firstName =
    typeof first.display_name === "string" ? first.display_name : "First place";
  const firstAsset = firstRoster?.character_id;

  return (
    <section className="results-panel" data-testid="results-panel" aria-labelledby="results-heading">
      <div className="results-winner">
        <ResultPortrait
          characterId={firstAsset}
          name={firstName}
          available={firstAsset ? assets[firstAsset] !== false : false}
          className="results-winner-hero-portrait"
        />
      </div>
      <div className="results-standings">
        <h2 id="results-heading">Final standings</h2>
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
          const rank = Number(player.rank ?? index + 1);
          return (
            <li key={id} className={rank === 1 ? "is-first" : undefined}>
              <span className="result-rank" aria-label={`Rank ${rank}`}>
                {rank}
              </span>
              <span className="result-name">{name}</span>
              <span className="result-score">{score === undefined ? "—" : score.toLocaleString()}</span>
            </li>
          );
        })}
        </ol>
      </div>
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
  const resultDismissRef = useRef<HTMLButtonElement>(null);
  const [portraitEffect, setPortraitEffect] = useState<RoundWinEffect | null>(
    null,
  );
  const portraitEventRef = useRef<number | null>(null);
  const remainingWins = useRef<RoundWinEffect[]>([]);
  useEffect(() => {
    if (portraitEventRef.current === eventToken) return;
    portraitEventRef.current = eventToken;
    const winners = storeEvents.flatMap((event) =>
      eventKind(event) === "hora" ? [portraitFromEvents([event], room, projection)].filter((value): value is RoundWinEffect => value !== null) : [],
    );
    if (winners.length) {
      remainingWins.current = winners.slice(1);
      setResultStage("win");
      setResultPaused(false);
      setPortraitEffect(winners[0]);
    }
  }, [eventToken, projection, room, storeEvents]);
  const [resultStage, setResultStage] = useState<"win" | "transfer">("win");
  const [resultPaused, setResultPaused] = useState(false);
  useEffect(() => {
    if (!portraitEffect || room?.phase === "post_match" || resultPaused) return;
    const hasTransfer = portraitEffect.scores?.length === roster(room).length &&
      portraitEffect.delta?.length === portraitEffect.scores.length &&
      roster(room).every((player) => typeof player.seat === "number" &&
        player.seat >= 0 && player.seat < portraitEffect.scores!.length);
    const timeout = window.setTimeout(() => {
      if (resultStage === "win" && hasTransfer) setResultStage("transfer");
      else if (remainingWins.current.length) {
        setResultStage("win");
        setPortraitEffect(remainingWins.current.shift()!);
      } else setPortraitEffect(null);
    }, 5000);
    return () => window.clearTimeout(timeout);
  }, [portraitEffect, resultStage, resultPaused]);
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
  const supported = useGameplayViewportSupport();
  const mode = projection?.mode ?? room?.game_mode ?? "4p-red-east";
  const ownController = room?.participants.find((participant) => participant.participant_id === participantId)?.controller
    ?? roster(room).find((player) => player.seat === viewer)?.controller
    ?? "";
  const inputDisabled = Boolean(pending) || status !== "connected" || !decision
    || actionResultHistory.some((result) => result.decision_id === decision.decision_id && result.status === "accepted");
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
            {portraitEffect && room?.phase !== "post_match" && (
              <div className="round-win-overlay">
                {resultStage === "transfer" && portraitEffect.scores && portraitEffect.delta ? (
                  <ScoreTransfer effect={portraitEffect} room={room} reducedMotion={reducedMotion} />
                ) : (
                  <RoundWinSurface effect={portraitEffect} assets={assets} reducedMotion={reducedMotion} />
                )}
                <div className="round-win-actions">
                  <button type="button" className="button" aria-pressed={resultPaused} onClick={() => setResultPaused(!resultPaused)}>
                    {resultPaused ? "Resume" : "Pause for review"}
                  </button>
                  <button type="button" className="button round-win-continue" onClick={() => { remainingWins.current = []; setPortraitEffect(null); }}>
                    Continue
                  </button>
                </div>
              </div>
            )}
            {room?.phase !== "post_match" && (
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
  portraitFromEvents,
  voiceEvents,
  isManganOrHigher,
  formatSeconds,
};
