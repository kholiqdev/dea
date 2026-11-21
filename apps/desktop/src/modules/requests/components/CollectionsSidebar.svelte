<!-- The Requests sidebar: the collection tree with a filter, and the environments. -->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import Plus from "@lucide/svelte/icons/plus";
  import { environmentColor, projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { TreeDrag } from "../state/drag.svelte";
  import {
    ROOT,
    ROW_HEIGHT as ROW,
    treeState,
    type TreeRowData,
  } from "../state/tree.svelte";
  import DeleteItem from "./DeleteItem.svelte";
  import MoveItem from "./MoveItem.svelte";
  import RenameRow from "./RenameRow.svelte";
  import TreeMenu from "./TreeMenu.svelte";
  import TreeRow from "./TreeRow.svelte";

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
  let newCollection = $state<HTMLButtonElement>();

  const height = $derived(clientHeight || FALLBACK_HEIGHT);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(
    Math.min(rows.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN),
  );
  const visible = $derived(rows.slice(start, end));
  const activeIndex = $derived(
    rows.findIndex((r) => r.path === treeState.selected),
  );
  const active = $derived(rows[activeIndex]);
  const renamingIndex = $derived(
    treeState.renaming
      ? rows.findIndex((r) => r.path === treeState.renaming)
      : -1,
  );

  const drag = new TreeDrag(treeState);
  // The click that ends a drag on the row it started on opens nothing.
  let dragEnded = false;

  /** The active row's menu, and where a right click opened it. */
  let menuOpen = $state(false);
  let menuAt = $state<{ x: number; y: number } | null>(null);
  /** The tree's own menu, open at this point. */
  let areaMenuAt = $state<{ x: number; y: number } | null>(null);
  let deleting = $state<TreeRowData | null>(null);
  let moving = $state<TreeRowData | null>(null);

  // jsdom has no ResizeObserver; the height then stays 0 and the fallback is used.
  function measure(el: HTMLElement): (() => void) | void {
    clientHeight = el.clientHeight;
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => (clientHeight = el.clientHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }

  onMount(() => {
    void treeState.reload();
    return clearTimings;
  });

  $effect(() => {
    if (project.changed.some((p) => p.startsWith("requests/")))
      void treeState.reload();
  });

  // An item being named is scrolled into view.
  $effect(() => {
    const index = renamingIndex;
    if (index >= 0) untrack(() => select(index));
  });

  // The timings of one search stay until it ends: the filter is emptied or
  // the sidebar goes away.
  function clearTimings(): void {
    performance.clearMarks("requests-filter-start");
    performance.clearMeasures("requests-filter");
  }

  function onFilter(event: Event): void {
    if (treeState.filter === "") clearTimings();
    performance.mark("requests-filter-start");
    treeState.filter = (event.currentTarget as HTMLInputElement).value;
    void tick().then(() => {
      performance.measure("requests-filter", "requests-filter-start");
    });
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
    if (event.key === "Escape" && drag.source) {
      drag.cancel();
      event.preventDefault();
      event.stopPropagation();
      return;
    }
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
    else
      shell.openTab("requests.request", row.name, {
        path: row.path,
        method: row.method,
      });
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
    if (isEditable(event.target)) return;
    if (event.altKey && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
      onAltKeydown(event);
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const index = activeIndex;
    const row = rows[index];
    switch (event.key) {
      case "F2":
        if (row) treeState.renaming = row.path;
        break;
      case "F10":
        if (!event.shiftKey) return;
        openMenu(null);
        break;
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

  /** Alt+ArrowUp and Alt+ArrowDown move the active row in its folder. */
  function onAltKeydown(event: KeyboardEvent): void {
    const by = { ArrowUp: -1, ArrowDown: 1 }[event.key] as -1 | 1 | undefined;
    if (!by) return;
    event.preventDefault();
    if (active) void treeState.shift(active.path, by);
  }

  function onRowClick(index: number): void {
    const row = rows[index];
    if (!row || dragEnded) return;
    select(index);
    tree?.focus();
    open(row);
  }

  /**
   * Opens the active row's menu at a point, or under its button; with no
   * active row, the tree's own menu.
   */
  function openMenu(at: { x: number; y: number } | null): void {
    if (activeIndex < 0) {
      const box = (tree ?? scroller)?.getBoundingClientRect();
      areaMenuAt = at ?? { x: box?.left ?? 0, y: box?.top ?? 0 };
      return;
    }
    select(activeIndex);
    menuAt = at;
    menuOpen = true;
  }

  function onRowContextMenu(event: MouseEvent, index: number): void {
    if (isEditable(event.target)) return;
    event.preventDefault();
    select(index);
    openMenu({ x: event.clientX, y: event.clientY });
  }

  // A right click on the empty part of the list, or the menu key on the tree.
  function onAreaContextMenu(event: MouseEvent): void {
    if (event.defaultPrevented || isEditable(event.target)) return;
    if (
      event.target instanceof Element &&
      event.target.closest(".actions, .over")
    )
      return;
    event.preventDefault();
    if (event.target === tree) openMenu(null);
    else areaMenuAt = { x: event.clientX, y: event.clientY };
  }

  function setMenuOpen(value: boolean): void {
    menuOpen = value;
    if (!value) menuAt = null;
  }

  // Focus that went back to a menu's button, or to whatever opened a
  // dialog, belongs in the tree.
  function backToTree(): void {
    (tree ?? newCollection)?.focus();
  }

  function closeDialog(): void {
    deleting = null;
    moving = null;
    // After the dialog has given the focus back.
    void tick().then(backToTree);
  }

  function finishRename(): void {
    void tick().then(() => {
      const focused = document.activeElement;
      if (!focused || focused === document.body) tree?.focus();
    });
  }

  function rename(path: string, name: string): void {
    void treeState.rename(path, name);
    finishRename();
  }

  function cancelRename(): void {
    treeState.renaming = null;
    finishRename();
  }

  function confirmDelete(): void {
    const row = deleting;
    closeDialog();
    if (row) void treeState.remove(row.path);
  }

  function moveTo(folder: string): void {
    const row = moving;
    closeDialog();
    if (!row) return;
    // To the end of the folder, which does not hold it now.
    const index = treeState.itemsOf(folder)?.length ?? 0;
    void treeState.move(row.path, folder, index);
  }

  /** Pixels from the top of the list. */
  function listY(clientY: number): number {
    if (!scroller) return clientY;
    return clientY - scroller.getBoundingClientRect().top + scroller.scrollTop;
  }

  function onRowPointerDown(event: PointerEvent, row: TreeRowData): void {
    if (event.button !== 0) return;
    drag.press(row, event.clientX, listY(event.clientY));
  }

  function onWindowPointerMove(event: PointerEvent): void {
    if (drag.pressed) drag.point(event.clientX, listY(event.clientY), rows);
  }

  function onWindowPointerUp(event: PointerEvent): void {
    if (!drag.pressed) return;
    drag.point(event.clientX, listY(event.clientY), rows);
    const dragged = drag.source !== null;
    const drop = drag.release();
    if (dragged) {
      dragEnded = true;
      setTimeout(() => (dragEnded = false));
    }
    if (drop) void treeState.move(drop.path, drop.folder, drop.index);
  }
</script>

<svelte:window
  onkeydown={onWindowKeydown}
  onpointermove={onWindowPointerMove}
  onpointerup={onWindowPointerUp}
  onpointercancel={() => drag.cancel()}
  onblur={() => drag.cancel()}
/>

<div class="sidebar">
  <div class="head">
    <h2>Collections</h2>
    <button
      type="button"
      class="icon"
      aria-label="New request"
      title="New request"
      onclick={() => void treeState.newRequest()}
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
    <div class="empty" role="presentation" oncontextmenu={onAreaContextMenu}>
      <p>No requests yet</p>
      <button
        type="button"
        class="action"
        bind:this={newCollection}
        onclick={() => void treeState.newFolder(ROOT)}>New collection</button
      >
    </div>
  {:else if treeState.info}
    <div
      class="scroller"
      class:dragging={drag.source !== null}
      role="presentation"
      bind:this={scroller}
      {@attach measure}
      onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
      oncontextmenu={onAreaContextMenu}
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
              dragging={row.path === drag.source?.path}
              onclick={() => onRowClick(start + i)}
              onpointerdown={(e: PointerEvent) => onRowPointerDown(e, row)}
              oncontextmenu={(e: MouseEvent) => onRowContextMenu(e, start + i)}
            />
          {/each}
        </div>
        {#if active && !drag.source && active.path !== treeState.renaming}
          <div class="actions" style:top="{activeIndex * ROW}px">
            <TreeMenu
              row={active}
              bind:open={() => menuOpen, setMenuOpen}
              at={menuAt}
              onmove={(row: TreeRowData) => (moving = row)}
              ondelete={(row: TreeRowData) => (deleting = row)}
              onclose={backToTree}
            />
          </div>
        {/if}
        {#if renamingIndex >= 0 && rows[renamingIndex]}
          {@const row = rows[renamingIndex]}
          {#key row.path}
            <div class="over" style:top="{renamingIndex * ROW}px">
              <RenameRow
                {row}
                onrename={(name: string) => rename(row.path, name)}
                oncancel={cancelRename}
              />
            </div>
          {/key}
        {/if}
        {#if drag.drop?.line}
          <div
            class="drop-line"
            data-drop="line"
            aria-hidden="true"
            style:top="{drag.drop.line.at * ROW}px"
            style:left="calc(var(--s-2) + {drag.drop.line.depth * 14}px)"
          ></div>
        {:else if drag.drop?.into !== undefined}
          <div
            class="drop-into"
            data-drop="into"
            aria-hidden="true"
            style:top="{drag.drop.into * ROW}px"
          ></div>
        {/if}
      </div>
    </div>
  {/if}

  {#if areaMenuAt}
    <TreeMenu
      row={null}
      hidden
      bind:open={
        () => areaMenuAt !== null,
        (value) => {
          if (!value) areaMenuAt = null;
        }
      }
      at={areaMenuAt}
      onmove={() => {}}
      ondelete={() => {}}
      onclose={backToTree}
    />
  {/if}

  {#if deleting}
    <DeleteItem
      row={deleting}
      ondelete={confirmDelete}
      oncancel={closeDialog}
    />
  {/if}

  {#if moving}
    <MoveItem
      row={moving}
      targets={treeState.moveTargets(moving.path)}
      onmove={moveTo}
      oncancel={closeDialog}
    />
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

  .scroller.dragging {
    cursor: grabbing;
  }

  .over {
    position: absolute;
    left: 0;
    right: 0;
  }

  .actions {
    position: absolute;
    right: var(--s-1);
    height: 28px;
    display: flex;
    align-items: center;
  }

  .drop-line {
    position: absolute;
    right: 0;
    height: 2px;
    margin-top: -1px;
    border-radius: 1px;
    background: var(--focus);
    pointer-events: none;
  }

  .drop-into {
    position: absolute;
    left: 0;
    right: 0;
    height: 28px;
    border-radius: var(--r-ctl);
    box-shadow: inset 0 0 0 1px var(--focus);
    pointer-events: none;
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
