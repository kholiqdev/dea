// A Vault the tests can lock and unlock while a screen is showing.

import type { UnlockMethod } from "$lib/ipc/bindings/UnlockMethod";
import { setVaultApi, type VaultApi } from "$app/contributions/vault-api";

export class ReactiveVault implements VaultApi {
  locked = $state(false);
  method: UnlockMethod = "deviceOwner";
  /** How many times `lock` was called. */
  locks = 0;

  unlock(): Promise<boolean> {
    this.locked = false;
    return Promise.resolve(true);
  }

  lock(): Promise<void> {
    this.locks += 1;
    this.locked = true;
    return Promise.resolve();
  }

  authorize(): Promise<boolean> {
    return Promise.resolve(true);
  }
}

export function reactiveVault(): ReactiveVault {
  const vault = new ReactiveVault();
  setVaultApi(vault);
  return vault;
}
