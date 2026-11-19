<!-- What the Vault shows while it is locked. -->
<script lang="ts">
  import { unlockLabel, vaultApi } from "$app/contributions/vault-api";
  import Button from "@dea/ui/Button.svelte";
  import Lock from "@lucide/svelte/icons/lock";

  const vault = vaultApi();
</script>

<div class="locked">
  <Lock size={28} aria-hidden="true" />
  <h2>Vault is locked</h2>
  <p>
    Requests, hosts and connections that use a secret wait until you unlock.
    Nothing is sent while the vault is locked.
  </p>
  <Button variant="primary" onclick={() => vault.unlock()}>
    {unlockLabel(vault.method)}
  </Button>
</div>

<style>
  .locked {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--s-2);
    padding: var(--s-4);
    text-align: center;
    color: var(--text-2);
  }

  h2 {
    margin: 0;
    color: var(--text-1);
  }

  p {
    margin: 0 0 var(--s-2);
    max-width: 420px;
  }
</style>
