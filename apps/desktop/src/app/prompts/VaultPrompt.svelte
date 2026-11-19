<!-- Shows the Vault's password question of this window, one at a time. -->
<script lang="ts">
  import type { VaultPrompts } from "./vault-prompt.svelte";
  import VaultPromptDialog from "./VaultPromptDialog.svelte";

  let { prompts }: { prompts: VaultPrompts } = $props();
</script>

{#if prompts.current}
  <!-- A new prompt starts with empty fields and its own countdown. -->
  {#key prompts.current.id}
    <VaultPromptDialog prompt={prompts.current} {prompts} />
  {/key}
{/if}
