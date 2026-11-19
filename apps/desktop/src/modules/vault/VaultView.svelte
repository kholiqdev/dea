<!-- The Vault module: the locked screen, or the secrets once unlocked. -->
<script lang="ts">
  import { settings } from "$lib/settings/settings.svelte";
  import { vaultApi } from "$app/contributions/vault-api";
  import LockedVault from "./components/LockedVault.svelte";
  import VaultFooter from "./components/VaultFooter.svelte";

  const vault = vaultApi();
</script>

<div class="vault">
  <div class="body">
    {#if vault.locked}
      <LockedVault />
    {/if}
  </div>
  <VaultFooter autoLockMinutes={settings.value.security.autoLockMinutes} />
</div>

<style>
  .vault {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .body {
    flex: 1;
    min-height: 0;
  }
</style>
