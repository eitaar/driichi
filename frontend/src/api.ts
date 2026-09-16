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

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  if (init?.body !== undefined) headers.set("Content-Type", "application/json");

  let response: Response;
  try {
    response = await fetch(path, {
      ...init,
      credentials: "include",
      headers,
    });
  } catch {
    throw new ApiProblem({
      type: "about:blank",
      title: "Host unavailable",
      detail: "The host could not be reached. Check the address and try again.",
      code: "network_error",
    });
  }

  const text = await response.text();
  let body: unknown = undefined;
  if (text.length > 0) {
    try {
      body = JSON.parse(text);
    } catch {
      body = undefined;
    }
  }

  if (!response.ok) {
    const problem = isProblemDetails(body)
      ? body
      : {
          type: "about:blank",
          title: "Request failed",
          detail: "The host could not complete the request.",
          code: "request_failed",
        };
    throw new ApiProblem(problem, response.status);
  }

  return body as T;
}

function isProblemDetails(value: unknown): value is ProblemDetails {
  return typeof value === "object" && value !== null && ("title" in value || "detail" in value);
}

function roomPath(joinCode: string): string {
  if (!/^\d{6}$/.test(joinCode)) throw new ApiProblem({ code: "invalid_room_code" });
  return encodeURI(`/api/v1/rooms/${joinCode}`);
}

export const api = {
  lookupRoom(joinCode: string) {
    return requestJson<RoomLookup>(roomPath(joinCode));
  },

  listHumanCharacters() {
    return requestJson<HumanCharacter[]>("/api/v1/characters/human");
  },

  joinHuman(joinCode: string, nickname: string, characterId: string) {
    return requestJson<HumanJoinResponse>(`${roomPath(joinCode)}/join`, {
      method: "POST",
      body: JSON.stringify({ nickname, character_id: characterId }),
    });
  },

  loginAdmin(username: string, password: string) {
    return requestJson<AdminLoginResponse>("/api/v1/admin/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    });
  },
};
