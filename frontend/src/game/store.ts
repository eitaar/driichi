import { create } from "zustand";
import { decodeKyokuEndStep } from "./results";
import {
  animationEvents,
  animationKindForEvent,
  enqueueAnimationEvents,
  type AnimationItem,
} from "./animation";
import type {
  GameEventEnvelope,
  PendingAction,
  ProjectedState,
  ResultStepId,
  RoomSnapshot,
  WebSocketStatus,
} from "./types";

/** A transport returns false when the message could not be sent on an open connection. */
export type Transport = (message: unknown) => unknown;

type ActionResult = {
  decision_id?: string;
  action_id?: string;
  status?: string;
  code?: string;
};

export interface GameStoreState {
  status: WebSocketStatus;
  reason: string;
  room: RoomSnapshot | null;
  projection: ProjectedState | null;
  commandError: string;
  actionError: string;
  pendingAction: PendingAction | null;
  /** Local-only acknowledgement for the currently displayed result step. */
  confirmedResultStep: ResultStepId | null;
  /** Local monotonic deadline capped on every receipt of the same server step. */
  resultDeadline: number | null;
  animationQueue: AnimationItem[];
  /** Monotonic queue identity prevents a just-consumed motion id from racing a new event. */
  nextAnimationId: number;
  connectionGeneration: number;
  lastRevision: number | null;
  lastEventToken: number;
  lastEvents: unknown[];
  lastActionResult: ActionResult | null;
  actionResultHistory: Array<Pick<ActionResult, "decision_id" | "action_id" | "status">>;
  animationEnqueuedCount: number;
  animationConsumedCount: number;
  transport?: Transport | null;
  setStatus: (status: WebSocketStatus, reason?: string) => void;
  reset: () => void;
  resetForReconnect: () => void;
  preserveForTerminal: (reason: string) => void;
  setTransport: (transport: Transport | null) => void;
  submitAction: (decisionId: string, actionId: string, transport?: Transport) => boolean;
  confirmResult: (stepId: ResultStepId, transport?: Transport) => boolean;
  receiveSnapshot: (room: RoomSnapshot | null, projection: unknown) => void;
  receiveUpdate: (room: RoomSnapshot | null, projection: unknown, envelope?: GameEventEnvelope | unknown) => void;
  receiveActionResult: (result: ActionResult) => void;
  setCommandError: (error: string) => void;
  clearPendingAction: () => void;
  consumeAnimations: (ids?: number[]) => void;
  cancelAnimations: (ids: number[]) => void;
}

const initialState = {
  status: "connecting" as WebSocketStatus,
  reason: "",
  room: null,
  projection: null,
  commandError: "",
  actionError: "",
  pendingAction: null,
  confirmedResultStep: null as ResultStepId | null,
  resultDeadline: null as number | null,
  animationQueue: [] as AnimationItem[],
  nextAnimationId: 0,
  connectionGeneration: 0,
  lastRevision: null,
  lastEventToken: 0,
  lastEvents: [] as unknown[],
  lastActionResult: null as ActionResult | null,
  actionResultHistory: [] as Array<Pick<ActionResult, "decision_id" | "action_id" | "status">>,
  animationEnqueuedCount: 0,
  animationConsumedCount: 0,
  transport: null as Transport | null,
};

function projectedState(value: unknown): ProjectedState | null {
  return value && typeof value === "object" ? value as ProjectedState : null;
}

function numericRevision(room: RoomSnapshot | null): number | null {
  return room && typeof room.revision === "number" && Number.isFinite(room.revision) ? room.revision : null;
}

function isDiscontinuous(previous: number | null, next: number | null): boolean {
  return previous !== null && next !== null && (next < previous || next > previous + 1);
}

function stepIdFromRoom(room: RoomSnapshot | null): ResultStepId | null {
  return decodeKyokuEndStep(room?.kyoku_end, room?.game_mode ?? "")?.id ?? null;
}

