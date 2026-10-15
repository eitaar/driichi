import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { App } from "../app";
import { AppShell } from "./app-shell";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

it("retains confirmed success toast across navigation", async () => {
  window.history.replaceState({}, "", "/admin/login");
  vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(new Response("[]"))));
  render(<App />);
  fireEvent.change(screen.getByLabelText("Username"), { target: { value: "host" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
  expect(await screen.findByText("Signed in")).toBeVisible();
  fireEvent.click(screen.getByRole("link", { name: "Open Admin" }));
  expect(await screen.findByRole("heading", { name: "Rooms" })).toBeVisible();
  expect(screen.getByText("Signed in")).toBeVisible();
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
  fireEvent.click(screen.getByRole("button", { name: "Toggle navigation" }));
  expect(await screen.findByRole("dialog")).toBeVisible();
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("link", { name: "Replays" }));
  expect(window.location.pathname).toBe("/admin/replays");
});

it.each([["/admin", "Rooms"], ["/admin/benchmark/runs/run-1", "Benchmarks"], ["/admin/replays/match-1", "Replays"]])("marks the selected Admin section at %s", async (path, label) => {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
  window.history.replaceState({}, "", path);
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("[]")));
  render(<QueryClientProvider client={new QueryClient()}><AppShell variant="admin"><p>Workspace</p></AppShell></QueryClientProvider>);
  const nav = await screen.findByRole("navigation", { name: "Admin" });
  expect(within(nav).getByRole("link", { name: label })).toHaveAttribute("data-active");
  expect(within(nav).getAllByRole("link").filter(link => link.hasAttribute("data-active"))).toHaveLength(1);
});
