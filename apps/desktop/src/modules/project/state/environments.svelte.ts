// The environment editor's drafts: kept in the module, so switching tabs
// does not lose what is being edited. The rules mirror dea-project, so
// problems show before anything is sent.

import { projectApi } from "$app/contributions/project-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
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

/** One environment as on disk, and as edited. */
export class EnvironmentDraft {
  #saved = $state.raw<Environment | null>(null);
  draft = $state<Environment>({ name: "", production: false });
  dirty = $derived(
    !this.#saved || normalized(this.draft) !== normalized(this.#saved),
  );

  /** `saved` is null for an environment not on disk yet. */
  constructor(saved: Environment | null, draft?: Environment) {
    this.#saved = saved;
    this.draft = copy(draft ?? saved ?? { name: "", production: false });
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
    const keep = this.selected?.draft.name;
    this.drafts = (info?.environments ?? []).map(
      (env) => new EnvironmentDraft(env),
    );
    this.selected =
      this.drafts.find((d) => d.draft.name === keep) ?? this.drafts[0] ?? null;
  }

  problems(d: EnvironmentDraft): string[] {
    const others = this.drafts.filter((o) => o !== d).map((o) => o.draft.name);
    return problemsOf(d.draft, others);
  }

  get canSave(): boolean {
    return (
      this.dirty &&
      this.drafts.every((d) => !d.dirty || this.problems(d).length === 0)
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

  /** Saves every changed environment, one after another. */
  async save(): Promise<void> {
    if (!this.canSave) return;
    let info: ProjectInfo | null = null;
    try {
      for (const d of this.drafts.filter((x) => x.dirty))
        info = await saveEnvironment(
          d.saved?.name ?? null,
          $state.snapshot(d.draft),
        );
      this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
      throw e;
    } finally {
      if (info) {
        projectApi().refresh(info);
        this.load(info);
      }
    }
  }

  async duplicate(d: EnvironmentDraft): Promise<void> {
    if (!d.saved) return;
    const info = await duplicateEnvironment(d.saved.name);
    projectApi().refresh(info);
    this.load(info);
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
