// The requests being edited, by tab id. They live in the module, so a tab
// that is not shown keeps its edits.

import { shellApi } from "$app/contributions/shell-api";
import type { Inspection } from "$lib/ipc/bindings/Inspection";
import type { Body } from "$lib/ipc/bindings/Body";
import type { FormField } from "$lib/ipc/bindings/FormField";
import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { Variable } from "$lib/ipc/bindings/Variable";
import { errorMessage, isIpcError } from "$lib/ipc/errors";
import { inspectRequest, readRequest, saveRequest } from "../api";
import {
  joinUrl,
  pathVariableNames,
  splitUrl,
  syncParams,
  syncPathVariables,
} from "../lib/url";

/** How long after the last change the draft is inspected, in milliseconds. */
export const INSPECT_DELAY = 200;

export type BodyMode = "none" | Body["mode"];

/** A form field without a name, a value or a file: not written. */
export function isBlankField(field: FormField): boolean {
  return field.name === "" && !field.value && !field.file;
}

/** What a mode starts with the first time it is chosen. */
function emptyBody(mode: Body["mode"]): Body {
  switch (mode) {
    case "json":
      return { mode, text: "" };
    case "raw":
      return { mode, language: "text", text: "" };
    case "urlencoded":
    case "formData":
      return { mode, fields: [] };
    case "binary":
      return { mode, file: "" };
  }
}

/** The body as Dea writes it; null when it says nothing to send. */
function writtenBody(body: Body | undefined): Body | null {
  switch (body?.mode) {
    case undefined:
      return null;
    case "urlencoded":
      return { mode: body.mode, fields: written(body.fields) };
    case "formData":
      return {
        mode: body.mode,
        fields: body.fields
          // A file row with no file yet stays in the draft but is not written.
          .filter((f) => !isBlankField(f) && f.file !== "")
          .map(({ name, value, file, contentType, description, enabled }) => ({
            name,
            ...(file ? { file } : { value: value ?? "" }),
            ...(contentType ? { contentType } : {}),
            ...(description ? { description } : {}),
            enabled: enabled !== false,
          })),
      };
    case "binary":
      return body.file === "" ? null : body;
    case "json":
    case "raw":
      return body;
  }
}

export type Section = "params" | "headers" | "body" | "settings";

/** A row without a name or a value: not in the URL, and not written. */
export function isBlank(row: KeyValue): boolean {
  return row.name === "" && row.value === "";
}

function written(rows: KeyValue[] | undefined): KeyValue[] {
  return (rows ?? [])
    .filter((row) => !isBlank(row))
    .map(({ name, value, description, enabled }) => ({
      name,
      value,
      ...(description ? { description } : {}),
      enabled: enabled !== false,
    }));
}

/**
 * The request as Dea writes it: no blank rows or path variables without a
 * value, and nothing that only says "inherit".
 */
export function cleaned(request: RequestFile): RequestFile {
  const out: RequestFile = {
    name: request.name,
    method: request.method,
    url: request.url,
  };
  const pathVariables = (request.pathVariables ?? [])
    .filter((v) => v.value !== "" || v.description)
    .map(({ name, value, description }) => ({
      name,
      value,
      ...(description ? { description } : {}),
    }));
  if (pathVariables.length > 0) out.pathVariables = pathVariables;
  const params = written(request.params);
  if (params.length > 0) out.params = params;
  const headers = written(request.headers);
  if (headers.length > 0) out.headers = headers;
  const body = writtenBody(request.body);
  if (body) out.body = body;
  if (request.auth && request.auth.type !== "inherit") out.auth = request.auth;
  if (Object.values(request.options ?? {}).some((v) => v !== undefined))
    out.options = request.options;
  return out;
}

/** Keys in one order, without what the file leaves out (`enabled: true`). */
function normalized(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(normalized);
  if (typeof value !== "object" || value === null) return value;
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(value).sort()) {
    const v = (value as Record<string, unknown>)[key];
    if (v === undefined || (key === "enabled" && v === true)) continue;
    out[key] = normalized(v);
  }
  return out;
}

/** Whether two requests would write the same file. */
export function sameRequest(a: RequestFile, b: RequestFile): boolean {
  return (
    JSON.stringify(normalized(cleaned(a))) ===
    JSON.stringify(normalized(cleaned(b)))
  );
}

const isMissing = (e: unknown) => isIpcError(e) && e.kind === "missing";

type Read = { file: RequestFile } | { error: unknown };

