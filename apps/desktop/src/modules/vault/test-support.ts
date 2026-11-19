import type { UnlockMethod } from "$lib/ipc/bindings/UnlockMethod";
import { setVaultApi, type VaultApi } from "$app/contributions/vault-api";

export interface FakeVault extends VaultApi {
  unlocked: number;
}

/** A Vault the tests can read and count unlocks on. */
export function fakeVault(
  locked: boolean,
  method: UnlockMethod = "deviceOwner",
): FakeVault {
  const vault: FakeVault = {
    locked,
    method,
    unlocked: 0,
    unlock: () => {
      vault.unlocked += 1;
      return Promise.resolve(true);
    },
    lock: () => Promise.resolve(),
    authorize: () => Promise.resolve(true),
  };
  setVaultApi(vault);
  return vault;
}
