// IPC fakes for the Requests tests: an in-memory request tree that answers
// every request command and records the calls.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { FileErrorInfo } from "$lib/ipc/bindings/FileErrorInfo";
import type { FolderFile } from "$lib/ipc/bindings/FolderFile";
import type { FolderNode } from "$lib/ipc/bindings/FolderNode";
import type { Inspection } from "$lib/ipc/bindings/Inspection";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { RequestLeaf } from "$lib/ipc/bindings/RequestLeaf";
import type { RequestNode } from "$lib/ipc/bindings/RequestNode";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";

export interface RequestsIpc {
  calls: [string, Record<string, unknown>][];
  tree: RequestTreeInfo;
  /** The request files `save_request` wrote, by path. */
  files: Map<string, RequestFile>;
  folders: Map<string, FolderFile>;
}

export function leaf(
  path: string,
  name: string,
  method = "GET",
  url = "",
): RequestNode {
  return { kind: "request", path, name, method, url };
}

export function folder(
  path: string,
  name: string,
  items: RequestNode[],
  broken = false,
): FolderNode & { kind: "folder" } {
  return {
    kind: "folder",
    path,
    name,
    broken,
    requestCount: items.reduce(
      (n, i) => n + (i.kind === "folder" ? i.requestCount : 1),
      0,
    ),
    items,
  };
}

/** One collection with `count` requests named `shipment 0`, `shipment 1`, … */
export function bigTree(count: number): RequestTreeInfo {
  const items: RequestNode[] = [];
  for (let i = 0; i < count; i++)
    items.push(
      leaf(
        `requests/shipments/shipment-${i}.yaml`,
        `shipment ${i}`,
        "GET",
        `https://harbor.test/shipments/${i}`,
      ),
    );
  return {
    collections: [folder("requests/shipments", "Shipments", items)],
    errors: [],
  };
}

const EMPTY_INSPECTION: Inspection = {
  variables: [],
  undefined: [],
  automaticHeaders: [],
  options: {
    timeoutMs: 30000,
    followRedirects: true,
    maxRedirects: 10,
    verifyTls: true,
    useSystemProxy: true,
    cookies: true,
  },
};

function find(
  nodes: RequestNode[],
  path: string,
): { list: RequestNode[]; index: number } | null {
  for (let index = 0; index < nodes.length; index++) {
    const node = nodes[index];
    if (!node) continue;
    if (node.path === path) return { list: nodes, index };
    if (node.kind === "folder") {
      const hit = find(node.items, path);
      if (hit) return hit;
    }
  }
  return null;
}

function recount(node: RequestNode): number {
  if (node.kind === "request") return 1;
  node.requestCount = node.items.reduce((n, i) => n + recount(i), 0);
  return node.requestCount;
}

function slug(name: string): string {
  return name.trim().toLowerCase().replace(/\s+/g, "-");
}

export function fakeRequestsIpc(
  opts: { tree?: RequestTreeInfo; errors?: FileErrorInfo[] } = {},
): RequestsIpc {
  const ipc: RequestsIpc = {
    calls: [],
    tree: opts.tree ?? { collections: [], errors: [] },
    files: new Map(),
    folders: new Map(),
  };
  if (opts.errors) ipc.tree.errors = opts.errors;
  const trash = new Map<
    number,
    { parent: string; index: number; node: RequestNode }
  >();
  let nextId = 1;

  // The collections carry `kind: "folder"` at runtime, like every folder.
  const roots = () => ipc.tree.collections as RequestNode[];
  const folderAt = (path: string): RequestNode[] | null => {
    if (path === "requests") return roots();
    const hit = find(roots(), path);
    const node = hit?.list[hit.index];
    return node?.kind === "folder" ? node.items : null;
  };
  const parentOf = (path: string) => path.slice(0, path.lastIndexOf("/"));
  const refresh = () => roots().forEach(recount);

  clearMocks();
  mockIPC((cmd, raw) => {
    const args = (raw ?? {}) as Record<string, unknown>;
    ipc.calls.push([cmd, args]);
    switch (cmd) {
      case "request_tree":
        return structuredClone(ipc.tree);
      case "read_request": {
        const path = args.path as string;
        const saved = ipc.files.get(path);
        if (saved) return saved;
        const hit = find(roots(), path);
        const node = hit?.list[hit.index] as RequestLeaf | undefined;
        return {
          name: node?.name ?? "",
          method: node?.method ?? "GET",
          url: node?.url ?? "",
        };
      }
      case "save_request": {
        const path = args.path as string;
        const request = args.request as RequestFile;
        ipc.files.set(path, request);
        const hit = find(roots(), path);
        const node = hit?.list[hit.index];
        if (node?.kind === "request") {
          node.method = request.method;
          node.url = request.url;
        }
        return null;
      }
      case "read_folder":
        return ipc.folders.get(args.path as string) ?? {};
      case "save_folder":
        ipc.folders.set(args.path as string, args.settings as FolderFile);
        return null;
      case "create_request": {
        const items = folderAt(args.folder as string);
        const name = args.name as string;
        const path = `${args.folder as string}/${slug(name)}.yaml`;
        items?.push(leaf(path, name));
        refresh();
        return path;
      }
      case "create_folder": {
        const items = folderAt(args.parent as string);
        const name = args.name as string;
        const path = `${args.parent as string}/${slug(name)}`;
        items?.push(folder(path, name, []));
        refresh();
        return path;
      }
      case "rename_request_item": {
        const path = args.path as string;
        const hit = find(roots(), path);
        if (!hit) return path;
        const node = hit.list[hit.index];
        if (!node) return path;
        const name = args.name as string;
        node.name = name;
        node.path =
          node.kind === "folder"
            ? `${parentOf(path)}/${slug(name)}`
            : `${parentOf(path)}/${slug(name)}.yaml`;
        return node.path;
      }
      case "duplicate_request_item": {
        const path = args.path as string;
        const hit = find(roots(), path);
        if (!hit) return path;
        const original = hit.list[hit.index];
        if (!original) return path;
        const copy = structuredClone(original);
        copy.name = `${copy.name} copy`;
        copy.path = path.replace(/(\.yaml)?$/, "-copy$1");
        hit.list.splice(hit.index + 1, 0, copy);
        refresh();
        return copy.path;
      }
      case "move_request_item": {
        const path = args.path as string;
        const hit = find(roots(), path);
        const target = folderAt(args.folder as string);
        if (!hit || !target) return path;
        const [node] = hit.list.splice(hit.index, 1);
        if (!node) return path;
        node.path = `${args.folder as string}/${path.slice(path.lastIndexOf("/") + 1)}`;
        target.splice(args.index as number, 0, node);
        refresh();
        return node.path;
      }
      case "delete_request_item": {
        const path = args.path as string;
        const hit = find(roots(), path);
        if (!hit) return 0;
        const [node] = hit.list.splice(hit.index, 1);
        if (!node) return 0;
        const id = nextId++;
        trash.set(id, { parent: parentOf(path), index: hit.index, node });
        refresh();
        return id;
      }
      case "restore_request_item": {
        const entry = trash.get(args.id as number);
        if (entry) {
          folderAt(entry.parent)?.splice(entry.index, 0, entry.node);
          trash.delete(args.id as number);
          refresh();
        }
        return null;
      }
      case "inspect_request":
        return EMPTY_INSPECTION;
      case "pick_request_file":
        return null;
      case "describe_file_ref":
        return {
          value: args.value as string,
          name: (args.value as string).split("/").pop() ?? "",
          outside: false,
          missing: false,
        };
      default:
        return null;
    }
  });
  return ipc;
}
