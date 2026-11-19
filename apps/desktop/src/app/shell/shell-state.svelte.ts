// Shell state: the open module, the rail and each module's sidebar. Saved
// as UI state so the window opens the same way next time.

import type { SidebarState } from "$lib/ipc/bindings/SidebarState";
import type { UiState } from "$lib/ipc/bindings/UiState";
import { saveUiState } from "$lib/ipc/settings";
import { tick } from "svelte";
import { registry, type Registry } from "../contributions/registry.svelte";
import type { AnyComponent } from "../contributions/types";

// Sizes from tokens.css (--w-rail, --w-rail-wide, --w-side, --w-side-min,
// --w-work-min).
export const RAIL_WIDTH = 52;
export const RAIL_WIDE_WIDTH = 216;
export const SIDEBAR_WIDTH = 280;
export const SIDEBAR_MIN = 200;
export const WORK_MIN = 480;

/** The UI state key until projects can be opened. */
export const NO_PROJECT = "no-project";

/**
 * A sidebar width that leaves the work area its minimum, and is never
 * below the sidebar's own minimum.
 */
export function clampSidebarWidth(
  px: number,
  windowWidth: number,
  railWidth: number,
): number {
  const max = windowWidth - railWidth - WORK_MIN;
  return Math.max(SIDEBAR_MIN, Math.min(px, max));
}

function mark(name: string): void {
  if (typeof performance.mark === "function") performance.mark(name);
}

function measure(name: string, start: string): void {
  if (typeof performance.measure === "function")
    performance.measure(name, start);
}

export type ShellSnapshot = Pick<
  UiState,
  "activeModule" | "railExpanded" | "sidebars"
>;

export class Shell {
  readonly registry: Registry;
  activeModule = $state<string | null>(null);
  railExpanded = $state(false);
  #sidebars = $state<Record<string, SidebarState>>({});
  #loaded = $state.raw<{ module: string; view: AnyComponent } | null>(null);
  // Views already loaded, by module: switching back is instant.
  #views: Record<string, AnyComponent> = {};
  #request = 0;

  constructor(registry: Registry) {
    this.registry = registry;
  }

  get railWidth(): number {
    return this.railExpanded ? RAIL_WIDE_WIDTH : RAIL_WIDTH;
  }

  sidebar(module: string): SidebarState {
    return this.#sidebars[module] ?? { visible: true, width: SIDEBAR_WIDTH };
  }

  setSidebarWidth(module: string, px: number): void {
    this.#sidebars[module] = { ...this.sidebar(module), width: Math.round(px) };
  }

  toggleSidebar(module: string): void {
    const s = this.sidebar(module);
    this.#sidebars[module] = { ...s, visible: !s.visible };
  }

  toggleRail(): void {
    this.railExpanded = !this.railExpanded;
  }

  /** The active module's main view, once it has loaded. */
  get activeView(): AnyComponent | null {
    const loaded = this.#loaded;
    return loaded && loaded.module === this.activeModule ? loaded.view : null;
  }

  /**
   * Shows a module. The rail follows at once; while a view is still
   * loading, a newer request wins, so the last module asked for is the one
   * shown.
   */
  async openModule(id: string): Promise<void> {
    const module = this.registry.module(id);
    if (!module) return;
    const request = ++this.#request;
    const start = `module-open-start:${id}`;
    mark(start);
    const previous = this.activeModule;
    this.activeModule = id;
    // A module may only contribute tab types; its tabs still need it active.
    if (!module.view) return;
    let view = this.#views[id];
    if (!view) {
      try {
        view = (await module.view()).default;
      } catch (e) {
        console.error(`Cannot load the ${id} view`, e);
        if (request === this.#request) this.activeModule = previous;
        return;
      }
      this.#views[id] = view;
    }
    if (request !== this.#request) return;
    this.#loaded = { module: id, view };
    await tick();
    measure(`module-open:${id}`, start);
  }

  snapshot(): ShellSnapshot {
    return {
      activeModule: this.activeModule,
      railExpanded: this.railExpanded,
      sidebars: $state.snapshot(this.#sidebars),
    };
  }

  restore(saved: UiState): void {
    this.railExpanded = saved.railExpanded;
    this.#sidebars = { ...saved.sidebars };
  }
}

/** A window as Dea first opens it: no tabs, the default layout. */
export function emptyUiState(): UiState {
  return {
    version: 1,
    activeModule: null,
    activeEnvironment: null,
    railExpanded: false,
    sidebars: {},
    tabs: [],
    activeTab: null,
    closedTabs: [],
  };
}

/**
 * Saves what `collect` returns for window `slot` of project `scope` each time it changes, after `delay` ms
 * without another change. The first value is the restored one, so it is
 * not written back. Returns a function that stops saving.
 */
export function persistUiState(
  scope: string,
  slot: number,
  collect: () => Partial<UiState>,
  delay = 300,
): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let first = true;
  const stop = $effect.root(() => {
    $effect(() => {
      const state: UiState = { ...emptyUiState(), ...collect() };
      if (first) {
        first = false;
        return;
      }
      clearTimeout(timer);
      // Losing a layout change is not worth bothering anyone about.
      timer = setTimeout(() => {
        saveUiState(scope, slot, state).catch(() => {});
      }, delay);
    });
  });
  return () => {
    clearTimeout(timer);
    stop();
  };
}

/** The application's shell. */
export const shell = new Shell(registry);
