<!--
  A body too large to keep in memory, read from Dea in pages of rows and
  drawn without formatting. Only the rows in view (and a few around them) are
  in the page; a body of millions of rows keeps its scroll space within what
  the engine can lay out.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import { shellApi } from "$app/contributions/shell-api";
  import type { BodyKind } from "$lib/ipc/bindings/BodyKind";
  import { errorMessage } from "$lib/ipc/errors";
  import {
    findInResponse,
    responseRows,
    saveResponse,
  } from "$lib/ipc/responses";
  import {
    chordFromEvent,
    currentPlatform,
    parseChord,
    sameChord,
  } from "$lib/keyboard/chord";
  import Button from "@dea/ui/Button.svelte";
  import { formatBytes, suggestedName } from "../../lib/format";

  const ROW = 18;
  const OVERSCAN = 50;
  const PAGE = 200;
  const MAX_PAGES = 20;
  const MAX_SPACE = 8_000_000;
  // jsdom has no layout, and a viewer not yet measured needs some height.
  const FALLBACK_HEIGHT = 400;
  const FIND_KEY = parseChord("Mod+F");

  let {
    id,
    size,
    name = "response",
    kind = "text",
    contentType = null,
  }: {
    id: string;
    size: number;
    /** The request's name, which the saved file is named after. */
    name?: string;
    kind?: BodyKind;
    contentType?: string | null;
  } = $props();

  const shell = shellApi();

  let scroller = $state<HTMLElement>();
  let scrollTop = $state(0);
  let clientHeight = $state(0);
  let total = $state<number | null>(null);
  let version = $state(0);
  let error = $state<string | null>(null);

  // Pages by number, the least recently used first. Only the effect below
  // changes it, then bumps `version` for what draws from it.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pages = new Map<number, string[]>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const asked = new Set<number>();
  let generation = 0;

  const height = $derived(clientHeight || FALLBACK_HEIGHT);
  const rows = $derived(total ?? 0);
  const inView = $derived(Math.ceil(height / ROW) + 1);
  const scaled = $derived(rows * ROW > MAX_SPACE);
  const space = $derived(scaled ? MAX_SPACE : rows * ROW);
  const maxScroll = $derived(Math.max(1, MAX_SPACE - height));
  const first = $derived.by(() => {
    if (rows === 0) return 0;
    const row = scaled
      ? Math.round((scrollTop / maxScroll) * Math.max(0, rows - inView))
      : Math.floor(scrollTop / ROW);
    return Math.min(Math.max(0, row), rows - 1);
  });
  const start = $derived(Math.max(0, first - OVERSCAN));
  const end = $derived(Math.min(rows, first + inView + OVERSCAN));
  const windowTop = $derived(
    scaled ? scrollTop - (first - start) * ROW : start * ROW,
  );

  // The row numbers drawn, with their text once its page is here.
  const visible = $derived.by(() => {
    void version;
    return Array.from({ length: Math.max(0, end - start) }, (_, i) => {
      const n = start + i;
      return { n, text: pages.get(Math.floor(n / PAGE))?.[n % PAGE] ?? "" };
    });
  });

  // A different body is read from its first page.
  $effect(() => {
    void id;
    generation++;
    pages.clear();
    asked.clear();
    total = null;
    error = null;
    scrollTop = 0;
    if (scroller) scroller.scrollTop = 0;
    untrack(() => version++);
  });

  $effect(() => {
    const want =
      total === null
        ? [0]
        : range(
            Math.floor(start / PAGE),
            Math.floor(Math.max(start, end - 1) / PAGE),
          );
    const at = id;
    const mine = generation;
    for (const page of want) {
      const kept = pages.get(page);
      if (kept) {
        // Used now, so the last to go.
        pages.delete(page);
        pages.set(page, kept);
        continue;
      }
      if (asked.has(page)) continue;
      asked.add(page);
      responseRows(at, page * PAGE, PAGE)
        .then((result) => {
          if (mine !== generation) return;
          asked.delete(page);
          total = result.total;
          pages.set(page, result.rows);
          while (pages.size > MAX_PAGES) {
            const oldest = pages.keys().next().value;
            if (oldest === undefined) break;
            pages.delete(oldest);
          }
          version++;
        })
        .catch((e: unknown) => {
          if (mine !== generation) return;
          asked.delete(page);
          error = errorMessage(e);
        });
    }
  });

  function range(from: number, to: number): number[] {
    return Array.from({ length: to - from + 1 }, (_, i) => from + i);
  }

  function measure(el: HTMLElement): (() => void) | void {
    clientHeight = el.clientHeight;
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => (clientHeight = el.clientHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }

  function onscroll(): void {
    if (scroller) scrollTop = scroller.scrollTop;
  }

  /** Scrolls so that `row` is in the middle of the viewer. */
  function reveal(row: number): void {
    if (!scroller) return;
    const top = Math.max(0, row - Math.floor(inView / 2));
    const to = scaled
      ? (top / Math.max(1, rows - inView)) * maxScroll
      : top * ROW;
    scroller.scrollTop = to;
    scrollTop = scroller.scrollTop || to;
  }

  async function save(): Promise<void> {
    try {
      const saved = await saveResponse(
        id,
        suggestedName(name, kind, contentType),
      );
      if (saved !== null) shell.toast(`Saved ${saved}`);
    } catch (e) {
      shell.toast(errorMessage(e));
    }
  }

  // Find
  let finding = $state(false);
  let query = $state("");
  let input = $state<HTMLInputElement>();
  let match = $state<{ row: number; start: number; end: number } | null>(null);
  let missed = $state(false);

  async function openFind(): Promise<void> {
    finding = true;
    await tick();
    input?.focus();
    input?.select();
  }

  async function search(backwards: boolean): Promise<void> {
    if (query === "") return;
    const at = id;
    const from = match ? Math.max(0, match.row + (backwards ? -1 : 1)) : first;
    try {
      const found = await findInResponse(at, query, from, backwards);
      if (at !== id) return;
      missed = found === null;
      if (found === null) return;
      match = found;
      reveal(found.row);
    } catch (e) {
      shell.toast(errorMessage(e));
    }
  }

  function onfindkeys(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      void search(e.shiftKey);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      finding = false;
      match = null;
      missed = false;
      scroller?.focus();
    }
  }

  function onviewerkeys(e: KeyboardEvent): void {
    const chord = chordFromEvent(e, currentPlatform());
    if (chord && sameChord(chord, FIND_KEY)) {
      e.preventDefault();
      void openFind();
    }
  }
