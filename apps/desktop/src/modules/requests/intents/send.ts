// Send request (Mod+Enter, the palette): sends the request tab being
// shown, from wherever the focus is in it.

import { shellApi } from "$app/contributions/shell-api";
import { drafts } from "../state/drafts.svelte";
import { exchangeFor } from "../state/exchange.svelte";

export default async function send(): Promise<void> {
  const shell = shellApi();
  const tab = shell.activeTab();
  if (tab?.tabType !== "requests.request") {
    shell.toast("Open a request to send it.");
    return;
  }
  // A request that could not be read has nothing to send.
  const draft = drafts.get(tab.id);
  if (!draft?.saved) return;
  await exchangeFor(tab.id).send(draft);
}
