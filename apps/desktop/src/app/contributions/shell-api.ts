// What modules may ask of the shell. The shell provides the implementation
// when it starts, so modules never import the rest of src/app.

import type { TabEnvironment } from "$lib/ipc/bindings/TabEnvironment";

export type { TabEnvironment };

export interface OpenTabOptions {
  environment?: TabEnvironment;
}

export interface ShellApi {
  /** Opens a tab and returns its id. */
  openTab(
    tabType: string,
    title: string,
    params: unknown,
    opts?: OpenTabOptions,
  ): string;
  /**
   * The id of an open tab of `tabType`, if there is one. With `match`, the
   * first such tab whose params it accepts.
   */
  findTab(
    tabType: string,
    match?: (params: unknown) => boolean,
  ): string | undefined;
  /** Changes a tab's title or params without remounting its view. */
  updateTab(id: string, patch: { title?: string; params?: unknown }): void;
  /** The tab being shown, or null when none is open. */
  activeTab(): { id: string; tabType: string; params: unknown } | null;
  /** Shows an open tab; false when no tab has that id. */
  activateTab(id: string): boolean;
  /** Closes a tab, asking first when it has unsaved changes. */
  closeTab(id: string): Promise<boolean>;
  setTabDirty(id: string, dirty: boolean): void;
  /** Whether a tab has unsaved changes; false when no tab has that id. */
  isTabDirty(id: string): boolean;
  /**
   * Registers how tabs of `tabType` are saved, for the Save choice when a
   * tab with unsaved changes is closed. A rejection keeps the tab open.
   */
  setSaveHandler(tabType: string, save: (tabId: string) => Promise<void>): void;
  openModule(id: string): Promise<void>;
  /**
   * Shows a module's sidebar. In a window too narrow for sidebars it shows
   * until the window widens, and the saved layout stays as it was.
   */
  showSidebar(module: string): void;
  /** Runs a registered command, as its shortcut would. */
  runCommand(id: string): Promise<void>;
  /**
   * Shows a short notice at the bottom of the window and returns its id. An
   * action may have a chord, which runs it while the notice shows and no
   * text field has the focus.
   */
  toast(
    text: string,
    opts?: {
      action?: { label: string; run: () => void; chord?: string };
      timeout?: number;
    },
  ): number;
  dismissToast(id: number): void;
  /**
   * Whether `Send this request to production?` shows, so a send waits for
   * its answer. Reactive.
   */
  readonly confirmingSend: boolean;
}

let current: ShellApi | null = null;

export function setShellApi(api: ShellApi): void {
  current = api;
}

export function shellApi(): ShellApi {
  if (!current) throw new Error("The shell has not started yet");
  return current;
}
