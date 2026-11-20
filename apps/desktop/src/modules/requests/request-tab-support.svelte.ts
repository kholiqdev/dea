// The request editor's tests: a shell whose tabs are state, as the real
// one's are, and a way to show a request tab in it.

import { render } from "@testing-library/svelte";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import { FakeProject } from "./fake-project.svelte";
import RequestTab from "./RequestTab.svelte";
import { resetDrafts } from "./state/drafts.svelte";

export interface FakeTab {
  id: string;
  tabType: string;
  title: string;
  params: { path: string };
  dirty: boolean;
}

export class FakeShell implements ShellApi {
  tabs = $state<FakeTab[]>([]);
  saveHandlers: Record<string, (tabId: string) => Promise<void>> = {};
  toasts: string[] = [];
  /** Every tab closeTab was asked to close. */
  closing: string[] = [];

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
    const id = `tab-${this.tabs.length + 1}`;
    this.tabs.push({
      id,
      tabType,
      title,
      params: params as { path: string },
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
      tab.params = patch.params as { path: string };
  }

  activateTab(id: string): boolean {
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

  isTabDirty(id: string): boolean {
    return this.tabs.some((t) => t.id === id && t.dirty);
  }

  setSaveHandler(tabType: string, save: (tabId: string) => Promise<void>) {
    this.saveHandlers[tabType] = save;
  }

  openModule(): Promise<void> {
    return Promise.resolve();
  }

  runCommand(): Promise<void> {
    return Promise.resolve();
  }

  toast(text: string): number {
    this.toasts.push(text);
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
  path: string,
  title = "Request",
): { id: string; unmount: () => void } {
  if (!shell) throw new Error("Call setUpRequestTabs first");
  const id = shell.openTab("requests.request", title, { path });
  const { unmount } = render(RequestTab, { props: { tab: shell.tab(id) } });
  return { id, unmount };
}
