<script lang="ts">
  import TabBar from "../tabs/TabBar.svelte";
  import { shellContext } from "./context";
  import LazyView from "./LazyView.svelte";
  import ProjectBanner from "./ProjectBanner.svelte";

  const ctx = shellContext();
  // A derived, not an {@const}: a component taken from {@const} keeps its
  // first value, so switching modules would not swap the view.
  const View = $derived(ctx.shell.activeView);
  // The active tab always belongs to the active module.
  const tab = $derived(ctx.tabs.active);
  const tabType = $derived(tab ? ctx.registry.tabType(tab.tabType) : undefined);

  let count = 0;
  // A closed tab takes the focus with it: give it to the tab that is now
  // active, or to the work area when none is left.
  function keepFocus(main: HTMLElement): void {
    const n = ctx.tabs.tabs.length;
    const closed = n < count;
    count = n;
    const lost =
      !document.activeElement || document.activeElement === document.body;
    if (!closed || !lost) return;
    (
      main.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ??
      main
    ).focus();
  }
</script>

<main class="main" tabindex="-1" {@attach keepFocus}>
  <ProjectBanner />
  {#if ctx.project.info}<TabBar />{/if}
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

  .main:focus {
    outline: none;
  }

  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
</style>
