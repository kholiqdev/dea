<!--
  What came back of a tab's send (F1-REQ-08): the two rows of a response's
  header, the progress of a request on its way, or why it ended without
  one. The first row is the toolbar; the second has the sections (body,
  headers, cookies) and, on the body, its view and JSONPath filter.
-->
<script lang="ts">
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import { tick } from "svelte";
  import { watchWidth } from "../../lib/columns";
  import { formatSeconds } from "../../lib/format";
  import type { Exchange } from "../../state/exchange.svelte";
  import { BodyControls } from "./body-controls.svelte";
  import type { BodyView } from "./body-view";
  import FailureView from "./FailureView.svelte";
  import HeadersTable from "./HeadersTable.svelte";
  import JsonFilter from "./JsonFilter.svelte";
  import ReceivedCookies from "./ReceivedCookies.svelte";
  import ResponseBody from "./ResponseBody.svelte";
  import ResponseToolbar from "./ResponseToolbar.svelte";
  import SentInPill from "./SentInPill.svelte";
  import EventList from "./EventList.svelte";
  import SendingView from "./SendingView.svelte";

  let {
    tabId,
    name,
    exchange,
    layout,
    stacked,
    ontogglelayout,
  }: {
    tabId: string;
    name: string;
    exchange: Exchange;
    layout: "right" | "below";
    stacked: boolean;
    ontogglelayout: () => void;
  } = $props();

  const SECTIONS = [
    { id: "body", label: "Body" },
    { id: "headers", label: "Headers" },
    { id: "cookies", label: "Cookies" },
  ] as const;
  type Section = (typeof SECTIONS)[number]["id"];
  const LABELS: Record<BodyView, string> = {
    pretty: "Pretty",
    raw: "Raw",
    preview: "Preview",
  };
  /** The pane width from which the filter fits in the second row. */
  const FILTER_INLINE_FROM = 700;

  const uid = $props.id();
  let section = $state<Section>("body");
  let tablist = $state<HTMLElement>();
  let radios = $state<HTMLElement>();
  let filterRow = $state<HTMLElement>();
  // Not measured yet, as in tests, the pane counts as wide.
  let width = $state(0);
  const wide = $derived(width === 0 || width >= FILTER_INLINE_FROM);

  async function onkeydown(e: KeyboardEvent): Promise<void> {
    const at = SECTIONS.findIndex((s) => s.id === section);
    const to: Record<string, number> = {
      ArrowRight: (at + 1) % SECTIONS.length,
      ArrowLeft: (at - 1 + SECTIONS.length) % SECTIONS.length,
      Home: 0,
      End: SECTIONS.length - 1,
    };
    const next = SECTIONS[to[e.key] ?? -1];
    if (!next) return;
    e.preventDefault();
    section = next.id;
    await tick();
    tablist?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
  }

  const project = projectApi();
  const shell = shellApi();
  const sendKeys = chordParts(parseChord("Mod+Enter"), currentPlatform());
  const current = $derived(exchange.state);
  // Dea asks whether to send to production: nothing has left, nothing runs.
  const waiting = $derived(
    current.phase === "sending" && !current.started && shell.confirmingSend,
  );
  // The view and filter chosen for a response last while it stays, and
  // when it comes back after a send that did not go out.
  const kept = new WeakMap<ResponseInfo, BodyControls>();
  const controls = $derived.by(() => {
    if (current.phase !== "done") return null;
    const response = current.response;
    let found = kept.get(response);
    if (!found) {
      found = new BodyControls(response);
      kept.set(response, found);
    }
    return found;
  });

  async function onviewkeys(e: KeyboardEvent): Promise<void> {
    if (!controls) return;
    const views = controls.views;
    const at = views.indexOf(controls.view);
    const to: Record<string, number> = {
      ArrowRight: (at + 1) % views.length,
      ArrowDown: (at + 1) % views.length,
      ArrowLeft: (at - 1 + views.length) % views.length,
      ArrowUp: (at - 1 + views.length) % views.length,
      Home: 0,
      End: views.length - 1,
    };
    const next = views[to[e.key] ?? -1];
    if (!next) return;
    e.preventDefault();
    controls.view = next;
    await tick();
    radios?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
  }

  async function togglefilter(of: BodyControls): Promise<void> {
    of.filterOpen = !of.filterOpen;
    if (!of.filterOpen) return;
    await tick();
    filterRow?.querySelector("input")?.focus();
  }

  /**
   * Follows the pane's width; a pane not laid out keeps the last one. A
   * path typed in the row goes on filtering when the pane narrows: the
   * filter opens in its own row.
   */
  function measure(pane: HTMLElement): () => void {
    return watchWidth(pane, (w) => {
      if (w <= 0) return;
      const was = wide;
      width = w;
      if (was && !wide && controls?.filterPath.trim())
        controls.filterOpen = true;
    });
  }

  // Seconds go by on the clock `startedAt` was taken from.
  let now = $state(performance.now());
  $effect(() => {
    if (current.phase !== "sending" || waiting) return;
    now = performance.now();
    const timer = setInterval(() => (now = performance.now()), 100);
    return () => clearInterval(timer);
  });
  const elapsed = $derived(
    current.phase === "sending" ? Math.max(0, now - current.startedAt) : 0,
  );

  // The environment a request was sent in, and whether the window moved on.
  const sentIn = $derived.by(() => {
    if (current.phase === "idle" || current.phase === "cancelled") return null;
    const sent = current.environment;
    if (sent === null) return null;
    const production =
      project.info?.environments.find((e) => e.name === sent)?.production ??
      false;
    return {
      name: sent,
      production,
      moved: sent !== project.environment?.name,
    };
  });
