// The folders and collections being edited, by tab id. Like the request
// drafts, they live in the module, so a tab that is not shown keeps its edits.

import type { FolderFile } from "$lib/ipc/bindings/FolderFile";
import type { OptionsInfo } from "$lib/ipc/bindings/OptionsInfo";
import type { SendOptions } from "$lib/ipc/bindings/SendOptions";
import type { TlsSettings } from "$lib/ipc/bindings/TlsSettings";
import { readFolder, saveFolder } from "../api";
import { normalized, written } from "./drafts.svelte";
import {
  openTabs,
  SyncedFile,
  syncedFor,
  watchDisk,
} from "./synced-file.svelte";

/** What a request uses when no folder and no request says otherwise. */
export const DEFAULT_OPTIONS: OptionsInfo = {
  timeoutMs: 30000,
  followRedirects: true,
  maxRedirects: 10,
  verifyTls: true,
  useSystemProxy: true,
  cookies: true,
};

const ROOT = "requests";

const parentOf = (path: string) => path.slice(0, path.lastIndexOf("/"));

/** The settings as Dea writes them: no blank rows, nothing that only inherits. */
export function cleanedFolder(folder: FolderFile): FolderFile {
  const out: FolderFile = {};
  if (folder.name !== undefined) out.name = folder.name;
  if (folder.order !== undefined) out.order = folder.order;
  const headers = written(folder.headers);
  if (headers.length > 0) out.headers = headers;
  const variables = (folder.variables ?? [])
    .filter((v) => !(v.name === "" && v.value === ""))
    .map(({ name, value, description }) => ({
      name,
      value,
      ...(description ? { description } : {}),
    }));
  if (variables.length > 0) out.variables = variables;
  if (folder.auth) out.auth = folder.auth;
  const options = Object.fromEntries(
    Object.entries(folder.options ?? {}).filter(([, v]) => v !== undefined),
  );
  if (Object.keys(options).length > 0) out.options = options;
  const tls: TlsSettings = {};
  const authorities = (folder.tls?.caCertificates ?? []).filter(
    (c) => c !== "",
  );
  if (authorities.length > 0) tls.caCertificates = authorities;
  const client = folder.tls?.clientCertificate;
  // A certificate without its key (or the other way) is not written yet.
  if (client && client.certificate !== "" && client.key !== "")
    tls.clientCertificate = client;
  if (Object.keys(tls).length > 0) out.tls = tls;
  return out;
}

/**
 * Whether two folders would write the same settings. The name and the order
 * belong to the sidebar, which writes them: this editor never does.
 */
export function sameFolder(a: FolderFile, b: FolderFile): boolean {
  const settings = (folder: FolderFile) => {
    const { name, order, ...rest } = cleanedFolder(folder);
    void name;
    void order;
    return JSON.stringify(normalized(rest));
  };
  return settings(a) === settings(b);
}

/** A client certificate with its file or its key, but not both. */
export function halfSetClient(tls: TlsSettings | undefined): boolean {
  const client = tls?.clientCertificate;
  return !!client && (client.certificate === "") !== (client.key === "");
}

/** One folder's settings as on disk, and as edited in its tab. */
export class RequestFolderState extends SyncedFile<FolderFile> {
  draft = $state<FolderFile>({});
  /** What the folders above set for the network options, else the defaults. */
  inherited = $state.raw<OptionsInfo>(DEFAULT_OPTIONS);

  get noun(): string {
    return this.isCollection ? "collection" : "folder";
  }

  /** Writing `_folder.yaml` alone would not bring back what the folder held. */
  override get recreatable(): boolean {
    return false;
  }

  protected snapshot(): FolderFile {
    return $state.snapshot(this.draft);
  }

  // A half-set client certificate is not written, so it would be lost on close.
  override get dirty(): boolean {
    return super.dirty || halfSetClient(this.draft.tls);
  }

  protected override adopt(file: FolderFile): void {
    this.saved = file;
    this.#followDisk(file);
  }

