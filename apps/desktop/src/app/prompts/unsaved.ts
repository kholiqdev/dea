// The check before something that would drop open tabs.

import type { Tabs } from "../tabs/tabs.svelte";
import { toasts } from "./toasts.svelte";

/** True, after saying so, when a tab has changes that are not saved. */
export function blockedByUnsaved(tabs: Tabs): boolean {
  if (!tabs.tabs.some((t) => t.dirty)) return false;
  toasts.show("Save or close the tabs with changes first");
  return true;
}
