// The Vault's password question: one at a time, each answered once.

import * as ipc from "$lib/ipc/vault";
import type { VaultPromptEvent } from "$lib/ipc/vault";

export class VaultPrompts {
  #current = $state.raw<VaultPromptEvent | null>(null);

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
    ipc.vaultAnswerPrompt(prompt.id, password).catch(() => {});
  }

  /** Shows the prompts of this window until the returned function is called. */
  async listen(subscribe = ipc.onVaultPrompt): Promise<() => void> {
    return await subscribe((prompt) => this.show(prompt));
  }
}

/** The application's Vault prompts. */
export const vaultPrompts = new VaultPrompts();