</script>

<div class="large">
  <div class="card">
    <div>
      <p class="title">The response is {formatBytes(size)}</p>
      <p class="why">
        Bodies over 10 MB are not kept in memory, so they are shown without
        formatting.
      </p>
    </div>
    <Button size="small" onclick={() => void save()}>Save to file</Button>
  </div>
  {#if error !== null}
    <p class="unread" role="alert">The body could not be read: {error}</p>
  {/if}
  <div class="frame" role="presentation" onkeydown={onviewerkeys}>
    {#if finding}
      <div class="find" role="search">
        <input
          type="text"
          aria-label="Find in response"
          bind:this={input}
          bind:value={query}
          oninput={() => {
            match = null;
            missed = false;
          }}
          onkeydown={onfindkeys}
        />
        <Button size="small" variant="ghost" onclick={() => void search(true)}>
          Previous
        </Button>
        <Button size="small" variant="ghost" onclick={() => void search(false)}>
          Next
        </Button>
        <span role="status" class="missed">{missed ? "No matches" : ""}</span>
      </div>
    {/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="viewer"
      role="document"
      aria-label="Response body"
      tabindex="0"
      bind:this={scroller}
      {@attach measure}
      {onscroll}
    >
      <div class="space" style:height="{space}px">
        <div class="window" style:top="{windowTop}px">
          {#each visible as row (row.n)}
            <div class="row">
              <span class="gutter" aria-hidden="true">{row.n + 1}</span>
              <span class="text"
                >{#if match && match.row === row.n}{row.text.slice(
                    0,
                    match.start,
                  )}<mark>{row.text.slice(match.start, match.end)}</mark
                  >{row.text.slice(match.end)}{:else}{row.text}{/if}</span
              >
            </div>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .large {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .card {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-4);
    margin: var(--s-3) var(--s-4);
    padding: var(--s-3) var(--s-4);
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .card p {
    margin: 0;
  }

  .title {
    color: var(--text-1);
    font-weight: 500;
  }

  .why {
    color: var(--text-3);
  }

  .unread {
    margin: 0;
    padding: var(--s-2) var(--s-4);
    color: var(--negative-text);
  }

  .frame {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .find {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-2) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .find input {
    width: 240px;
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-2);
    color: var(--text-1);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .missed {
    color: var(--text-3);
  }

  .viewer {
    flex: 1;
    min-height: 0;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 18px;
  }

  .space {
    position: relative;
    min-width: 100%;
    width: max-content;
  }

  .window {
    position: absolute;
    left: 0;
    min-width: 100%;
  }

  .row {
    display: flex;
    height: 18px;
    white-space: pre;
  }

  .gutter {
    flex: none;
    width: 72px;
    padding-right: var(--s-3);
    text-align: right;
    color: var(--text-4);
    user-select: none;
  }

  .text {
    color: var(--text-1);
  }

  mark {
    background: var(--accent);
    color: var(--bg-1);
  }
</style>
