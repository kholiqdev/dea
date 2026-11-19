// The secrets of this window's project, shared by the sidebar and the view.
// Rows never hold a value; a revealed value lives in the inspector only.

import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
import { vaultReferences } from "$lib/vault/references";
import { vaultSecrets } from "../api";

/** Names of the environment's variables that use the secret. */
export function usedBy(
  info: ProjectInfo,
  environment: string,
  name: string,
): string[] {
  const env = info.environments.find((e) => e.name === environment);
  return (env?.variables ?? [])
    .filter((v) => vaultReferences(v.value).includes(name))
    .map((v) => v.name);
}

class Secrets {
  rows = $state.raw<SecretRow[]>([]);
  /** The environment picked in the sidebar; not kept across restarts. */
  chosen = $state<string | null>(null);
  /** The secret shown in the inspector, by its place. */
  choice = $state<{ environment: string; name: string } | null>(null);

  async load(): Promise<void> {
    this.rows = await vaultSecrets();
  }

  of(environment: string): SecretRow[] {
    return this.rows.filter((r) => r.environment === environment);
  }

  /** Shows one secret: its environment in the sidebar, the secret selected. */
  show(environment: string, name: string): void {
    this.chosen = environment;
    this.choice = { environment, name };
  }

  /** How many values this device has in the environment. */
  count(environment: string): number {
    return this.of(environment).filter((r) => !r.missing).length;
  }

  /** The chosen environment if the project still has it, else the default. */
  current(info: ProjectInfo): string | null {
    const names = info.environments.map((e) => e.name);
    if (this.chosen && names.includes(this.chosen)) return this.chosen;
    if (info.defaultEnvironment && names.includes(info.defaultEnvironment))
      return info.defaultEnvironment;
    return names[0] ?? null;
  }
}

export const secrets = new Secrets();

export function resetSecrets(): void {
  secrets.rows = [];
  secrets.chosen = null;
  secrets.choice = null;
}
