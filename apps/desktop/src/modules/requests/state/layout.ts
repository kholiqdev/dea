// Where the response panel sits: the one place that writes the setting.

import { shellApi } from "$app/contributions/shell-api";
import type { ResponseLayout } from "$lib/ipc/bindings/ResponseLayout";
import type { ResponseSplit } from "$lib/ipc/bindings/ResponseSplit";
import { errorMessage } from "$lib/ipc/errors";
import { settings } from "$lib/settings/settings.svelte";

export const SPLIT_MIN = 20;
export const SPLIT_MAX = 80;
export const SPLIT_DEFAULT: ResponseSplit = { right: 44, below: 40 };

/** A whole percent within the allowed range. */
export function clampSplit(percent: number): number {
  return Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, Math.round(percent)));
}

/** Saves the request panel's share for one layout, keeping the other's. */
export function setResponseSplit(
  layout: ResponseLayout,
  percent: number,
): Promise<void> {
  const responseSplit = {
    ...settings.requests.responseSplit,
    [layout]: clampSplit(percent),
  };
  return settings.update({ requests: { responseSplit } });
}

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
