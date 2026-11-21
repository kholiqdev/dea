// Tabs from every module in one bar (F1-FON-02). The active tab decides
// the active module; picking a module on the rail shows its last tab.

import { tick } from "svelte";
import type { TabState } from "$lib/ipc/bindings/TabState";
import type { UiState } from "$lib/ipc/bindings/UiState";
import { registry, type Registry } from "../contributions/registry.svelte";
import type {
  OpenTabOptions,
  TabEnvironment,
} from "../contributions/shell-api";
import { closeQuestion } from "../prompts/confirm-close.svelte";
import { shell, type Shell } from "../shell/shell-state.svelte";

/** How many closed tabs can be reopened. */
export const MAX_CLOSED_TABS = 20;

export type CloseChoice = "save" | "discard" | "cancel";

export interface TabRecord {
  id: string;
  tabType: string;
  module: string;
  title: string;
  params: unknown;
  dirty: boolean;
  environment?: TabEnvironment;
}

export type SaveHandler = (tabId: string) => Promise<void>;

export interface TabsOptions {
  /** Asks what to do with a tab that has unsaved changes. */
  confirmClose: (tab: TabRecord) => Promise<CloseChoice>;
}

export class Tabs {
  readonly #registry: Registry;
  readonly #shell: Shell;
  readonly #confirm: TabsOptions["confirmClose"];
  #tabs = $state.raw<TabRecord[]>([]);
  // The tab each module last showed, by module id.
  #selected = $state<Record<string, string>>({});
  #closed = $state.raw<TabState[]>([]);
  // Not shown anywhere, so not reactive.
  #saveHandlers: Record<string, SaveHandler> = {};

