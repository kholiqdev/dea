<script lang="ts">
  import ChevronsLeft from "@lucide/svelte/icons/chevrons-left";
  import {
    currentPlatform,
    formatChord,
    parseChord,
  } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import type { ModuleManifest } from "../contributions/types";
  import { railIcon } from "../modules";
  import { shellContext } from "./context";
  import { effectiveShortcuts } from "./hotkeys.svelte";

  const ctx = shellContext();
  const platform = currentPlatform();
  const expanded = $derived(ctx.shell.railExpanded);
  const main = $derived(
    ctx.registry.modules.filter((m) => m.rail?.group === "main"),
  );
  const bottom = $derived(
    ctx.registry.modules.filter((m) => m.rail?.group === "bottom"),
  );
  const bindings = $derived(
    effectiveShortcuts(ctx.registry, settings.shortcuts),
  );

  function chordOf(m: ModuleManifest): string | null {
    const b = bindings.find((s) => s.command === `${m.id}.open`);
    if (!b) return null;
    try {
      return formatChord(parseChord(b.chord), platform);
    } catch {
      return null;
    }
  }
</script>

{#snippet item(m: ModuleManifest)}
  {@const Icon = railIcon(m.rail?.icon ?? "")}
  {@const chord = chordOf(m)}
  <button
    type="button"
    class={expanded ? "srow" : "rail-btn"}
    data-module={m.id}
    data-shell-control
    aria-label={m.title}
    aria-current={ctx.shell.activeModule === m.id ? "page" : undefined}
    title={chord ? `${m.title}, ${chord}` : m.title}
    onclick={() => ctx.shell.openModule(m.id)}
  >
    {#if Icon}<Icon size={16} strokeWidth={1.5} />{/if}
    {#if expanded}
      <span class="label">{m.title}</span>
      {#if chord}<span class="meta">{chord}</span>{/if}
    {/if}
  </button>
{/snippet}

<nav class="rail" class:wide={expanded} aria-label="Modules">
  {#if expanded}
    <div class="head">
      <span class="brand">Dea</span>
      <button
        type="button"
        class="iconbtn"
        data-shell-control
        aria-label="Collapse the rail"
        title="Collapse the rail"
        onclick={() => ctx.shell.toggleRail()}
      >
        <ChevronsLeft size={16} strokeWidth={1.5} />
      </button>
    </div>
    <h3 class="group">Modules</h3>
  {:else}
    <button
      type="button"
      class="logo"
      data-shell-control
      aria-label="Show module names"
      title="Show module names"
      onclick={() => ctx.shell.toggleRail()}
    >
      <span class="mark"></span>
    </button>
  {/if}
  {#each main as m (m.id)}
    {@render item(m)}
  {/each}
  <div class="bottom">
    {#each bottom as m (m.id)}
      {@render item(m)}
    {/each}
  </div>
</nav>

<style>
  .rail {
    width: var(--w-rail);
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-1);
    padding: var(--s-2) 0;
    background: var(--bg-0);
    border-right: 1px solid var(--line);
    overflow-y: auto;
  }

  .rail.wide {
    width: var(--w-rail-wide);
    align-items: stretch;
    padding: var(--s-2);
    gap: 0;
  }

  .logo {
    width: 36px;
    height: 36px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--s-2);
    padding: 0;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
  }

  .mark {
    display: block;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }

  .head {
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--s-1) 0 var(--s-2);
    margin-bottom: var(--s-1);
  }

  .brand {
    font-weight: 600;
  }

  .group {
    margin: var(--s-1) var(--s-2) 0;
    height: 16px;
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-3);
  }

  .rail-btn,
  .srow,
  .iconbtn {
    border: 0;
    background: none;
    color: var(--text-3);
  }

  .rail-btn {
    width: 36px;
    height: 36px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: var(--r-ctl);
  }

  .srow {
    width: 100%;
    height: 30px;
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2);
    border-radius: var(--r-ctl);
    color: var(--text-2);
    text-align: left;
  }

  .srow .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    font: 500 11px/1 var(--font-mono);
    color: var(--text-3);
  }

  .rail-btn:hover,
  .srow:hover,
  .iconbtn:hover,
  .logo:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .rail-btn[aria-current="page"],
  .srow[aria-current="page"] {
    background: var(--bg-3);
    box-shadow:
      var(--highlight),
      inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .iconbtn {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: var(--r-small);
  }

  .bottom {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    align-items: inherit;
    gap: var(--s-1);
  }

  .wide .bottom {
    gap: 0;
    padding-top: var(--s-2);
  }
</style>
