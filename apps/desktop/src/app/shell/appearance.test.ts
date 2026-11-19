import { expect, test } from "vitest";
import { applyAppearance } from "./appearance";

const base = {
  theme: "system" as const,
  uiFont: "IBM Plex Sans",
  monoFont: "IBM Plex Mono",
  fontSize: 13,
};

test("applies the theme the preference resolves to", () => {
  const root = document.createElement("html");
  applyAppearance(root, base, true);
  expect(root.dataset.theme).toBe("dark");
  applyAppearance(root, { ...base, theme: "light" }, true);
  expect(root.dataset.theme).toBe("light");
});

test("applies the font size and fonts without a restart", () => {
  const root = document.createElement("html");
  applyAppearance(root, { ...base, fontSize: 16, uiFont: "System UI" }, false);
  expect(root.style.getPropertyValue("--font-size")).toBe("16px");
  expect(root.style.getPropertyValue("--font-ui")).toMatch(/^system-ui/);
  // The bundled fonts are the token defaults.
  expect(root.style.getPropertyValue("--font-mono")).toBe("");
});

test("a font name is quoted, never read as CSS", () => {
  const root = document.createElement("html");
  applyAppearance(root, { ...base, monoFont: 'Evil"; color: red' }, false);
  const value = root.style.getPropertyValue("--font-mono");
  expect(value.startsWith('"Evil\\"; color: red", ')).toBe(true);
});
