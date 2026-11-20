// What a request tab and a folder tab share: a file read into a draft, kept
// in step with the disk, and written back (F1-FON-05).

import { untrack } from "svelte";
import { projectApi, type ProjectApi } from "$app/contributions/project-api";
import { shellApi } from "$app/contributions/shell-api";
import { settings } from "$lib/settings/settings.svelte";
import { errorMessage, isIpcError } from "$lib/ipc/errors";

/** How long after the last edit an autosave writes, in milliseconds. */
export const AUTOSAVE_DELAY = 1000;

const isMissing = (e: unknown) => isIpcError(e) && e.kind === "missing";

type Read<T> = { file: T } | { error: unknown };

/** One file as on disk, and as edited in its tab. */
export abstract class SyncedFile<T> {
  path = $state("");
  /** The file as last read or written; null until it has been read. */
  saved = $state.raw<T | null>(null);
  /** Set while a change on disk waits for a decision. */
  outside = $state<"changed" | "deleted" | null>(null);
  /** There is no file: a tab restored after it was deleted. */
  missing = $state(false);
  /** Why the file could not be read. */
  error = $state<string | null>(null);

  /** The edits; the subclass makes it reactive. */
  abstract draft: T;

  #theirs: T | null = null;
  // Changes with every write and every file taken, so a read that started
  // before one is dropped.
  #version = 0;
  // A check of the file after a move, which a save waits for.
  #settling: Promise<void> = Promise.resolve();
  #autosave: ReturnType<typeof setTimeout> | undefined;

  constructor(path: string) {
    this.path = path;
  }

  /** What the file is, in messages: "request", "folder" or "collection". */
  abstract get noun(): string;

  /** Whether Save again can write the file back after it was deleted on disk. */
  get recreatable(): boolean {
    return true;
  }

  protected abstract readFile(path: string): Promise<T>;
  protected abstract writeFile(path: string, file: T): Promise<void>;
  /** The draft as plain data. */
  protected abstract snapshot(): T;
  /** The file as Dea writes it. */
  protected abstract clean(file: T): T;
  /** Whether two files would write the same. */
  protected abstract same(a: T, b: T): boolean;
  /** Called after the draft started over from `file`. */
  protected taken(file: T): void {
    void file;
  }
  /** Called with a file the disk has that says the same as the saved one. */
  protected adopt(file: T): void {
    void file;
  }
  /** The file as it is about to be written; may look at the disk again. */
  protected prepare(file: T): Promise<T> {
    return Promise.resolve(file);
  }
  /**
   * A change on disk that needs no question, such as a rename from the
   * sidebar. Returns whether it was dealt with.
   */
  protected quiet(disk: T, saved: T): boolean {
    void disk;
    void saved;
    return false;
  }

  // A getter, not `$derived`: the draft outlives the view that made it.
  get dirty(): boolean {
    return this.saved !== null && !this.same(this.draft, this.saved);
  }

  /** Reads the file and starts over from it. */
  async load(): Promise<void> {
    const read = await this.#read();
    if (!read) return;
    if ("file" in read) this.take(read.file);
    else this.#failed(read.error);
  }

