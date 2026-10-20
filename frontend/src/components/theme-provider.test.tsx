import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { ThemeProvider, useTheme } from "./theme-provider";
import { useTableAppearance } from "../game/table-appearance";

function Consumer() {
  const { preference, setPreference } = useTheme();
  const { appearance } = useTableAppearance();
  return <>
    <output aria-label="Theme preference">{preference}</output>
    <output aria-label="Scene appearance">{JSON.stringify(appearance)}</output>
    <button onClick={() => setPreference("light")}>Light</button>
    <button onClick={() => setPreference("dark")}>Dark</button>
    <button onClick={() => setPreference("system")}>System</button>
  </>;
}

let scheme: MediaQueryList;
beforeEach(() => {
  localStorage.clear();
  document.documentElement.className = "";
  document.documentElement.style.colorScheme = "";
  scheme = Object.assign(new EventTarget(), { matches: true, media: "(prefers-color-scheme: dark)" }) as MediaQueryList;
  vi.spyOn(window, "matchMedia").mockReturnValue(scheme);
});

// Catches ignoring OS changes, explicit choices, or the consumer context.
test("uses system scheme until explicit choice", () => {
  render(<ThemeProvider><Consumer /></ThemeProvider>);
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("system");
  expect(document.documentElement).toHaveClass("dark");
  expect(document.documentElement.style.colorScheme).toBe("dark");
  act(() => {
    Object.assign(scheme, { matches: false });
    scheme.dispatchEvent(new Event("change"));
  });
  expect(document.documentElement).not.toHaveClass("dark");
  expect(document.documentElement.style.colorScheme).toBe("light");
  fireEvent.click(screen.getByRole("button", { name: "Dark" }));
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("dark");
  act(() => scheme.dispatchEvent(new Event("change")));
  expect(document.documentElement).toHaveClass("dark");
  fireEvent.click(screen.getByRole("button", { name: "System" }));
  expect(document.documentElement.style.colorScheme).toBe("light");
});

// Catches storage errors preventing rendering or in-memory preference updates.
test("survives rejected localStorage access", () => {
  vi.spyOn(window, "localStorage", "get").mockImplementation(() => { throw new DOMException("Denied", "SecurityError"); });
  render(<ThemeProvider><Consumer /></ThemeProvider>);
  fireEvent.click(screen.getByRole("button", { name: "Light" }));
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("light");
  expect(document.documentElement.style.colorScheme).toBe("light");
});

// Catches missing persistence/cross-tab updates and accepting invalid external data.
test("synchronizes explicit cross-tab preference", () => {
  const first = render(<ThemeProvider><Consumer /></ThemeProvider>);
  fireEvent.click(screen.getByRole("button", { name: "Light" }));
  first.unmount();
  render(<ThemeProvider><Consumer /></ThemeProvider>);
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("light");
  const key = Object.keys(localStorage)[0];
  act(() => window.dispatchEvent(new StorageEvent("storage", { key, newValue: "dark", storageArea: localStorage })));
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("dark");
  expect(document.documentElement.style.colorScheme).toBe("dark");
  act(() => window.dispatchEvent(new StorageEvent("storage", { key, newValue: "invalid", storageArea: localStorage })));
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("system");
  act(() => window.dispatchEvent(new StorageEvent("storage", { key, newValue: "light", storageArea: localStorage })));
  act(() => window.dispatchEvent(new StorageEvent("storage", { key: null, storageArea: localStorage })));
  expect(screen.getByLabelText("Theme preference")).toHaveTextContent("system");
});

// Catches coupling shell theme to the actual scene appearance hook.
test("does not change scene appearance settings", async () => {
  const image = { width: 1, height: 1, close() {} } as ImageBitmap;
  vi.stubGlobal("createImageBitmap", async () => image);
  const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0]);
  const file = new File([bytes], "table.png", { type: "image/png" });
  Object.defineProperty(file, "slice", { value: () => ({ arrayBuffer: async () => bytes.buffer }) });
  const view = renderHook(() => ({ theme: useTheme(), scene: useTableAppearance() }), { wrapper: ThemeProvider });
  try {
    await act(async () => { await view.result.current.scene.select("surface", file); });
    const selected = view.result.current.scene.appearance.surface;
    expect(selected?.image).toBe(image);
    act(() => view.result.current.theme.setPreference("light"));
    expect(document.documentElement.style.colorScheme).toBe("light");
    expect(view.result.current.scene.appearance.surface).toBe(selected);
    expect(view.result.current.scene.appearance.surround).toBeNull();
  } finally {
    view.unmount();
    vi.unstubAllGlobals();
  }
});
