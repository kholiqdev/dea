<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import type { EnvReference } from "$lib/ipc/bindings/EnvReference";
  import { environmentReferences } from "../api";

  let {
    name,
    ondelete,
    oncancel,
  }: { name: string; ondelete: () => void; oncancel: () => void } = $props();

  const references = $derived(
    environmentReferences(name).catch((): EnvReference[] => []),
  );
</script>

<Dialog title="Delete {name}?" {oncancel}>
  {#await references then refs}
    {#if refs.length > 0}
      <p class="text">These use it now:</p>
      <dl class="kv">
        {#each refs as ref (ref.label)}
          <div>
            <dt>{ref.label}</dt>
            <dd>{ref.count}</dd>
          </div>
        {/each}
      </dl>
    {:else}
      <p class="text">Nothing in this project uses it.</p>
    {/if}
  {/await}
  {#snippet actions()}
    <Button data-autofocus onclick={oncancel}>Cancel</Button>
    <Button variant="danger" onclick={ondelete}>Delete {name}</Button>
  {/snippet}
</Dialog>

<style>
  .text {
    color: var(--text-2);
  }

  .kv {
    margin: 0;
    display: grid;
    gap: var(--s-1);
  }

  .kv div {
    display: flex;
    justify-content: space-between;
  }

  dt {
    color: var(--text-2);
  }

  dd {
    margin: 0;
    font-family: var(--font-mono);
  }
</style>
