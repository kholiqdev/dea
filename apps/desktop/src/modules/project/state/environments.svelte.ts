// The environment editor's drafts: kept in the module, so switching tabs
// does not lose what is being edited. The rules mirror dea-project, so
// problems show before anything is sent.

import { projectApi } from "$app/contributions/project-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { FileErrorInfo } from "$lib/ipc/bindings/FileErrorInfo";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import { errorMessage } from "$lib/ipc/errors";
import {
  deleteEnvironment,
  duplicateEnvironment,
  saveEnvironment,
} from "../api";

/** Same rule as `is_production_name` in dea-project (F1-FON-07 #5). */
export function isProductionName(name: string): boolean {
  return ["prod", "production", "prd"].includes(name.trim().toLowerCase());
}

/** Same rule as `is_valid_variable_name` in dea-project. */
export function isValidVariableName(name: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_.-]*$/.test(name);
}

/** What stops `env` from saving; `others` are the other environments' names. */
export function problemsOf(env: Environment, others: string[]): string[] {
  const problems: string[] = [];
  const name = env.name.trim();
  if (!name) problems.push("An environment needs a name");
  else if (others.some((o) => o.toLowerCase() === name.toLowerCase()))
    problems.push(`An environment called "${name}" already exists`);
  const seen: string[] = [];
  for (const v of env.variables ?? []) {
    if (!isValidVariableName(v.name))
      problems.push(
        `"${v.name}" is not a variable name: start with a letter or _, then use letters, digits, _, . or -`,
      );
    else if (seen.includes(v.name))
      problems.push(`The variable "${v.name}" is listed twice`);
    seen.push(v.name);
  }
  return problems;
}

/** Secret names a value refers to with `{{vault:name}}`. */
export function vaultReferences(value: string): string[] {
  return [...value.matchAll(/\{\{\s*vault:\s*([^}\s]+)\s*\}\}/g)].map(
    (m) => m[1] ?? "",
  );
}

/** One line of Compare: a field or variable that differs. */
export interface Difference {
  field: string;
  /** Null when this side does not have it. */
  mine: string | null;
  theirs: string | null;
}

function variableLines(env: Environment): Record<string, string> {
  return Object.fromEntries(
    (env.variables ?? []).map((v) => [
      v.name,
      v.description ? `${v.value}  # ${v.description}` : v.value,
    ]),
  );
}

/** What differs between the draft and the file on disk, for Compare. */
export function differences(
  mine: Environment,
  theirs: Environment,
): Difference[] {
  const rows: Difference[] = [];
  const add = (field: string, a: string | null, b: string | null) => {
    if (a !== b) rows.push({ field, mine: a, theirs: b });
  };
  add("name", mine.name, theirs.name);
  add("production", String(mine.production), String(theirs.production));
  add("color", mine.color ?? "slate", theirs.color ?? "slate");
  const m = variableLines(mine);
  const t = variableLines(theirs);
  const names = [...Object.keys(m), ...Object.keys(t)].filter(
    (name, i, all) => all.indexOf(name) === i,
  );
  for (const name of names) add(name, m[name] ?? null, t[name] ?? null);
  return rows;
}

/** What happened to a draft's file outside Dea while it had edits. */
export type Outside =
  | { kind: "changed"; theirs: Environment }
  | { kind: "deleted" }
  | { kind: "unreadable"; error: FileErrorInfo };

function normalized(env: Environment): string {
  return JSON.stringify({
    name: env.name,
    production: env.production,
    color: env.color ?? null,
    variables: (env.variables ?? []).map((v) => ({
      name: v.name,
      value: v.value,
      description: v.description ?? null,
    })),
  });
}

function copy(env: Environment): Environment {
  return {
    ...env,
    variables: (env.variables ?? []).map((v) => ({ ...v })),
  };
}

function same(a: Environment, b: Environment): boolean {
  return normalized(a) === normalized(b);
}

