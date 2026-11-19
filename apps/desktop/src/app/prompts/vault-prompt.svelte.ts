// The Vault's password question: one at a time, each answered once.

import { errorMessage } from "$lib/ipc/errors";
import * as ipc from "$lib/ipc/vault";
import type { VaultPromptEvent } from "$lib/ipc/vault";
import { toasts } from "./toasts.svelte";

export class VaultPrompts {
  #current = $state.raw<VaultPromptEvent | null>(null);
  #wait: "waited" | "cancelled" | null = null;

  get current(): VaultPromptEvent | null {
    return this.#current;
  }

  /** Shows a prompt; one still open is cancelled, so none is left waiting. */
  show(prompt: VaultPromptEvent): void {
    if (this.#current && this.#current.id !== prompt.id) this.answer(null);
    this.#current = prompt;
  }

  /** Answers the open prompt with a password, or `null` to cancel it. */
  answer(password: string | null): void {
    const prompt = this.#current;
    if (!prompt) return;
    this.#current = null;
    if (prompt.problem?.kind === "retryAfter")
      this.#wait = password === null ? "cancelled" : "waited";
    ipc
      .vaultAnswerPrompt(prompt.id, password)
      .catch((e: unknown) => toasts.show(errorMessage(e)));
  }

  /**
   * How the last "too many wrong passwords" prompt ended, once: the wait was
   * sat out, or the user cancelled. Null if there was none.
   */
  takeWaitOutcome(): "waited" | "cancelled" | null {
    const outcome = this.#wait;
    this.#wait = null;
    return outcome;
  }

  /** Shows the prompts of this window until the returned function is called. */
  async listen(subscribe = ipc.onVaultPrompt): Promise<() => void> {
    return await subscribe((prompt) => this.show(prompt));
  }
}

/** The application's Vault prompts. */
export const vaultPrompts = new VaultPrompts();
