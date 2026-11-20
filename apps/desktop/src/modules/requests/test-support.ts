// IPC fakes for the Requests tests: an in-memory request tree that answers
// every request command and records the calls.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { FileErrorInfo } from "$lib/ipc/bindings/FileErrorInfo";
import type { FileRef } from "$lib/ipc/bindings/FileRef";
import type { FolderFile } from "$lib/ipc/bindings/FolderFile";
import type { FolderNode } from "$lib/ipc/bindings/FolderNode";
import type { Inspection } from "$lib/ipc/bindings/Inspection";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { RequestNode } from "$lib/ipc/bindings/RequestNode";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { VariableLookup } from "./state/variables.svelte";
import type { IpcError } from "$lib/ipc/errors";

/** What `inspect_request` answers: fixed, or worked out from the draft. */
export type FakeInspection =
  | Partial<Inspection>
  | ((
      path: string,
      request: RequestFile,
    ) => Partial<Inspection> | Promise<Partial<Inspection>>);

export interface RequestsIpc {
  calls: [string, Record<string, unknown>][];
  tree: RequestTreeInfo;
  /**
   * The request files on disk, by path: given, or written by
   * `save_request`. A request in the tree without a file here reads as its
   * name, method and URL.
   */
  files: Map<string, RequestFile>;
  /** Every request `save_request` wrote, in order. */
  saved: RequestFile[];
  folders: Map<string, FolderFile>;
  /** Commands that reject, with the error each rejects with. */
  failures: Map<string, IpcError>;
  inspection: FakeInspection;
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

/** Gives a node, and everything in a folder, the path under `to`. */
function repath(node: RequestNode, to: string): void {
  const from = node.path;
  const visit = (n: RequestNode) => {
    n.path = to + n.path.slice(from.length);
    if (n.kind === "folder") n.items.forEach(visit);
  };
  visit(node);
}

function slug(name: string): string {
  return name.trim().toLowerCase().replace(/\s+/g, "-");
}

/** Gives the files of `from`, and of everything in it, the path under `to`. */
function moveFiles(
  files: Map<string, RequestFile>,
  from: string,
  to: string,
): void {
  for (const [path, file] of [...files]) {
    if (path !== from && !path.startsWith(`${from}/`)) continue;
    files.delete(path);
    files.set(to + path.slice(from.length), file);
  }
}

export function fakeRequestsIpc(
  opts: {
    tree?: RequestTreeInfo;
    errors?: FileErrorInfo[];
    files?: Record<string, RequestFile>;
    inspection?: FakeInspection;
    /** What the file chooser answers; `null`, the default, is a cancel. */
    pick?: FileRef | null;
    /** What `describe_file_ref` says about a value, over the plain default. */
    describe?: Record<string, Partial<FileRef>>;
    /** The names `vault_secrets` lists (one row each, in `local`). */
    secrets?: string[];
  } = {},
): RequestsIpc {
  const ipc: RequestsIpc = {
    calls: [],
    tree: opts.tree ?? { collections: [], errors: [] },
    files: new Map(Object.entries(opts.files ?? {})),
    saved: [],
    folders: new Map(),
    failures: new Map(),
    inspection: opts.inspection ?? {},
  };
  if (opts.errors) ipc.tree.errors = opts.errors;
  const trash = new Map<
    number,
    {
      parent: string;
      index: number;
      node: RequestNode;
      files: Map<string, RequestFile>;
    }
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
    const failure = ipc.failures.get(cmd);
    // Dea's commands reject with a plain { kind, message }.
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    if (failure) throw failure;
    switch (cmd) {
      case "request_tree":
        return structuredClone(ipc.tree);
      case "read_request": {
        const path = args.path as string;
        const saved = ipc.files.get(path);
        if (saved) return structuredClone(saved);
        const hit = find(roots(), path);
        const node = hit?.list[hit.index];
        if (node?.kind !== "request")
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { kind: "missing", message: `there is no request at ${path}` };
        return { name: node.name, method: node.method, url: node.url };
      }
      case "save_request": {
        const path = args.path as string;
        const request = args.request as RequestFile;
        ipc.files.set(path, structuredClone(request));
        ipc.saved.push(structuredClone(request));
        const hit = find(roots(), path);
        const node = hit?.list[hit.index];
        if (node?.kind === "request") {
          node.method = request.method;
          node.url = request.url;
        }
        return null;
      }
      case "read_folder":
      case "save_folder": {
        const path = args.path as string;
        // Like Dea, a folder that is not there is missing.
        if (path !== "requests" && folderAt(path) === null)
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { kind: "missing", message: `${path} does not exist` };
        if (cmd === "read_folder") return ipc.folders.get(path) ?? {};
        ipc.folders.set(path, args.settings as FolderFile);
        return null;
      }
      case "create_request": {
        const items = folderAt(args.folder as string);
        const name = args.name as string;
        const path = `${args.folder as string}/${slug(name)}.yaml`;
        items?.push(leaf(path, name));
        // Dea adds the new file to the folder's order.
        const parent = ipc.folders.get(args.folder as string) ?? {};
        ipc.folders.set(args.folder as string, {
          ...parent,
          order: [...(parent.order ?? []), slug(name)],
        });
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
        repath(
          node,
          node.kind === "folder"
            ? `${parentOf(path)}/${slug(name)}`
            : `${parentOf(path)}/${slug(name)}.yaml`,
        );
        // Dea writes a request's new name into its file.
        const file = ipc.files.get(path);
        if (file && node.kind === "request") file.name = name;
        moveFiles(ipc.files, path, node.path);
        // A folder's file keeps its settings, and Dea writes its new name.
        for (const [at, settings] of [...ipc.folders])
          if (at === path || at.startsWith(`${path}/`)) {
            ipc.folders.delete(at);
            ipc.folders.set(node.path + at.slice(path.length), {
              ...settings,
              ...(at === path ? { name } : {}),
            });
          }
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
        repath(copy, path.replace(/(\.yaml)?$/, "-copy$1"));
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
        repath(
          node,
          `${args.folder as string}/${path.slice(path.lastIndexOf("/") + 1)}`,
        );
        target.splice(args.index as number, 0, node);
        moveFiles(ipc.files, path, node.path);
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
        const files = new Map<string, RequestFile>();
        for (const [at, file] of [...ipc.files])
          if (at === path || at.startsWith(`${path}/`)) {
            files.set(at, file);
            ipc.files.delete(at);
          }
        trash.set(id, {
          parent: parentOf(path),
          index: hit.index,
          node,
          files,
        });
        refresh();
        return id;
      }
      case "restore_request_item": {
        const entry = trash.get(args.id as number);
        if (entry) {
          folderAt(entry.parent)?.splice(entry.index, 0, entry.node);
          for (const [at, file] of entry.files) ipc.files.set(at, file);
          trash.delete(args.id as number);
          refresh();
        }
        return null;
      }
      case "inspect_request": {
        const answer =
          typeof ipc.inspection === "function"
            ? ipc.inspection(args.path as string, args.request as RequestFile)
            : ipc.inspection;
        return Promise.resolve(answer).then((a) => ({
          ...EMPTY_INSPECTION,
          ...a,
        }));
      }
      case "vault_secrets":
        return (opts.secrets ?? []).map((name) => ({
          name,
          kind: "secret",
          description: null,
          environment: "local",
          missing: false,
          updatedAt: null,
          expiresAt: null,
        }));
      case "pick_request_file":
        return opts.pick ?? null;
      case "describe_file_ref": {
        const value = args.value as string;
        return {
          value,
          name: value.split("/").pop() ?? "",
          outside: false,
          missing: false,
          ...opts.describe?.[value],
        };
      }
      default:
        return null;
    }
  });
  return ipc;
}

/** A lookup over a made-up inspection, for the variable field tests. */
export function lookupFrom(
  inspection: Partial<Inspection> | null,
  environment: string | null,
): VariableLookup {
  const full = inspection && { ...EMPTY_INSPECTION, ...inspection };
  return new VariableLookup(
    () => full,
    () => environment,
  );
}
