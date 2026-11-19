<script lang="ts">
  import { registry } from "$app/contributions/registry.svelte";
  import { sections } from "./state/sections.svelte";

  const list = $derived(registry.settingsSections);
  const section = $derived(
    list.find((s) => s.id === sections.current) ?? list[0],
  );
  const loaded = $derived(section?.view());
</script>

<div class="scroll">
  {#if loaded}
    {#await loaded then mod}
      {@const View = mod.default}
      <View />
    {/await}
  {/if}
</div>

<style>
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
</style>
