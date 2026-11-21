<!--
  A JSONPath over a JSON body. The path and what it selects live in the
  body's controls, so the body can show the result; while the filter is not
  shown, it selects nothing and keeps its path. Shown again, it runs at once.
-->
<script lang="ts">
  import { filterResponse } from "$lib/ipc/responses";
  import { errorMessage, isIpcError } from "$lib/ipc/errors";
  import type { BodyControls } from "./body-controls.svelte";

  let {
    tabId,
    controls,
    inline = false,
  }: {
    tabId: string;
    controls: BodyControls;
    /** In the row of the sections: a field of its own width, not the row's. */
    inline?: boolean;
  } = $props();

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

  // Hidden, the filter no longer narrows the body.
  $effect(() => () => clear());

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

<div class={["filter", { inline }]}>
  <input
    type="text"
    aria-label="Filter with JSONPath"
    placeholder="$.events[*].code"
    autocomplete="off"
    spellcheck="false"
    bind:value={controls.filterPath}
    onkeydown={(e) => {
      if (e.key !== "Escape" || controls.filterPath === "") return;
      e.preventDefault();
      controls.filterPath = "";
    }}
  />
  {#if caption}
    <p class="note" aria-live="polite">{caption}</p>
  {/if}
</div>

<style>
  .filter {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--s-3);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  /* The field keeps its place at the end of the row; the count goes before it. */
  .inline {
    flex: none;
    flex-direction: row-reverse;
  }

  .inline input {
    flex: none;
    width: 240px;
  }

  .note {
    flex: none;
    margin: 0;
    white-space: nowrap;
    color: var(--text-3);
  }
</style>
