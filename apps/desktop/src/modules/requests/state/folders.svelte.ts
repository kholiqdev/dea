// The folders and collections being edited, by tab id. Like the request
// drafts, they live in the module, so a tab that is not shown keeps its edits.

import { shellApi } from "$app/contributions/shell-api";
import type { FolderFile } from "$lib/ipc/bindings/FolderFile";
import type { OptionsInfo } from "$lib/ipc/bindings/OptionsInfo";
import type { SendOptions } from "$lib/ipc/bindings/SendOptions";
import type { TlsSettings } from "$lib/ipc/bindings/TlsSettings";
import { errorMessage, isIpcError } from "$lib/ipc/errors";
import { readFolder, saveFolder } from "../api";
import { normalized, written } from "./drafts.svelte";

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

/** Whether two folders would write the same file. */
export function sameFolder(a: FolderFile, b: FolderFile): boolean {
  return (
    JSON.stringify(normalized(cleanedFolder(a))) ===
    JSON.stringify(normalized(cleanedFolder(b)))
  );
}

const isMissing = (e: unknown) => isIpcError(e) && e.kind === "missing";

type Read = { file: FolderFile } | { error: unknown };

/** One folder's settings as on disk, and as edited in its tab. */
export class RequestFolderState {
  path = $state("");
  /** The file as last read or written; null until it has been read. */
  saved = $state.raw<FolderFile | null>(null);
  draft = $state<FolderFile>({});
  /** Set while a change on disk waits for a decision. */
  outside = $state<"changed" | "deleted" | null>(null);
  /** There is no folder: a tab restored after it was deleted. */
  missing = $state(false);
  /** Why the file could not be read. */
  error = $state<string | null>(null);
  /** What the folders above set for the network options, else the defaults. */
  inherited = $state.raw<OptionsInfo>(DEFAULT_OPTIONS);

  #theirs: FolderFile | null = null;
  // Changes with every write and every file taken, so a read that started
  // before one is dropped.
  #version = 0;
  #settling: Promise<void> = Promise.resolve();

  constructor(path: string) {
    this.path = path;
  }

  // A getter, not `$derived`: the state outlives the view that made it.
  get dirty(): boolean {
    return this.saved !== null && !sameFolder(this.draft, this.saved);
  }

  /** A collection sits directly under `requests`; it alone has certificates. */
  get isCollection(): boolean {
    return parentOf(this.path) === ROOT;
  }

  /** Reads the file and starts over from it. */
  async load(): Promise<void> {
    const read = await this.#read();
    if (!read) return;
    if ("file" in read) this.#take(read.file);
    else this.#failed(read.error);
  }

  /** Looks at the file after a change on disk: edits are kept and asked about. */
  check(): Promise<void> {
    if (!this.saved) return this.load();
    return this.#read().then((read) => {
      if (!read) return;
      if ("error" in read) this.#failed(read.error);
      else this.#ondisk(read.file);
    });
  }

  /** Writes the draft. A failure rejects with Dea's message. */
  async save(): Promise<void> {
    await this.#settling;
    if (!this.saved) return;
    const folder = cleanedFolder($state.snapshot(this.draft));
    this.#version++;
    try {
      await saveFolder(this.path, folder);
    } catch (e) {
      throw new Error(errorMessage(e), { cause: e });
    } finally {
      this.#version++;
    }
    this.saved = folder;
    this.#theirs = null;
    this.outside = null;
    this.error = null;
  }

  /** Follows a rename or move from the sidebar; the edits stay. */
  moveTo(path: string): void {
    if (path === this.path) return;
    this.path = path;
    this.#settling = this.check();
    void this.loadInherited();
  }

  takeTheirs(): Promise<void> {
    return this.load();
  }

  keepMine(): void {
    if (this.#theirs) this.saved = this.#theirs;
    this.#theirs = null;
    this.outside = null;
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

  /** Reads the file; null when a write or a move came in between. */
  async #read(): Promise<Read | null> {
    const version = this.#version;
    const path = this.path;
    let read: Read;
    try {
      read = { file: await readFolder(path) };
    } catch (error) {
      read = { error };
    }
    return version === this.#version && path === this.path ? read : null;
  }

  #take(file: FolderFile): void {
    this.#version++;
    this.saved = file;
    this.draft = structuredClone(file);
    this.#theirs = null;
    this.outside = null;
    this.missing = false;
    this.error = null;
  }

  #failed(error: unknown): void {
    if (!isMissing(error)) {
      this.error = errorMessage(error);
    } else if (this.dirty) {
      this.outside = "deleted";
    } else {
      this.saved = null;
      this.outside = null;
      this.missing = true;
    }
  }

  #ondisk(disk: FolderFile): void {
    const saved = this.saved;
    if (!saved) return this.#take(disk);
    this.error = null;
    this.#theirs = null;
    this.outside = null;
    if (sameFolder(disk, saved)) return;
    if (!this.dirty || sameFolder(disk, this.draft)) return this.#take(disk);
    this.#theirs = disk;
    this.outside = "changed";
  }
}

const TAB_TYPE = "requests.folder";

/** The states of the folder tabs shown so far, by tab id. */
// Only looked up by id, never watched, so a plain Map is right here.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
export const folderStates = new Map<string, RequestFolderState>();

function openTabs(): string[] {
  const shell = shellApi();
  const ids: string[] = [];
  const seen: unknown[] = [];
  for (;;) {
    let params: unknown;
    const id = shell.findTab(TAB_TYPE, (p) => {
      if (seen.includes(p)) return false;
      params = p;
      return true;
    });
    if (id === undefined) return ids;
    ids.push(id);
    seen.push(params);
  }
}

/** Forgets the states of folder tabs that were closed. */
export function pruneFolderStates(): void {
  const open = openTabs();
  for (const id of [...folderStates.keys()])
    if (!open.includes(id)) folderStates.delete(id);
}

/** The state of the folder tab `id` as it is shown. */
export function folderStateFor(id: string, path: string): RequestFolderState {
  pruneFolderStates();
  const known = folderStates.get(id);
  // Reopening a closed tab brings back its id but not its unsaved mark.
  const reopened = known?.dirty === true && !shellApi().isTabDirty(id);
  if (known && !reopened) {
    if (known.path === path) void known.check();
    else known.moveTo(path);
    void known.loadInherited();
    return known;
  }
  const created = new RequestFolderState(path);
  folderStates.set(id, created);
  void created.load();
  void created.loadInherited();
  return created;
}

/** Forgets every state (tests). */
export function resetFolderStates(): void {
  folderStates.clear();
}
