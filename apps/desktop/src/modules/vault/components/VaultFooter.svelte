<!-- How the Vault is unlocked, and how long it stays so. -->
<script lang="ts">
  import { methodPhrase, vaultApi } from "$app/contributions/vault-api";

  let { autoLockMinutes }: { autoLockMinutes: number | null } = $props();

  const vault = vaultApi();
  const state = $derived.by(() => {
    if (vault.locked) return "Locked";
    const how = `Unlocked with ${methodPhrase(vault.method)}`;
    return autoLockMinutes === null
      ? `${how}, stays unlocked while Dea is open`
      : `${how}, locks after ${autoLockMinutes} min idle`;
  });
</script>

<footer class="vault-footer">
  <span>{state}</span>
  <span>Encrypted on this device</span>
</footer>

<style>
  .vault-footer {
    display: flex;
    justify-content: space-between;
    gap: var(--s-3);
    padding: var(--s-2) var(--s-3);
    border-top: 1px solid var(--line);
    color: var(--text-3);
  }
</style>