</script>

{#snippet sections(counts: Record<Section, number> | null)}
  <!-- While sending it stays in place, not in use, so the body below does
       not move when the response comes. -->
  <div class="row" inert={counts === null}>
    <div
      class="sections"
      role="tablist"
      aria-label="Response sections"
      tabindex="-1"
      bind:this={tablist}
      onkeydown={(e) => void onkeydown(e)}
    >
      {#each SECTIONS as item (item.id)}
        {@const selected = section === item.id}
        <button
          type="button"
          role="tab"
          id="{uid}-{item.id}"
          aria-selected={selected}
          aria-controls="{uid}-panel"
          tabindex={selected ? 0 : -1}
          onclick={() => (section = item.id)}
        >
          {item.label}
          {#if counts?.[item.id]}<span class="count">{counts[item.id]}</span
            >{/if}
        </button>
      {/each}
    </div>
    {#if controls && section === "body"}
      {@const body = controls}
      {#if body.views.length > 1}
        <div
          class="views"
          role="radiogroup"
          aria-label="Body view"
          tabindex="-1"
          bind:this={radios}
          onkeydown={(e) => void onviewkeys(e)}
        >
          {#each body.views as option (option)}
            <button
              type="button"
              role="radio"
              aria-checked={body.view === option}
              tabindex={body.view === option ? 0 : -1}
              onclick={() => (body.view = option)}
            >
              {LABELS[option]}
            </button>
          {/each}
        </div>
      {/if}
      {#if body.filterable && wide}
        <JsonFilter {tabId} controls={body} inline />
      {:else if body.filterable}
        {@const label = body.filterOpen
          ? "Hide the JSONPath filter"
          : "Show the JSONPath filter"}
        <button
          type="button"
          class="icon"
          aria-label={label}
          title={label}
          aria-expanded={body.filterOpen}
          aria-controls="{uid}-filter"
          onclick={() => void togglefilter(body)}
        >
          <ListFilter size={16} strokeWidth={1.5} />
        </button>
      {/if}
    {/if}
  </div>
  {#if controls?.filterable && controls.filterOpen && !wide && section === "body"}
    <div class="filter-row" id="{uid}-filter" bind:this={filterRow}>
      <JsonFilter {tabId} {controls} />
    </div>
  {/if}
{/snippet}

<section
  class="pane"
  aria-label="Response"
  aria-busy={current.phase === "sending" && !waiting ? "true" : undefined}
  {@attach measure}
>
  {#if current.phase === "idle"}
    <p class="idle">
      Send the request to see the response here.
      <span class="keys" aria-hidden="true">
        {#each sendKeys as key, i (i)}<kbd>{key}</kbd>{/each}
      </span>
    </p>
  {:else if current.phase === "sending"}
    <ResponseToolbar
      {tabId}
      {name}
      elapsed={waiting ? null : elapsed}
      {waiting}
      environment={sentIn}
      {layout}
      {stacked}
      {width}
      {ontogglelayout}
    />
    {@render sections(null)}
    <SendingView sending={current} {elapsed} {waiting} />
  {:else if current.phase === "done"}
    {@const response = current.response}
    <ResponseToolbar
      {tabId}
      {name}
      {response}
      environment={sentIn}
      stale={exchange.warning !== null}
      {layout}
      {stacked}
      {width}
      {ontogglelayout}
    />
    {@render sections({
      body: 0,
      headers: response.headers.length,
      cookies: response.cookies.length,
    })}
    <div
      class="body"
      role="tabpanel"
      id="{uid}-panel"
      aria-labelledby="{uid}-{section}"
    >
      {#if section === "headers"}
        <HeadersTable
          headers={response.headers}
          redirects={response.redirects}
        />
      {:else if section === "cookies"}
        <ReceivedCookies cookies={response.cookies} />
      {:else if controls}
        {#key controls}
          <ResponseBody
            {tabId}
            {name}
            {response}
            events={current.events}
            dropped={current.droppedEvents}
            startedAt={current.startedAt}
            {controls}
          />
        {/key}
      {/if}
    </div>
  {:else if current.phase === "failed"}
    {#if sentIn?.moved}<p class="sent-in"><SentInPill {...sentIn} /></p>{/if}
    <FailureView failure={current.failure} />
  {:else}
    <div class="cancelled">
      <p>Cancelled after {formatSeconds(current.afterMs)}.</p>
      {#if current.head}
        <p>
          {current.head.status}
          {current.head.reason}. The body was not received in full.
        </p>
      {/if}
      {#if current.events.length > 0}
        <EventList
          events={current.events}
          startedAt={current.startedAt}
          dropped={current.droppedEvents}
        />
      {/if}
    </div>
  {/if}
</section>

<style>
  .pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }

  .idle {
    margin: auto;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    color: var(--text-3);
  }

  .keys {
    display: inline-flex;
    gap: 4px;
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

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }

  /* The second row of the header: the sections, then the body's view and
     filter at the end. */
  .row {
    height: 36px;
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 0 var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .sections {
    align-self: stretch;
    flex: 1;
    min-width: 0;
    display: flex;
    gap: var(--s-5);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .sections:focus,
  .views:focus {
    outline: none;
  }

  .views {
    flex: none;
    display: inline-flex;
    padding: 2px;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .views button {
    height: 24px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .views button[aria-checked="true"] {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .icon {
    flex: none;
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .icon:hover,
  .icon[aria-expanded="true"] {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  /* The filter of a narrow pane, in a row of its own under the second. */
  .filter-row {
    flex: none;
    display: flex;
    padding: var(--s-2) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  [role="tab"] {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    white-space: nowrap;
  }

  [role="tab"]:hover {
    color: var(--text-1);
  }

  [role="tab"][aria-selected="true"] {
    color: var(--text-1);
    box-shadow: inset 0 -2px 0 var(--text-1);
  }

  /* Not in use while sending. */
  .row[inert] [role="tab"] {
    color: var(--text-3);
  }

  .row[inert] [role="tab"][aria-selected="true"] {
    box-shadow: inset 0 -2px 0 var(--text-3);
  }

  .count {
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .cancelled {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-4);
    color: var(--text-2);
  }

  .cancelled p,
  .sent-in {
    margin: 0;
  }

  .sent-in {
    padding: var(--s-3) var(--s-4) 0;
  }
</style>
