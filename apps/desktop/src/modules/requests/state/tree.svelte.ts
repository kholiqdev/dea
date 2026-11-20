// The request tree of this window's project, as the sidebar shows it.

import { SvelteSet } from "svelte/reactivity";
import { shellApi } from "$app/contributions/shell-api";
import type { RequestNode } from "$lib/ipc/bindings/RequestNode";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { errorMessage, isIpcError } from "$lib/ipc/errors";
import {
  createFolder,
  createRequest,
  deleteRequestItem,
  duplicateRequestItem,
  moveRequestItem,
  renameRequestItem,
  requestTree,
  restoreRequestItem,
} from "../api";
import { drafts } from "./drafts.svelte";

/** The folder that holds the collections. */
export const ROOT = "requests";

/** The height of a row of the tree, in pixels. */
export const ROW_HEIGHT = 28;

/** How long a deleted item can be brought back, in milliseconds. */
export const UNDO_TIMEOUT = 10_000;

/** The tab types whose params name a request or folder by `path`. */
const PATH_TABS = ["requests.request", "requests.folder"];

export type TreeRowData = {
  kind: "folder" | "request";
  path: string;
  name: string;
  depth: number;
  /** Place among the siblings shown, from 1, and how many there are. */
  position: number;
  size: number;
  method?: string;
  count?: number;
  broken?: boolean;
  open?: boolean;
};

/** A folder or collection the Move dialog offers. */
export type FolderChoice = { path: string; name: string; depth: number };

/** The folder a request or folder is in; `requests` for a collection. */
export function parentOf(path: string): string {
  return path.slice(0, path.lastIndexOf("/"));
}

/** Whether `path` is `folder` or lies somewhere below it. */
export function isWithin(path: string, folder: string): boolean {
  return path === folder || path.startsWith(`${folder}/`);
}

export class RequestTreeState {
  info = $state.raw<RequestTreeInfo | null>(null);
  filter = $state("");
  expanded = new SvelteSet<string>();
  selected = $state<string | null>(null);
  /** The item whose name is being edited in its row. */
  renaming = $state<string | null>(null);

  async load(): Promise<void> {
    this.info = await requestTree();
  }

  /** A request or folder of the loaded tree. */
  find(path: string): RequestNode | null {
    let nodes = this.#collections();
    for (;;) {
      const node = nodes.find((n) => isWithin(path, n.path));
      if (!node || node.path === path) return node ?? null;
      if (node.kind !== "folder") return null;
      nodes = node.items;
    }
  }

  /** What a folder holds, in order; the collections for `requests`. */
  itemsOf(folder: string): readonly RequestNode[] | null {
    if (folder === ROOT) return this.#collections();
    const node = this.find(folder);
    return node?.kind === "folder" ? node.items : null;
  }

