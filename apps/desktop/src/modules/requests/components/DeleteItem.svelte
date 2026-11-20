<!-- Asks before a request or folder is deleted; Undo can bring it back. -->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import type { TreeRowData } from "../state/tree.svelte";

  let {
    row,
    ondelete,
    oncancel,
  }: { row: TreeRowData; ondelete: () => void; oncancel: () => void } =
    $props();

  const count = $derived(row.count ?? 0);
</script>

<Dialog title="Delete {row.name}?" {oncancel}>
  <p class="text">
    {#if row.kind === "folder"}
      The folder and the {count}
      {count === 1 ? "request" : "requests"} in it are removed from the project. You
      can undo this for 10 seconds.
    {:else}
      The request file is removed from the project. You can undo this for 10
      seconds.
    {/if}
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
