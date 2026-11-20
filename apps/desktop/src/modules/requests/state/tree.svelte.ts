// The request tree of this window's project, as the sidebar shows it.

import { SvelteSet } from "svelte/reactivity";
import type { RequestNode } from "$lib/ipc/bindings/RequestNode";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { requestTree } from "../api";

export type TreeRowData = {
  kind: "folder" | "request";
  path: string;
  name: string;
  depth: number;
  method?: string;
  count?: number;
  broken?: boolean;
  open?: boolean;
};

export class RequestTreeState {
  info = $state.raw<RequestTreeInfo | null>(null);
  filter = $state("");
  expanded = new SvelteSet<string>();
  selected = $state<string | null>(null);

  async load(): Promise<void> {
    this.info = await requestTree();
  }

  toggle(path: string): void {
    if (this.expanded.has(path)) this.expanded.delete(path);
    else this.expanded.add(path);
  }

  /**
   * The rows to show, depth first. Without a filter a folder is open when the
   * person opened it. With one, only requests whose name or URL contains it
   * show, under their folders (open); a folder whose name contains it shows
   * everything inside.
   */
  get rows(): TreeRowData[] {
    const out: TreeRowData[] = [];
    const query = this.filter.trim().toLowerCase();
    const items = this.info?.collections ?? [];
    for (const collection of items) {
      const node: RequestNode = { kind: "folder", ...collection };
      if (query) visitFiltered(node, 0, false, query, out);
      else visit(node, 0, this.expanded, out);
    }
    return out;
  }
}

function folderRow(
  node: Extract<RequestNode, { kind: "folder" }>,
  depth: number,
  open: boolean,
): TreeRowData {
  return {
    kind: "folder",
    path: node.path,
    name: node.name,
    depth,
    count: node.requestCount,
    broken: node.broken,
    open,
  };
}

function requestRow(
  node: Extract<RequestNode, { kind: "request" }>,
  depth: number,
): TreeRowData {
  return {
    kind: "request",
    path: node.path,
    name: node.name,
    depth,
    method: node.method,
  };
}

function visit(
  node: RequestNode,
  depth: number,
  expanded: ReadonlySet<string>,
  out: TreeRowData[],
): void {
  if (node.kind === "request") {
    out.push(requestRow(node, depth));
    return;
  }
  const open = expanded.has(node.path);
  out.push(folderRow(node, depth, open));
  if (open)
    for (const item of node.items) visit(item, depth + 1, expanded, out);
}

function visitFiltered(
  node: RequestNode,
  depth: number,
  all: boolean,
  query: string,
  out: TreeRowData[],
): void {
  if (node.kind === "request") {
    if (
      all ||
      node.name.toLowerCase().includes(query) ||
      node.url.toLowerCase().includes(query)
    )
      out.push(requestRow(node, depth));
    return;
  }
  const named = all || node.name.toLowerCase().includes(query);
  const at = out.length;
  out.push(folderRow(node, depth, true));
  for (const item of node.items)
    visitFiltered(item, depth + 1, named, query, out);
  if (!named && out.length === at + 1) out.pop();
}

export const treeState = new RequestTreeState();

export function resetTreeState(): void {
  treeState.info = null;
  treeState.filter = "";
  treeState.expanded.clear();
  treeState.selected = null;
}
