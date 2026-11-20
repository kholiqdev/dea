// Requests in the command palette: one item per request of the tree, found
// by name, opened in its tab or a new one.

import { shellApi } from "$app/contributions/shell-api";
import type { PaletteItem } from "$app/contributions/types";
import type { RequestNode } from "$lib/ipc/bindings/RequestNode";
import { ROOT, treeState } from "./state/tree.svelte";

/** The tab type that shows one request, with its `path` as params. */
const REQUEST_TAB = "requests.request";

/**
 * Every request of the tree, depth first, as palette items. The tree is
 * read each time, as the sidebar may not be showing it.
 */
export default async function requestItems(): Promise<PaletteItem[]> {
  await treeState.load();
  const out: PaletteItem[] = [];
  const visit = (nodes: readonly RequestNode[]) => {
    for (const node of nodes) {
      if (node.kind === "folder") visit(node.items);
      else out.push(requestItem(node));
    }
  };
  visit(treeState.itemsOf(ROOT) ?? []);
  return out;
}

function requestItem(
  node: Extract<RequestNode, { kind: "request" }>,
): PaletteItem {
  const { path, name } = node;
  return {
    id: path,
    title: name,
    meta: `${node.method} ${node.url}`,
    opensTab: true,
    open: (target) => {
      // The palette lists what was there when it opened; the tree may have
      // changed since.
      const now = treeState.find(path);
      if (now?.kind !== "request") {
        shellApi().toast(`${name} no longer exists`);
        return;
      }
      const shell = shellApi();
      if (target === "active") {
        const id = shell.findTab(
          REQUEST_TAB,
          (params) => (params as { path?: string })?.path === path,
        );
        if (id !== undefined && shell.activateTab(id)) return;
      }
      shell.openTab(REQUEST_TAB, now.name, { path });
    },
  };
}
