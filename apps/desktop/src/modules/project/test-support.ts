// IPC fakes for the Welcome tests.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { RecentEntry } from "$lib/ipc/bindings/RecentEntry";

export interface WelcomeIpc {
  calls: { cmd: string; args: Record<string, unknown> }[];
  recent: RecentEntry[];
  /** The next folder the picker returns. */
  picked: string | null;
  /** Folders that open_project refuses as not a project. */
  notProjects: Set<string>;
}

export const DAY = 24 * 60 * 60;

export function info(name: string, root: string): ProjectInfo {
  return {
    root,
    name,
    scope: "s",
    slot: 0,
    readOnly: false,
    defaultEnvironment: "local",
    environments: [],
    secretNames: [],
    errors: [],
    migratedFrom: null,
    missing: false,
  };
}

export function fakeWelcomeIpc(recent: RecentEntry[] = []): WelcomeIpc {
  const ipc: WelcomeIpc = {
    calls: [],
    recent,
    picked: null,
    notProjects: new Set(),
  };
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = (raw ?? {}) as Record<string, unknown>;
    ipc.calls.push({ cmd, args });
    switch (cmd) {
      case "recent_projects":
        return ipc.recent;
      case "remove_recent":
        ipc.recent = ipc.recent.filter((r) => r.root !== args.root);
        return null;
      case "pick_folder":
        return ipc.picked;
      case "open_project": {
        const root = args.root as string;
        if (ipc.notProjects.has(root))
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw {
            kind: "notAProject",
            message: `${root} has no dea.yaml, so it is not a Dea project`,
          };
        return info("opened", root);
      }
      case "create_project":
        if (!(args.name as string).trim())
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { kind: "invalid", message: "a project needs a name" };
        return info(args.name as string, args.location as string);
      default:
        return null;
    }
  });
  return ipc;
}
