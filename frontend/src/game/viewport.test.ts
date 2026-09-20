import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  GAMEPLAY_VIEWPORT_QUERY,
  isGameplayViewportSupported,
  useGameplayViewportSupport,
} from "./viewport";

describe("gameplay viewport gate", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("accepts the exact minimum landscape viewport and rejects smaller dimensions", () => {
    expect(isGameplayViewportSupported(1024, 600)).toBe(true);
    expect(isGameplayViewportSupported(1023, 600)).toBe(false);
    expect(isGameplayViewportSupported(1024, 599)).toBe(false);
  });

  it("reacts to both matchMedia changes and window resize", () => {
    const listeners = new Set<() => void>();
    const query = {
      matches: false,
      media: GAMEPLAY_VIEWPORT_QUERY,
      addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
    } as unknown as MediaQueryList;
    vi.spyOn(window, "matchMedia").mockReturnValue(query);
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1024 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 600 });

    const { result } = renderHook(() => useGameplayViewportSupport());
    expect(result.current).toBe(true);

    Object.defineProperty(window, "innerWidth", { configurable: true, value: 900 });
    act(() => window.dispatchEvent(new Event("resize")));
    expect(result.current).toBe(false);

    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1024 });
    act(() => {
      (query as unknown as { matches: boolean }).matches = true;
      listeners.forEach((listener) => listener());
    });
    expect(result.current).toBe(true);
  });
});
