import { screen, waitFor, within } from "@testing-library/svelte";
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import { fakeIpc } from "./shell/test-support";
import { start } from "./start";

// The OS theme, which the test can switch.
let osDark = false;
const listeners = new Set<(e: { matches: boolean }) => void>();
vi.stubGlobal("matchMedia", (query: string) => ({
  get matches() {
    return query === "(prefers-color-scheme: dark)" ? osDark : false;
  },
  media: query,
  addEventListener: (_: string, fn: (e: { matches: boolean }) => void) => {
    if (query === "(prefers-color-scheme: dark)") listeners.add(fn);
  },
  removeEventListener: (_: string, fn: (e: { matches: boolean }) => void) =>
    listeners.delete(fn),
}));

function switchOs(dark: boolean): void {
  osDark = dark;
  for (const fn of listeners) fn({ matches: dark });
  flushSync();
}

const root = document.documentElement;

test("starts with the built-in modules and the Welcome view", async () => {
  const log = fakeIpc();
  const target = document.body.appendChild(document.createElement("div"));
  await start(target);

  const rail = screen.getByRole("navigation", { name: "Modules" });
  for (const name of [
    "Requests",
    "Database",
    "Terminal",
    "Mobile",
    "Vault",
    "Settings",
  ])
    expect(within(rail).getByRole("button", { name })).toBeInTheDocument();
  expect(
    await screen.findByRole("heading", { name: "Welcome" }),
  ).toBeInTheDocument();
  expect(root.dataset.theme).toBe("light");
  expect(log.savedUiStates).toEqual([]);
});

test("appearance changes apply at once", async () => {
  await settings.update({ appearance: { theme: "dark", fontSize: 16 } });
  flushSync();
  expect(root.dataset.theme).toBe("dark");
  expect(root.style.getPropertyValue("--font-size")).toBe("16px");
  await settings.update({ appearance: { theme: "light", fontSize: 13 } });
  flushSync();
  expect(root.dataset.theme).toBe("light");
});

test("Match system follows a change of the OS theme", async () => {
  await settings.update({ appearance: { theme: "system" } });
  flushSync();
  expect(root.dataset.theme).toBe("light");
  switchOs(true);
  expect(root.dataset.theme).toBe("dark");
  switchOs(false);
  expect(root.dataset.theme).toBe("light");
  // An explicit choice ignores the OS.
  await settings.update({ appearance: { theme: "dark" } });
  flushSync();
  switchOs(false);
  expect(root.dataset.theme).toBe("dark");
});

test("Settings opens with Mod+, and lists its sections", async () => {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: ",", code: "Comma", ctrlKey: true }),
  );
  const nav = await screen.findByRole("navigation", {
    name: "Settings sections",
  });
  expect(nav).toHaveTextContent("Appearance");
  expect(
    await screen.findByRole("heading", { name: "Appearance", level: 1 }),
  ).toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    ),
  );
});
