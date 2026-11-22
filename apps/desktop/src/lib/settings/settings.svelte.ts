// The person's settings, shared by the shell and the Settings module. The
// shell applies the appearance to the window whenever it changes.

import type { Appearance } from "$lib/ipc/bindings/Appearance";
import type { Settings } from "$lib/ipc/bindings/Settings";
import {
  getSettings,
  onSettingsChanged,
  saveSettings,
} from "$lib/ipc/settings";

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
  requests: {
    autosave: false,
    responseLayout: "right",
    responseSplit: { right: 44, below: 40 },
  },
};

export interface SettingsPatch {
  appearance?: Partial<Appearance>;
  /** Replaces every shortcut override. */
  shortcuts?: Settings["shortcuts"];
  security?: Partial<Settings["security"]>;
  requests?: Partial<Settings["requests"]>;
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

  get requests(): Settings["requests"] {
    return this.#value.requests;
  }

  /** Reads the saved settings. Outside the app the defaults stay. */
  async load(): Promise<void> {
    try {
      this.#value = await getSettings();
    } catch {
      this.#value = DEFAULT_SETTINGS;
    }
  }

  /** Takes settings saved by any window. */
  apply(saved: Settings): void {
    this.#value = saved;
  }

  /** Follows the settings every window saves until the returned function is called. */
  async listen(subscribe = onSettingsChanged): Promise<() => void> {
    return await subscribe((saved) => this.apply(saved));
  }

  /**
   * Saves the change, then makes it current. A refused save changes nothing.
   * The patch goes over the latest settings, those saved by other windows
   * included, so this window never sends back their older values.
   */
  update(patch: SettingsPatch): Promise<void> {
    const run = this.#queue.then(async () => {
      await saveSettings(merged(this.#value, patch));
      // Settings from another window may have come while this one saved.
      this.#value = merged(this.#value, patch);
    });
    this.#queue = run.catch(() => {});
    return run;
  }
}

function merged(value: Settings, patch: SettingsPatch): Settings {
  return {
    ...value,
    appearance: { ...value.appearance, ...patch.appearance },
    shortcuts: patch.shortcuts ?? value.shortcuts,
    security: { ...value.security, ...patch.security },
    requests: { ...value.requests, ...patch.requests },
  };
}

export function createSettings(): SettingsState {
  return new SettingsState();
}

/** The application's settings. */
export const settings = createSettings();
