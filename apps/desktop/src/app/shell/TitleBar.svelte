<script lang="ts">
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import SearchIcon from "@lucide/svelte/icons/search";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import { shellContext } from "./context";
  import { effectiveShortcuts } from "../contributions/shortcuts";
  import { PALETTE_COMMAND } from "../palette/palette-search.svelte";
  import EnvironmentPicker from "./EnvironmentPicker.svelte";
  import { menus } from "./menus.svelte";
  import ProjectMenu from "./ProjectMenu.svelte";

  const ctx = shellContext();
  const module = $derived(
    ctx.shell.activeModule
      ? ctx.registry.module(ctx.shell.activeModule)
      : undefined,
  );
  const sidebarVisible = $derived(
    module ? ctx.shell.sidebarShown(module.id) : false,
  );
  const toggleChord = $derived.by(() => {
    const b = effectiveShortcuts(ctx.registry, settings.shortcuts).find(
      (s) => s.command === "shell.toggleSidebar",
    );
    return b ? formatChordText(b.chord, currentPlatform()) : null;
  });
  const paletteChord = $derived.by(() => {
    const b = effectiveShortcuts(ctx.registry, settings.shortcuts).find(
      (s) => s.command === PALETTE_COMMAND,
    );
    return b ? formatChordText(b.chord, currentPlatform()) : null;
  });
  const toggleLabel = $derived(
    sidebarVisible ? "Hide sidebar" : "Show sidebar",
  );
</script>

<header class="titlebar">
  <div class="start">
    {#if module?.sidebar}
      <button
        type="button"
        class="iconbtn"
        data-shell-control
        aria-label={toggleLabel}
        title={toggleChord ? `${toggleLabel}, ${toggleChord}` : toggleLabel}
        onclick={() => module && ctx.shell.toggleSidebar(module.id)}
      >
        <PanelLeft size={16} strokeWidth={1.5} />
      </button>
    {/if}
    {#if ctx.project.info}
      <div class="where">
        <ProjectMenu />
        <span class="sep" aria-hidden="true">/</span>
        <EnvironmentPicker />
      </div>
    {:else}
      <span class="project">No project open</span>
      {#if ctx.shell.activeModule && ctx.shell.activeModule !== "project"}
        <button
          type="button"
          class="back"
          data-shell-control
          onclick={() => ctx.shell.openModule("project")}>Back</button
        >
      {/if}
    {/if}
  </div>
  <button
    type="button"
    class="search"
    data-shell-control
    onclick={() => (menus.palette = true)}
  >
    <SearchIcon size={14} strokeWidth={1.5} />
    <span class="search-label">Search</span>
    {#if paletteChord}<kbd class="kc">{paletteChord}</kbd>{/if}
  </button>
</header>

<style>
  .titlebar {
    height: var(--h-titlebar);
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 var(--s-3) 0 var(--s-4);
    background: var(--bg-0);
    border-bottom: 1px solid var(--line);
  }

  .search {
    margin: 0 auto;
    width: min(320px, 40%);
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-3);
    font: inherit;
  }

  .search:hover {
    color: var(--text-2);
  }

  .search-label {
    flex: 1;
    text-align: left;
  }

  .kc {
    font: 500 11px/1 var(--font-mono);
    color: var(--text-3);
  }

  .start {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    min-width: 0;
  }

  .where {
    display: flex;
    align-items: center;
    gap: var(--s-1);
    min-width: 0;
  }

  .sep {
    color: var(--text-4);
  }

  .back {
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .back:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  /* Production is marked on the title bar as well as in words. */
  :global(.app[data-env="production"]) .titlebar {
    box-shadow: inset 0 -2px 0 var(--negative);
  }

  .project {
    font-weight: 500;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .iconbtn {
    width: 28px;
    height: 28px;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-3);
  }

  .iconbtn:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }
</style>
