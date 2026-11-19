<!-- The status bar's way into the command palette, with its shortcut. -->
<script lang="ts">
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import { effectiveShortcuts } from "../contributions/shortcuts";
  import { PALETTE_COMMAND } from "../palette/palette-search.svelte";
  import { shellContext } from "./context";
  import { menus } from "./menus.svelte";

  const ctx = shellContext();
  const chord = $derived.by(() => {
    const b = effectiveShortcuts(ctx.registry, settings.shortcuts).find(
      (s) => s.command === PALETTE_COMMAND,
    );
    return b ? formatChordText(b.chord, currentPlatform()) : null;
  });
</script>

<button type="button" class="commands" onclick={() => (menus.palette = true)}>
  {#if chord}<kbd class="kc">{chord}</kbd>{/if}
  <span>Commands</span>
</button>

<style>
  .commands {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
  }

  .commands:hover {
    color: var(--text-1);
  }

  .kc {
    font: 500 11px/1 var(--font-mono);
  }
</style>
