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
  /** The id of an open tab of `tabType`, if there is one. */
  findTab(tabType: string): string | undefined;
  /** Shows an open tab; false when no tab has that id. */
  activateTab(id: string): boolean;
  /** Closes a tab, asking first when it has unsaved changes. */
  closeTab(id: string): Promise<boolean>;
  setTabDirty(id: string, dirty: boolean): void;
  /**
   * Registers how tabs of `tabType` are saved, for the Save choice when a
   * tab with unsaved changes is closed. A rejection keeps the tab open.
   */
  setSaveHandler(tabType: string, save: (tabId: string) => Promise<void>): void;
  openModule(id: string): Promise<void>;
  /** Shows a short notice at the bottom of the window. */
  toast(text: string): void;
}

let current: ShellApi | null = null;

export function setShellApi(api: ShellApi): void {
  current = api;
}

export function shellApi(): ShellApi {
  if (!current) throw new Error("The shell has not started yet");
  return current;
}
