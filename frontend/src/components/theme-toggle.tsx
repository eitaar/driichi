import { Monitor } from "@phosphor-icons/react/dist/csr/Monitor";
import { Sun } from "@phosphor-icons/react/dist/csr/Sun";
import { Moon } from "@phosphor-icons/react/dist/csr/Moon";
import { Button } from "./ui/button";
import { useTheme } from "./theme-provider";

const nextTheme = { system: "light", light: "dark", dark: "system" } as const;
const labels = { system: "System", light: "Light", dark: "Dark" };

export function ThemeToggle(): React.JSX.Element {
  const { preference, setPreference } = useTheme();
  const next = nextTheme[preference];
  const label = `Theme: ${labels[preference]}. Switch to ${labels[next]}`;
  return <Button type="button" variant="ghost" size="icon" className="theme-toggle" aria-label={label} title={label} onClick={() => setPreference(next)}>
    {preference === "system" ? <Monitor aria-hidden="true" /> : preference === "light" ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
  </Button>;
}
