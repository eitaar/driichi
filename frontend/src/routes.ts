export type Route =
  | { kind: "entry" }
  | { kind: "room"; joinCode: string }
  | { kind: "admin" }
  | { kind: "not-found" };

export function routeForPath(pathname: string): Route {
  if (pathname === "/" || pathname === "") return { kind: "entry" };
  const room = pathname.match(/^\/room\/(\d{6})\/?$/);
  if (room) return { kind: "room", joinCode: room[1] };
  if (/^\/admin\/login\/?$/.test(pathname)) return { kind: "admin" };
  return { kind: "not-found" };
}

export function navigate(path: string) {
  if (`${window.location.pathname}${window.location.search}` !== path) {
    window.history.pushState({}, "", path);
  }
  window.dispatchEvent(new PopStateEvent("popstate"));
}
