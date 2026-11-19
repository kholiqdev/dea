// Puts the person's appearance settings on the window: theme, fonts and
// text size. Runs whenever they change, so nothing needs a restart.

import { applyTheme, resolveTheme } from "@dea/ui/theme";
import type { Appearance } from "$lib/ipc/bindings/Appearance";

// The bundled fonts are the token defaults in @dea/ui.
const BUNDLED = new Set(["IBM Plex Sans", "IBM Plex Mono"]);
const SYSTEM_UI = "system-ui, -apple-system, 'Segoe UI', sans-serif";
const SYSTEM_MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
const SYSTEM: Record<string, string> = {
  "System UI": SYSTEM_UI,
  "System monospace": SYSTEM_MONO,
};

function quoted(name: string): string {
  return `"${name.replace(/["\\]/g, (c) => `\\${c}`)}"`;
}

function setFont(
  root: HTMLElement,
  variable: string,
  name: string,
  fallback: string,
): void {
  if (BUNDLED.has(name)) root.style.removeProperty(variable);
  else
    root.style.setProperty(
      variable,
      SYSTEM[name] ?? `${quoted(name)}, ${fallback}`,
    );
}

export function applyAppearance(
  root: HTMLElement,
  a: Appearance,
  systemPrefersDark: boolean,
): void {
  applyTheme(root, resolveTheme(a.theme, systemPrefersDark));
  setFont(root, "--font-ui", a.uiFont, SYSTEM_UI);
  setFont(root, "--font-mono", a.monoFont, SYSTEM_MONO);
  root.style.setProperty("--font-size", `${a.fontSize}px`);
}