  /** The name and order are the disk's: the latest ones are written back. */
  protected override async prepare(file: FolderFile): Promise<FolderFile> {
    let disk: FolderFile = {};
    try {
      disk = await readFolder(this.path);
    } catch {
      // A missing or unreadable file: nothing of the sidebar's to keep.
    }
    const { name, order, ...rest } = file;
    void name;
    void order;
    return {
      ...(disk.name !== undefined ? { name: disk.name } : {}),
      ...(disk.order !== undefined ? { order: disk.order } : {}),
      ...rest,
    };
  }

  #followDisk(file: FolderFile): void {
    this.draft.name = file.name;
    this.draft.order = file.order;
  }

  protected readFile(path: string): Promise<FolderFile> {
    return readFolder(path);
  }

  protected writeFile(path: string, file: FolderFile): Promise<void> {
    return saveFolder(path, file);
  }

  protected clean(file: FolderFile): FolderFile {
    return cleanedFolder(file);
  }

  protected same(a: FolderFile, b: FolderFile): boolean {
    return sameFolder(a, b);
  }

  /** A collection sits directly under `requests`; it alone has certificates. */
  get isCollection(): boolean {
    return parentOf(this.path) === ROOT;
  }

  override moveTo(path: string): void {
    if (path === this.path) return;
    super.moveTo(path);
    void this.loadInherited();
  }

  setHeaders(rows: FolderFile["headers"]): void {
    this.draft.headers = rows;
  }

  setVariables(rows: FolderFile["variables"]): void {
    this.draft.variables = rows;
  }

  setOptions(options: SendOptions): void {
    this.draft.options = options;
  }

  setTls(tls: TlsSettings | undefined): void {
    this.draft.tls = tls;
  }

  /**
   * Works out the network options the folders above give: for each, the
   * nearest folder that sets it, else the built-in default.
   */
  async loadInherited(): Promise<void> {
    const path = this.path;
    const found: SendOptions = {};
    for (let at = parentOf(path); at !== ROOT && at !== ""; at = parentOf(at)) {
      let options: SendOptions | undefined;
      try {
        options = (await readFolder(at)).options;
      } catch {
        continue;
      }
      for (const [key, value] of Object.entries(options ?? {}))
        if (value !== undefined && !(key in found))
          Object.assign(found, { [key]: value });
    }
    if (path === this.path) this.inherited = { ...DEFAULT_OPTIONS, ...found };
  }
}

const TAB_TYPE = "requests.folder";

/** The states of the folder tabs shown so far, by tab id. */
// Only looked up by id, never watched, so a plain Map is right here.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
export const folderStates = new Map<string, RequestFolderState>();

/** Forgets the states of folder tabs that were closed. */
export function pruneFolderStates(): void {
  const open = openTabs(TAB_TYPE);
  for (const [id, state] of [...folderStates])
    if (!open.includes(id)) {
      state.cancelAutosave();
      folderStates.delete(id);
    }
}

/**
 * Looks at the folders a change on disk touched, shown or not: their own
 * file, or one above that they inherit from.
 */
function followDisk(changed: readonly string[]): void {
  pruneFolderStates();
  const settings = changed.some((p) => p.endsWith("/_folder.yaml"));
  for (const state of folderStates.values()) {
    const own = `${state.path}/_folder.yaml`;
    if (changed.some((p) => p === own || p === state.path)) void state.check();
    if (settings) void state.loadInherited();
  }
}

/** The state of the folder tab `id` as it is shown. */
export function folderStateFor(id: string, path: string): RequestFolderState {
  watchDisk(followDisk);
  pruneFolderStates();
  const state = syncedFor(
    folderStates,
    id,
    path,
    (p) => new RequestFolderState(p),
    (known) => known.cancelAutosave(),
  );
  void state.loadInherited();
  return state;
}

/** Forgets every state (tests). */
export function resetFolderStates(): void {
  for (const state of folderStates.values()) state.cancelAutosave();
  folderStates.clear();
}
