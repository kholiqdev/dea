// The person's settings, shared by the shell and the Settings module. The
// shell applies the appearance to the window whenever it changes.

import type { Appearance } from "$lib/ipc/bindings/Appearance";
import type { Settings } from "$lib/ipc/bindings/Settings";
import { getSettings, saveSettings } from "$lib/ipc/settings";

export const DEFAULT_SETTINGS: Settings = {
  version: 1,
  appearance: {
    theme: "system",
    uiFont: "IBM Plex Sans",
    monoFont: "IBM Plex Mono",
    fontSize: 13,
  },
  shortcuts: {},
  security: {
    autoLockMinutes: 15,
    productionMinutes: 10,
  },
};

export interface SettingsPatch {
  appearance?: Partial<Appearance>;
  /** Replaces every shortcut override. */
  shortcuts?: Settings["shortcuts"];
  security?: Partial<Settings["security"]>;
}

export class SettingsState {
  #value = $state.raw<Settings>(DEFAULT_SETTINGS);
  // Updates run one after another, so a quick second change starts from
  // the first one's result instead of overwriting it.
  #queue: Promise<unknown> = Promise.resolve();

  get value(): Settings {
    return this.#value;
  }

  get appearance(): Appearance {
    return this.#value.appearance;
  }

  get shortcuts(): Settings["shortcuts"] {
    return this.#value.shortcuts;
  }

  get security(): Settings["security"] {
    return this.#value.security;
  }

  /** Reads the saved settings. Outside the app the defaults stay. */
  async load(): Promise<void> {
    try {
      this.#value = await getSettings();
    } catch {
      this.#value = DEFAULT_SETTINGS;
    }
  }

  /** Saves the change, then makes it current. A refused save changes nothing. */
  update(patch: SettingsPatch): Promise<void> {
    const run = this.#queue.then(async () => {
      const next: Settings = {
        ...this.#value,
        appearance: { ...this.#value.appearance, ...patch.appearance },
        shortcuts: patch.shortcuts ?? this.#value.shortcuts,
        security: { ...this.#value.security, ...patch.security },
      };
      await saveSettings(next);
      this.#value = next;
    });
    this.#queue = run.catch(() => {});
    return run;
  }
}

export function createSettings(): SettingsState {
  return new SettingsState();
}

/** The application's settings. */
export const settings = createSettings();