function sameStep(left: ResultStepId | null, right: ResultStepId | null): boolean {
  return left !== null && right !== null
    && left.match_id === right.match_id
    && left.kyoku_sequence === right.kyoku_sequence
    && left.step_index === right.step_index;
}

function resultDeadlineFor(room: RoomSnapshot | null, state: GameStoreState): number | null {
  const step = decodeKyokuEndStep(room?.kyoku_end, room?.game_mode ?? "");
  if (!step) return null;
  const deadline = performance.now() + step.remaining_ms;
  return state.resultDeadline !== null && sameStep(stepIdFromRoom(state.room), step.id)
    ? Math.min(state.resultDeadline, deadline)
    : deadline;
}

export const useGameStore = create<GameStoreState>((set, get) => ({
  ...initialState,
  setStatus: (status, reason = "") => set({ status, reason }),
  reset: () => set({ ...initialState, lastEventToken: get().lastEventToken + 1 }),
  resetForReconnect: () => set((state) => ({
    status: "reconnecting",
    reason: "",
    // Keep the last authoritative scene mounted while the transport retries.
    room: state.room,
    projection: state.projection,
    commandError: "",
    actionError: "",
    pendingAction: null,
    confirmedResultStep: null,
    animationQueue: [],
    lastRevision: null,
    lastEvents: [],
    lastActionResult: null,
    actionResultHistory: [],
    animationEnqueuedCount: 0,
    animationConsumedCount: 0,
    lastEventToken: state.lastEventToken + 1,
  })),
  preserveForTerminal: (reason) => set((state) => ({
    status: "closed",
    reason,
    // Terminal UI is rendered by GameplaySurface over this authoritative scene.
    room: state.room,
    projection: state.projection,
    commandError: "",
    actionError: "",
    pendingAction: null,
    confirmedResultStep: null,
    animationQueue: [],
    lastEvents: [],
    lastActionResult: null,
    actionResultHistory: [],
    transport: null,
    lastEventToken: state.lastEventToken + 1,
  })),
  setTransport: (transport) => set({ transport }),
  submitAction: (decisionId, actionId, transport) => {
    const state = get();
    if (state.pendingAction || !decisionId || !actionId) return false;
    const send = transport ?? state.transport;
    if (!send) return false;
    const message = { type: "submit_action", decision_id: decisionId, action_id: actionId };
    try {
      if (send(message) === false) return false;
    } catch {
      return false;
    }
    set({ pendingAction: { decisionId, actionId }, actionError: "", commandError: "" });
    return true;
  },
  confirmResult: (stepId, transport) => {
    const state = get();
    if (state.status !== "connected" || !sameStep(stepIdFromRoom(state.room), stepId)
      || state.resultDeadline === null || performance.now() >= state.resultDeadline
      || sameStep(state.confirmedResultStep, stepId)) return false;
    const send = transport ?? state.transport;
    if (!send) return false;
    const message = { type: "confirm_result", step_id: stepId };
    try {
      if (send(message) === false) return false;
    } catch {
      return false;
    }
    set({ confirmedResultStep: { ...stepId } });
    return true;
  },
  receiveSnapshot: (room, projection) => set((state) => {
    const nextProjection = projectedState(projection);
    const sameOpenDecision = Boolean(state.actionError)
      && state.projection?.decision?.decision_id !== undefined
      && state.projection.decision.decision_id === nextProjection?.decision?.decision_id;
    const serverStep = stepIdFromRoom(room);
    return {
      room,
      resultDeadline: resultDeadlineFor(room, state),
      projection: nextProjection,
      animationQueue: [],
      pendingAction: null,
      confirmedResultStep: sameStep(state.confirmedResultStep, serverStep) ? state.confirmedResultStep : null,
      actionError: sameOpenDecision ? state.actionError : "",
      commandError: "",
      lastRevision: numericRevision(room),
      lastEvents: [],
      lastEventToken: state.lastEventToken + 1,
    };
  }),
  receiveUpdate: (room, projection, envelope) => set((state) => {
    const nextRevision = numericRevision(room);
    const discontinuity = isDiscontinuous(state.lastRevision, nextRevision);
    const events = animationEvents(envelope);
    const enqueuedEvents = events.filter((event) => animationKindForEvent(event)).length;
    const result = discontinuity
      ? { queue: [], overflow: false }
      : enqueueAnimationEvents(state.animationQueue, events, state.nextAnimationId);
    const projectionValue = projection === undefined ? state.projection : projectedState(projection);
    const pending = state.pendingAction && projectionValue?.decision?.decision_id !== state.pendingAction.decisionId
      ? null
      : state.pendingAction;
    const serverStep = stepIdFromRoom(room ?? state.room);
    return {
      room: room ?? state.room,
      resultDeadline: resultDeadlineFor(room ?? state.room, state),
      projection: projectionValue,
      confirmedResultStep: sameStep(state.confirmedResultStep, serverStep) ? state.confirmedResultStep : null,
      animationQueue: result.queue,
      nextAnimationId: state.nextAnimationId
        + (!discontinuity ? enqueuedEvents : 0),
      pendingAction: pending,
      actionError: pending ? state.actionError : "",
      lastRevision: nextRevision ?? state.lastRevision,
      lastEvents: events,
      animationEnqueuedCount:
        state.animationEnqueuedCount +
        (!discontinuity && !result.overflow ? enqueuedEvents : 0),
      lastEventToken: state.lastEventToken + 1,
    };
  }),
  receiveActionResult: (result) => set((state) => {
    const pending = state.pendingAction;
    const matching = pending !== null
      && typeof result.decision_id === "string"
      && typeof result.action_id === "string"
      && result.decision_id === pending.decisionId
      && result.action_id === pending.actionId;
    if (!matching) return state;
    if (result.status === "accepted") {
      const actionResultHistory = [
        ...state.actionResultHistory.filter(
          (entry) => entry.action_id !== result.action_id,
        ),
        {
          decision_id: result.decision_id,
          action_id: result.action_id,
          status: result.status,
        },
      ].slice(-64);
      return {
        pendingAction: null,
        actionError: "",
        commandError: "",
        lastActionResult: result,
        actionResultHistory,
      };
    }
    if (result.status === "rejected") {
      const actionResultHistory = [
        ...state.actionResultHistory.filter(
          (entry) => entry.action_id !== result.action_id,
        ),
        {
          decision_id: result.decision_id,
          action_id: result.action_id,
          status: result.status,
        },
      ].slice(-64);
      return {
        pendingAction: null,
        actionError: result.code ?? "action_rejected",
        lastActionResult: result,
        actionResultHistory,
      };
    }
    return state;
  }),
  setCommandError: (error) => set({ commandError: error }),
  clearPendingAction: () => set({ pendingAction: null }),
  consumeAnimations: (ids) => set((state) => {
    if (!ids || ids.length === 0) {
      return {
        animationQueue: [],
        animationConsumedCount:
          state.animationConsumedCount + state.animationQueue.length,
      };
    }
    const remove = new Set(ids);
    const consumed = state.animationQueue.filter((item) => remove.has(item.id)).length;
    return {
      animationQueue: state.animationQueue.filter((item) => !remove.has(item.id)),
      animationConsumedCount: state.animationConsumedCount + consumed,
    };
  }),
  cancelAnimations: (ids) => set((state) => {
    const remove = new Set(ids);
    return {
      animationQueue: state.animationQueue.filter((item) => !remove.has(item.id)),
    };
  }),
}));

export const gameStore = useGameStore;

export function createGameStore() {
  return useGameStore;
}

/** Keep a single transport field out of the public state shape while allowing external WS tests to drive it. */
export function sendGameAction(
  decisionId: string,
  actionId: string,
  transport: Transport,
): boolean {
  return useGameStore.getState().submitAction(decisionId, actionId, transport);
}
