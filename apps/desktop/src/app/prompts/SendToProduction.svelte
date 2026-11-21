<!-- Asks before a request goes to production, one confirmation at a time. -->
<script lang="ts">
  import type { SendConfirmations } from "./send-confirm.svelte";
  import SendToProductionDialog from "./SendToProductionDialog.svelte";

  let { confirmations }: { confirmations: SendConfirmations } = $props();
</script>

{#if confirmations.current}
  <!-- Each confirmation starts with the box unchecked. -->
  {#key confirmations.current.id}
    <SendToProductionDialog
      confirmation={confirmations.current}
      {confirmations}
    />
  {/key}
{/if}
