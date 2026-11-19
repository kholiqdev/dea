// What modules may read about the Vault, and ask of it.

import type { UnlockMethod } from "$lib/ipc/bindings/UnlockMethod";

export interface VaultApi {
  readonly locked: boolean;
  /** How this device unlocks the Vault. */
  readonly method: UnlockMethod;
  /** Unlocks. False when cancelled or failed; a failure shows a toast. */
  unlock(): Promise<boolean>;
  lock(): Promise<void>;
  /** Asks the user to prove who they are before `action`. False as `unlock`. */
  authorize(action: string): Promise<boolean>;
}

let current: VaultApi | null = null;

export function setVaultApi(api: VaultApi): void {
  current = api;
}

export function vaultApi(): VaultApi {
  if (!current) throw new Error("The shell has not started yet");
  return current;
}

const PHRASES: Record<UnlockMethod, string> = {
  deviceOwner: "Touch ID",
  windowsHello: "Windows Hello",
  windowsPassword: "your Windows password",
  vaultPassword: "your Vault password",
};

/** The way to unlock inside a sentence: `Touch ID`, `your Vault password`. */
export function methodPhrase(method: UnlockMethod): string {
  return PHRASES[method];
}

/** The unlock button's label. */
export function unlockLabel(method: UnlockMethod): string {
  return `Unlock with ${PHRASES[method]}`;
}
