// The requests being edited, by tab id. They live in the module, so a tab
// that is not shown keeps its edits.

import type { Inspection } from "$lib/ipc/bindings/Inspection";
import type { Body } from "$lib/ipc/bindings/Body";
import type { FormField } from "$lib/ipc/bindings/FormField";
import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
import type { SendOptions } from "$lib/ipc/bindings/SendOptions";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { Variable } from "$lib/ipc/bindings/Variable";
import { errorMessage } from "$lib/ipc/errors";
import { lintJson } from "@dea/editor";
import { inspectRequest, readRequest, saveRequest } from "../api";
import {
  joinUrl,
  pathVariableNames,
  splitUrl,
  syncParams,
  syncPathVariables,
} from "../lib/url";
import { openTabs, SyncedFile, syncedFor } from "./synced-file.svelte";

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

export function written(rows: KeyValue[] | undefined): KeyValue[] {
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
export function normalized(value: unknown): unknown {
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

/** One request as on disk, and as edited in its tab. */
export class RequestDraft extends SyncedFile<RequestFile> {
  draft = $state<RequestFile>({ name: "", method: "GET", url: "" });
  inspection = $state.raw<Inspection | null>(null);
  /** Why the draft could not be inspected: a broken folder, a file outside the project. */
  inspectionError = $state<string | null>(null);
  /** The URL bar's text while the person types in it. */
  typedUrl = $state<string | null>(null);
  section = $state<Section>("params");

  // What each body mode held, so switching away and back loses nothing.
  // Read only when the mode changes, so it is not reactive.
  #bodies: Partial<Record<Body["mode"], Body>> = {};

  #inspections = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;

  protected snapshot(): RequestFile {
    return $state.snapshot(this.draft);
  }

  protected readFile(path: string): Promise<RequestFile> {
    return readRequest(path);
  }

  protected writeFile(path: string, file: RequestFile): Promise<void> {
    return saveRequest(path, file);
  }

  protected clean(file: RequestFile): RequestFile {
    return cleaned(file);
  }

  protected same(a: RequestFile, b: RequestFile): boolean {
    return sameRequest(a, b);
  }

  /** A JSON body that does not parse is not written on its own. */
  protected override autosavable(): boolean {
    const body = this.draft.body;
    return body?.mode !== "json" || lintJson(body.text) === null;
  }

  protected override taken(): void {
    this.#bodies = {};
    this.typedUrl = null;
  }

  /**
   * Only the name changed, as a rename from the sidebar does: the draft
   * takes it unless the name is one of its edits.
   */
  protected override quiet(disk: RequestFile, saved: RequestFile): boolean {
    if (!sameRequest({ ...disk, name: saved.name }, saved)) return false;
    if (this.draft.name === saved.name) this.draft.name = disk.name;
    this.saved = disk;
    return true;
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

  setOptions(options: SendOptions): void {
    this.draft.options = options;
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
}

const TAB_TYPE = "requests.request";

/** The drafts of the request tabs shown so far, by tab id. */
// Only looked up by id, never watched, so a plain Map is right here.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
export const drafts = new Map<string, RequestDraft>();

/** Forgets the drafts of request tabs that were closed. */
export function pruneDrafts(): void {
  const open = openTabs(TAB_TYPE);
  for (const [id, draft] of drafts)
    if (!open.includes(id)) {
      draft.cancelInspection();
      draft.cancelAutosave();
      drafts.delete(id);
    }
}

/**
 * The draft of the request tab `id` as it is shown: the one it had, after a
 * look at the file, or a new one read from `path`.
 */
export function draftFor(id: string, path: string): RequestDraft {
  pruneDrafts();
  return syncedFor(
    drafts,
    id,
    path,
    (p) => new RequestDraft(p),
    (known) => {
      known.cancelInspection();
      known.cancelAutosave();
    },
  );
}

/** Forgets every draft (tests). */
export function resetDrafts(): void {
  for (const draft of drafts.values()) {
    draft.cancelInspection();
    draft.cancelAutosave();
  }
  drafts.clear();
}
