import { act, fireEvent, render, screen } from "@testing-library/react";
import { lazy, useEffect } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import { DesktopTableBoundary, DesktopTableFallback } from "./desktop-table-boundary";

function resize(width: number, height = 700) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height });
  act(() => window.dispatchEvent(new Event("resize")));
}
beforeEach(() => resize(390));

it("mobile viewer does not mount renderer or fetch confidential observer", () => {
  const resources = vi.fn();
  const load = vi.fn(async () => ({ default: () => { resources(); return <canvas />; } }));
  const Viewer = lazy(load);
  render(<DesktopTableBoundary fallback={<p>Room status</p>}><Viewer /></DesktopTableBoundary>);
  expect(screen.getByText("Room status")).toBeVisible();
  expect(load).not.toHaveBeenCalled();
  expect(resources).not.toHaveBeenCalled();
});

it("resize to unsupported viewport disposes viewer and preserves dashboard", () => {
  resize(1024, 600);
  const observe = vi.fn(); const dispose = vi.fn();
  function Viewer() { useEffect(() => { observe(); return dispose; }, []); return <canvas aria-label="Live table" />; }
  render(<><h1>Dashboard</h1><DesktopTableBoundary fallback={<p>Desktop required</p>}><Viewer /></DesktopTableBoundary></>);
  expect(observe).toHaveBeenCalledOnce();
  resize(1023, 600);
  expect(dispose).toHaveBeenCalledOnce();
  expect(screen.getByRole("heading", { name: "Dashboard" })).toBeVisible();
  expect(screen.queryByLabelText("Live table")).not.toBeInTheDocument();
  resize(1024, 600);
  expect(observe).toHaveBeenCalledTimes(2);
});

it("3D-only deep link has desktop limitation and safe Back", () => {
  window.history.replaceState({}, "", "/admin/replays/example");
  render(<DesktopTableBoundary fallback={<DesktopTableFallback backHref="/admin/replays" />}><canvas /></DesktopTableBoundary>);
  expect(screen.getByText(/1024.*600/)).toBeVisible();
  fireEvent.click(screen.getByRole("link", { name: /Back/ }));
  expect(window.location.pathname).toBe("/admin/replays");
});
