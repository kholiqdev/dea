<script lang="ts">
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import { shellContext } from "./context";
  import { effectiveShortcuts } from "../contributions/shortcuts";

  const ctx = shellContext();
  const module = $derived(
    ctx.shell.activeModule
      ? ctx.registry.module(ctx.shell.activeModule)
      : undefined,
  );
  const sidebarVisible = $derived(
    module ? ctx.shell.sidebar(module.id).visible : false,
  );
  const toggleChord = $derived.by(() => {
    const b = effectiveShortcuts(ctx.registry, settings.shortcuts).find(
      (s) => s.command === "shell.toggleSidebar",
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
    <span class="project">No project open</span>
    <!-- The environment picker joins here in Plan 3b. -->
  </div>
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

  .start {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    min-width: 0;
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