/** One environment as on disk, and as edited. */
export class EnvironmentDraft {
  #saved = $state.raw<Environment | null>(null);
  draft = $state<Environment>({ name: "", production: false });
  dirty = $derived(!this.#saved || !same(this.draft, this.#saved));
  /** The file it was read from, relative to the project. */
  file = $state<string | null>(null);
  /** Set while a change outside Dea waits for a decision. */
  outside = $state.raw<Outside | null>(null);

  /** `saved` is null for an environment not on disk yet. */
  constructor(
    saved: Environment | null,
    draft?: Environment,
    file: string | null = null,
  ) {
    this.#saved = saved;
    this.draft = copy(draft ?? saved ?? { name: "", production: false });
    this.file = file;
  }

  get saved(): Environment | null {
    return this.#saved;
  }

  addVariable(): void {
    (this.draft.variables ??= []).push({ name: "", value: "" });
  }

  removeVariable(index: number): void {
    this.draft.variables?.splice(index, 1);
  }

  /** Takes the version on disk, dropping the edits ("Take theirs"). */
  reset(saved: Environment): void {
    this.#saved = saved;
    this.draft = copy(saved);
    this.outside = null;
  }

  /** Drops the edits for the version on disk that changed ("Take theirs"). */
  takeTheirs(): void {
    if (this.outside?.kind === "changed") this.reset(this.outside.theirs);
  }

  /** Records that `saved` is now on disk; edits made since stay edits. */
  markSaved(saved: Environment): void {
    this.#saved = saved;
    this.outside = null;
  }

  /** Keeps the edits; the next save writes them over the disk version. */
  keepMine(): void {
    if (this.outside?.kind === "changed") this.#saved = this.outside.theirs;
    this.outside = null;
  }

  /** Writes the draft as a new file next time ("Save it again"). */
  saveAgain(): void {
    this.#saved = null;
    this.file = null;
    this.outside = null;
  }

  /** Follows the version on disk: quietly without edits, else it asks. */
  ondisk(theirs: Environment, file: string | null): void {
    this.file = file;
    if (this.#saved && same(theirs, this.#saved)) this.outside = null;
    else if (!this.dirty || same(theirs, this.draft)) this.reset(theirs);
    else this.outside = { kind: "changed", theirs };
  }

  setName(name: string): void {
    this.draft.name = name;
    if (!this.#saved && isProductionName(name)) this.draft.production = true;
  }
}

/** Every environment of the project, with the one being edited. */
export class EnvironmentsEditor {
  drafts = $state.raw<EnvironmentDraft[]>([]);
  selected = $state<EnvironmentDraft | null>(null);
  error = $state<string | null>(null);
  dirty = $derived(this.drafts.some((d) => d.dirty));

  constructor(info: ProjectInfo | null) {
    this.load(info);
  }

  /** Starts over from the project as on disk. */
  load(info: ProjectInfo | null): void {
    const files = info?.environmentFiles ?? {};
    this.#use(
      (info?.environments ?? []).map(
        (env) => new EnvironmentDraft(env, undefined, files[env.name] ?? null),
      ),
    );
  }

  /**
   * Takes a change from disk (F1-FON-05): environments without edits follow
   * it quietly; one with edits keeps them and says what happened.
   */
  sync(info: ProjectInfo): void {
    const files = info.environmentFiles;
    const matched: EnvironmentDraft[] = [];
    const next = info.environments.map((env) => {
      const d = this.drafts.find((x) => x.saved?.name === env.name);
      if (!d)
        return new EnvironmentDraft(env, undefined, files[env.name] ?? null);
      matched.push(d);
      d.ondisk(env, files[env.name] ?? null);
      return d;
    });
    for (const d of this.drafts) {
      if (matched.includes(d) || (d.saved && !d.dirty)) continue;
      if (d.saved) {
        const error = info.errors.find((e) => e.path === d.file);
        d.outside = error ? { kind: "unreadable", error } : { kind: "deleted" };
      }
      next.push(d);
    }
    this.#use(next);
  }

  #use(drafts: EnvironmentDraft[]): void {
    const keep = this.selected;
    this.drafts = drafts;
    this.selected =
      (keep && drafts.includes(keep) ? keep : undefined) ??
      drafts.find((d) => d.draft.name === keep?.draft.name) ??
      drafts[0] ??
      null;
  }

  problems(d: EnvironmentDraft): string[] {
    const others = this.drafts.filter((o) => o !== d).map((o) => o.draft.name);
    return problemsOf(d.draft, others);
  }

  /** Nothing is saved while a change from disk waits for a decision. */
  get canSave(): boolean {
    return (
      this.dirty &&
      this.drafts.every(
        (d) => !d.outside && (!d.dirty || this.problems(d).length === 0),
      )
    );
  }

  newEnvironment(): void {
    const taken = this.drafts.map((d) => d.draft.name.toLowerCase());
    let name = "new environment";
    for (let n = 2; taken.includes(name); n++) name = `new environment ${n}`;
    const d = new EnvironmentDraft(null, { name, production: false });
    this.drafts = [...this.drafts, d];
    this.selected = d;
  }

  /**
   * Saves every changed environment, one after another. Refuses while
   * something stops a save, so a close that asked to save keeps the tab.
   * A failure keeps every draft not written yet.
   */
  async save(): Promise<void> {
    if (!this.dirty) return;
    if (!this.canSave)
      throw new Error(
        this.drafts.some((d) => d.outside)
          ? "Decide what to do with the changes on disk before saving"
          : "Fix the problems in the environments before saving",
      );
    const project = projectApi();
    const active = project.environment?.name;
    let info: ProjectInfo | null = null;
    try {
      for (const d of this.drafts.filter((x) => x.dirty)) {
        const env = $state.snapshot(d.draft);
        const previous = d.saved?.name ?? null;
        info = await saveEnvironment(previous, env);
        d.markSaved(env);
        // A renamed environment stays the one this window uses.
        if (previous !== null && previous === active && env.name !== previous)
          await project.setEnvironment(env.name);
      }
      this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
      throw e;
    } finally {
      if (info) {
        project.refresh(info);
        this.sync(info);
      }
    }
  }

  /** Copies an environment on disk; edits elsewhere stay as they are. */
  async duplicate(d: EnvironmentDraft): Promise<void> {
    if (!d.saved) return;
    const info = await duplicateEnvironment(d.saved.name);
    projectApi().refresh(info);
    this.sync(info);
  }

  /** Drops a draft whose file is gone, leaving the disk alone. */
  discard(d: EnvironmentDraft): void {
    this.#use(this.drafts.filter((x) => x !== d));
  }

  async remove(d: EnvironmentDraft): Promise<void> {
    if (d.saved) {
      const info = await deleteEnvironment(d.saved.name);
      projectApi().refresh(info);
    }
    this.drafts = this.drafts.filter((x) => x !== d);
    if (this.selected === d) this.selected = this.drafts[0] ?? null;
  }
}

let current: EnvironmentsEditor | null = null;

/** The editor of this window's project, made on first use. */
export function environmentsEditor(): EnvironmentsEditor {
  current ??= new EnvironmentsEditor(projectApi().info);
  return current;
}

/** Forgets the drafts (tests, and a project that changed under them). */
export function resetEnvironmentsEditor(): void {
  current = null;
}
