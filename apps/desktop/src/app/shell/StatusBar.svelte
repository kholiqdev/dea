<script lang="ts">
  import { shellContext } from "./context";
  import LazyView from "./LazyView.svelte";

  const ctx = shellContext();
  const start = $derived(
    ctx.registry.statusItems.filter((s) => s.align === "start"),
  );
  const end = $derived(
    ctx.registry.statusItems.filter((s) => s.align === "end"),
  );
</script>

<footer class="statusbar">
  {#each start as item (item.id)}
    <div class="status-item" data-status-item={item.id}>
      <LazyView loader={item.view} />
    </div>
  {/each}
  <div class="end">
    {#each end as item (item.id)}
      <div class="status-item" data-status-item={item.id}>
        <LazyView loader={item.view} />
      </div>
    {/each}
  </div>
</footer>

<style>
  .statusbar {
    height: var(--h-status);
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-4);
    padding: 0 var(--s-3);
    background: var(--bg-0);
    border-top: 1px solid var(--line);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
  }

  .end {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--s-4);
  }

  .status-item {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
