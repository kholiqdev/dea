<!-- Asks before a value is removed from this device. -->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";

  let {
    name,
    environment,
    ondelete,
    oncancel,
  }: {
    name: string;
    environment: string;
    ondelete: () => void;
    oncancel: () => void;
  } = $props();
</script>

<Dialog title="Delete the value of {name} in {environment}?" {oncancel}>
  <p class="text">
    Removes the value from this device. vault.yaml still lists the secret, so it
    shows as Missing.
  </p>
  {#snippet actions()}
    <Button data-autofocus onclick={oncancel}>Cancel</Button>
    <Button variant="danger" onclick={ondelete}>Delete</Button>
  {/snippet}
</Dialog>

<style>
  .text {
    color: var(--text-2);
  }
</style>
