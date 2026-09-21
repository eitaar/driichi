export type ProjectionAudience = "player" | "public" | "replay_admin";

export interface ProjectedMeld {
  tiles?: number[];
  opened?: boolean;
  from_who?: number | null;
  called_tile?: number | null;
  [key: string]: unknown;
}

export interface ProjectedPlayer {
  seat: number;
  participant_id: string;
  display_name: string;
  kind?: string;
  score?: number;
  hand?: number[];
  concealed_count?: number;
  discards?: number[];
  melds?: ProjectedMeld[];
  riichi?: boolean;
  [key: string]: unknown;
}

export interface VisibleAction {
  action_id: string;
  action: unknown;
}

export interface ProjectedDecision {
  decision_id: string;
  kind: "turn" | "response" | string;
  actions: VisibleAction[];
  default_action_id?: string;
  duration_ms?: number | null;
  remaining_ms?: number | null;
  watchdog?: boolean;
  [key: string]: unknown;
}

/** The server's projected view. Unknown fields remain available to the renderer. */
export interface ProjectedState {
  audience?: ProjectionAudience | string;
  viewer_seat?: number;
  mode?: string;
  player_count?: number;
  players?: ProjectedPlayer[];
  dora_indicators?: number[];
  decision?: ProjectedDecision | null;
  [key: string]: unknown;
}

export interface RoomPlayerSnapshot {
  participant_id: string;
  display_name: string;
  kind: string;
  seat: number;
  character_id: string | null;
  controller: string;
  [key: string]: unknown;
}

export interface RoomParticipantSnapshot {
  participant_id: string;
  display_name: string;
  kind: string;
  presence: string;
  selected: boolean;
  ready: boolean;
  character_id: string;
  role: string;
  controller: string;
  [key: string]: unknown;
}

export interface RoomSnapshot {
  join_code: string;
  room_name: string;
  game_mode: string;
  phase: string;
  revision: number;
  participants: RoomParticipantSnapshot[];
  match_players: RoomPlayerSnapshot[];
  roster: RoomPlayerSnapshot[];
  result: unknown;
  [key: string]: unknown;
}

export type WebSocketStatus = "connecting" | "connected" | "reconnecting" | "closed" | "error";

export interface PendingAction {
  decisionId: string;
  actionId: string;
}

export interface GameEventEnvelope {
  type?: string;
  event?: unknown;
  events?: unknown[];
  [key: string]: unknown;
}
