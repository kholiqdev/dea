<!-- The Vault's state at the right end of the status bar; opens the Vault. -->
<script lang="ts">
  import { shellApi } from "$app/contributions/shell-api";
  import { vaultApi } from "$app/contributions/vault-api";
  import Lock from "@lucide/svelte/icons/lock";
  import LockOpen from "@lucide/svelte/icons/lock-open";

  const vault = vaultApi();
  const Icon = $derived(vault.locked ? Lock : LockOpen);
</script>

<button
  type="button"
  class="vault-status"
  onclick={() => shellApi().openModule("vault")}
>
  <Icon size={12} aria-hidden="true" />
  {vault.locked ? "Vault locked" : "Vault unlocked"}
</button>

<style>
  .vault-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
  }

  .vault-status:hover {
    color: var(--text-1);
  }
</style>
