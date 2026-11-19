// This window's view of the Vault: locked or not, and how to unlock it.

import type { UnlockMethod } from "$lib/ipc/bindings/UnlockMethod";
import type { VaultStatus } from "$lib/ipc/bindings/VaultStatus";
import { errorMessage, isIpcError } from "$lib/ipc/errors";
import * as ipc from "$lib/ipc/vault";
import type { VaultApi } from "../contributions/vault-api";
import {
  vaultPrompts,
  type VaultPrompts,
} from "../prompts/vault-prompt.svelte";
import { toasts } from "../prompts/toasts.svelte";

/** The Vault locks after idle time; activity is told at most this often. */
const ACTIVITY_EVERY_MS = 10_000;

export class WindowVault implements VaultApi {
  #locked = $state(true);
  #method = $state<UnlockMethod>("vaultPassword");
  #lastActivity = -Infinity;
  readonly #prompts: VaultPrompts;

  constructor(prompts: VaultPrompts = vaultPrompts) {
    this.#prompts = prompts;
  }

  get locked(): boolean {
    return this.#locked;
  }

  get method(): UnlockMethod {
    return this.#method;
  }

  /** Reads the status; outside the app the Vault stays locked. */
  async load(): Promise<void> {
    try {
      const info = await ipc.vaultInfo();
      this.#locked = info.locked;
      this.#method = info.method;
    } catch {
      // Keep the defaults.
    }
  }

  apply(status: VaultStatus): void {
    this.#locked = status.locked;
  }

  /** Follows the Vault until the returned function is called. */
  async listen(subscribe = ipc.onVaultStatus): Promise<() => void> {
    return await subscribe((status) => this.apply(status));
  }

  /**
   * Tells the Vault the user is active, so it does not lock them out while
   * they work. Returns the function that stops.
   */
  trackActivity(target: EventTarget = window): () => void {
    const onActivity = () => {
      if (this.#locked) return;
      const now = Date.now();
      if (now - this.#lastActivity < ACTIVITY_EVERY_MS) return;
      this.#lastActivity = now;
      ipc.vaultActivity().catch(() => {});
    };
    target.addEventListener("keydown", onActivity, true);
    target.addEventListener("pointerdown", onActivity, true);
    return () => {
      target.removeEventListener("keydown", onActivity, true);
      target.removeEventListener("pointerdown", onActivity, true);
    };
  }

  unlock(): Promise<boolean> {
    return this.#asked(() => ipc.vaultUnlock());
  }

  authorize(action: string): Promise<boolean> {
    return this.#asked(() => ipc.vaultAuthorize(action));
  }

  async lock(): Promise<void> {
    try {
      await ipc.vaultLock();
    } catch (e) {
      toasts.show(errorMessage(e));
    }
  }

  /**
   * True when `ask` went through; a cancel is silent, a failure a toast. After
   * a wait that was sat out, `ask` runs once more, as the person expects.
   */
  async #asked(ask: () => Promise<void>, retry = true): Promise<boolean> {
    this.#prompts.takeWaitOutcome();
    try {
      await ask();
      return true;
    } catch (e) {
      if (isIpcError(e) && e.kind === "cancelled") return false;
      if (isIpcError(e) && e.kind === "retryAfter") {
        const outcome = this.#prompts.takeWaitOutcome();
        if (outcome === "cancelled") return false;
        if (outcome === "waited" && retry) return this.#asked(ask, false);
      }
      toasts.show(errorMessage(e));
      return false;
    }
  }
}

/** This window's Vault. */
export const thisVault = new WindowVault();
