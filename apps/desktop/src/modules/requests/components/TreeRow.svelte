<!-- One row of the collection tree: a folder or a request. -->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import type { TreeRowData } from "../state/tree.svelte";
  import MethodBadge from "./MethodBadge.svelte";

  let {
    row,
    id,
    selected,
    dragging = false,
    onclick,
    onpointerdown,
    oncontextmenu,
  }: {
    row: TreeRowData;
    id: string;
    selected: boolean;
    /** The row is being dragged. */
    dragging?: boolean;
    onclick: () => void;
    onpointerdown?: (e: PointerEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
  } = $props();
</script>

<div
  {id}
  class="row"
  class:selected
  class:dragging
  role="treeitem"
  tabindex="-1"
  aria-level={row.depth + 1}
  aria-setsize={row.size}
  aria-posinset={row.position}
  aria-expanded={row.kind === "folder" ? row.open : undefined}
  aria-selected={selected}
  style:padding-left="calc(var(--s-2) + {row.depth * 14}px)"
  {onclick}
  {onpointerdown}
  {oncontextmenu}
  onkeydown={() => {}}
>
  {#if row.kind === "folder"}<span class="chevron"
      >{#if row.open}<ChevronDown
          size={14}
          strokeWidth={1.5}
        />{:else}<ChevronRight size={14} strokeWidth={1.5} />{/if}</span
    ><span class="name">{row.name}</span>{#if row.broken}<span
        class="warn"
        title="_folder.yaml has a problem"
        ><TriangleAlert size={14} strokeWidth={1.5} /></span
      >{/if}<span class="count">{row.count}</span>{:else}<MethodBadge
      method={row.method ?? ""}
      size="small"
    /><span class="name">{row.name}</span>{/if}
</div>

<style>
  .row {
    height: 28px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding-right: var(--s-2);
    border-radius: var(--r-ctl);
    color: var(--text-2);
    cursor: default;
    outline: none;
    user-select: none;
  }

  .row:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .selected {
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
    /* Room for the More actions button over the row. */
    padding-right: 32px;
  }

  .dragging {
    opacity: 0.5;
  }

  .chevron {
    width: 14px;
    flex: none;
    display: inline-flex;
    color: var(--text-3);
  }

  .name {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .warn {
    display: inline-flex;
    flex: none;
    color: var(--warning-text);
  }

  .count {
    flex: none;
    font: 400 12px/1 var(--font-mono);
    color: var(--text-3);
  }
</style>
