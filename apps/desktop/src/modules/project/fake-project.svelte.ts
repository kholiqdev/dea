// A project for tests that changes on disk the way a watcher batch changes
// it: one new ProjectInfo and the files that changed.

import type { ProjectApi } from "$app/contributions/project-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectChanged } from "$lib/ipc/bindings/ProjectChanged";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";

export class FakeProject implements ProjectApi {
  info = $state.raw<ProjectInfo | null>(null);
  changed = $state.raw<readonly string[]>([]);
  readonly environment: Environment | null = null;

  constructor(info: ProjectInfo) {
    this.info = info;
  }

  setEnvironment(): Promise<void> {
    return Promise.resolve();
  }

  refresh(info: ProjectInfo): void {
    this.info = info;
  }

  apply(change: ProjectChanged): void {
    this.info = change.info;
    this.changed = change.changed;
  }
}
