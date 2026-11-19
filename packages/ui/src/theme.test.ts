import { afterEach, expect, test, vi } from "vitest";
import { applyTheme, resolveTheme } from "./theme";

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