  /**
   * Looks at the file after a change on disk: a draft without edits follows
   * it quietly; one with edits keeps them and asks.
   */
  check(): Promise<void> {
    if (!this.saved) return this.load();
    return this.#read().then((read) => {
      if (!read) return;
      if ("error" in read) this.#failed(read.error);
      else this.#ondisk(read.file);
    });
  }

  /**
   * Writes the draft. A failure rejects with Dea's message, and so does a
   * change on disk that waits for an answer: only the banner writes over it.
   */
  async save(): Promise<void> {
    await this.#settling;
    if (this.outside !== null) throw new Error(this.#unanswered());
    await this.#write();
  }

  /** Writes the draft back after its file was deleted on disk ("Save again"). */
  async saveAgain(): Promise<void> {
    await this.#settling;
    await this.#write();
  }

  #unanswered(): string {
    if (this.outside === "changed")
      return `This ${this.noun} changed on disk. Choose Keep mine or Take theirs first.`;
    return this.recreatable
      ? `This ${this.noun} was deleted on disk. Choose Save again or Close tab first.`
      : `This ${this.noun} was deleted on disk and cannot be saved.`;
  }

  async #write(): Promise<void> {
    // Nothing was read, so there is nothing to write.
    if (!this.saved) return;
    const file = await this.prepare(this.clean(this.snapshot()));
    this.#version++;
    try {
      await this.writeFile(this.path, file);
    } catch (e) {
      throw new Error(errorMessage(e), { cause: e });
    } finally {
      this.#version++;
    }
    this.saved = file;
    this.adopt(file);
    this.#theirs = null;
    this.outside = null;
    this.error = null;
  }

  /** Whether the edits are fit to write on their own; false holds an autosave back. */
  protected autosavable(): boolean {
    return true;
  }

  /**
   * Saves `AUTOSAVE_DELAY` ms after the last call, when autosave is on. The
   * conditions are looked at again then, not now.
   */
  scheduleAutosave(): void {
    clearTimeout(this.#autosave);
    if (!settings.requests.autosave) return;
    this.#autosave = setTimeout(() => this.#autosaveNow(), AUTOSAVE_DELAY);
  }

  cancelAutosave(): void {
    clearTimeout(this.#autosave);
  }

  #autosaveNow(): void {
    const saved = this.saved;
    if (
      !settings.requests.autosave ||
      projectApi().info?.readOnly === true ||
      this.outside !== null ||
      !saved ||
      // What would be written is what is there: nothing to do (a half-set
      // client certificate is dirty but writes nothing).
      this.same(this.draft, saved) ||
      !this.autosavable()
    )
      return;
    this.save().catch((e: unknown) => shellApi().toast(errorMessage(e)));
  }

  /** Follows a rename or move from the sidebar. The edits stay. */
  moveTo(path: string): void {
    if (path === this.path) return;
    this.path = path;
    this.recheck();
  }

  /**
   * Looks at the file again after the sidebar wrote it in place, as a
   * rename that keeps the file name does. A save waits for the look.
   */
  recheck(): void {
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

  /** Reads the file; null when a write or a move came in between. */
  async #read(): Promise<Read<T> | null> {
    const version = this.#version;
    const path = this.path;
    let read: Read<T>;
    try {
      read = { file: await this.readFile(path) };
    } catch (error) {
      read = { error };
    }
    return version === this.#version && path === this.path ? read : null;
  }

  /** Starts over from the file on disk. */
  protected take(file: T): void {
    this.#version++;
    this.saved = file;
    this.draft = structuredClone(file);
    this.#theirs = null;
    this.outside = null;
    this.missing = false;
    this.error = null;
    this.taken(file);
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

  #ondisk(disk: T): void {
    const saved = this.saved;
    if (!saved) return this.take(disk);
    this.error = null;
    this.#theirs = null;
    this.outside = null;
    if (this.same(disk, saved)) return this.adopt(disk);
    if (this.quiet(disk, saved)) return;
    if (!this.dirty || this.same(disk, this.draft)) return this.take(disk);
    this.#theirs = disk;
    this.outside = "changed";
  }
}

type DiskWatcher = (changed: readonly string[]) => void;

// Only gone through when a change comes in, never watched.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const watchers = new Set<DiskWatcher>();
let watching: { project: ProjectApi; stop: () => void } | null = null;

/**
 * Calls `watcher` with each batch of files changed on disk from now on,
 * whether or not a tab is shown: a tab that is not shown can still be
 * saved, from the close dialog or by autosave. The batch already there is
 * old news. A new project (in tests) is watched from scratch.
 */
export function watchDisk(watcher: DiskWatcher): void {
  watchers.add(watcher);
  const project = projectApi();
  if (watching?.project === project) return;
  watching?.stop();
  const stop = $effect.root(() => {
    let seen = project.changed;
    $effect(() => {
      const changed = project.changed;
      if (changed === seen) return;
      seen = changed;
      untrack(() => {
        for (const watch of watchers) watch(changed);
      });
    });
  });
  watching = { project, stop };
}

/**
 * Saves the state of the tab `id`, as the shell's Save does (the close
 * dialog, Mod+S). A save refused for a change on disk shows the tab, so
 * its banner can be answered; the rejection keeps the tab open.
 */
export async function saveTab(
  states: ReadonlyMap<string, SyncedFile<unknown>>,
  id: string,
): Promise<void> {
  const state = states.get(id);
  if (!state) return;
  try {
    await state.save();
  } catch (e) {
    if (state.outside !== null) shellApi().activateTab(id);
    throw e;
  }
}

/** The ids of the open tabs of one type. */
export function openTabs(tabType: string): string[] {
  const shell = shellApi();
  const ids: string[] = [];
  const seen: unknown[] = [];
  for (;;) {
    let params: unknown;
    const id = shell.findTab(tabType, (p) => {
      if (seen.includes(p)) return false;
      params = p;
      return true;
    });
    if (id === undefined) return ids;
    ids.push(id);
    seen.push(params);
  }
}

/**
 * The state a tab shows: the one it had, after a look at the file, or a new
 * one read from `path`. `drop` is told of a state that is let go.
 */
export function syncedFor<S extends SyncedFile<unknown>>(
  registry: Map<string, S>,
  id: string,
  path: string,
  create: (path: string) => S,
  drop: (state: S) => void,
): S {
  const known = registry.get(id);
  // Reopening a closed tab brings back its id but not its unsaved mark:
  // the edits were discarded when it closed.
  const reopened = known?.dirty === true && !shellApi().isTabDirty(id);
  if (known && !reopened) {
    if (known.path === path) void known.check();
    else known.moveTo(path);
    return known;
  }
  if (known) drop(known);
  const created = create(path);
  registry.set(id, created);
  void created.load();
  return created;
}
