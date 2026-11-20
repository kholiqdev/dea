// The registry of extension points: the only way a module reaches the
// shell (F1-FON-18).

import { SvelteSet } from "svelte/reactivity";
import { runIntent, type IntentResult } from "./intents";
import type {
  Command,
  FromModule,
  ModuleManifest,
  SearchSourceContribution,
  SettingsSectionContribution,
  ShortcutData,
  StatusItemContribution,
  TabTypeContribution,
} from "./types";

const MODULE_ID = /^[a-z][a-z0-9]*$/;
const CONTRIBUTION_ID = /^[a-z][a-z0-9]*\.[a-z][A-Za-z0-9.]*$/;
const GROUPS = ["main", "bottom"];

export class ContributionError extends Error {
  override name = "ContributionError";
}

function contributionIds(m: ModuleManifest): string[] {
  return [
    ...(m.tabTypes ?? []),
    ...(m.panels ?? []),
    ...(m.commands ?? []),
    ...(m.statusItems ?? []),
    ...(m.settings ?? []),
    ...(m.searchSources ?? []),
  ].map((c) => c.id);
}

function byOrder(a: { order: number }, b: { order: number }): number {
  return a.order - b.order;
}

function flatten<T>(
  manifests: ModuleManifest[],
  pick: (m: ModuleManifest) => T[] | undefined,
): FromModule<T>[] {
  return manifests.flatMap((m) =>
    (pick(m) ?? []).map((c) => ({ ...c, module: m.id })),
  );
}

export class Registry {
  #manifests = $state.raw<ModuleManifest[]>([]);
  #disabled = new SvelteSet<string>();
  #projectOpen = $state(true);
  #active = $derived(
    this.#manifests.filter(
      (m) =>
        !this.#disabled.has(m.id) && (this.#projectOpen || m.scope === "app"),
    ),
  );
  // The same objects until the modules change: a tab's view reloads, and
  // remounts, whenever its tab type is a new object.
  #tabTypes = $derived(flatten(this.#active, (m) => m.tabTypes));

  /** Adds a module. Throws ContributionError and adds nothing if any id is bad. */
  register(m: ModuleManifest): void {
    if (!MODULE_ID.test(m.id))
      throw new ContributionError(`Invalid module id "${m.id}"`);
    if (this.#manifests.some((other) => other.id === m.id))
      throw new ContributionError(`Duplicate module id "${m.id}"`);
    const taken = this.#manifests.flatMap(contributionIds);
    for (const id of contributionIds(m)) {
      if (taken.includes(id))
        throw new ContributionError(`Duplicate contribution id "${id}"`);
      if (!id.startsWith(`${m.id}.`) || !CONTRIBUTION_ID.test(id))
        throw new ContributionError(
          `Contribution id "${id}" must look like "${m.id}.name"`,
        );
      taken.push(id);
    }
    this.#manifests = [...this.#manifests, m];
  }

  setEnabled(module: string, on: boolean): void {
    if (on) this.#disabled.delete(module);
    else this.#disabled.add(module);
  }

  /** Turns the modules that need a project on or off for this window. */
  setProjectOpen(open: boolean): void {
    this.#projectOpen = open;
  }

  /** An enabled module, rail or not. */
  module(id: string): ModuleManifest | undefined {
    return this.#active.find((m) => m.id === id);
  }

  /** Enabled modules on the rail, by group and then order. */
  get modules(): ModuleManifest[] {
    return this.#active
      .filter((m) => m.rail)
      .sort(
        (a, b) =>
          GROUPS.indexOf(a.rail!.group) - GROUPS.indexOf(b.rail!.group) ||
          a.rail!.order - b.rail!.order,
      );
  }

  get commands(): Command[] {
    return flatten(this.#active, (m) => m.commands).filter(
      (c) => c.scope !== "project" || this.#projectOpen,
    );
  }

  get shortcuts(): FromModule<ShortcutData>[] {
    return flatten(this.#active, (m) => m.shortcuts);
  }

  get statusItems(): FromModule<StatusItemContribution>[] {
    return flatten(this.#active, (m) => m.statusItems).sort(byOrder);
  }

  get settingsSections(): FromModule<SettingsSectionContribution>[] {
    return flatten(this.#active, (m) => m.settings).sort(byOrder);
  }

  /** Sources of command palette items, by group order. */
  get searchSources(): FromModule<SearchSourceContribution>[] {
    return flatten(this.#active, (m) => m.searchSources).sort(byOrder);
  }

  tabType(id: string): FromModule<TabTypeContribution> | undefined {
    return this.#tabTypes.find((t) => t.id === id);
  }

  invoke(intent: string, args?: unknown): Promise<IntentResult> {
    const handler = this.#active
      .flatMap((m) => m.intents ?? [])
      .find((i) => i.intent === intent);
    return runIntent(intent, handler, args);
  }
}

export function createRegistry(): Registry {
  return new Registry();
}

/** The application's registry. */
export const registry = createRegistry();
