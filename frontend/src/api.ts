export interface ProblemDetails {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  code?: string;
  request_id?: string;
}

export class ApiProblem extends Error {
  readonly problem: ProblemDetails;
  readonly status: number;

  constructor(problem: ProblemDetails, status = problem.status ?? 0) {
    super(problem.detail ?? problem.title ?? "The request could not be completed.");
    this.name = "ApiProblem";
    this.problem = problem;
    this.status = status;
  }
}

export interface RoomLookup {
  room_name: string;
  game_mode: string;
  phase: string;
  join_allowed: boolean;
  participant_count: number;
  participant_limit: number;
}

export interface HumanCharacter {
  id: string;
  name: string;
}

export interface HumanJoinResponse {
  participant_id: string;
  websocket_url: string;
}

export interface AdminLoginResponse {
  expires_at: string;
}

export type RoomPhase = "lobby" | "playing" | "post_match" | string;
export type GameMode = "4p-red-east" | "4p-red-half" | "3p-red-east" | "3p-red-half";
export type TimeControl = "casual" | "riichi_dev" | "unlimited";

export interface AdminRoomSummary {
  join_code: string;
  room_name: string;
  game_mode: GameMode;
  phase: RoomPhase;
  connected_count: number;
  participant_count: number;
  selected_count: number;
  created_at: string;
}

export interface RoomParticipant {
  participant_id: string;
  display_name: string;
  kind: "human" | "mjai" | "mcp" | "built_in_bot" | string;
  presence: "connected" | "disconnected" | string;
  selected: boolean;
  ready: boolean;
  character_id: string;
  role: string;
  controller: string;
}

export interface MatchPlayer {
  participant_id: string;
  display_name: string;
  kind: string;
  seat: number;
  character_id: string | null;
  controller: string;
}

export interface AdminRoomDetail extends AdminRoomSummary {
  time_control: TimeControl;
  replay_save: boolean;
  participant_limit: number;
  participants: RoomParticipant[];
  match_players: MatchPlayer[];
  roster: MatchPlayer[];
  result: unknown;
  revision: number;
  persistence_degraded: boolean;
  replay_available: boolean;
}

export interface BotTokenRecord {
  token_id: string;
  name: string;
  state: "active" | "revoked" | string;
  created_at: string;
  revoked_at: string | null;
}

export interface CreatedBotToken extends BotTokenRecord {
  token: string;
}

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  if (init?.body !== undefined) headers.set("Content-Type", "application/json");

  let response: Response;
  try {
    response = await fetch(path, { ...init, credentials: "include", headers });
  } catch {
    throw new ApiProblem({
      type: "about:blank",
      title: "Host unavailable",
      detail: "The host could not be reached. Check the address and try again.",
      code: "network_error",
    });
  }

  const text = await response.text();
  let body: unknown;
  if (text.length > 0) {
    try { body = JSON.parse(text); } catch { body = undefined; }
  }
  if (!response.ok) {
    const problem = isProblemDetails(body)
      ? body
      : { type: "about:blank", title: "Request failed", detail: "The host could not complete the request.", code: "request_failed" };
    throw new ApiProblem(problem, response.status);
  }
  return body as T;
}

function isProblemDetails(value: unknown): value is ProblemDetails {
  return typeof value === "object" && value !== null && ("title" in value || "detail" in value);
}

function roomPath(joinCode: string): string {
  if (!/^\d{6}$/.test(joinCode)) throw new ApiProblem({ code: "invalid_room_code", detail: "Enter the six-digit room code." });
  return `/api/v1/rooms/${encodeURIComponent(joinCode)}`;
}

function adminRoomPath(joinCode: string): string {
  if (!/^\d{6}$/.test(joinCode)) throw new ApiProblem({ code: "invalid_room_code" });
  return `/api/v1/admin/rooms/${encodeURIComponent(joinCode)}`;
}

export const api = {
  lookupRoom(joinCode: string) { return requestJson<RoomLookup>(roomPath(joinCode)); },
  listHumanCharacters() { return requestJson<HumanCharacter[]>("/api/v1/characters/human"); },
  joinHuman(joinCode: string, nickname: string, characterId: string) {
    return requestJson<HumanJoinResponse>(`${roomPath(joinCode)}/join`, { method: "POST", body: JSON.stringify({ nickname, character_id: characterId }) });
  },
  loginAdmin(username: string, password: string) {
    return requestJson<AdminLoginResponse>("/api/v1/admin/login", { method: "POST", body: JSON.stringify({ username, password }) });
  },
  logoutAdmin() { return requestJson<void>("/api/v1/admin/logout", { method: "POST" }); },
  listAdminRooms() { return requestJson<AdminRoomSummary[]>("/api/v1/admin/rooms"); },
  getAdminRoom(joinCode: string) { return requestJson<AdminRoomDetail>(adminRoomPath(joinCode)); },
  createAdminRoom(input: { room_name: string; game_mode: GameMode; time_control: TimeControl; replay_save: boolean; participant_limit?: number }) {
    return requestJson<AdminRoomDetail>("/api/v1/admin/rooms", { method: "POST", body: JSON.stringify(input) });
  },
  patchAdminRoom(joinCode: string, input: Partial<{ room_name: string; game_mode: GameMode; time_control: TimeControl; replay_save: boolean; participant_limit: number }>) {
    return requestJson<AdminRoomDetail>(adminRoomPath(joinCode), { method: "PATCH", body: JSON.stringify(input) });
  },
  deleteAdminRoom(joinCode: string) { return requestJson<void>(adminRoomPath(joinCode), { method: "DELETE" }); },
  selectParticipant(joinCode: string, participantId: string) { return adminParticipantCommand(joinCode, participantId, "select"); },
  deselectParticipant(joinCode: string, participantId: string) { return adminParticipantCommand(joinCode, participantId, "deselect"); },
  kickParticipant(joinCode: string, participantId: string) { return adminParticipantCommand(joinCode, participantId, "kick"); },
  fillWithBots(joinCode: string) { return roomCommand(joinCode, "fill-with-bots"); },
  startRoom(joinCode: string) { return roomCommand(joinCode, "start"); },
  rematchRoom(joinCode: string) { return roomCommand(joinCode, "rematch"); },
  backToLobby(joinCode: string) { return roomCommand(joinCode, "back-to-lobby"); },
  listBotTokens() { return requestJson<BotTokenRecord[]>("/api/v1/admin/tokens"); },
  createBotToken(name: string) { return requestJson<CreatedBotToken>("/api/v1/admin/tokens", { method: "POST", body: JSON.stringify({ name }) }); },
  revokeBotToken(tokenId: string) { return requestJson<BotTokenRecord>(`/api/v1/admin/tokens/${encodeURIComponent(tokenId)}/revoke`, { method: "POST" }); },
};

function adminParticipantCommand(joinCode: string, participantId: string, command: "select" | "deselect" | "kick") {
  return requestJson<AdminRoomDetail>(`${adminRoomPath(joinCode)}/participants/${encodeURIComponent(participantId)}/${command}`, { method: "POST" });
}

function roomCommand(joinCode: string, command: "fill-with-bots" | "start" | "rematch" | "back-to-lobby") {
  return requestJson<AdminRoomDetail>(`${adminRoomPath(joinCode)}/${command}`, { method: "POST" });
}

export function problemFrom(error: unknown): ProblemDetails {
  if (error instanceof ApiProblem) return error.problem;
  return { detail: "The host could not complete the request.", code: "request_failed" };
}
