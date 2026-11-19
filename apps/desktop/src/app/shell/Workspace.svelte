<script lang="ts">
  import { shellContext } from "./context";

  const ctx = shellContext();
  // A derived, not an {@const}: a component taken from {@const} keeps its
  // first value, so switching modules would not swap the view.
  const View = $derived(ctx.shell.activeView);
</script>

<main class="main">
  {#if View}
    <View />
  {/if}
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
</style>
