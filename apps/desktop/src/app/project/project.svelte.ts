// This window's project: what it is, which environment requests use, and
// what changed on disk last.

import { tick } from "svelte";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectChanged } from "$lib/ipc/bindings/ProjectChanged";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import * as ipc from "$lib/ipc/project";
import type { ProjectApi } from "../contributions/project-api";

/**
 * The environment to use: the saved choice while it exists, else the
 * project's default, else the first one (F1-FON-06 #8).
 */
export function activeEnvironment(
  info: ProjectInfo | null,
  saved: string | null,
): Environment | null {
  if (!info) return null;
  const envs = info.environments;
  return (
    envs.find((e) => e.name === saved) ??
    envs.find((e) => e.name === info.defaultEnvironment) ??
    envs[0] ??
    null
  );
}

/**
 * The project after a change: one whose dea.yaml cannot be read keeps what
 * the window showed, with the problem added, so drafts are not lost.
 */
function merged(before: ProjectInfo | null, next: ProjectInfo): ProjectInfo {
  const manifestBroken =
    !next.missing && next.errors.some((e) => e.path === "dea.yaml");
  return manifestBroken && before && !before.missing
    ? { ...before, errors: next.errors }
    : next;
}

export class WindowProject implements ProjectApi {
  #info = $state.raw<ProjectInfo | null>(null);
  #chosen = $state<string | null>(null);
  #changed = $state.raw<readonly string[]>([]);
  #environment = $derived(activeEnvironment(this.#info, this.#chosen));

  get info(): ProjectInfo | null {
    return this.#info;
  }

  get environment(): Environment | null {
    return this.#environment;
  }

  get changed(): readonly string[] {
    return this.#changed;
  }

  /** Reads this window's project; outside the app there is none. */
  async load(): Promise<ProjectInfo | null> {
    try {
      this.#info = await ipc.windowProject();
    } catch {
      this.#info = null;
    }
    return this.#info;
  }

  /** The choice saved with this window's layout. */
  restoreEnvironment(name: string | null): void {
    this.#chosen = name;
  }

  async setEnvironment(name: string): Promise<void> {
    performance.mark?.("env-switch-start");
    this.#chosen = name;
    await tick();
    performance.measure?.("env-switch", "env-switch-start");
    // The window's choice only matters to Dea for "used by" lists.
    await ipc.setActiveEnvironment(name).catch(() => {});
  }

  refresh(info: ProjectInfo): void {
    this.#info = info;
  }

  apply(change: ProjectChanged): void {
    this.#info = merged(this.#info, change.info);
    this.#changed = change.changed;
  }

  /**
   * Reads the project again whenever the window gains focus: moving the
   * folder away sends no change, but coming back to Dea shows it. Returns
   * the function that stops.
   */
  recheckOnFocus(): () => void {
    const recheck = () => {
      ipc
        .windowProject()
        .then((info) => {
          if (info) this.#info = merged(this.#info, info);
        })
        .catch(() => {});
    };
    window.addEventListener("focus", recheck);
    return () => window.removeEventListener("focus", recheck);
  }

  /** Follows changes on disk until the returned function is called. */
  async listen(subscribe = ipc.onProjectChanged): Promise<() => void> {
    return await subscribe((change) => this.apply(change));
  }
}

/** This window's project. */
export const thisProject = new WindowProject();