  /**
   * Where the Move dialog can put an item: every collection and folder,
   * depth first, except the item, what is inside it, and where it is now.
   */
  moveTargets(path: string): FolderChoice[] {
    const out: FolderChoice[] = [];
    const visit = (nodes: readonly RequestNode[], depth: number) => {
      for (const node of nodes) {
        if (node.kind !== "folder" || isWithin(node.path, path)) continue;
        if (node.path !== parentOf(path))
          out.push({ path: node.path, name: node.name, depth });
        visit(node.items, depth + 1);
      }
    };
    visit(this.#collections(), 0);
    return out;
  }

  /**
   * Creates `New request` and starts naming it. Without a folder it goes
   * into the selected folder, next to the selected request, or into the
   * first collection; a project without one gets `New collection` first.
   */
  async newRequest(folder?: string): Promise<void> {
    await this.#change(async () => {
      if (!this.info) await this.load();
      const into =
        folder ??
        this.#selectedFolder() ??
        this.info?.collections[0]?.path ??
        (await createFolder(ROOT, "New collection"));
      this.#startNaming(await createRequest(into, "New request"));
    });
  }

  /** Creates `New folder`, or `New collection` at the top, and starts naming it. */
  async newFolder(parent: string): Promise<void> {
    const name = parent === ROOT ? "New collection" : "New folder";
    await this.#change(async () => {
      this.#startNaming(await createFolder(parent, name));
    });
  }

  async rename(path: string, name: string): Promise<void> {
    this.renaming = null;
    if (!name.trim() || name === this.find(path)?.name) return;
    await this.#change(async () => {
      this.#moved(path, await renameRequestItem(path, name), name);
    });
  }

  /** Copies an item next to it and selects the copy. */
  async duplicate(path: string): Promise<void> {
    await this.#change(async () => {
      this.#show(await duplicateRequestItem(path));
    });
  }

  /**
   * Moves an item into `folder` at `index`, counted after it left its
   * place, and shows it there.
   */
  async move(path: string, folder: string, index: number): Promise<void> {
    await this.#change(async () => {
      const to = await moveRequestItem(path, folder, index);
      this.#moved(path, to);
      this.#show(to);
    });
  }

  /**
   * Deletes an item and offers Undo for a while. Its tabs close without
   * asking, as the files come back with Undo; a tab with unsaved edits
   * stays open.
   */
  async remove(path: string): Promise<void> {
    const name = this.find(path)?.name ?? path.slice(path.lastIndexOf("/") + 1);
    await this.#change(async () => {
      const id = await deleteRequestItem(path);
      this.#forget(path);
      shellApi().toast(`Deleted ${name}`, {
        action: {
          label: "Undo",
          chord: "Mod+Z",
          run: () => void this.#restore(id, path, name),
        },
        timeout: UNDO_TIMEOUT,
      });
    });
  }

  async #restore(id: number, path: string, name: string): Promise<void> {
    try {
      await restoreRequestItem(id);
      this.#show(path);
    } catch (e) {
      shellApi().toast(`${name} could not be restored: ${reason(e)}`);
    }
    await this.reload();
  }

  /** Runs a change, says why when it fails, and loads the tree again. */
  async #change(change: () => Promise<void>): Promise<void> {
    try {
      await change();
    } catch (e) {
      shellApi().toast(errorMessage(e));
    }
    await this.reload();
  }

  /** Loads the tree, and says why when it cannot. */
  async reload(): Promise<void> {
    try {
      await this.load();
    } catch (e) {
      shellApi().toast(errorMessage(e));
    }
  }

  #collections(): RequestNode[] {
    return (this.info?.collections ?? []).map((c) => ({
      kind: "folder",
      ...c,
    }));
  }

  #selectedFolder(): string | null {
    if (!this.selected) return null;
    const node = this.find(this.selected);
    if (!node) return null;
    return node.kind === "folder" ? node.path : parentOf(node.path);
  }

  /** Opens the folders above an item and selects it. */
  #show(path: string): void {
    for (let at = path.indexOf("/", ROOT.length + 1); at > 0;) {
      this.expanded.add(path.slice(0, at));
      at = path.indexOf("/", at + 1);
    }
    this.selected = path;
  }

  #startNaming(path: string): void {
    // A filter could hide the new item.
    this.filter = "";
    this.#show(path);
    this.renaming = path;
  }

  /** An item, and all in it, has a new path: the tree and its tabs follow. */
  #moved(from: string, to: string, title?: string): void {
    const moved = (path: string) => to + path.slice(from.length);
    for (const path of [...this.expanded])
      if (isWithin(path, from)) {
        this.expanded.delete(path);
        this.expanded.add(moved(path));
      }
    if (this.selected && isWithin(this.selected, from))
      this.selected = moved(this.selected);
    eachTabWithin(from, (id, params) => {
      const next = { ...params, path: moved(params.path) };
      shellApi().updateTab(
        id,
        params.path === from && title !== undefined
          ? { title, params: next }
          : { params: next },
      );
      // A tab not shown has no view to follow the path, and its edits are
      // saved from the draft.
      drafts.get(id)?.moveTo(next.path);
      return next;
    });
  }

  /** An item is gone: so are its tabs and its place in the tree. */
  #forget(path: string): void {
    for (const open of [...this.expanded])
      if (isWithin(open, path)) this.expanded.delete(open);
    if (this.selected && isWithin(this.selected, path)) this.selected = null;
    if (this.renaming && isWithin(this.renaming, path)) this.renaming = null;
    // Undo brings back the file, not a tab's unsaved edits: such a tab
    // stays open as it is.
    const shell = shellApi();
    eachTabWithin(path, (id) => {
      if (!shell.isTabDirty(id)) void shell.closeTab(id);
    });
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
    return withPositions(out);
  }
}

type PathParams = { path: string };

function hasPath(params: unknown): params is PathParams {
  return (
    typeof params === "object" &&
    params !== null &&
    typeof (params as PathParams).path === "string"
  );
}

/**
 * Calls `visit` once for every open tab of `path` or of anything in it.
 * `visit` returns the params it gave the tab, if any.
 */
function eachTabWithin(
  path: string,
  visit: (id: string, params: PathParams) => unknown,
): void {
  const shell = shellApi();
  const seen: unknown[] = [];
  for (const type of PATH_TABS)
    for (;;) {
      let hit: PathParams | undefined;
      const id = shell.findTab(type, (params) => {
        if (seen.includes(params) || !hasPath(params)) return false;
        if (!isWithin(params.path, path)) return false;
        hit = params;
        return true;
      });
      if (id === undefined || !hit) break;
      seen.push(hit, visit(id, hit));
    }
}

/** A command's message as part of a sentence: as Dea wrote it. */
function reason(e: unknown): string {
  if (isIpcError(e)) return e.message;
  return e instanceof Error ? e.message : String(e);
}

/** Sets each row's place among the siblings that share its parent. */
function withPositions(rows: TreeRowData[]): TreeRowData[] {
  // The sibling group at each depth; going back up drops the deeper ones.
  const groups: TreeRowData[][] = [];
  const all: TreeRowData[][] = [];
  for (const row of rows) {
    groups.length = row.depth + 1;
    let group = groups[row.depth];
    if (!group) {
      group = groups[row.depth] = [];
      all.push(group);
    }
    group.push(row);
    row.position = group.length;
  }
  for (const group of all) for (const row of group) row.size = group.length;
  return rows;
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
    position: 0,
    size: 0,
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
    position: 0,
    size: 0,
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
  treeState.renaming = null;
}
