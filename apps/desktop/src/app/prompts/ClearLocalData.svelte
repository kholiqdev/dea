<!-- Asks before clearing what Dea keeps for a project on this device. -->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";

  let {
    name,
    oncancel,
    onclear,
  }: { name: string; oncancel: () => void; onclear: () => void } = $props();
</script>

<Dialog title="Clear local data for {name}?" {oncancel}>
  <p class="text">
    Removes the window layout and open tabs Dea keeps for this project on this
    device. Project files are not touched.
  </p>
  {#snippet actions()}
    <Button data-autofocus onclick={oncancel}>
      Cancel <kbd class="kc">Esc</kbd>
    </Button>
    <Button variant="danger" onclick={onclear}>Clear</Button>
  {/snippet}
</Dialog>

<style>
  .text {
    color: var(--text-2);
  }

  .kc {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-2);
  }
</style>
