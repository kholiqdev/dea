import { afterEach, expect, test, vi } from "vitest";
import { applyTheme, resolveTheme, watchSystemTheme } from "./theme";

afterEach(() => vi.unstubAllGlobals());

test("system follows the OS", () => {
  expect(resolveTheme("system", true)).toBe("dark");
  expect(resolveTheme("system", false)).toBe("light");
});

test("an explicit choice ignores the OS", () => {
  expect(resolveTheme("dark", false)).toBe("dark");
  expect(resolveTheme("light", true)).toBe("light");
});

test("apply sets data-theme", () => {
  const root = document.createElement("html");
  applyTheme(root, "light");
  expect(root.dataset.theme).toBe("light");
  applyTheme(root, "dark");
  expect(root.dataset.theme).toBe("dark");
});

test("watching reports changes and stops", () => {
  const listeners = new Set<(e: { matches: boolean }) => void>();
  const query = {
    matches: true,
    addEventListener: (_: string, fn: (e: { matches: boolean }) => void) =>
      listeners.add(fn),
    removeEventListener: (_: string, fn: (e: { matches: boolean }) => void) =>
      listeners.delete(fn),
  };
  const matchMedia = vi.fn(() => query);
  vi.stubGlobal("matchMedia", matchMedia);
  const seen: boolean[] = [];

  const stop = watchSystemTheme((dark) => seen.push(dark));
  expect(matchMedia).toHaveBeenCalledWith("(prefers-color-scheme: dark)");
  for (const fn of listeners) fn({ matches: false });
  stop();
  for (const fn of listeners) fn({ matches: true });

  expect(seen).toEqual([false]);
  expect(listeners.size).toBe(0);
});
