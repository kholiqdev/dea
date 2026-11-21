<!--
  A JSONPath over a JSON body. What it selects goes to `onresult`; `null`
  while the filter is empty or not valid.
-->
<script lang="ts">
  import { filterResponse } from "$lib/ipc/responses";
  import { errorMessage, isIpcError } from "$lib/ipc/errors";
  import type { Filtered } from "$lib/ipc/bindings/Filtered";

  let {
    tabId,
    onresult,
  }: { tabId: string; onresult: (result: Filtered | null) => void } = $props();

  const DEBOUNCE_MS = 200;

  let path = $state("");
  let count = $state<number | null>(null);
  let error = $state<string | null>(null);
  let asked = 0;

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
    error = null;
    onresult(null);
  }

  $effect(() => {
    const wanted = path.trim();
    if (wanted === "") {
      clear();
      return;
    }
    const timer = setTimeout(() => void run(wanted), DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  async function run(wanted: string): Promise<void> {
    const mine = ++asked;
    try {
      const result = await filterResponse(tabId, wanted);
      if (mine !== asked) return;
      count = result.count;
      error = null;
      onresult(result);
    } catch (e) {
      if (mine !== asked) return;
      count = null;
      error =
        isIpcError(e) && e.kind === "invalid"
          ? `Not a valid JSONPath: ${e.message}`
          : `The filter could not run: ${errorMessage(e)}`;
      onresult(null);
    }
  }
</script>

<div class="filter">
  <input
    type="text"
    aria-label="Filter with JSONPath"
    placeholder="$.events[*].code"
    autocomplete="off"
    spellcheck="false"
    bind:value={path}
    onkeydown={(e) => {
      if (e.key !== "Escape" || path === "") return;
      e.preventDefault();
      path = "";
    }}
  />
  {#if error}
    <p class="note error" role="alert">{error}</p>
  {:else if caption}
    <p class="note" aria-live="polite">{caption}</p>
  {/if}
</div>

<style>
  .filter {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-2) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
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

  .note {
    margin: 0;
    white-space: nowrap;
    color: var(--text-3);
  }

  .error {
    color: var(--negative-text);
    white-space: normal;
  }
</style>
