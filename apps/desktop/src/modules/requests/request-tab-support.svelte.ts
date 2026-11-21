// The request editor's tests: a shell whose tabs are state, as the real
// one's are, and a way to show a request tab in it.

import { render } from "@testing-library/svelte";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import { FakeProject } from "./fake-project.svelte";
import FolderTab from "./FolderTab.svelte";
import RequestTab from "./RequestTab.svelte";
import { resetDrafts } from "./state/drafts.svelte";
import { resetExchanges } from "./state/exchange.svelte";
import { resetFolderStates } from "./state/folders.svelte";

export interface FakeTab {
  id: string;
  tabType: string;
  title: string;
  params: { path: string; method?: string };
  dirty: boolean;
}

export class FakeShell implements ShellApi {
  tabs = $state<FakeTab[]>([]);
  saveHandlers: Record<string, (tabId: string) => Promise<void>> = {};
  toasts: string[] = [];
  /** The action of each toast, by the same index as `toasts`. */
  toastActions: ({ label: string; run: () => void } | undefined)[] = [];
  /** Every tab closeTab was asked to close. */
  closing: string[] = [];
  /** Every tab activateTab was asked to show. */
  activated: string[] = $state([]);
  /** Every command runCommand was asked to run. */
  commands: string[] = [];
  #opened = 0;

  tab(id: string): FakeTab {
    const tab = this.tabs.find((t) => t.id === id);
    if (!tab) throw new Error(`No tab ${id}`);
    return tab;
  }

  /** Saves a tab as `Mod+S` does: through its type's save handler. */
  save(id: string): Promise<void> {
    const save = this.saveHandlers[this.tab(id).tabType];
    if (!save) throw new Error("No save handler");
    return save(id);
  }

  openTab(tabType: string, title: string, params: unknown): string {
    const id = `tab-${++this.#opened}`;
    this.tabs.push({
      id,
      tabType,
      title,
      params: params as { path: string; method?: string },
      dirty: false,
    });
    return id;
  }

  findTab(
    tabType: string,
    match?: (params: unknown) => boolean,
  ): string | undefined {
    return this.tabs.find(
      (t) => t.tabType === tabType && (!match || match(t.params)),
    )?.id;
  }

  updateTab(id: string, patch: { title?: string; params?: unknown }): void {
    const tab = this.tabs.find((t) => t.id === id);
    if (!tab) return;
    if (patch.title !== undefined) tab.title = patch.title;
    if (patch.params !== undefined)
      tab.params = patch.params as { path: string; method?: string };
  }

  activateTab(id: string): boolean {
    this.activated.push(id);
    return this.tabs.some((t) => t.id === id);
  }

  /** Like the shell's, except that a tab with unsaved changes stays open. */
  closeTab(id: string): Promise<boolean> {
    this.closing.push(id);
    const tab = this.tabs.find((t) => t.id === id);
    if (!tab || tab.dirty) return Promise.resolve(false);
    this.tabs = this.tabs.filter((t) => t !== tab);
    return Promise.resolve(true);
  }

  setTabDirty(id: string, dirty: boolean): void {
    const tab = this.tabs.find((t) => t.id === id);
    if (tab) tab.dirty = dirty;
  }

  activeTab(): { id: string; tabType: string; params: unknown } | null {
    const tab = this.activated.length
      ? this.tabs.find((t) => t.id === this.activated.at(-1))
      : undefined;
    return tab
      ? { id: tab.id, tabType: tab.tabType, params: tab.params }
      : null;
  }

  isTabDirty(id: string): boolean {
    return this.tabs.some((t) => t.id === id && t.dirty);
  }

  setSaveHandler(tabType: string, save: (tabId: string) => Promise<void>) {
    this.saveHandlers[tabType] = save;
  }

  openModule(): Promise<void> {
    return Promise.resolve();
  }

  runCommand(id: string): Promise<void> {
    this.commands.push(id);
    return Promise.resolve();
  }

  toast(
    text: string,
    opts?: { action?: { label: string; run: () => void } },
  ): number {
    this.toasts.push(text);
    this.toastActions.push(opts?.action);
    return this.toasts.length;
  }

  dismissToast(): void {}
}

let shell: FakeShell | null = null;

/**
 * A fresh shell and project, with no drafts. Call before each test; the
 * project starts writable with `local` and `staging` and none active.
 */
export function setUpRequestTabs(): { shell: FakeShell; project: FakeProject } {
  resetDrafts();
  resetExchanges();
  resetFolderStates();
  shell = new FakeShell();
  const project = new FakeProject([
    { name: "local", production: false },
    { name: "staging", production: false },
  ]);
  setShellApi(shell);
  setProjectApi(project);
  return { shell, project };
}

/** Opens a tab for the request at `path` and shows it, as the shell does. */
export function openRequestTab(
  target: string | { path: string; method?: string },
  title = "Request",
): { id: string; unmount: () => void } {
  if (!shell) throw new Error("Call setUpRequestTabs first");
  const id = shell.openTab(
    "requests.request",
    title,
    typeof target === "string" ? { path: target } : target,
  );
  const { unmount } = render(RequestTab, { props: { tab: shell.tab(id) } });
  return { id, unmount };
}

/** Opens a tab for the folder at `path` and shows it, as the shell does. */
export function openFolderTab(
  path: string,
  title = "Folder",
): { id: string; unmount: () => void } {
  if (!shell) throw new Error("Call setUpRequestTabs first");
  const id = shell.openTab("requests.folder", title, { path });
  const { unmount } = render(FolderTab, { props: { tab: shell.tab(id) } });
  return { id, unmount };
}
