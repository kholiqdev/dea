// Lock Vault, from the palette or Mod+L.

import { vaultApi } from "$app/contributions/vault-api";

export default function lockVault(): Promise<void> {
  return vaultApi().lock();
}
