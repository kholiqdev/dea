<!-- The Requests sidebar: the collection tree with a filter, and the environments. -->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import Plus from "@lucide/svelte/icons/plus";
  import { environmentColor, projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { treeState, type TreeRowData } from "../state/tree.svelte";
  import TreeRow from "./TreeRow.svelte";

  const ROW = 28;
  const OVERSCAN = 10;
  const FALLBACK_HEIGHT = 800;

  const project = projectApi();
  const rows = $derived(treeState.rows);
  const environments = $derived(project.info?.environments ?? []);

  let scrollTop = $state(0);
  let clientHeight = $state(0);
  let scroller = $state<HTMLElement>();
  let tree = $state<HTMLElement>();
  let filterInput = $state<HTMLInputElement>();

  const height = $derived(clientHeight || FALLBACK_HEIGHT);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(
    Math.min(rows.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN),
  );
  const visible = $derived(rows.slice(start, end));
  const activeIndex = $derived(
    rows.findIndex((r) => r.path === treeState.selected),
  );

  // jsdom has no ResizeObserver; the height then stays 0 and the fallback is used.
  function measure(el: HTMLElement): (() => void) | void {
    clientHeight = el.clientHeight;
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => (clientHeight = el.clientHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }

  onMount(() => {
    void treeState.load();
  });

  $effect(() => {
    if (project.changed.some((p) => p.startsWith("requests/")))
      void treeState.load();
  });

  function onFilter(event: Event): void {
    performance.mark("requests-filter-start");
    treeState.filter = (event.currentTarget as HTMLInputElement).value;
    void tick().then(() =>
      performance.measure("requests-filter", "requests-filter-start"),
    );
  }

  function isEditable(target: EventTarget | null): boolean {
    return (
      target instanceof Element &&
      target.closest(
        "input, textarea, select, [contenteditable]:not([contenteditable='false'])",
      ) !== null
    );
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey)
      return;
    if (isEditable(event.target)) return;
    event.preventDefault();
    filterInput?.focus();
  }

  function open(row: TreeRowData): void {
    if (row.kind === "folder") {
      treeState.toggle(row.path);
      return;
    }
    const shell = shellApi();
    const id = shell.findTab(
      "requests.request",
      (params) => (params as { path?: string } | null)?.path === row.path,
    );
    if (id !== undefined) shell.activateTab(id);
    else shell.openTab("requests.request", row.name, { path: row.path });
  }

  function select(index: number): void {
    const row = rows[index];
    if (!row) return;
    treeState.selected = row.path;
    if (!scroller) return;
    const top = index * ROW;
    if (top < scrollTop) scrollTop = scroller.scrollTop = top;
    else if (top + ROW > scrollTop + height)
      scrollTop = scroller.scrollTop = top + ROW - height;
  }

  function parentIndex(index: number): number {
    const depth = rows[index]?.depth ?? 0;
    for (let i = index - 1; i >= 0; i--)
      if ((rows[i]?.depth ?? 0) < depth) return i;
    return -1;
  }

  function onTreeKeydown(event: KeyboardEvent): void {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const index = activeIndex;
    const row = rows[index];
    switch (event.key) {
      case "ArrowDown":
        select(Math.min(rows.length - 1, index + 1));
        break;
      case "ArrowUp":
        select(index < 0 ? 0 : Math.max(0, index - 1));
        break;
      case "Home":
        select(0);
        break;
      case "End":
        select(rows.length - 1);
        break;
      case "ArrowRight":
        if (row?.kind !== "folder") break;
        if (!row.open) treeState.expanded.add(row.path);
        else if ((rows[index + 1]?.depth ?? 0) > row.depth) select(index + 1);
        break;
      case "ArrowLeft":
        if (!row) break;
        if (row.kind === "folder" && row.open && !treeState.filter.trim())
          treeState.expanded.delete(row.path);
        else if (parentIndex(index) >= 0) select(parentIndex(index));
        break;
      case "Enter":
        if (row) open(row);
        break;
      default:
        return;
    }
    event.preventDefault();
  }

  function onRowClick(index: number): void {
    const row = rows[index];
    if (!row) return;
    select(index);
    tree?.focus();
    open(row);
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="sidebar">
  <div class="head">
    <h2>Collections</h2>
    <button
      type="button"
      class="icon"
      aria-label="New request"
      title="New request"
    >
      <Plus size={16} strokeWidth={1.5} />
    </button>
  </div>

  <div class="filter">
    <label class="sr" for="requests-filter">Filter collections</label>
    <input
      id="requests-filter"
      type="search"
      placeholder="Filter"
      autocomplete="off"
      bind:this={filterInput}
      value={treeState.filter}
      oninput={onFilter}
    />
    <kbd class="kc">/</kbd>
  </div>

  {#if treeState.info && treeState.info.collections.length === 0}
    <div class="empty">
      <p>No requests yet</p>
      <button type="button" class="action">New collection</button>
    </div>
  {:else if treeState.info}
    <div
      class="scroller"
      bind:this={scroller}
      {@attach measure}
      onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    >
      <div class="spacer" style:height="{rows.length * ROW}px">
        <div
          class="tree"
          role="tree"
          aria-label="Collections"
          tabindex="0"
          aria-activedescendant={activeIndex >= start && activeIndex < end
            ? `request-row-${activeIndex}`
            : undefined}
          bind:this={tree}
          onkeydown={onTreeKeydown}
          style:transform="translateY({start * ROW}px)"
        >
          {#each visible as row, i (row.path)}
            <TreeRow
              {row}
              id="request-row-{start + i}"
              selected={row.path === treeState.selected}
              onclick={() => onRowClick(start + i)}
            />
          {/each}
        </div>
      </div>
    </div>
  {/if}

  <div class="envs">
    <h3>Environments</h3>
    {#each environments as env (env.name)}
      <button
        type="button"
        class="env"
        aria-current={project.environment?.name === env.name
          ? "true"
          : undefined}
        onclick={() => void project.setEnvironment(env.name)}
      >
        {#if env.production}
          <span class="tab-env env-prod">{env.name}</span>
        {:else}
          <span class="dot" style:background={environmentColor(env)}></span>
          <span class="label">{env.name}</span>
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .sidebar {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--s-2) var(--s-2) 0;
  }

  h2,
  h3 {
    font-size: 13px;
    font-weight: 500;
  }

  h3 {
    padding: var(--s-2);
  }

  .icon {
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--text-2);
  }

  .icon:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .filter {
    position: relative;
    margin: 0 var(--s-2);
  }

  .filter input {
    width: 100%;
    height: 28px;
    padding: 0 28px 0 var(--s-2);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .filter input:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }

  .kc {
    position: absolute;
    right: var(--s-2);
    top: 50%;
    transform: translateY(-50%);
    padding: 2px 5px;
    border-radius: 4px;
    background: var(--bg-3);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-3);
    pointer-events: none;
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .scroller {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--s-1);
  }

  .spacer {
    position: relative;
  }

  .tree {
    outline: none;
  }

  .tree:focus-visible {
    box-shadow: inset 0 0 0 1px var(--line-strong);
    border-radius: var(--r-ctl);
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--s-2);
    color: var(--text-3);
  }

  .action {
    height: 28px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
  }

  .envs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: var(--s-2);
  }

  .env {
    height: 28px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--text-2);
    text-align: left;
  }

  .env:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .env[aria-current="true"] {
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
  }

  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 4px;
  }

  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tab-env {
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
</style>
