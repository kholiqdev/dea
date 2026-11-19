// Settings and UI state, kept by dea-engine in the app data folder.

import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "./bindings/Settings";
import type { UiState } from "./bindings/UiState";

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export function saveSettings(settings: Settings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

export function getUiState(key: string): Promise<UiState> {
  return invoke<UiState>("get_ui_state", { key });
}

export function saveUiState(key: string, state: UiState): Promise<void> {
  return invoke<void>("save_ui_state", { key, state });
}