/** One request as on disk, and as edited in its tab. */
export class RequestDraft {
  path = $state("");
  /** The file as last read or written; null until it has been read. */
  saved = $state.raw<RequestFile | null>(null);
  draft = $state<RequestFile>({ name: "", method: "GET", url: "" });
  inspection = $state.raw<Inspection | null>(null);
  /** Why the draft could not be inspected: a broken folder, a file outside the project. */
  inspectionError = $state<string | null>(null);
  /** Set while a change on disk waits for a decision. */
  outside = $state<"changed" | "deleted" | null>(null);
  /** There is no file: a tab restored after its request was deleted. */
  missing = $state(false);
  /** Why the file could not be read. */
  error = $state<string | null>(null);
  /** The URL bar's text while the person types in it. */
  typedUrl = $state<string | null>(null);
  section = $state<Section>("params");

  // What each body mode held, so switching away and back loses nothing.
  // Read only when the mode changes, so it is not reactive.
  #bodies: Partial<Record<Body["mode"], Body>> = {};

  #theirs: RequestFile | null = null;
  // Changes with every write and every file taken, so a read that started
  // before one is dropped.
  #version = 0;
  #inspections = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;
  // A check of the file after a move, which a save waits for.
  #settling: Promise<void> = Promise.resolve();

  constructor(path: string) {
    this.path = path;
  }

  // A getter, not `$derived`: the draft outlives the view that made it.
  get dirty(): boolean {
    return this.saved !== null && !sameRequest(this.draft, this.saved);
  }

  /** The URL bar: the text being typed there, else the URL with its params. */
  get urlBar(): string {
    return (
      this.typedUrl ??
      joinUrl(
        this.draft.url,
        (this.draft.params ?? []).filter((p) => !isBlank(p)),
      )
    );
  }

  /** The number each section's tab shows (F1-REQ-02 #6); null for none. */
  get counts(): Record<Section, number | null> {
    const on = (rows: { enabled?: boolean }[]) =>
      rows.filter((r) => r.enabled !== false).length;
    const body = writtenBody(this.draft.body);
    return {
      params:
        pathVariableNames(this.draft.url).length +
        on((this.draft.params ?? []).filter((p) => !isBlank(p))),
      headers: on((this.draft.headers ?? []).filter((h) => !isBlank(h))),
      body:
        body?.mode === "urlencoded" || body?.mode === "formData"
          ? on(body.fields)
          : null,
      settings: null,
    };
  }

  /** Reads the file and starts over from it. */
  async load(): Promise<void> {
    const read = await this.#read();
    if (!read) return;
    if ("file" in read) this.#take(read.file);
    else this.#failed(read.error);
  }

