<!--
  A JSONPath over a JSON body. The path and what it selects live in the
  body's controls, so the body can show the result; while the filter is not
  shown, it selects nothing and keeps its path. Shown again, it runs at once.
  At rest the field shows the keycaps of its shortcut; the count of matches
  takes their place.
-->
<script lang="ts">
  import { filterResponse } from "$lib/ipc/responses";
  import { errorMessage, isIpcError } from "$lib/ipc/errors";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import type { BodyControls } from "./body-controls.svelte";
  import { FILTER_CHORD } from "./filter-layout";

  let {
    tabId,
    controls,
    keys,
  }: {
    tabId: string;
    controls: BodyControls;
    /** Whether the row leaves the field room for its keycaps. */
    keys: boolean;
  } = $props();

  const platform = currentPlatform();
  const chord = parseChord(FILTER_CHORD);
  const keycaps = chordParts(chord, platform);
  // The same chord, in the key names of `aria-keyshortcuts`.
  const shortcut = [
    chord.mod && (platform === "mac" ? "Meta" : "Control"),
    chord.shift && "Shift",
    chord.alt && "Alt",
    chord.key.toUpperCase(),
  ]
    .filter(Boolean)
    .join("+");

  const DEBOUNCE_MS = 200;

  let count = $state<number | null>(null);
  let asked = 0;
  // Typing waits for a pause; a path the filter comes back with does not.
  let shown = false;

  const caption = $derived(
    count === null
      ? ""
      : count === 0
        ? "No matches"
        : count === 1
          ? "1 match"
          : `${count} matches`,
  );

  function clear(): void {
    asked++;
    count = null;
    controls.filtered = null;
    controls.filterError = null;
  }

  $effect(() => {
    const wanted = controls.filterPath.trim();
    const wait = shown ? DEBOUNCE_MS : 0;
    shown = true;
    if (wanted === "") {
      clear();
      return;
    }
    const timer = setTimeout(() => void run(wanted), wait);
    return () => clearTimeout(timer);
  });

  // Hidden, the filter no longer narrows the body. A focused field that
  // goes may never hear its blur, which would keep the views hidden.
  $effect(() => () => {
    clear();
    controls.filterFocused = false;
  });

  async function run(wanted: string): Promise<void> {
    const mine = ++asked;
    try {
      const result = await filterResponse(tabId, wanted);
      if (mine !== asked) return;
      count = result.count;
      controls.filterError = null;
      controls.filtered = result;
    } catch (e) {
      if (mine !== asked) return;
      count = null;
      controls.filterError =
        isIpcError(e) && e.kind === "invalid"
          ? `Not a valid JSONPath: ${e.message}`
          : `The filter could not run: ${errorMessage(e)}`;
      controls.filtered = null;
    }
  }
</script>

<div class={["field", { active: controls.filterActive }]}>
  <input
    type="text"
    aria-label="Filter with JSONPath"
    aria-keyshortcuts={shortcut}
    placeholder="$.data[*].id"
    autocomplete="off"
    spellcheck="false"
    bind:value={controls.filterPath}
    onfocus={() => (controls.filterFocused = true)}
    onblur={() => (controls.filterFocused = false)}
    onkeydown={(e) => {
      if (e.key !== "Escape" || controls.filterPath === "") return;
      e.preventDefault();
      controls.filterPath = "";
    }}
  />
  {#if caption}
    <span class="count" aria-live="polite">{caption}</span>
  {:else if keys && controls.filterPath === ""}
    <span class="keys" aria-hidden="true">
      {#each keycaps as key (key)}<kbd>{key}</kbd>{/each}
    </span>
  {/if}
</div>

<style>
  /* At rest the field leaves the row to the sections and the views; in use
     it widens to 280 px, or to the room there is. Short of room it gives way
     before the sections, down to 120 px. filter-layout.ts follows these. */
  .field {
    flex: 0 1000 168px;
    min-width: 120px;
    height: 28px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: var(--s-1);
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .field.active {
    flex: 0 1000 280px;
  }

  .field:focus-within {
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 0 0 var(--s-3);
    border: 0;
    outline: none;
    background: none;
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .count {
    flex: none;
    padding-right: var(--s-2);
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-3);
  }

  .keys {
    flex: none;
    display: inline-flex;
    gap: 2px;
  }

  kbd {
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
