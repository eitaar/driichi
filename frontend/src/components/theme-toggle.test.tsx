import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import { ThemeProvider } from "./theme-provider";
import { ThemeToggle } from "./theme-toggle";

beforeEach(() => localStorage.removeItem("driichi-theme"));

it("cycles_system_light_dark_system", async () => {
  const user = userEvent.setup();
  render(<ThemeProvider><ThemeToggle /></ThemeProvider>);
  for (const preference of ["light", "dark", "system"]) {
    await user.click(screen.getByRole("button"));
    expect(localStorage.getItem("driichi-theme")).toBe(preference);
  }
});

it("announces_current_and_next_theme", async () => {
  const user = userEvent.setup();
  render(<ThemeProvider><ThemeToggle /></ThemeProvider>);
  for (const label of ["Theme: System. Switch to Light", "Theme: Light. Switch to Dark", "Theme: Dark. Switch to System"]) {
    const button = screen.getByRole("button", { name: label });
    expect(button).toHaveAttribute("title", label);
    expect(button).toHaveAttribute("type", "button");
    expect(button).toHaveAttribute("data-slot", "button");
    await user.click(button);
  }
  expect(screen.getByRole("button", { name: "Theme: System. Switch to Light" })).toBeVisible();
});
