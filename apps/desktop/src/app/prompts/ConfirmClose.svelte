<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import type { CloseQuestion } from "./confirm-close.svelte";

  let { question }: { question: CloseQuestion } = $props();
</script>

{#if question.pending}
  {@const pending = question.pending}
  <Dialog
    title="Save changes to {pending.tab.title}?"
    oncancel={() => pending.answer("cancel")}
  >
    <p class="text">This tab has changes that are not saved.</p>
    {#snippet actions()}
      <Button onclick={() => pending.answer("cancel")}>
        Cancel <kbd class="kc">Esc</kbd>
      </Button>
      <Button variant="danger" onclick={() => pending.answer("discard")}
        >Discard</Button
      >
      <Button
        variant="primary"
        data-autofocus
        onclick={() => pending.answer("save")}>Save</Button
      >
    {/snippet}
  </Dialog>
{/if}

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
