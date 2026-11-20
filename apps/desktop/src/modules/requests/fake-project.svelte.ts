// A project for the sidebar tests: the environments, the active one, and
// the files a watcher batch changed.

import type { ProjectApi } from "$app/contributions/project-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";

export class FakeProject implements ProjectApi {
  info = $state.raw<ProjectInfo | null>(null);
  changed = $state.raw<readonly string[]>([]);
  environment = $state.raw<Environment | null>(null);
  /** Every name setEnvironment was called with. */
  chosen: string[] = [];

  constructor(environments: Environment[]) {
    this.info = {
      root: "/code/harbor-api",
      name: "harbor-api",
      scope: "s",
      slot: 0,
      readOnly: false,
      defaultEnvironment: "local",
      environments,
      environmentFiles: {},
      secretNames: [],
      errors: [],
      migratedFrom: null,
      missing: false,
    };
  }

  setEnvironment(name: string): Promise<void> {
    this.chosen.push(name);
    this.environment =
      this.info?.environments.find((e) => e.name === name) ?? null;
    return Promise.resolve();
  }

  refresh(info: ProjectInfo): void {
    this.info = info;
  }

  /** Files changed on disk, as a watcher batch reports them. */
  change(files: string[]): void {
    this.changed = files;
  }
}
