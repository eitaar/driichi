import { create } from "zustand";
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
  animationQueue: AnimationItem[];
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
  animationQueue: [] as AnimationItem[],
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
  receiveSnapshot: (room, projection) => set((state) => {
    const nextProjection = projectedState(projection);
    const sameOpenDecision = Boolean(state.actionError)
      && state.projection?.decision?.decision_id !== undefined
      && state.projection.decision.decision_id === nextProjection?.decision?.decision_id;
    return {
      room,
      projection: nextProjection,
      animationQueue: [],
      pendingAction: null,
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
      : enqueueAnimationEvents(state.animationQueue, events);
    const projectionValue = projection === undefined ? state.projection : projectedState(projection);
    const pending = state.pendingAction && projectionValue?.decision?.decision_id !== state.pendingAction.decisionId
      ? null
      : state.pendingAction;
    return {
      room: room ?? state.room,
      projection: projectionValue,
      animationQueue: result.queue,
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
    const matching = !state.pendingAction
      || (result.action_id
        ? result.action_id === state.pendingAction.actionId
        : (!result.decision_id || result.decision_id === state.pendingAction.decisionId));
    if (!matching) return state;
    if (result.status === "accepted") {
      const actionResultHistory = result.action_id
        ? [
            ...state.actionResultHistory.filter(
              (entry) => entry.action_id !== result.action_id,
            ),
            {
              decision_id: state.pendingAction?.decisionId ?? result.decision_id,
              action_id: result.action_id,
              status: result.status,
            },
          ].slice(-64)
        : state.actionResultHistory;
      return {
        pendingAction: null,
        actionError: "",
        commandError: "",
        lastActionResult: result,
        actionResultHistory,
      };
    }
    if (result.status === "rejected") {
      const actionResultHistory = result.action_id
        ? [
            ...state.actionResultHistory.filter(
              (entry) => entry.action_id !== result.action_id,
            ),
            {
              decision_id: state.pendingAction?.decisionId ?? result.decision_id,
              action_id: result.action_id,
              status: result.status,
            },
          ].slice(-64)
        : state.actionResultHistory;
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
