export type Route =
  | { kind: "entry" }
  | { kind: "room"; joinCode: string }
  | { kind: "lobby"; joinCode: string }
  | { kind: "admin-login" }
  | { kind: "admin-replays" }
  | { kind: "admin-replay"; matchId: string }
  | { kind: "admin"; joinCode?: string }
  | { kind: "not-found" };

export function routeForPath(pathname: string): Route {
  if (pathname === "/" || pathname === "") return { kind: "entry" };
  const lobby = pathname.match(/^\/room\/(\d{6})\/lobby\/?$/);
  if (lobby) return { kind: "lobby", joinCode: lobby[1] };
  const room = pathname.match(/^\/room\/(\d{6})\/?$/);
  if (room) return { kind: "room", joinCode: room[1] };
  if (/^\/admin\/login\/?$/.test(pathname)) return { kind: "admin-login" };
  const replay = pathname.match(/^\/admin\/replays\/([^/]+)\/?$/);
  if (replay) {
    try { return { kind: "admin-replay", matchId: decodeURIComponent(replay[1]) }; }
    catch { return { kind: "not-found" }; }
  }
  if (/^\/admin\/replays\/?$/.test(pathname)) return { kind: "admin-replays" };
  const admin = pathname.match(/^\/admin(?:\/rooms\/(\d{6}))?\/?$/);
  if (admin) return { kind: "admin", joinCode: admin[1] };
  return { kind: "not-found" };
}

export function navigate(path: string) {
  if (`${window.location.pathname}${window.location.search}` !== path) window.history.pushState({}, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}
