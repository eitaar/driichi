import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { App } from "../app";
import { AppShell } from "./app-shell";
import { ThemeProvider } from "./theme-provider";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

it("opens the real Admin shell directly after accepted login without an interstitial", async () => {
  window.history.replaceState({}, "", "/admin/login");
  vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(new Response("[]"))));
  render(<App />);
  fireEvent.change(screen.getByLabelText("Username"), { target: { value: "host" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
  expect(await screen.findByRole("heading", { name: "Rooms" })).toBeVisible();
  expect(window.location.pathname).toBe("/admin");
  expect(screen.queryByRole("link", { name: "Open Admin" })).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Password")).not.toBeInTheDocument();
});

it("exposes the persisted theme choice through public navigation", async () => {
  window.history.replaceState({}, "", "/");
  localStorage.removeItem("driichi-theme");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("[]")));
  render(<App />);
  const user = userEvent.setup();
  const nav = screen.getByRole("navigation", { name: "Primary navigation" });
  expect(nav.lastElementChild).toBe(screen.getByRole("button", { name: "Theme: System. Switch to Light" }));
  for (const value of ["light", "dark", "system"]) {
    await user.click(within(nav).getByRole("button", { name: /^Theme:/ }));
    expect(localStorage.getItem("driichi-theme")).toBe(value);
    expect(document.documentElement.classList.contains("dark")).toBe(value === "dark");
  }
});

it("guest navigation contains no Admin-only links", () => {
  window.history.replaceState({}, "", "/");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("[]")));
  render(<App />);
  const nav = screen.getByRole("navigation", { name: "Primary navigation" });
  expect(within(nav).queryByRole("link", { name: /Rooms|Benchmarks|Replays/ })).not.toBeInTheDocument();
  expect(within(nav).getByRole("link", { name: /Admin sign in/ })).toBeVisible();
});

it("admin mobile menu is labeled and navigates without losing the shell", async () => {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 390 });
  window.history.replaceState({}, "", "/admin");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("[]")));
  render(<App />);
  const user = userEvent.setup();
  const trigger = screen.getByRole("button", { name: "Toggle navigation" });
  trigger.focus();
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("dialog")).toBeVisible();
  const link = within(screen.getByRole("dialog")).getByRole("link", { name: "Replays" });
  link.focus();
  await user.keyboard("{Enter}");
  expect(window.location.pathname).toBe("/admin/replays");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(await screen.findAllByRole("button", { name: "Toggle navigation" })).toHaveLength(1);
});

it.each([["/admin", "Rooms"], ["/admin/benchmark/runs/run-1", "Benchmarks"], ["/admin/replays/match-1", "Replays"]])("marks the selected Admin section at %s", async (path, label) => {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
  window.history.replaceState({}, "", path);
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("[]")));
  render(<QueryClientProvider client={new QueryClient()}><ThemeProvider><AppShell variant="admin"><p>Workspace</p></AppShell></ThemeProvider></QueryClientProvider>);
  const nav = await screen.findByRole("navigation", { name: "Admin" });
  expect(within(nav).getByRole("link", { name: label })).toHaveAttribute("data-active");
  expect(within(nav).getAllByRole("link").filter(link => link.hasAttribute("data-active"))).toHaveLength(1);
  expect(within(nav).getByRole("link", { name: label }).querySelector("svg")).not.toBeNull();
  const theme = screen.getByRole("button", { name: /^Theme:/ });
  const header = theme.closest("header")!;
  expect(header.lastElementChild).toContainElement(theme);
  expect(document.querySelector('[data-slot="sidebar-wrapper"]')).toHaveStyle({ "--sidebar-width": "16rem" });
  expect(screen.getByRole("button", { name: "Sign out" }).closest('[data-slot="sidebar-footer"]')).not.toBeNull();
});

it("preserves modified navigation and immediately purges private Admin queries on sign out", () => {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
  window.history.replaceState({}, "", "/admin");
  const client = new QueryClient();
  client.setQueryData(["admin", "private"], { concealed: true });
  client.setQueryData(["public"], "retained");
  const cleared = vi.fn();
  window.addEventListener("admin-session-cleared", cleared, { once: true });
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}")));
  render(<QueryClientProvider client={client}><ThemeProvider><AppShell variant="admin"><p>Workspace</p></AppShell></ThemeProvider></QueryClientProvider>);
  expect(fireEvent.click(screen.getByRole("link", { name: "Benchmarks" }), { ctrlKey: true })).toBe(true);
  expect(window.location.pathname).toBe("/admin");
  fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
  expect(cleared).toHaveBeenCalledOnce();
  expect(client.getQueryData(["admin", "private"])).toBeUndefined();
  expect(client.getQueryData(["public"])).toBe("retained");
  expect(window.location.pathname).toBe("/admin/login");
});
