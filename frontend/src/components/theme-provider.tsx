import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type ThemePreference = "system" | "light" | "dark";
type ThemeContextValue = { preference: ThemePreference; setPreference(value: ThemePreference): void };
const ThemeContext = createContext<ThemeContextValue | null>(null);
const storageKey = "driichi-theme";
const parsePreference = (value: string | null): ThemePreference =>
  value === "light" || value === "dark" ? value : "system";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => {
    try { return parsePreference(localStorage.getItem(storageKey)); }
    catch { return "system"; }
  });

  useEffect(() => {
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = preference === "dark" || (preference === "system" && scheme.matches);
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.style.colorScheme = dark ? "dark" : "light";
    };
    apply();
    scheme.addEventListener("change", apply);
    return () => scheme.removeEventListener("change", apply);
  }, [preference]);

  useEffect(() => {
    const synchronize = (event: StorageEvent) => {
      try {
        if (event.storageArea !== localStorage) return;
      } catch { return; }
      if (event.key === storageKey || event.key === null) {
        setPreferenceState(parsePreference(event.newValue));
      }
    };
    window.addEventListener("storage", synchronize);
    return () => window.removeEventListener("storage", synchronize);
  }, []);

  const setPreference = (value: ThemePreference) => {
    setPreferenceState(value);
    try { localStorage.setItem(storageKey, value); }
    catch { /* Storage may be blocked; the in-memory choice still applies. */ }
  };

  return <ThemeContext.Provider value={{ preference, setPreference }}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const theme = useContext(ThemeContext);
  if (!theme) throw new Error("useTheme must be used within ThemeProvider");
  return theme;
}
