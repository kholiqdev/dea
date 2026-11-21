// Where the response panel sits: the one place that writes the setting.

import { shellApi } from "$app/contributions/shell-api";
import type { ResponseLayout } from "$lib/ipc/bindings/ResponseLayout";
import { errorMessage } from "$lib/ipc/errors";
import { settings } from "$lib/settings/settings.svelte";

/** Saves the position; rejects when the save is refused. */
export function setResponseLayout(
  responseLayout: ResponseLayout,
): Promise<void> {
  return settings.update({ requests: { responseLayout } });
}

/** Moves the panel below the request, or back to its right; a refusal is a toast. */
export async function toggleResponseLayout(): Promise<void> {
  try {
    await setResponseLayout(
      settings.requests.responseLayout === "right" ? "below" : "right",
    );
  } catch (e) {
    shellApi().toast(errorMessage(e));
  }
}