  /**
   * The tab shown in the work area: the active module's selected tab, or
   * its last tab. Picking a module on the rail therefore shows its tab.
   */
  #active = $derived.by(() => {
    const module = this.#shell.activeModule;
    if (!module) return undefined;
    return (
      this.#tabs.find((t) => t.id === this.#selected[module]) ??
      this.#tabs.findLast((t) => t.module === module)
    );
  });

  constructor(registry: Registry, shell: Shell, opts: TabsOptions) {
    this.#registry = registry;
    this.#shell = shell;
    this.#confirm = opts.confirmClose;
  }

  get tabs(): TabRecord[] {
    return this.#tabs;
  }

  get active(): TabRecord | undefined {
    return this.#active;
  }

  /** The shown tab as modules see it, or null when none is open. */
  activeTab(): { id: string; tabType: string; params: unknown } | null {
    const tab = this.#active;
    return tab
      ? { id: tab.id, tabType: tab.tabType, params: tab.params }
      : null;
  }

  open(
    tabType: string,
    title: string,
    params: unknown,
    opts: OpenTabOptions = {},
  ): string {
    const type = this.#registry.tabType(tabType);
    if (!type) throw new Error(`Unknown tab type "${tabType}"`);
    const tab: TabRecord = {
      id: crypto.randomUUID(),
      tabType,
      module: type.module,
      title,
      params,
      dirty: false,
      ...(opts.environment ? { environment: opts.environment } : {}),
    };
    this.#tabs = [...this.#tabs, tab];
    this.activate(tab.id);
    return tab.id;
  }

  activate(id: string): void {
    const tab = this.#tabs.find((t) => t.id === id);
    if (!tab) return;
    // Timed for the end-to-end check of F1-FON-02 #5 (switch ≤ 50 ms).
    const start = `tab-switch-start:${id}`;
    if (typeof performance.mark === "function") {
      performance.mark(start);
      void tick().then(() => performance.measure(`tab-switch:${id}`, start));
    }
    this.#selected[tab.module] = id;
    if (this.#shell.activeModule !== tab.module)
      void this.#shell.openModule(tab.module);
  }

  /**
   * Closes a tab. A tab with unsaved changes asks first; returns whether it
   * closed. A failed save leaves the tab open and rejects.
   */
  async close(id: string): Promise<boolean> {
    const tab = this.#tabs.find((t) => t.id === id);
    if (!tab) return false;
    if (tab.dirty) {
      const choice = await this.#confirm(tab);
      if (choice === "cancel") return false;
      if (choice === "save") {
        const save = this.#saveHandlers[tab.tabType];
        if (!save) throw new Error(`"${tab.title}" cannot be saved`);
        await save(tab.id);
      }
    }
    const index = this.#tabs.findIndex((t) => t.id === id);
    if (index < 0) return true;
    const rest = this.#tabs.filter((t) => t.id !== id);
    this.#tabs = rest;
    this.#closed = [...this.#closed, toState(tab)].slice(-MAX_CLOSED_TABS);
    if (this.#selected[tab.module] === id) {
      const next = nearest(rest, index, tab.module);
      if (next) this.#selected[tab.module] = next.id;
      else delete this.#selected[tab.module];
    }
    return true;
  }

  move(id: string, toIndex: number): void {
    const tab = this.#tabs.find((t) => t.id === id);
    if (!tab) return;
    const rest = this.#tabs.filter((t) => t.id !== id);
    const at = Math.max(0, Math.min(toIndex, rest.length));
    this.#tabs = [...rest.slice(0, at), tab, ...rest.slice(at)];
  }

  /** Moves the active tab one place left (-1) or right (+1). */
  moveActive(delta: -1 | 1): void {
    const id = this.#active?.id;
    if (!id) return;
    const index = this.#tabs.findIndex((t) => t.id === id);
    this.move(id, index + delta);
  }

  setDirty(id: string, dirty: boolean): void {
    // No new tab object when nothing changes: views report their state on
    // every change and would otherwise never settle.
    if (this.#tabs.find((t) => t.id === id)?.dirty === dirty) return;
    this.#tabs = this.#tabs.map((t) => (t.id === id ? { ...t, dirty } : t));
  }

  /** The first tab of `tabType` whose params `match` accepts, if any. */
  find(
    tabType: string,
    match?: (params: unknown) => boolean,
  ): string | undefined {
    return this.#tabs.find(
      (t) => t.tabType === tabType && (!match || match(t.params)),
    )?.id;
  }

  update(id: string, patch: { title?: string; params?: unknown }): void {
    const tab = this.#tabs.find((t) => t.id === id);
    if (!tab) return;
    const next = { ...tab };
    if (patch.title !== undefined) next.title = patch.title;
    if ("params" in patch) next.params = patch.params;
    if (next.title === tab.title && next.params === tab.params) return;
    this.#tabs = this.#tabs.map((t) => (t.id === id ? next : t));
  }

  /**
   * The environment a tab shows: its own, or for a tab type that follows
   * the window, the window's active one.
   */
  environmentOf(
    tab: TabRecord,
    windowEnvironment: TabEnvironment | null | undefined,
  ): TabEnvironment | undefined {
    if (tab.environment) return tab.environment;
    if (this.#registry.tabType(tab.tabType)?.environment !== "window")
      return undefined;
    return windowEnvironment ?? undefined;
  }

  setSaveHandler(tabType: string, save: SaveHandler): void {
    this.#saveHandlers[tabType] = save;
  }

  /** Saves the active tab; nothing happens without a handler for its type. */
  async saveActive(): Promise<void> {
    const tab = this.#active;
    const save = tab && this.#saveHandlers[tab.tabType];
    if (tab && save) await save(tab.id);
  }

  reopenLast(): void {
    const last = this.#closed.at(-1);
    if (!last) return;
    this.#closed = this.#closed.slice(0, -1);
    const type = this.#registry.tabType(last.tabType);
    if (!type) return;
    const open = this.#tabs.some((t) => t.id === last.id);
    const tab: TabRecord = {
      ...last,
      id: open ? crypto.randomUUID() : last.id,
      module: type.module,
      dirty: false,
    };
    this.#tabs = [...this.#tabs, tab];
    this.activate(tab.id);
  }

  /** Takes the saved tabs back, skipping types no module registers now. */
  restore(saved: UiState): void {
    const known = (t: TabState) => this.#registry.tabType(t.tabType);
    this.#tabs = saved.tabs.flatMap((t) => {
      const type = known(t);
      return type ? [{ ...t, module: type.module, dirty: false }] : [];
    });
    this.#closed = saved.closedTabs.filter(known).slice(-MAX_CLOSED_TABS);
    const active =
      this.#tabs.find((t) => t.id === saved.activeTab) ?? this.#tabs[0];
    this.#selected = active ? { [active.module]: active.id } : {};
  }

  snapshot(): Pick<UiState, "tabs" | "activeTab" | "closedTabs"> {
    return {
      tabs: this.#tabs.map(toState),
      activeTab: this.#active?.id ?? null,
      closedTabs: this.#closed,
    };
  }
}

function toState(t: TabRecord): TabState {
  // JSON drops undefined, and the store needs params to be present.
  return {
    id: t.id,
    tabType: t.tabType,
    title: t.title,
    params: t.params ?? null,
    ...(t.environment ? { environment: t.environment } : {}),
  };
}

/** The tab of `module` closest to `index`, looking right first. */
function nearest(
  tabs: TabRecord[],
  index: number,
  module: string,
): TabRecord | undefined {
  for (let d = 0; d < tabs.length; d++) {
    const right = tabs[index + d];
    if (right?.module === module) return right;
    const left = tabs[index - 1 - d];
    if (left?.module === module) return left;
  }
  return undefined;
}

/** The application's tabs. */
export const tabs = new Tabs(registry, shell, {
  confirmClose: closeQuestion.ask,
});
