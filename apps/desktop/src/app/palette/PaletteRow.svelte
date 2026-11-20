<!-- One palette result: its title, environment, detail and shortcut. -->
<script lang="ts">
  import type { PaletteEntry } from "./palette-search.svelte";

  let {
    entry,
    id,
    active,
    shortcut,
    onchoose,
    onpoint,
  }: {
    entry: PaletteEntry;
    id: string;
    active: boolean;
    /** The shortcut text, such as "⌘,", or null. */
    shortcut: string | null;
    onchoose: () => void;
    onpoint: () => void;
  } = $props();
</script>

<div
  {id}
  class={["row", { active }]}
  role="option"
  tabindex="-1"
  aria-selected={active}
  onclick={onchoose}
  onkeydown={(e) => {
    if (e.key === "Enter") onchoose();
  }}
  onmousemove={onpoint}
>
  <span class="label">{entry.title}</span>
  {#if entry.environment}
    <!-- An environment item would otherwise read its name twice. -->
    <span
      class={["tab-env", { "env-prod": entry.environment.production }]}
      data-env={entry.environment.name}
      aria-hidden={entry.environment.name === entry.title || undefined}
      >{entry.environment.name}</span
    >
  {/if}
  {#if entry.meta}<span class="meta">{entry.meta}</span>{/if}
  <span class="end">
    {#if shortcut}<kbd class="shortcut kc">{shortcut}</kbd>{/if}
    {#if active}<kbd class="kc" aria-hidden="true">↵</kbd>{/if}
  </span>
</div>

<style>
  .row {
    height: 32px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-3);
    border-radius: var(--r-ctl);
    color: var(--text-1);
    cursor: default;
  }

  .row.active {
    background: var(--bg-3);
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--text-3);
  }

  .end {
    margin-left: auto;
    display: flex;
    gap: var(--s-1);
  }

  .tab-env {
    flex: none;
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 400 11px/1 var(--font-mono);
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .env-prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .kc {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-2);
  }
</style>
