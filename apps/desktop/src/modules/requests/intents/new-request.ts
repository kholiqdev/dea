// New request, from the palette: shows the Requests sidebar and names the
// new request there.

import { shellApi } from "$app/contributions/shell-api";
import { treeState } from "../state/tree.svelte";

export default async function newRequest(): Promise<void> {
  await shellApi().openModule("requests");
  await treeState.newRequest();
}
