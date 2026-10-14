import { expect, type Page } from "@playwright/test";

export async function waitForGameplayReady(page: Page): Promise<void> {
  // Lazy gameplay must finish loading before its shell can exist; result clocks start later.
  await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true", { timeout: 30_000 });
  await expect(page.getByTestId("gameplay-shell")).toBeVisible();
}
