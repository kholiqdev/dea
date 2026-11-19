<script lang="ts">
  import { registry } from "$app/contributions/registry.svelte";
  import { sections } from "./state/sections.svelte";

  const list = $derived(registry.settingsSections);
  const current = $derived(sections.current ?? list[0]?.id);
</script>

<nav class="list" aria-label="Settings sections">
  {#each list as section (section.id)}
    <button
      type="button"
      class="srow"
      aria-current={section.id === current ? "true" : undefined}
      onclick={() => (sections.current = section.id)}
    >
      <span class="label">{section.title}</span>
    </button>
  {/each}
</nav>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .srow {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--text-2);
    text-align: left;
  }

  .srow:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .srow[aria-current="true"] {
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
  }

  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
