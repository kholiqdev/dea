// A send to production that is not confirmed gives the focus back. The
// dialog took it, and what had it may have gone while the send waited, as
// the response pane's own controls do.

import type { RequestDraft } from "../state/drafts.svelte";
import type { Exchange, SendResult } from "../state/exchange.svelte";

/**
 * Sends `draft` with `exchange`. When the send is not confirmed, the focus
 * goes back to what had it before, or to Send when that is gone or nothing
 * had it.
 */
export async function sendKeepingFocus(
  exchange: Exchange,
  draft: RequestDraft,
  opts: { anyway?: boolean } = {},
): Promise<SendResult> {
  const before = document.activeElement;
  const result = await exchange.send(draft, opts);
  if (result !== "notConfirmed") return result;
  const back =
    before instanceof HTMLElement &&
    before.isConnected &&
    before !== document.body
      ? before
      : document.querySelector<HTMLElement>("button[data-send]");
  back?.focus();
  return result;
}
