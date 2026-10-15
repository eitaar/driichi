import type { QueryClient } from "@tanstack/react-query";
import { api } from "./api";
import { navigate } from "./routes";

export function clearAdminSession(client: QueryClient) {
  window.dispatchEvent(new Event("admin-session-cleared"));
  void client.cancelQueries({ queryKey: ["admin"] }, { silent: true });
  client.removeQueries({ queryKey: ["admin"] });
  navigate("/admin/login");
}

export function signOutAdmin(client: QueryClient) {
  clearAdminSession(client);
  void api.logoutAdmin().catch(() => {});
}
