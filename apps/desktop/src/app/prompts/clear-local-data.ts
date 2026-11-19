// Clearing what Dea keeps for a project on this device: the layout and
// open tabs of its windows. Project files are not touched.

import { clearLocalData as clearOnDisk } from "$lib/ipc/project";
import { emptyUiState, type Shell } from "../shell/shell-state.svelte";
import type { Tabs } from "../tabs/tabs.svelte";
import { toasts } from "./toasts.svelte";

export async function clearLocalData(shell: Shell, tabs: Tabs): Promise<void> {
  if (tabs.tabs.some((t) => t.dirty)) {
    toasts.show("Save or close the tabs with changes first");
    return;
  }
  // Reset before clearing: the next save then holds the empty layout, and a
  // save of the old one still waiting is dropped.
  const empty = emptyUiState();
  shell.restore(empty);
  tabs.restore(empty);
  await clearOnDisk();
  toasts.show("Local data cleared");
}
