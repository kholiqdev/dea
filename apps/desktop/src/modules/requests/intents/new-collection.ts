// New collection, from the palette: shows the Requests sidebar and names
// the new collection there.

import { shellApi } from "$app/contributions/shell-api";
import { ROOT, treeState } from "../state/tree.svelte";

export default async function newCollection(): Promise<void> {
  await shellApi().openModule("requests");
  await treeState.newFolder(ROOT);
}
