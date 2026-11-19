// IPC and registry fakes for the Settings tests.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { registry } from "$app/contributions/registry.svelte";
import type { ModuleManifest } from "$app/contributions/types";
import type { Settings } from "$lib/ipc/bindings/Settings";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";

export function fakeSettingsIpc(): { saved: Settings[] } {
  const log = { saved: [] as Settings[] };
  let stored = structuredClone(DEFAULT_SETTINGS);
  clearMocks();
  mockIPC((cmd, args) => {
    if (cmd === "get_settings") return stored;
    if (cmd === "save_settings") {
      stored = (args as { settings: Settings }).settings;
      log.saved.push(stored);
      return null;
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  return log;
}

export async function resetSettings(): Promise<void> {
  await settings.update({
    appearance: DEFAULT_SETTINGS.appearance,
    shortcuts: {},
  });
}

function opener(id: string, title: string, chord: string): ModuleManifest {
  return {
    id,
    title,
    commands: [
      {
        id: `${id}.open`,
        title: `Open ${title}`,
        action: { type: "openModule", module: id },
      },
    ],
    shortcuts: [{ command: `${id}.open`, chord }],
  };
}

/** Registers two modules with Mod+1 and Mod+2, once per test file. */
export function registerTestModules(): void {
  if (registry.module("requests")) return;
  registry.register(opener("requests", "Requests", "Mod+1"));
  registry.register(opener("database", "Database", "Mod+2"));
}
