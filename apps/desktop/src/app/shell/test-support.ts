// Fakes shared by the shell tests.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { UiState } from "$lib/ipc/bindings/UiState";
import { DEFAULT_SETTINGS } from "$lib/settings/settings.svelte";
import Placeholder from "@dea/ui/Placeholder.svelte";
import { WindowProject } from "../project/project.svelte";
import {
  createRegistry,
  type Registry,
} from "../contributions/registry.svelte";
import type { AnyComponent, ModuleManifest } from "../contributions/types";

export interface IpcLog {
  calls: { cmd: string; args: unknown }[];
  savedUiStates: { scope: string; slot: number; state: UiState }[];
}

export interface FakeIpcOptions {
  /** What window_project returns. */
  project?: ProjectInfo | null;
  /** What get_ui_state returns. */
  uiState?: UiState;
  /** How long clear_local_data takes, in milliseconds. */
  clearTakes?: number;
  /** What pick_folder returns. */
  picked?: string | null;
}

export function fakeIpc(opts: FakeIpcOptions = {}): IpcLog {
  const log: IpcLog = { calls: [], savedUiStates: [] };
  clearMocks();
  mockIPC((cmd, args) => {
    const a = args as Record<string, unknown>;
    log.calls.push({ cmd, args });
    if (cmd === "get_settings") return structuredClone(DEFAULT_SETTINGS);
    if (cmd === "window_project") return opts.project ?? null;
    if (cmd === "get_ui_state" && opts.uiState) return opts.uiState;
    if (cmd === "save_ui_state") {
      log.savedUiStates.push({
        scope: a.scope as string,
        slot: a.slot as number,
        state: a.state as UiState,
      });
      return null;
    }
    if (cmd === "pick_folder") return opts.picked ?? null;
    if (cmd === "open_project") return projectInfo({ root: a.root as string });
    if (cmd === "close_project") return null;
    if (cmd === "clear_local_data")
      return new Promise((resolve) =>
        setTimeout(() => resolve(null), opts.clearTakes ?? 0),
      );
    if (
      cmd === "save_settings" ||
      cmd === "set_active_environment" ||
      cmd.startsWith("perf_")
    )
      return null;
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

/** A project with local, staging and production. */
export function projectInfo(over: Partial<ProjectInfo> = {}): ProjectInfo {
  return {
    root: "/work/harbor-api",
    name: "harbor-api",
    scope: "scope-1",
    slot: 0,
    readOnly: false,
    defaultEnvironment: "local",
    environments: [
      { name: "local", production: false },
      { name: "staging", production: false, color: "blue" },
      { name: "production", production: true },
    ],
    environmentFiles: {
      local: "environments/local.yaml",
      staging: "environments/staging.yaml",
      production: "environments/production.yaml",
    },
    secretNames: [],
    errors: [],
    migratedFrom: null,
    missing: false,
    ...over,
  };
}

/** A window project that already has `info` (or none). */
export function openProject(
  info: ProjectInfo | null = projectInfo(),
): WindowProject {
  const p = new WindowProject();
  if (info) p.refresh(info);
  return p;
}
