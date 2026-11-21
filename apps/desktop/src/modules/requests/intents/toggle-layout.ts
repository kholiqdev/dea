// Toggle response position (the palette): moves the response panel below
// the request, or back to its right.

import { shellApi } from "$app/contributions/shell-api";
import { errorMessage } from "$lib/ipc/errors";
import { settings } from "$lib/settings/settings.svelte";

export default async function toggleLayout(): Promise<void> {
  const responseLayout =
    settings.requests.responseLayout === "right" ? "below" : "right";
  try {
    await settings.update({ requests: { responseLayout } });
  } catch (e) {
    shellApi().toast(errorMessage(e));
  }
}
