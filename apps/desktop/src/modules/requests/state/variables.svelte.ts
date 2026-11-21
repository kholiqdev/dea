// What the variable field knows about `{{names}}`: the request's inspection
// and the active environment. Its shape is `VariableState` from @dea/editor,
// so the body editor can take the same object.

import type { Inspection } from "$lib/ipc/bindings/Inspection";

const DYNAMIC = new Set(["$guid", "$timestamp", "$isoTimestamp", "$randomInt"]);

/**
 * A name as Dea compares it: `{{ vault: token }}` is the secret `token`,
 * which the inspection lists as `vault:token`.
 */
export const key = (name: string) => name.replace(/^vault:\s*/, "vault:");

export class VariableLookup {
  readonly #get: () => Inspection | null;
  readonly #environment: () => string | null;

  constructor(get: () => Inspection | null, environment: () => string | null) {
    this.#get = get;
    this.#environment = environment;
  }

  /** False only when the inspection says the name has no value. */
  defined(name: string): boolean {
    if (DYNAMIC.has(name)) return true;
    const inspection = this.#get();
    // Still loading: nothing is known, so nothing is flagged.
    if (!inspection) return true;
    const k = key(name);
    if (inspection.variables.some((v) => key(v.name) === k)) return true;
    return !inspection.undefined.some((u) => key(u) === k);
  }

  describe(name: string): { shown: string; source: string } | null {
    if (DYNAMIC.has(name))
      return { shown: "A new value", source: "Dynamic, new on each send" };
    const found = this.#get()?.variables.find((v) => key(v.name) === key(name));
    return found ? { shown: found.shown, source: found.source } : null;
  }

  undefinedText(name: string): string {
    void name; // the text is the same for every name
    const environment = this.#environment();
    return environment
      ? `Not defined in ${environment} or in this collection.`
      : "Not defined. No environment is active.";
  }
}
