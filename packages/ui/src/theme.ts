// Dark, light, or whatever the OS uses. The tokens for each theme live in
// tokens.css under [data-theme].

export type ThemePreference = "dark" | "light" | "system";
export type Theme = "dark" | "light";

const QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(
  pref: ThemePreference,
  systemPrefersDark: boolean,
): Theme {
  if (pref === "system") return systemPrefersDark ? "dark" : "light";
  return pref;
}

export function applyTheme(root: HTMLElement, theme: Theme): void {
  root.dataset.theme = theme;
}

/** Whether the OS currently prefers a dark theme. */
export function systemPrefersDark(): boolean {
  return matchMedia(QUERY).matches;
}
