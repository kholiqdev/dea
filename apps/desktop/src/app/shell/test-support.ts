// Fakes shared by the shell tests.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { UiState } from "$lib/ipc/bindings/UiState";
import { DEFAULT_SETTINGS } from "$lib/settings/settings.svelte";
import Placeholder from "@dea/ui/Placeholder.svelte";
import {
  createRegistry,
  type Registry,
} from "../contributions/registry.svelte";
import type { AnyComponent, ModuleManifest } from "../contributions/types";

export interface IpcLog {
  savedUiStates: { key: string; state: UiState }[];
}

export function fakeIpc(): IpcLog {
  const log: IpcLog = { savedUiStates: [] };
  clearMocks();
  mockIPC((cmd, args) => {
    const a = args as Record<string, unknown>;
    if (cmd === "get_settings") return structuredClone(DEFAULT_SETTINGS);
    if (cmd === "save_ui_state") {
      log.savedUiStates.push({
        key: a.key as string,
        state: a.state as UiState,
      });
      return null;
    }
    if (cmd === "save_settings" || cmd.startsWith("perf_")) return null;
    throw new Error(`unexpected command ${cmd}`);
  });
  return log;
}

/** A view showing `text`, loaded after `ms` milliseconds. */
export function view(text: string, ms = 0) {
  const component = ((anchor: never, props: Record<string, unknown>) =>
    (Placeholder as AnyComponent)(anchor, {
      ...props,
      title: text,
      text: "",
    })) as AnyComponent;
  return () =>
    new Promise<{ default: AnyComponent }>((resolve) => {
      setTimeout(() => resolve({ default: component }), ms);
    });
}

export function railModule(
  id: string,
  order: number,
  extra: Partial<ModuleManifest> = {},
): ModuleManifest {
  return {
    id,
    title: id.charAt(0).toUpperCase() + id.slice(1),
    rail: { icon: "database", group: "main", order },
    view: view(`${id} view`),
    commands: [
      {
        id: `${id}.open`,
        title: `Open ${id}`,
        action: { type: "openModule", module: id },
      },
    ],
    shortcuts: [{ command: `${id}.open`, chord: `Mod+${order}` }],
    ...extra,
  };
}

export function registryWith(...modules: ModuleManifest[]): Registry {
  const r = createRegistry();
  for (const m of modules) r.register(m);
  return r;
}
