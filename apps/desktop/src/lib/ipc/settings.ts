// Settings and UI state, kept by dea-engine in the app data folder.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Settings } from "./bindings/Settings";
import type { UiState } from "./bindings/UiState";

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/** Saves the settings; every window then gets them as `settings-changed`. */
export function saveSettings(settings: Settings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

/** Calls `handler` with the settings each time any window saves them. */
export function onSettingsChanged(
  handler: (settings: Settings) => void,
): Promise<() => void> {
  return getCurrentWebviewWindow().listen<Settings>("settings-changed", (e) =>
    handler(e.payload),
  );
}

/** The UI state of window `slot` of project `scope`. */
export function getUiState(scope: string, slot: number): Promise<UiState> {
  return invoke<UiState>("get_ui_state", { scope, slot });
}

export function saveUiState(
  scope: string,
  slot: number,
  state: UiState,
): Promise<void> {
  return invoke<void>("save_ui_state", { scope, slot, state });
}