  /**
   * Looks at the file after a change on disk (F1-FON-05): a draft without
   * edits follows it quietly; one with edits keeps them and asks.
   */
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
    // Nothing was read, so there is nothing to write.
    if (!this.saved) return;
    const request = cleaned($state.snapshot(this.draft));
    this.#version++;
    try {
      await saveRequest(this.path, request);
    } catch (e) {
      throw new Error(errorMessage(e), { cause: e });
    } finally {
      this.#version++;
    }
    this.saved = request;
    this.#theirs = null;
    this.outside = null;
    this.error = null;
  }

  /**
   * Follows a rename or move from the sidebar. The edits stay; a rename
   * wrote a new name into the file, which the check takes.
   */
  moveTo(path: string): void {
    if (path === this.path) return;
    this.path = path;
    this.#settling = this.check();
  }

  /** Drops the edits for the file on disk ("Take theirs"). */
  takeTheirs(): Promise<void> {
    return this.load();
  }

  /** Keeps the edits; the next save writes them over the disk ("Keep mine"). */
  keepMine(): void {
    if (this.#theirs) this.saved = this.#theirs;
    this.#theirs = null;
    this.outside = null;
  }

  /** Text typed in the URL bar: it stays as typed, its query fills the params. */
  typeUrl(text: string): void {
    this.typedUrl = text;
    const { base, query } = splitUrl(text);
    this.draft.url = base;
    this.draft.params = syncParams(
      (this.draft.params ?? []).filter((p) => !isBlank(p)),
      query,
    );
    this.draft.pathVariables = syncPathVariables(
      this.draft.pathVariables ?? [],
      pathVariableNames(base),
    );
  }

  /** Rows edited in the table: the URL bar shows them from now on. */
  setParams(rows: KeyValue[]): void {
    this.typedUrl = null;
    this.draft.params = rows;
  }

  setPathVariables(rows: Variable[]): void {
    this.draft.pathVariables = rows;
  }

  setHeaders(rows: KeyValue[]): void {
    this.draft.headers = rows;
  }

  get bodyMode(): BodyMode {
    return this.draft.body?.mode ?? "none";
  }

  /** Chooses the body's mode; each mode comes back as it was left. */
  setBodyMode(mode: BodyMode): void {
    const current = this.draft.body;
    if (current) this.#bodies[current.mode] = $state.snapshot(current);
    if (mode === "none") delete this.draft.body;
    else
      this.draft.body = structuredClone(this.#bodies[mode] ?? emptyBody(mode));
  }

  setBody(body: Body): void {
    this.draft.body = body;
  }

  setMethod(method: string): void {
    this.draft.method = method;
  }

  /** Inspects the draft `INSPECT_DELAY` ms after the last call. */
  scheduleInspection(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.inspect(), INSPECT_DELAY);
  }

  cancelInspection(): void {
    clearTimeout(this.#timer);
  }

  /** Asks Dea what the draft uses; an answer to an older call is dropped. */
  async inspect(): Promise<void> {
    const call = ++this.#inspections;
    try {
      const inspection = await inspectRequest(
        this.path,
        cleaned($state.snapshot(this.draft)),
      );
      if (call !== this.#inspections) return;
      this.inspection = inspection;
      this.inspectionError = null;
    } catch (e) {
      if (call !== this.#inspections) return;
      this.inspection = null;
      this.inspectionError = errorMessage(e);
    }
  }

  /** Reads the file; null when a write or a move came in between. */
  async #read(): Promise<Read | null> {
    const version = this.#version;
    const path = this.path;
    let read: Read;
    try {
      read = { file: await readRequest(path) };
    } catch (error) {
      read = { error };
    }
    return version === this.#version && path === this.path ? read : null;
  }

  /** Starts over from the file on disk. */
  #take(file: RequestFile): void {
    this.#version++;
    this.saved = file;
    this.draft = structuredClone(file);
    this.#bodies = {};
    this.#theirs = null;
    this.outside = null;
    this.typedUrl = null;
    this.missing = false;
    this.error = null;
  }

  /** A missing file asks about the edits; without edits there is nothing to keep. */
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

  #ondisk(disk: RequestFile): void {
    const saved = this.saved;
    if (!saved) return this.#take(disk);
    this.error = null;
    this.#theirs = null;
    this.outside = null;
    if (sameRequest(disk, saved)) return;
    if (sameRequest({ ...disk, name: saved.name }, saved)) {
      // Only the name changed, as a rename from the sidebar does: the
      // draft takes it unless the name is one of its edits.
      if (this.draft.name === saved.name) this.draft.name = disk.name;
      this.saved = disk;
      return;
    }
    if (!this.dirty || sameRequest(disk, this.draft)) return this.#take(disk);
    this.#theirs = disk;
    this.outside = "changed";
  }
}

const TAB_TYPE = "requests.request";

/** The drafts of the request tabs shown so far, by tab id. */
// Only looked up by id, never watched, so a plain Map is right here.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
export const drafts = new Map<string, RequestDraft>();

/** The ids of the request tabs the shell has open. */
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

/** Forgets the drafts of request tabs that were closed. */
export function pruneDrafts(): void {
  const open = openTabs();
  for (const [id, draft] of drafts)
    if (!open.includes(id)) {
      draft.cancelInspection();
      drafts.delete(id);
    }
}

/**
 * The draft of the request tab `id` as it is shown: the one it had, after a
 * look at the file, or a new one read from `path`.
 */
export function draftFor(id: string, path: string): RequestDraft {
  pruneDrafts();
  const known = drafts.get(id);
  // Reopening a closed tab brings back its id but not its unsaved mark:
  // the edits were discarded when it closed.
  const reopened = known?.dirty === true && !shellApi().isTabDirty(id);
  if (known && !reopened) {
    if (known.path === path) void known.check();
    else known.moveTo(path);
    return known;
  }
  known?.cancelInspection();
  const created = new RequestDraft(path);
  drafts.set(id, created);
  void created.load();
  return created;
}

/** Forgets every draft (tests). */
export function resetDrafts(): void {
  for (const draft of drafts.values()) draft.cancelInspection();
  drafts.clear();
}
