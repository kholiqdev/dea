<!-- A send that ended without a response: what went wrong, and its code. -->
<script lang="ts">
  import type { SendFailure } from "$lib/ipc/bindings/SendFailure";

  let { failure }: { failure: SendFailure } = $props();
</script>

<div class="card" role="alert">
  <h3>{failure.title}</h3>
  <p>{failure.message}</p>
  {#if failure.code}<code>{failure.code}</code>{/if}
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    margin: var(--s-4);
    padding: var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  h3 {
    margin: 0;
    font-size: 14px;
    line-height: 20px;
    font-weight: 500;
    color: var(--negative-text);
  }

  p {
    margin: 0;
    color: var(--text-2);
  }

  code {
    align-self: flex-start;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }
</style>
