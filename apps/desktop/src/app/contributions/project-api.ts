// What modules may read about this window's project, and ask of it.

import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";

export interface ProjectApi {
  /** This window's project, or null on Welcome. */
  readonly info: ProjectInfo | null;
  /** The environment request tabs use. */
  readonly environment: Environment | null;
  /** The files of the last change from disk, relative to the project. */
  readonly changed: readonly string[];
  setEnvironment(name: string): Promise<void>;
  /** Takes the project as a command that wrote to it returned it. */
  refresh(info: ProjectInfo): void;
}

let current: ProjectApi | null = null;

export function setProjectApi(api: ProjectApi): void {
  current = api;
}

export function projectApi(): ProjectApi {
  if (!current) throw new Error("The shell has not started yet");
  return current;
}

/**
 * The CSS colour of an environment's marker. Production is always red; the
 * others use the named palette, which has no red (F1-FON-07 #2).
 */
export function environmentColor(env: {
  production: boolean;
  color?: string;
}): string {
  if (env.production) return "var(--negative)";
  return `var(--env-${env.color ?? "slate"}, var(--env-slate))`;
}
