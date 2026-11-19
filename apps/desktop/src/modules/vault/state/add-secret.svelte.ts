// The `Add a secret` form: which one is open, and the rules it follows.
// The typed value is not here: it lives in the form and goes with it.

import type { SecretKind } from "$lib/ipc/bindings/SecretKind";
import { isValidSecretName } from "$lib/vault/references";

/** What the form starts with, and what it may not change. */
export interface Draft {
  environment: string;
  name: string;
  kind: SecretKind;
  description: string;
  /** Name and type are fixed: the secret is already declared. */
  locked: boolean;
}

class AddSecret {
  draft = $state.raw<Draft | null>(null);

  open(draft: Draft): void {
    this.draft = draft;
  }

  close(): void {
    this.draft = null;
  }
}

export const addSecret = new AddSecret();

/** How long Reveal lasts (F1-FON-08 #8). */
export const HIDE_MS = 30_000;

export interface Problems {
  name: string | null;
  value: string | null;
}

/** What is wrong with the form, before anything is sent. */
export function problems(name: string, value: string): Problems {
  const trimmed = name.trim();
  return {
    name:
      trimmed === ""
        ? "A secret needs a name"
        : isValidSecretName(trimmed)
          ? null
          : `"${trimmed}" is not a secret name: start with a letter or _, then use letters, digits, _, . or -`,
    value: value === "" ? "A secret needs a value" : null,
  };
}

/** A `datetime-local` value (local time) as RFC 3339 in UTC; null if blank. */
export function toRfc3339(local: string): string | null {
  if (local.trim() === "") return null;
  // A throwaway date for the conversion, not reactive state.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const date = new Date(local);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}
