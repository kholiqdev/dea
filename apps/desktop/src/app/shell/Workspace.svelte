<script lang="ts">
  import TabBar from "../tabs/TabBar.svelte";
  import { shellContext } from "./context";
  import LazyView from "./LazyView.svelte";

  const ctx = shellContext();
  // A derived, not an {@const}: a component taken from {@const} keeps its
  // first value, so switching modules would not swap the view.
  const View = $derived(ctx.shell.activeView);
  // The active tab always belongs to the active module.
  const tab = $derived(ctx.tabs.active);
  const tabType = $derived(tab ? ctx.registry.tabType(tab.tabType) : undefined);
</script>

<main class="main">
  <TabBar />
  <div class="content">
    {#if tab && tabType}
      {#key tab.id}
        <LazyView loader={tabType.view} props={{ tab }} />
      {/key}
    {:else if View}
      <View />
    {/if}
  </div>
</main>

<style>
  .main {
    flex: 999 1 560px;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-1);
  }

  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
</style>
